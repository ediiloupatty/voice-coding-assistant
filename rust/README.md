# Voca (Rust)

Port Rust dari Voca — CLI asisten coding. Tujuan: **startup instan** + **install
satu perintah** (ala Claude/Codex CLI). Lihat `../PLAN.md` untuk rencana lengkap.

Status: **Fase 0–3 selesai** — chat + tool-calling + distribusi/installer + suara
**hybrid** (core Rust + sidecar suara Python).

## Arsitektur suara — hybrid (core Rust + sidecar Python)

Suara ditangani oleh **sidecar Python** (`voca.voice_server`) yang dijalankan core
Rust sekali; model tetap *warm* sehingga TTS/STT mulus. Ini memakai ulang seluruh
logika suara matang di Python (faster-whisper, VAD/gerbang-hening, piper, ejaan
fonetik) tanpa membebani build Rust dengan FFI ML yang rapuh.

```
Rust core ──(JSON per-baris via stdin/stdout)──> python -m voca.voice_server
  chat + tool                                       speak() / listen_auto()
```

Kenapa hybrid: percobaan in-process (`whisper-rs`, `cpal`, piper murni-Rust) semua
tersandung ekosistem (bindgen overflow, ALSA-dev, espeak-ng). Sidecar Python jauh
lebih mulus & matang, sementara core tetap 1 binary Rust kecil.

### Pakai

```sh
voca --voice          # ucapkan jawaban
voca --listen         # input mic (VAD) + ucapkan jawaban
voca --say "halo"     # uji sidecar lalu keluar
```

### Konfigurasi (env)

| Env | Arti | Default |
|---|---|---|
| `VOCA_VOICE_PYTHON` | python yang punya paket `voca` + deps suara | `python3` |
| `VOCA_VOICE_HOME`   | folder berisi paket `voca` (di-set sbg cwd + PYTHONPATH) | (warisi) |
| `VOCA_LANG`         | bahasa suara `id`/`en` | `en` |

Contoh dev (dari repo ini):
```sh
VOCA_VOICE_PYTHON="$PWD/.venv/bin/python" VOCA_VOICE_HOME="$PWD" voca --voice
```

Kalau sidecar tak bisa start, mode suara dimatikan dan Voca tetap jalan sebagai teks.

## Suara (Fase 2, shell-out)

```sh
voca --voice          # ucapkan jawaban (TTS via piper)
voca --listen         # input lewat mic (STT) + ucapkan jawaban
voca --say "halo"     # uji TTS lalu keluar
```

Dependensi runtime:
- **TTS**: `piper` di PATH + model (`PIPER_MODEL`, `PIPER_MODEL_EN`; default `models/…onnx`).
  Pemutar: `paplay`/`aplay`/`ffplay`.
- **STT**: `ffmpeg` (rekam) + `whisper-cli` (whisper.cpp) + model ggml
  (`WHISPER_BIN`, `WHISPER_MODEL`; default `models/ggml-small.bin`).
  Device rekam bisa di-override: `VOCA_REC_FORMAT`, `VOCA_REC_INPUT`.

Kalau dependensi tak ada, mode suara memberi peringatan dan tetap jalan sebagai teks.

## Jalankan dari sumber

```sh
cd rust
cargo run            # debug
cargo build --release && ./target/release/voca
```

API key dibaca dari (urut prioritas): env / `.env` → `~/.config/voca/config.json`
→ prompt interaktif (tersimpan otomatis). Provider & model dari env var yang sama
seperti versi Python (`DASHSCOPE_API_KEY`, `VOCA_PROVIDER`, `QWEN_MODEL`, dst.).

## Pasang sebagai pengguna (setelah ada Release)

Installer ada di **akar repo** (`install.sh` / `install.ps1`):

```sh
# Linux / macOS — mode teks (kilat)
curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash
# + suara (sidecar Python: venv + model)
curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash -s -- --with-voice

# Windows (PowerShell) — core
irm https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.ps1 | iex
```

Override sumber binary (mis. ke Cloudflare R2 nanti):
`VOCA_BASE_URL=https://get.voca.dev/bin` sebelum menjalankan installer.

## Merilis (maintainer)

Binary dibuat otomatis oleh `.github/workflows/release.yml` untuk
Linux x64, macOS arm64/x64, dan Windows x64.

```sh
git tag v0.1.0
git push origin v0.1.0     # → Actions build 4 binary → lampirkan ke Release
```

Atau jalankan manual lewat tab **Actions → release → Run workflow** (mengisi tag
`nightly`). Setelah Release terbit, `install.sh`/`install.ps1` otomatis mengunduh
dari `releases/latest/download`.

### Pindah ke Cloudflare R2 (opsional, nanti)
Upload aset `voca-*` + `install.sh`/`install.ps1` ke bucket R2, sajikan lewat
custom domain, lalu arahkan user ke domain itu. Tak perlu ubah kode — cukup
`VOCA_BASE_URL`. Lihat `../PLAN.md` §8.
