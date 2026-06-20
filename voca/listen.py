"""
listen.py — "Telinga" si agent (Speech-to-Text dengan faster-whisper).

Merekam suara dari mikrofon lalu mengubahnya jadi teks dengan model Whisper
yang berjalan lokal/offline. Dua mode rekam:
  - push-to-talk (tekan ENTER mulai/berhenti) — untuk mode teks.
  - auto-berhenti saat hening — untuk mode hands-free.

Dependensi: faster-whisper, sounddevice (butuh PortAudio di sistem).
"""

import os
import re
import select
import sys

import numpy as np
import sounddevice as sd
from faster_whisper import WhisperModel

from . import config
from . import lang
from . import ui

SAMPLE_RATE = config.SAMPLE_RATE

_model = None

# Frasa yang sering "dikarang" Whisper saat audio hening/noise (halusinasi).
# Model Indonesia paling sering memunculkan kalimat penutup video YouTube.
_HALUSINASI = {
    "terima kasih telah menonton", "terima kasih sudah menonton",
    "terima kasih karena menonton", "terimakasih karena menonton",
    "terima kasih banyak", "terima kasih", "terimakasih",
    "jangan lupa subscribe", "jangan lupa like dan subscribe",
    "tolong berlangganan", "sampai jumpa di video selanjutnya",
    "thank you for watching", "thanks for watching", "thank you",
    "please subscribe", "you", "bye", "silakan", "terjemahan",
}


def _get_model() -> WhisperModel:
    """Muat model Whisper sekali saja (lazy, lalu di-cache)."""
    global _model
    if _model is None:
        ui.info(f"  memuat model Whisper '{config.WHISPER_MODEL}' (sekali di awal)…")
        try:
            # Coba muat lokal secara instan tanpa cek internet ke Hugging Face
            _model = WhisperModel(config.WHISPER_MODEL, device="cpu", compute_type="int8", local_files_only=True)
        except Exception:
            # Jika belum diunduh, biarkan terhubung online untuk mengunduh
            _model = WhisperModel(config.WHISPER_MODEL, device="cpu", compute_type="int8", local_files_only=False)
    return _model


# ── Voice Activity Detection (VAD) — Silero (neural) ────────────────────────
# Penangkapan suara sepenuhnya memakai Silero VAD: deteksi ucapan neural yang
# tahan noise & menangkap suara pelan. Ini DEPENDENSI WAJIB (lihat install.sh /
# requirements.txt) — tak ada lagi fallback RMS.

_VAD_INSTALL_HINT = (
    "VAD Silero tidak tersedia. Pasang dulu:\n"
    "    pip install torch torchaudio --index-url https://download.pytorch.org/whl/cpu\n"
    "    pip install silero-vad"
)

_vad_model = None


def _load_silero():
    """Muat model Silero VAD sekali. Raise RuntimeError (pesan jelas) bila gagal."""
    global _vad_model
    if _vad_model is None:
        try:
            from silero_vad import load_silero_vad
            _vad_model = load_silero_vad()
            ui.info("  VAD: Silero (neural) aktif.")
        except Exception as e:
            raise RuntimeError(f"{_VAD_INSTALL_HINT}\n  (detail: {e})") from e
    return _vad_model


def preload_vad() -> None:
    """Preload model VAD di awal (dipanggil sidecar) agar ucapan pertama tak lambat.

    Bila gagal, cetak instruksi pasang yang jelas ke stderr lalu teruskan error —
    suara TIDAK akan jalan tanpa Silero (sengaja, tak ada fallback senyap).
    """
    try:
        _load_silero()
    except RuntimeError as e:
        print(f"\033[1;31m✗ {e}\033[0m", file=sys.stderr, flush=True)
        raise


class _SileroGate:
    """Deteksi suara neural via Silero VAD (butuh window 512-sampel @16kHz)."""
    WIN = 512

    def __init__(self, threshold: float):
        import torch
        self._torch = torch
        self.threshold = threshold
        self.buf = np.empty(0, dtype=np.float32)
        model = _load_silero()
        try:
            model.reset_states()
        except Exception:
            pass

    def voiced(self, chunk: np.ndarray) -> bool:
        # Akumulasi lalu proses per window 512-sampel; voiced bila ada window
        # yang melewati ambang probabilitas. `chunk` dari sounddevice berbentuk
        # 2D (frames, channels) → ratakan ke 1D mono dulu.
        mono = np.asarray(chunk, dtype=np.float32).reshape(-1)
        self.buf = np.concatenate([self.buf, mono])
        hit = False
        while len(self.buf) >= self.WIN:
            win = self.buf[:self.WIN]
            self.buf = self.buf[self.WIN:]
            prob = _vad_model(self._torch.from_numpy(win), SAMPLE_RATE).item()
            if prob >= self.threshold:
                hit = True
        return hit


def _make_gate(_rms_threshold: float | None = None):
    """Detektor suara — selalu Silero VAD (raise bila paket tak terpasang)."""
    return _SileroGate(config.VAD_THRESHOLD)


def record_until_enter() -> np.ndarray | None:
    """Rekam dari mikrofon sampai user menekan ENTER. Return audio float32."""
    frames = []

    def callback(indata, frames_count, time_info, status):
        frames.append(indata.copy())

    input("\nTekan ENTER untuk MULAI bicara...")
    try:
        with sd.InputStream(samplerate=SAMPLE_RATE, channels=1,
                            dtype="float32", callback=callback):
            input("Merekam... tekan ENTER lagi untuk BERHENTI.")
    except Exception as e:
        ui.error(f"Perangkat audio bermasalah: {e}")
        return None

    if not frames:
        return None
    return np.concatenate(frames, axis=0).flatten()


def record_until_silence(max_seconds: float = 60.0,
                         silence_threshold: float | None = None,
                         silence_duration: float | None = None,
                         start_timeout: float | None = None,
                         should_cancel=None,
                         on_vad=None) -> np.ndarray | None:
    """Rekam otomatis: mulai saat ada suara, berhenti setelah hening sejenak.

    Tanpa perlu menekan ENTER — untuk mode hands-free. Pakai debounce mulai
    (butuh beberapa chunk bersuara berturut-turut) + durasi minimum agar noise
    kecil/blip tak memicu rekaman.

    `should_cancel`: callable opsional yang dicek tiap chunk; bila mengembalikan
    True, perekaman dibatalkan & mengembalikan None (dipakai sidecar agar tombol
    `t` di core Rust bisa menghentikan dengar seketika).
    """
    if silence_threshold is None:
        silence_threshold = config.MIN_SPEECH_RMS
    if silence_duration is None:
        silence_duration = config.SILENCE_DURATION
    if start_timeout is None:
        start_timeout = config.SPEECH_START_TIMEOUT

    chunk_dur = 0.1
    chunk_frames = int(SAMPLE_RATE * chunk_dur)
    frames = []
    started = False
    silent_count = 0
    voiced_run = 0     # chunk bersuara berturut-turut (debounce mulai)
    voiced_total = 0   # total chunk bersuara (durasi minimum)
    last_voiced = None  # status terakhir yang dikirim ke on_vad (anti-spam)
    gate = _make_gate(silence_threshold)

    try:
        with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32") as stream:
            for i in range(int(max_seconds / chunk_dur)):
                if should_cancel is not None and should_cancel():
                    return None
                data, _overflow = stream.read(chunk_frames)
                frames.append(data.copy())
                voiced_now = gate.voiced(data)
                # Lapor perubahan status suara (untuk indikator real-time di TUI).
                if on_vad is not None and voiced_now != last_voiced:
                    try:
                        on_vad(voiced_now)
                    except Exception:
                        pass
                    last_voiced = voiced_now
                if voiced_now:
                    voiced_run += 1
                    voiced_total += 1
                    if voiced_run >= config.SPEECH_START_CHUNKS:
                        started = True
                    silent_count = 0
                else:
                    voiced_run = 0
                    if started:
                        silent_count += 1
                        if silent_count * chunk_dur >= silence_duration:
                            break  # sudah diam cukup lama -> selesai
                # Belum mulai bicara sampai batas tunggu → tutup jendela, recycle cepat.
                if not started and (i + 1) * chunk_dur >= start_timeout:
                    break
    except Exception as e:
        ui.error(f"Perangkat audio bermasalah: {e}")
        return None

    # Buang kalau tak pernah mulai, atau total suara terlalu pendek (noise/blip).
    if not started or voiced_total * chunk_dur < config.MIN_SPEECH_SECONDS:
        return None
    return np.concatenate(frames, axis=0).flatten()


def _terlalu_hening(audio) -> bool:
    """True kalau energi audio terlalu kecil (kemungkinan cuma hening/noise)."""
    if audio is None or len(audio) == 0:
        return True
    rms = float(np.sqrt(np.mean(np.square(audio))))
    return rms < config.MIN_SPEECH_RMS


def _is_halusinasi(teks: str) -> bool:
    """True kalau teks cuma frasa halusinasi khas Whisper (bukan ucapan asli)."""
    bersih = re.sub(r"[^\w\s]", "", teks.lower()).strip()
    return bersih in _HALUSINASI


def _rekam_atau_ketik(max_seconds: float = 60.0,
                      silence_threshold: float | None = None,
                      silence_duration: float | None = None,
                      start_timeout: float | None = None):
    """Rekam mic sampai hening, TAPI kalau user menekan ENTER -> beralih ketik.

    Return salah satu:
      ('ketik', baris)   user menekan Enter (baris bisa kosong)
      ('suara', audio)   ada ucapan terekam
      ('kosong', None)   tidak ada ucapan maupun ketikan
    """
    if silence_threshold is None:
        silence_threshold = config.MIN_SPEECH_RMS
    if silence_duration is None:
        silence_duration = config.SILENCE_DURATION
    if start_timeout is None:
        start_timeout = config.SPEECH_START_TIMEOUT

    chunk_dur = 0.1
    chunk_frames = int(SAMPLE_RATE * chunk_dur)
    frames = []
    started = False
    silent_count = 0
    voiced_run = 0
    voiced_total = 0
    gate = _make_gate(silence_threshold)
    bisa_keyboard = os.name == "posix"  # select(stdin) andal di POSIX saja

    try:
        with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32") as stream:
            for i in range(int(max_seconds / chunk_dur)):
                if bisa_keyboard and select.select([sys.stdin], [], [], 0)[0]:
                    return ("ketik", sys.stdin.readline().strip())
                data, _overflow = stream.read(chunk_frames)
                frames.append(data.copy())
                if gate.voiced(data):
                    voiced_run += 1
                    voiced_total += 1
                    if voiced_run >= config.SPEECH_START_CHUNKS:
                        started = True
                    silent_count = 0
                else:
                    voiced_run = 0
                    if started:
                        silent_count += 1
                        if silent_count * chunk_dur >= silence_duration:
                            break
                # Belum mulai bicara sampai batas tunggu → kembali (recycle cepat).
                if not started and (i + 1) * chunk_dur >= start_timeout:
                    break
    except Exception as e:
        ui.error(f"Perangkat audio bermasalah: {e}")
        if bisa_keyboard:
            ui.info("  beralih ke mode ketik — silakan ketik perintah di bawah")
            return ("ketik", sys.stdin.readline().strip())
        return ("kosong", None)

    if not started or voiced_total * chunk_dur < config.MIN_SPEECH_SECONDS:
        return ("kosong", None)
    return ("suara", np.concatenate(frames, axis=0).flatten())


def _trim_to_speech(audio: np.ndarray) -> np.ndarray | None:
    """Pangkas audio ke rentang ucapan saja (pakai timestamp Silero) + sedikit
    padding. Buang hening/noise di awal-akhir → Whisper lebih cepat & akurat,
    halusinasi berkurang. Return None bila Silero tak menemukan ucapan sama sekali.
    Bila Silero tak tersedia/gagal, kembalikan audio apa adanya (jangan menghambat).
    """
    try:
        from silero_vad import get_speech_timestamps
        model = _load_silero()
    except Exception:
        return audio
    ts = get_speech_timestamps(
        audio, model, sampling_rate=SAMPLE_RATE, threshold=config.VAD_THRESHOLD,
    )
    if not ts:
        return None
    pad = int(0.15 * SAMPLE_RATE)  # 150 ms bantal agar kata tepi tak terpotong
    start = max(0, ts[0]["start"] - pad)
    end = min(len(audio), ts[-1]["end"] + pad)
    return audio[start:end]


def transcribe(audio) -> str:
    """Ubah audio jadi teks. VAD + filter halusinasi agar hening tak jadi teks."""
    # Lapis 1: audio terlalu pelan -> anggap tak ada ucapan (skip Whisper).
    if not isinstance(audio, str) and _terlalu_hening(audio):
        return ""

    # Lapis 1b: pangkas ke rentang ucapan (Silero). None = tak ada ucapan → skip.
    if not isinstance(audio, str):
        audio = _trim_to_speech(audio)
        if audio is None or len(audio) == 0:
            return ""

    model = _get_model()
    # Lapis 2: VAD bawaan + ambang keyakinan agar noise/hening tak jadi teks.
    segments, _info = model.transcribe(
        audio,
        language=lang.whisper(),
        beam_size=config.WHISPER_BEAM_SIZE,
        vad_filter=True,
        vad_parameters=dict(min_silence_duration_ms=500),
        condition_on_previous_text=False,   # cegah halusinasi berulang
        no_speech_threshold=config.NO_SPEECH_THRESHOLD,
        log_prob_threshold=config.LOGPROB_THRESHOLD,
        compression_ratio_threshold=2.4,    # buang keluaran berulang-ulang
    )
    # Saring per-segmen: buang yang kemungkinan "bukan ucapan" atau keyakinan rendah.
    teks = " ".join(
        seg.text for seg in segments
        if getattr(seg, "no_speech_prob", 0.0) < config.NO_SPEECH_THRESHOLD
        and getattr(seg, "avg_logprob", 0.0) > config.LOGPROB_THRESHOLD
    ).strip()

    # Lapis 3: jaring pengaman frasa halusinasi khas.
    if _is_halusinasi(teks):
        return ""
    return teks


def listen() -> str:
    """Push-to-talk: rekam (ENTER mulai/berhenti) lalu transkripsi jadi teks."""
    audio = record_until_enter()
    if audio is None or len(audio) == 0:
        return ""
    ui.info("  mentranskripsi…")
    return transcribe(audio)


def listen_auto(should_cancel=None, on_vad=None) -> str:
    """Hands-free: rekam otomatis (berhenti saat hening) lalu transkripsi.

    `should_cancel`: lihat record_until_silence — untuk pembatalan dari sidecar.
    `on_vad`: callback(bool) saat status suara berubah — untuk indikator TUI.
    """
    audio = record_until_silence(should_cancel=should_cancel, on_vad=on_vad)
    if audio is None or len(audio) == 0:
        return ""
    ui.info("  mentranskripsi…")
    return transcribe(audio)
