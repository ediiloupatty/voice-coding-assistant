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

# --- Provider LLM aktif: 'qwen' (default) atau 'openai' ---------------------
# Qwen TIDAK diganti — OpenAI hanya opsi tambahan. Bisa di-toggle saat jalan
# (ketik 'openai'/'gpt' atau 'qwen') atau diset default lewat VOCA_PROVIDER.
VOCA_PROVIDER = os.getenv("VOCA_PROVIDER", "qwen")
OPENAI_API_KEY = os.getenv("OPENAI_API_KEY")
OPENAI_BASE_URL = os.getenv("OPENAI_BASE_URL", "https://api.openai.com/v1")
OPENAI_MODEL = os.getenv("OPENAI_MODEL", "gpt-4o")

# OpenRouter (opsi; OpenAI-SDK compatible, akses banyak model lewat 1 key).
OPENROUTER_API_KEY = os.getenv("OPENROUTER_API_KEY")
OPENROUTER_BASE_URL = os.getenv("OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1")
OPENROUTER_MODEL = os.getenv("OPENROUTER_MODEL", "openai/gpt-oss-120b:free")
# Aktifkan mode reasoning OpenRouter (model berpikir dulu sebelum menjawab).
OPENROUTER_REASONING = os.getenv("OPENROUTER_REASONING", "1") != "0"

# DeepSeek (opsi; OpenAI-SDK compatible, mode thinking opsional).
DEEPSEEK_API_KEY = os.getenv("DEEPSEEK_API_KEY")
DEEPSEEK_BASE_URL = os.getenv("DEEPSEEK_BASE_URL", "https://api.deepseek.com")
DEEPSEEK_MODEL = os.getenv("DEEPSEEK_MODEL", "deepseek-v4-flash")
DEEPSEEK_THINKING = os.getenv("DEEPSEEK_THINKING", "1") != "0"  # mode berpikir + reasoning_effort high
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
VOCA_LANG = os.getenv("VOCA_LANG", "en")                 # bahasa default: 'en' / 'id'

# --- Suara keluar: TTS Piper (lokal/offline) -------------------------------
VOICE_ENABLED = os.getenv("VOICE_ENABLED", "1") != "0"
PIPER_MODEL = os.getenv("PIPER_MODEL", str(MODELS_DIR / "id_ID-news_tts-medium.onnx"))
# Model Piper English opsional. Kalau filenya ada -> suara English lokal & streaming;
# kalau tidak -> otomatis pakai gTTS (online) untuk suara English.
PIPER_MODEL_EN = os.getenv("PIPER_MODEL_EN", str(MODELS_DIR / "en_US-amy-medium.onnx"))
VOICE_PITCH = float(os.getenv("VOICE_PITCH", "1.1"))     # nada: >1 lebih tinggi
VOICE_SPEED = float(os.getenv("VOICE_SPEED", "0.95"))    # tempo: <1 lebih CEPAT
VOICE_VOLUME = float(os.getenv("VOICE_VOLUME", "0.9"))   # 0..1: kecil = lembut
# Variasi suara Piper agar TIDAK terdengar robot/monoton:
#   VOICE_NOISE   (noise_scale)   — variasi timbre/warna suara.
#   VOICE_NOISE_W (noise_w_scale) — variasi durasi/ritme fonem (intonasi natural).
# Terlalu tinggi = goyang/warbly; terlalu rendah = datar/robot. 0.6–1.0 wajar.
VOICE_NOISE = float(os.getenv("VOICE_NOISE", "0.7"))
VOICE_NOISE_W = float(os.getenv("VOICE_NOISE_W", "0.9"))
SPEAK_PHONETIC = os.getenv("SPEAK_PHONETIC", "1") != "0"  # eja kata Inggris umum saat bicara

# --- Suara masuk: STT Whisper (lokal/offline) ------------------------------
WHISPER_MODEL = os.getenv("WHISPER_MODEL", "small")      # tiny..large-v3
SAMPLE_RATE = 16000                                      # Whisper butuh 16 kHz mono

# --- Sensitivitas suara masuk (anti-"mimpi"/halusinasi saat noise kecil) ---
# MIN_SPEECH_RMS: kini HANYA dipakai _terlalu_hening() sebagai pra-cek sebelum
#   transkrip (lewati Whisper bila audio nyaris hening). Deteksi MULAI/berhenti
#   bicara sudah ditangani Silero VAD (lihat VAD_THRESHOLD), bukan ambang ini.
MIN_SPEECH_RMS = float(os.getenv("MIN_SPEECH_RMS", "0.008"))
# Chunk bersuara berturut-turut (0.1s/chunk) sebelum dianggap MULAI bicara.
# Silero sudah tahan noise → 1 chunk cukup (latency mulai lebih rendah).
SPEECH_START_CHUNKS = int(os.getenv("SPEECH_START_CHUNKS", "1"))
# Durasi suara minimum (detik) agar rekaman diterima — buang blip/ketukan.
MIN_SPEECH_SECONDS = float(os.getenv("MIN_SPEECH_SECONDS", "0.3"))  # 0.4→0.3
# Hening (detik) untuk berhenti merekam otomatis. Lebih kecil = lebih responsif.
# Dengan endpointing Silero yang akurat, 0.7 terasa lebih "nyambung" tanpa
# memotong jeda alami antar-kata.
SILENCE_DURATION = float(os.getenv("SILENCE_DURATION", "0.7"))
# Batas tunggu (detik) untuk MULAI bicara. Kalau dalam waktu ini tak ada suara,
# jendela dengar ditutup & dibuka lagi (recycle cepat) — mencegah "diam" 60 detik
# kalau ucapan tak terdeteksi. Durasi rekam sesungguhnya tetup max_seconds.
SPEECH_START_TIMEOUT = float(os.getenv("SPEECH_START_TIMEOUT", "8.0"))
# Ambang Silero VAD (0–1): prob ≥ nilai ini dianggap "ada ucapan". Naikkan kalau
# noise masih lolos; turunkan kalau ucapan pelan tak tertangkap.
VAD_THRESHOLD = float(os.getenv("VAD_THRESHOLD", "0.5"))
# Barge-in: izinkan user MENYELA saat asisten bicara (TTS) dengan langsung bicara.
# EKSPERIMENTAL & default MATI — tanpa acoustic echo cancellation, mic akan
# mendengar suara TTS sendiri & langsung "menyela". Aktifkan HANYA dengan headphone:
#   export VOCA_BARGE_IN=1
BARGE_IN = os.getenv("VOCA_BARGE_IN", "0") == "1"
# Chunk bersuara berturut (0.1s) yang dibutuhkan untuk memicu barge-in.
BARGE_IN_CHUNKS = int(os.getenv("VOCA_BARGE_IN_CHUNKS", "3"))
# beam_size Whisper: 1 = greedy (PALING CEPAT, akurasi sedikit turun),
# 5 = paling akurat tapi lambat di CPU. Untuk perintah pendek, 1 biasanya cukup.
WHISPER_BEAM_SIZE = int(os.getenv("WHISPER_BEAM_SIZE", "1"))
# Ambang Whisper untuk membuang segmen "bukan ucapan" / keyakinan rendah.
NO_SPEECH_THRESHOLD = float(os.getenv("NO_SPEECH_THRESHOLD", "0.7"))  # 0.6→0.7 (lebih toleran)
LOGPROB_THRESHOLD = float(os.getenv("LOGPROB_THRESHOLD", "-1.5"))    # -1.0→-1.5 (lebih toleran)
