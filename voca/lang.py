"""
lang.py — Sumber kebenaran "bahasa aktif" Voca (Indonesia / English).

Satu tempat menyimpan bahasa yang sedang dipakai beserta semua setelan turunannya
(kode STT Whisper, kode/​model TTS, instruksi bahasa untuk LLM, kata pemicu ganti
bahasa). Modul lain (voice, listen, agent) cukup membaca getter di sini, sehingga
ganti bahasa saat runtime otomatis berlaku di STT, TTS, dan jawaban AI.

Catatan: modul ini HANYA bergantung pada `config` agar tidak ada impor siklik.
"""

import re

from . import config

# Tabel setelan per bahasa.
_LANGS = {
    "id": {
        "name": "Bahasa Indonesia",
        "whisper": "id",                  # kode bahasa Whisper (STT)
        "gtts": "id",                     # kode bahasa gTTS (TTS online)
        "piper": config.PIPER_MODEL,      # model Piper lokal (TTS offline)
        "phonetic": config.SPEAK_PHONETIC,  # eja kata Inggris (hanya untuk suara ID)
        "directive": "Selalu balas dalam Bahasa Indonesia yang natural dan santai.",
        "switched": "Oke, sekarang pakai Bahasa Indonesia.",
        "cmd": {"indonesia", "indo", "id", "bahasa indonesia", "bahasa", "ke indonesia"},
    },
    "en": {
        "name": "English",
        "whisper": "en",
        "gtts": "en",
        "piper": config.PIPER_MODEL_EN,   # opsional; kalau tak ada -> gTTS
        "phonetic": False,                # jangan eja-fonetik saat bahasa English
        "directive": ("IMPORTANT: From now on, respond ONLY in English, regardless of "
                      "the language used in the instructions above. Keep the same casual, "
                      "concise style."),
        "switched": "Okay, switching to English.",
        "cmd": {"english", "en", "inggris", "bahasa inggris", "ke english", "to english"},
    },
}

# Bahasa aktif saat ini (default dari config; jatuh ke 'id' kalau tak dikenal).
CURRENT = config.VOCA_LANG if config.VOCA_LANG in _LANGS else "id"


def set(code: str) -> bool:
    """Ganti bahasa aktif. Return True kalau berhasil (kode dikenal)."""
    global CURRENT
    if code in _LANGS:
        CURRENT = code
        return True
    return False


def code() -> str:
    """Kode bahasa aktif ('id' / 'en')."""
    return CURRENT


def _cur() -> dict:
    return _LANGS[CURRENT]


def whisper() -> str:
    """Kode bahasa untuk Whisper (STT)."""
    return _cur()["whisper"]


def gtts() -> str:
    """Kode bahasa untuk gTTS (TTS online)."""
    return _cur()["gtts"]


def piper_model() -> str:
    """Path model Piper untuk bahasa aktif (mungkin file-nya tidak ada)."""
    return _cur()["piper"]


def phonetic() -> bool:
    """Apakah eja-fonetik kata Inggris dipakai (hanya untuk suara Indonesia)."""
    return _cur()["phonetic"]


def name() -> str:
    """Nama bahasa aktif yang enak dibaca."""
    return _cur()["name"]


def directive() -> str:
    """Instruksi bahasa untuk disisipkan ke system prompt LLM."""
    return _cur()["directive"]


def switched_msg() -> str:
    """Kalimat konfirmasi setelah ganti bahasa (diucapkan & dicetak)."""
    return _cur()["switched"]


def detect_command(teks: str) -> str | None:
    """Deteksi perintah ganti bahasa dari ucapan/ketikan pendek.

    Return kode bahasa ('id'/'en') kalau teks jelas-jelas perintah ganti bahasa,
    selain itu None. Dibatasi ucapan pendek (<=3 kata) supaya kata 'english' di
    tengah kalimat biasa tidak salah memicu.
    """
    bersih = re.sub(r"[^\w\s]", "", teks.lower()).strip()
    if not bersih or len(bersih.split()) > 3:
        return None
    for kode, data in _LANGS.items():
        if bersih in data["cmd"]:
            return kode
    return None
