"""
listen.py — "Telinga" si agent (Speech-to-Text dengan faster-whisper).

Merekam suara dari mikrofon lalu mengubahnya jadi teks dengan model Whisper
yang berjalan lokal/offline. Dua mode rekam:
  - push-to-talk (tekan ENTER mulai/berhenti) — untuk mode teks.
  - auto-berhenti saat hening — untuk mode hands-free.

Dependensi: faster-whisper, sounddevice (butuh PortAudio di sistem).
"""

import numpy as np
import sounddevice as sd
from faster_whisper import WhisperModel

from . import config

SAMPLE_RATE = config.SAMPLE_RATE

_model = None


def _get_model() -> WhisperModel:
    """Muat model Whisper sekali saja (lazy, lalu di-cache)."""
    global _model
    if _model is None:
        print(f"   [memuat model Whisper '{config.WHISPER_MODEL}' (sekali di awal)...]")
        _model = WhisperModel(config.WHISPER_MODEL, device="cpu", compute_type="int8")
    return _model


def record_until_enter() -> np.ndarray | None:
    """Rekam dari mikrofon sampai user menekan ENTER. Return audio float32."""
    frames = []

    def callback(indata, frames_count, time_info, status):
        frames.append(indata.copy())

    input("\nTekan ENTER untuk MULAI bicara...")
    with sd.InputStream(samplerate=SAMPLE_RATE, channels=1,
                        dtype="float32", callback=callback):
        input("Merekam... tekan ENTER lagi untuk BERHENTI.")

    if not frames:
        return None
    return np.concatenate(frames, axis=0).flatten()


def record_until_silence(max_seconds: float = 15.0,
                         silence_threshold: float = 0.01,
                         silence_duration: float = 1.2) -> np.ndarray | None:
    """Rekam otomatis: mulai saat ada suara, berhenti setelah hening sejenak.

    Tanpa perlu menekan ENTER — untuk mode hands-free.
    """
    chunk_dur = 0.1
    chunk_frames = int(SAMPLE_RATE * chunk_dur)
    frames = []
    started = False
    silent_count = 0

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

    if not started:
        return None
    return np.concatenate(frames, axis=0).flatten()


def transcribe(audio) -> str:
    """Ubah audio (numpy array atau path file) jadi teks."""
    model = _get_model()
    segments, _info = model.transcribe(audio, language=config.WHISPER_LANG, beam_size=5)
    return " ".join(seg.text for seg in segments).strip()


def listen() -> str:
    """Push-to-talk: rekam (ENTER mulai/berhenti) lalu transkripsi jadi teks."""
    audio = record_until_enter()
    if audio is None or len(audio) == 0:
        return ""
    print("   [mentranskripsi...]")
    return transcribe(audio)


def listen_auto() -> str:
    """Hands-free: rekam otomatis (berhenti saat hening) lalu transkripsi."""
    audio = record_until_silence()
    if audio is None or len(audio) == 0:
        return ""
    print("   [mentranskripsi...]")
    return transcribe(audio)


if __name__ == "__main__":
    # Tes cepat: python -m voca.listen  -> bicara, lihat teksnya
    print(f"\nKamu bilang: {listen()!r}")
