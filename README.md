<div align="center">

# 🎙️ Voca

**Rekan ngoding berbasis suara — ngomong, biar dia yang ngerjain.**

Kamu beri perintah (ketik atau bicara), Voca menganalisis folder kerja,
mengerjakan tugas, lalu **menarasikan progresnya secara real-time lewat suara** —
seperti pair-programming dengan rekan yang aktif berkomunikasi.

[![Python](https://img.shields.io/badge/python-3.9%2B-blue.svg)](https://www.python.org/)
[![Platform](https://img.shields.io/badge/platform-Linux-informational.svg)](#kebutuhan-sistem)
[![Version](https://img.shields.io/badge/version-1.0.0-orange.svg)](#)

<br>

<img src="assets/diagram.png" alt="Alur Voca: Voice In → AI Works → Voice Out" width="760">

<sub><b>Voice In</b> (Whisper) → <b>AI Works</b> (Qwen + tools) → <b>Voice Out</b> (Piper)</sub>

</div>

---

## ✨ Fitur Utama

| | |
|---|---|
| 🗣️ **Hands-free penuh** | Bicara langsung — rekaman berhenti otomatis saat kamu diam — Voca kerja dan balas lewat suara. |
| ⚡ **Narasi real-time** | Balasan diucapkan per kalimat begitu siap, tanpa nunggu seluruh jawaban selesai. |
| 🔍 **Paham kode** | Tool `search_files` (mirip `grep`) & baca file per rentang baris — cepat menemukan apa yang relevan tanpa baca file utuh. |
| 🎨 **Diff berwarna** | Setiap edit file menampilkan perubahan (🟢 tambah / 🔴 hapus) di terminal **sebelum** kamu konfirmasi. |
| 🛡️ **Aman & terkendali** | Semua aksi dibatasi di folder kerja; menulis file & menjalankan command selalu minta konfirmasi. |
| 🔌 **Offline untuk suara** | STT (Whisper) & TTS (Piper) jalan lokal — hanya LLM yang butuh internet. |

---

## 🧠 Arsitektur

Voca dirakit dari tiga "indra" yang dapat ditukar lewat konfigurasi:

```
                ┌──────────────────────────────────────────┐
   🎤 suara ──► │  👂 Telinga   faster-whisper  (STT, lokal) │
                └──────────────────────────────────────────┘
                                   │  teks
                                   ▼
                ┌──────────────────────────────────────────┐
                │  🧠 Otak      Qwen / qwen-plus (DashScope) │
                │     loop LLM + tool use → kerjakan tugas   │
                └──────────────────────────────────────────┘
                                   │  narasi
                                   ▼
                ┌──────────────────────────────────────────┐
   🔊 suara ◄── │  👄 Mulut     Piper  (TTS, lokal, cepat)   │
                └──────────────────────────────────────────┘
```

- **Otak** — Qwen (`qwen-plus`) via DashScope (endpoint OpenAI-compatible).
- **Telinga** — faster-whisper (speech-to-text, lokal/offline).
- **Mulut** — Piper (text-to-speech, lokal/offline) dengan fallback gTTS.

---

## 🚀 Instalasi Cepat (Linux)

Satu perintah — otomatis unduh kode, buat virtualenv, install dependensi, unduh
model suara, minta API key, dan pasang perintah `voca`:

```bash
curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash
```

Setelah selesai, langsung jalankan:

```bash
voca
```

> [!NOTE]
> Windows & macOS belum didukung — pemutaran audio masih khusus Linux/ALSA.
> Setiap pengguna memakai **API key Qwen sendiri**.

---

## 🧩 Kebutuhan Sistem

| Kebutuhan | Untuk |
|-----------|-------|
| `python3` (3.9+), `git`, `curl` | menjalankan & memasang |
| `ffmpeg` | pemrosesan audio |
| `alsa-utils` (`aplay`) | pemutaran suara |
| **PortAudio** | input mikrofon |

```bash
# Debian/Ubuntu
sudo apt install python3 git curl ffmpeg alsa-utils portaudio19-dev

# Fedora
sudo dnf install python3 git curl ffmpeg alsa-utils portaudio-devel
```

---

## 🛠️ Setup Manual (tanpa install.sh)

```bash
# 1. Virtual environment
python3 -m venv .venv && source .venv/bin/activate

# 2. Dependensi Python
pip install -r requirements.txt

# 3. API key
cp .env.example .env          # lalu isi DASHSCOPE_API_KEY di .env

# 4. Model suara Piper (unduh sekali, ~60 MB)
mkdir -p models
BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium"
curl -L "$BASE/id_ID-news_tts-medium.onnx"      -o models/id_ID-news_tts-medium.onnx
curl -L "$BASE/id_ID-news_tts-medium.onnx.json" -o models/id_ID-news_tts-medium.onnx.json
```

---

## ▶️ Menjalankan

```bash
voca                 # mode teks  — folder saat ini jadi area kerja
voca --voice         # mode hands-free penuh — ngomong → kerja → lapor suara

# tanpa perintah global 'voca':
python -m voca
python -m voca --voice
```

> [!IMPORTANT]
> Voca bekerja di **folder tempat kamu menjalankannya** (current directory).
> `cd` dulu ke project yang mau dikerjakan, baru ketik `voca`.

**Contoh perintah:**

> *"Lihat ada file apa di sini."*
> *"Buatkan script python cek bilangan prima."*
> *"Cari di mana fungsi login didefinisikan, lalu perbaiki bug-nya."*
> *"Jalankan test-nya lalu laporkan hasilnya."*

**Mode hands-free** (`voca --voice`): bicara langsung (rekam berhenti otomatis
saat kamu diam), konfirmasi aksi dijawab "ya"/"tidak" pakai suara, ucapkan
"berhenti"/"stop" atau tekan `Ctrl+C` untuk keluar. **Tetap bisa mengetik kapan
saja** — cukup tekan `ENTER` lalu ketik perintahmu, tanpa keluar dari mode suara.

---

## 🔧 Kemampuan Agent (tools)

Voca menyelesaikan tugas dengan memanggil tool berikut secara mandiri:

| Tool | Fungsi | Konfirmasi |
|------|--------|:---------:|
| `list_files` | Lihat struktur folder kerja (dibatasi kedalaman & jumlah). | — |
| `search_files` | Cari teks/kode di seluruh folder — pakai `ripgrep` bila ada, fallback Python. | — |
| `read_file` | Baca isi file; bisa per rentang baris (`start_line`/`end_line`). | — |
| `edit_file` | Edit sebagian file (find/replace) — hemat & akurat, dengan **diff berwarna**. | ✅ |
| `write_file` | Buat file baru / timpa total — menampilkan **diff berwarna** lebih dulu. | ✅ |
| `run_command` | Jalankan perintah terminal dengan **output live**, bisa di-Ctrl+C. | ✅ |

---

## ⚙️ Konfigurasi

Semua diatur lewat environment variable atau file `.env`.

### Otak (LLM)

| Variabel | Default | Fungsi |
|----------|---------|--------|
| `DASHSCOPE_API_KEY` | — | **(wajib)** API key Qwen / Alibaba Model Studio. |
| `QWEN_BASE_URL` | endpoint intl | Base URL endpoint OpenAI-compatible DashScope. |
| `QWEN_MODEL` | `qwen-plus` | Model LLM. |
| `QWEN_TEMPERATURE` | `0.3` | Rendah = lebih fokus/akurat; tinggi = lebih kreatif. |
| `LLM_MAX_RETRIES` | `4` | Jumlah percobaan saat koneksi LLM error/timeout. |
| `LLM_RETRY_BASE_DELAY` | `2.0` | Jeda awal retry (detik), naik eksponensial (maks 30s). |

### Suara

| Variabel | Default | Fungsi |
|----------|---------|--------|
| `VOICE_ENABLED` | `1` | `0` = matikan suara (mode teks saja). |
| `VOICE_PITCH` | `1.1` | Nada — `>1` lebih tinggi, `<1` lebih dalam (formant terjaga). |
| `VOICE_SPEED` | `1.12` | Tempo — `>1` lebih pelan/kalem. |
| `VOICE_VOLUME` | `0.9` | `0..1` — kecil = lebih lembut. |
| `PIPER_MODEL` | model ID | Path model Piper lain. |
| `SPEAK_PHONETIC` | `1` | `0` = jangan eja-ulang kata Inggris saat bicara. Mengoreksi lafal kata seperti *file*, *commit*, *error* (hanya audio, teks tak berubah). |
| `WHISPER_MODEL` | `small` | Ukuran STT: `tiny`/`base`/`small`/`medium`/`large-v3`. |
| `MIN_SPEECH_RMS` | `0.01` | Ambang energi minimum dianggap ada ucapan. Naikkan kalau masih ada teks "hantu" saat diam; turunkan kalau suara pelanmu terabaikan. |

### Perilaku agent (hemat & andal)

| Variabel | Default | Fungsi |
|----------|---------|--------|
| `MAX_TOOL_ITERS` | `15` | Maks putaran tool per giliran (anti muter selamanya). |
| `MAX_HISTORY` | `30` | Maks pesan disimpan (anti boros token & overflow context). |
| `MAX_HISTORY_TOKENS` | `12000` | Batas estimasi token history (pemangkasan kedua). |
| `CHARS_PER_TOKEN` | `3.5` | Heuristik estimasi token (karakter per token). |
| `MAX_READ_CHARS` | `20000` | Batas karakter saat baca file utuh. |
| `MAX_OUTPUT_CHARS` | `8000` | Batas karakter output `run_command`. |
| `COMMAND_TIMEOUT` | `300` | Batas waktu `run_command` (detik). |
| `LIST_MAX_DEPTH` | `4` | Kedalaman maksimum `list_files`. |
| `LIST_MAX_ENTRIES` | `400` | Jumlah baris maksimum `list_files`. |
| `SEARCH_MAX_RESULTS` | `60` | Hasil maksimum `search_files`. |
| `SHOW_DIFF` | `1` | `0` = jangan tampilkan diff saat edit file. |
| `DIFF_MAX_LINES` | `200` | Batas baris diff yang dicetak. |

### Sesi (simpan & lanjutkan)

| Variabel | Default | Fungsi |
|----------|---------|--------|
| `SESSION_ENABLED` | `1` | `0` = jangan simpan/lanjutkan sesi. |
| `SESSION_FILE` | `.voca/session.json` | Lokasi file sesi (relatif ke folder kerja). |

Saat `voca` dijalankan di folder yang punya sesi tersimpan, ia menawarkan untuk
**melanjutkan percakapan sebelumnya**.

**Contoh:**

```bash
VOICE_PITCH=1.15 VOICE_SPEED=1.18 VOICE_VOLUME=0.82 voca
QWEN_TEMPERATURE=0.1 MAX_TOOL_ITERS=25 voca --voice
```

---

## 📁 Struktur Proyek

```
voice-coding-assistant/
├── voca/                 # paket utama
│   ├── __main__.py       # entry: python -m voca [--voice]
│   ├── config.py         # SEMUA setting & path terpusat di sini
│   ├── agent.py          # otak: loop LLM + tool use + history + sesi + UI
│   ├── tools.py          # tangan: list/search/read/edit/write, run command, diff
│   ├── voice.py          # mulut: TTS Piper (+ fallback gTTS)
│   └── listen.py         # telinga: STT Whisper
├── tests/                # tes pytest (tanpa API/audio)
├── .github/workflows/    # CI GitHub Actions
├── models/               # model suara Piper (.onnx, tidak ikut git)
├── install.sh            # pemasang satu-perintah (Linux)
├── requirements.txt      # dependensi runtime
├── requirements-dev.txt  # + pytest untuk pengembangan
└── .env                  # API key & setting (tidak ikut git)
```

---

## 🧪 Tes & Pengembangan

Tes per-komponen (manual):

```bash
python -m voca.voice     # tes suara keluar (TTS)
python -m voca.listen    # tes mikrofon + transkripsi (STT)
```

Tes otomatis (pytest) — tidak butuh API key, mikrofon, atau model suara:

```bash
pip install -r requirements-dev.txt
pytest -q
```

Tes berjalan otomatis di **GitHub Actions** (Python 3.9 / 3.11 / 3.12) pada
setiap push & pull request — lihat `.github/workflows/ci.yml`.

---

## 🔒 Keamanan

- Semua operasi file **dibatasi di dalam folder kerja** — path di luar folder ditolak.
- Menulis file & menjalankan command **selalu minta konfirmasi** (keyboard `[y/N]`,
  atau suara di mode hands-free), dengan diff perubahan ditampilkan lebih dulu.
- Batas iterasi tool & pemangkasan history mencegah eksekusi liar dan pemborosan.

---

## 📄 Lisensi

Belum ditentukan. Tambahkan file `LICENSE` untuk menetapkan ketentuan
penggunaan (mis. MIT, Apache-2.0).
