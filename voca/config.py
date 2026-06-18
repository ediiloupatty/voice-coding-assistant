"""
config.py — Pusat semua konfigurasi & path proyek.

Semua setting yang bisa diatur lewat environment variable dikumpulkan di sini
(tidak lagi tersebar di banyak file). Modul lain cukup `from . import config`
lalu membaca `config.NAMA_SETTING`.
"""

import os
from pathlib import Path

from dotenv import load_dotenv

# Akar proyek = satu folder di atas paket 'voca'.
PROJECT_ROOT = Path(__file__).resolve().parent.parent
MODELS_DIR = PROJECT_ROOT / "models"

# Baca .env dari akar proyek (bukan folder tempat 'voca' dipanggil) supaya
# API key & setting tetap ketemu walau dijalankan dari direktori mana pun.
load_dotenv(PROJECT_ROOT / ".env")


# --- Model LLM: Qwen via DashScope (endpoint OpenAI-compatible) -------------
QWEN_API_KEY = os.getenv("DASHSCOPE_API_KEY")
QWEN_BASE_URL = os.getenv(
    "QWEN_BASE_URL", "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
)
QWEN_MODEL = os.getenv("QWEN_MODEL", "qwen-plus")
QWEN_TEMPERATURE = float(os.getenv("QWEN_TEMPERATURE", "0.3"))  # rendah = lebih fokus/akurat
LLM_MAX_RETRIES = int(os.getenv("LLM_MAX_RETRIES", "4"))        # percobaan saat error koneksi LLM
LLM_RETRY_BASE_DELAY = float(os.getenv("LLM_RETRY_BASE_DELAY", "2.0"))  # jeda awal retry (detik, naik eksponensial)

# --- Batas perilaku agent (anti-boros, anti-nyangkut, anti-overflow) -------
MAX_TOOL_ITERS = int(os.getenv("MAX_TOOL_ITERS", "15"))         # maks putaran tool per giliran
MAX_HISTORY = int(os.getenv("MAX_HISTORY", "30"))              # maks pesan disimpan (di luar system)
MAX_READ_CHARS = int(os.getenv("MAX_READ_CHARS", "20000"))     # batas karakter saat baca file
MAX_OUTPUT_CHARS = int(os.getenv("MAX_OUTPUT_CHARS", "8000"))  # batas karakter output command
LIST_MAX_DEPTH = int(os.getenv("LIST_MAX_DEPTH", "4"))         # kedalaman maks list_files
LIST_MAX_ENTRIES = int(os.getenv("LIST_MAX_ENTRIES", "400"))   # jumlah baris maks list_files
SEARCH_MAX_RESULTS = int(os.getenv("SEARCH_MAX_RESULTS", "60"))  # hasil maks search_files
SHOW_DIFF = os.getenv("SHOW_DIFF", "1") != "0"                 # tampilkan diff saat edit file
DIFF_MAX_LINES = int(os.getenv("DIFF_MAX_LINES", "200"))       # batas baris diff yang dicetak
COMMAND_TIMEOUT = int(os.getenv("COMMAND_TIMEOUT", "300"))     # batas waktu run_command (detik)
MAX_HISTORY_TOKENS = int(os.getenv("MAX_HISTORY_TOKENS", "12000"))  # estimasi token maks history
CHARS_PER_TOKEN = float(os.getenv("CHARS_PER_TOKEN", "3.5"))   # heuristik estimasi token

# --- Sesi: simpan & lanjutkan percakapan ------------------------------------
SESSION_ENABLED = os.getenv("SESSION_ENABLED", "1") != "0"     # 0 = jangan simpan sesi
SESSION_FILE = os.getenv("SESSION_FILE", ".voca/session.json")  # relatif ke folder kerja

# --- Bahasa aktif (Indonesia/English, bisa diganti saat jalan) -------------
VOCA_LANG = os.getenv("VOCA_LANG", "id")                 # bahasa default: 'id' / 'en'

# --- Suara keluar: TTS Piper (lokal/offline) -------------------------------
VOICE_ENABLED = os.getenv("VOICE_ENABLED", "1") != "0"
PIPER_MODEL = os.getenv("PIPER_MODEL", str(MODELS_DIR / "id_ID-news_tts-medium.onnx"))
# Model Piper English opsional. Kalau filenya ada -> suara English lokal & streaming;
# kalau tidak -> otomatis pakai gTTS (online) untuk suara English.
PIPER_MODEL_EN = os.getenv("PIPER_MODEL_EN", str(MODELS_DIR / "en_US-amy-medium.onnx"))
VOICE_PITCH = float(os.getenv("VOICE_PITCH", "1.1"))     # nada: >1 lebih tinggi
VOICE_SPEED = float(os.getenv("VOICE_SPEED", "1.12"))    # tempo: >1 lebih pelan
VOICE_VOLUME = float(os.getenv("VOICE_VOLUME", "0.9"))   # 0..1: kecil = lembut
VOICE_LANG = os.getenv("VOICE_LANG", "id")               # untuk fallback gTTS
SPEAK_PHONETIC = os.getenv("SPEAK_PHONETIC", "1") != "0"  # eja kata Inggris umum saat bicara

# --- Suara masuk: STT Whisper (lokal/offline) ------------------------------
WHISPER_MODEL = os.getenv("WHISPER_MODEL", "small")      # tiny..large-v3
WHISPER_LANG = os.getenv("WHISPER_LANG", "id")
SAMPLE_RATE = 16000                                      # Whisper butuh 16 kHz mono
MIN_SPEECH_RMS = float(os.getenv("MIN_SPEECH_RMS", "0.01"))  # energi min dianggap ada ucapan (anti-halusinasi)
