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
                         silence_duration: float = 1.2) -> np.ndarray | None:
    """Rekam otomatis: mulai saat ada suara, berhenti setelah hening sejenak.

    Tanpa perlu menekan ENTER — untuk mode hands-free.
    """
    if silence_threshold is None:
        silence_threshold = config.MIN_SPEECH_RMS

    chunk_dur = 0.1
    chunk_frames = int(SAMPLE_RATE * chunk_dur)
    frames = []
    started = False
    silent_count = 0

    try:
        with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32") as stream:
            for _ in range(int(max_seconds / chunk_dur)):
                data, _overflow = stream.read(chunk_frames)
                frames.append(data.copy())
                if float(np.abs(data).mean()) > silence_threshold:
                    started = True
                    silent_count = 0
                elif started:
                    silent_count += 1
                    if silent_count * chunk_dur >= silence_duration:
                        break  # sudah diam cukup lama -> selesai
    except Exception as e:
        ui.error(f"Perangkat audio bermasalah: {e}")
        return None

    if not started:
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
                      silence_duration: float = 1.2):
    """Rekam mic sampai hening, TAPI kalau user menekan ENTER -> beralih ketik.

    Return salah satu:
      ('ketik', baris)   user menekan Enter (baris bisa kosong)
      ('suara', audio)   ada ucapan terekam
      ('kosong', None)   tidak ada ucapan maupun ketikan
    """
    if silence_threshold is None:
        silence_threshold = config.MIN_SPEECH_RMS

    chunk_dur = 0.1
    chunk_frames = int(SAMPLE_RATE * chunk_dur)
    frames = []
    started = False
    silent_count = 0
    bisa_keyboard = os.name == "posix"  # select(stdin) andal di POSIX saja

    try:
        with sd.InputStream(samplerate=SAMPLE_RATE, channels=1, dtype="float32") as stream:
            for _ in range(int(max_seconds / chunk_dur)):
                if bisa_keyboard and select.select([sys.stdin], [], [], 0)[0]:
                    return ("ketik", sys.stdin.readline().strip())
                data, _overflow = stream.read(chunk_frames)
                frames.append(data.copy())
                if float(np.abs(data).mean()) > silence_threshold:
                    started = True
                    silent_count = 0
                elif started:
                    silent_count += 1
                    if silent_count * chunk_dur >= silence_duration:
                        break
    except Exception as e:
        ui.error(f"Perangkat audio bermasalah: {e}")
        if bisa_keyboard:
            ui.info("  beralih ke mode ketik — silakan ketik perintah di bawah")
            return ("ketik", sys.stdin.readline().strip())
        return ("kosong", None)

    if not started:
        return ("kosong", None)
    return ("suara", np.concatenate(frames, axis=0).flatten())


def transcribe(audio) -> str:
    """Ubah audio jadi teks. VAD + filter halusinasi agar hening tak jadi teks."""
    # Lapis 1: audio terlalu pelan -> anggap tak ada ucapan (skip Whisper).
    if not isinstance(audio, str) and _terlalu_hening(audio):
        return ""

    model = _get_model()
    # Lapis 2: VAD bawaan + cegah halusinasi berulang.
    segments, _info = model.transcribe(
        audio,
        language=lang.whisper(),
        beam_size=5,
        vad_filter=True,
        condition_on_previous_text=False,
        no_speech_threshold=0.6,
    )
    teks = " ".join(
        seg.text for seg in segments
        if getattr(seg, "no_speech_prob", 0.0) < 0.6
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


def listen_auto() -> str:
    """Hands-free: rekam otomatis (berhenti saat hening) lalu transkripsi."""
    audio = record_until_silence()
    if audio is None or len(audio) == 0:
        return ""
    ui.info("  mentranskripsi…")
    return transcribe(audio)


if __name__ == "__main__":
    # Tes cepat: python -m voca.listen  -> bicara, lihat teksnya
    print(f"\nKamu bilang: {listen()!r}")
