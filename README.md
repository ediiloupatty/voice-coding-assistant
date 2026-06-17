# 🎙️ AI Coding Companion

Rekan ngoding berbasis **suara**: kamu beri perintah (ketik atau ngomong), ia
menganalisis folder kerja, mengerjakan tugas, dan **menarasikan progresnya
secara real-time lewat suara** — seperti pair-programming dengan rekan yang
aktif berkomunikasi.

- **Otak:** Qwen (`qwen-plus`) via DashScope (endpoint OpenAI-compatible)
- **Telinga:** faster-whisper (STT, lokal/offline)
- **Mulut:** Piper (TTS, lokal/offline, cepat)

## Install cepat (Linux)

Satu baris — otomatis unduh kode, buat venv, install dependensi, unduh model
suara, dan pasang perintah `kong`:

```bash
curl -fsSL https://raw.githubusercontent.com/ediiloupatty/AI-AGENT/main/install.sh | bash
```

Setelah itu: isi API key Qwen di `~/.kong/.env`, lalu jalankan `kong`.

> Butuh `python3`, `git`, `curl`, plus `ffmpeg`, `alsa-utils` (aplay), dan
> PortAudio untuk suara. Tiap pengguna memakai **API key Qwen sendiri**.
> (Windows/macOS belum didukung — pemutaran audio masih khusus Linux/ALSA.)

## Struktur proyek

```
ai/
├── companion/            # paket utama
│   ├── __main__.py       # entry: python -m companion [--voice]
│   ├── config.py         # SEMUA setting & path terpusat di sini
│   ├── agent.py          # otak: loop LLM + tool use
│   ├── tools.py          # tangan: list/read/write file, run command
│   ├── voice.py          # mulut: TTS Piper (+ fallback gTTS)
│   └── listen.py         # telinga: STT Whisper
├── models/               # model suara Piper (.onnx, tidak ikut git)
├── requirements.txt
└── .env                  # API key & setting (tidak ikut git)
```

## Setup

```bash
# 1. virtual environment
python3 -m venv .venv && source .venv/bin/activate

# 2. dependensi
pip install -r requirements.txt

# 3. API key
cp .env.example .env       # lalu isi DASHSCOPE_API_KEY di .env

# 4. model suara Piper (unduh sekali)
mkdir -p models
BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium"
curl -L "$BASE/id_ID-news_tts-medium.onnx"      -o models/id_ID-news_tts-medium.onnx
curl -L "$BASE/id_ID-news_tts-medium.onnx.json" -o models/id_ID-news_tts-medium.onnx.json
```

Dependensi sistem: **aplay** (alsa-utils) & **ffmpeg** untuk audio,
**PortAudio** untuk mikrofon.

## Menjalankan

```bash
kong                 # mode teks (folder saat ini jadi area kerja, ala Claude CLI)
kong --voice         # mode hands-free penuh (ngomong → kerja → lapor suara)

# tanpa perintah global 'kong':
python -m companion
python -m companion --voice
```

Contoh perintah: *"Lihat ada file apa di sini"*, *"Buatkan script python cek
bilangan prima"*, *"Jalankan test-nya lalu laporkan hasilnya"*.

## Pengaturan suara (semua lewat env / file `.env`)

| Variabel | Default | Fungsi |
|----------|---------|--------|
| `VOICE_ENABLED` | `1` | `0` = matikan suara (mode teks saja) |
| `VOICE_PITCH` | `1.1` | nada — `>1` lebih tinggi, `<1` lebih dalam (formant terjaga) |
| `VOICE_SPEED` | `1.12` | tempo — `>1` lebih pelan/kalem |
| `VOICE_VOLUME` | `0.9` | `0..1` — kecil = lebih lembut |
| `PIPER_MODEL` | model ID | path model Piper lain |
| `WHISPER_MODEL` | `small` | ukuran STT: tiny/base/small/medium/large-v3 |
| `QWEN_MODEL` | `qwen-plus` | model LLM |

Contoh: `VOICE_PITCH=1.15 VOICE_SPEED=1.18 VOICE_VOLUME=0.82 kong`

**Mode hands-free** (`kong --voice`): bicara langsung (rekam berhenti otomatis
saat kamu diam), konfirmasi aksi dijawab "ya"/"tidak" pakai suara, ucapkan
"berhenti"/"stop" atau Ctrl+C untuk keluar.

## Tes per-komponen

```bash
python -m companion.voice    # tes suara keluar (TTS)
python -m companion.listen   # tes mikrofon + transkripsi (STT)
```

## Keamanan

- Semua operasi file dibatasi di dalam folder kerja.
- Menulis file & menjalankan command selalu minta konfirmasi dulu
  (keyboard `[y/N]`, atau suara di mode hands-free).
