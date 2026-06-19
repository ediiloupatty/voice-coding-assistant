# Rencana Migrasi Voca: Python → Rust (CLI lintas-OS, install ala Claude/Codex CLI)

> Status: **RENCANA** (belum ada kode yang diubah). Dokumen ini acuan sebelum mulai ngoding.
> Tanggal disusun: 2026-06-19

---

## 1. Tujuan

1. **Pengalaman install se-enak Claude Code / Codex CLI / Gemini CLI**:
   ```sh
   # Linux / macOS
   curl -fsSL https://get.voca.dev/install.sh | sh

   # Windows (PowerShell)
   irm https://get.voca.dev/install.ps1 | iex

   # lalu langsung jalan:
   voca
   ```
   User **tidak pernah** lihat Rust, pip, Node, atau venv.

2. **Satu binary lintas-OS** (Windows / macOS / Linux), startup instan.

3. **Distribusi lewat Cloudflare R2** (zero egress fee) untuk binary + file model.

4. Migrasi **bertahap**, bukan rewrite big-bang. Python lama jadi referensi perilaku.

---

## 1.5. Ekspektasi & justifikasi (perkiraan peningkatan dari Python)

> Angka startup di bawah **terukur** dari app sekarang (`.venv` warm).
> Sisanya perkiraan jujur — bukan klaim marketing.

**Startup app terukur (Python, warm):**
```
numpy           : ~193 ms
sounddevice     : ~136 ms
faster_whisper  : ~326 ms
openai          : ~1059 ms
------------------------------
voca.agent total: ~1160 ms   (di Windows cold bisa 2–4 detik)
```

### Ringkasan peningkatan vs Python

| Aspek | Peningkatan realistis | Catatan |
|---|---|---|
| ⚡ **Startup** | **~40× (~97%)** — dari ~1.160 ms (warm) / 2–4 dtk (Win cold) → ~10–50 ms | satu-satunya kecepatan yang user *rasakan* |
| ⚡ Total kerja (LLM + STT + TTS) | **~beberapa % saja** | didominasi network + backend C++ yang **identik** |
| 💾 RAM baseline | **~40–60% lebih hemat** | |
| 🎯 **Kemudahan install** | **~80–90% friction turun** | dari ~6–8 langkah & ~10–15 mnt → **1 perintah**, ~1–2 mnt; sukses non-dev ~100% |
| 📺 Info yang tampil | **~0% dari bahasa** (murni desain) | `ratatui` bisa +5–15% kalau TUI sengaja dibuat lebih kaya |

### Yang TIDAK membaik (penting, biar tak salah harap)
- **Kecepatan jawaban LLM** → ~0% (network/model bound).
- **Kecepatan STT/TTS** → ~0% (whisper.cpp / ONNX yang sama).
- **Eksekusi tool (file/shell)** → ~0% (I/O bound).

### Verdict
Pindah ke Rust **bukan** soal "lebih cepat" — kerja berat identik di kedua bahasa.
Keuntungan nyatanya: **startup instan + install super gampang (ala Claude/Codex CLI)**.
Kalau dua hal itu tujuannya → worth it. Kalau berharap transkripsi/jawaban lebih
ngebut → akan kecewa.

---

## 2. Kenapa Rust + kenapa ini bisa terasa "enak install"

Pelajaran kunci: **install yang nyaman itu soal CARA DISTRIBUSI, bukan bahasa.**
Codex CLI (Rust) membuktikan Rust + skrip install = persis rasa itu.

- Rust → **1 binary statis**, cross-compile mulus ke 3 OS (lebih baik dari Go yang
  kena masalah cgo begitu pakai whisper).
- Binary diunduh oleh skrip install dari R2 → ditaruh di PATH.
- Model `.onnx` **tidak dibundel** → diunduh saat pertama jalan (binary tetap kecil).

---

## 3. Bahasa dalam stack ini

| Peran | Bahasa | Kenapa |
|---|---|---|
| **Aplikasi inti** | **Rust** | binary tunggal, cepat, lintas-OS |
| Skrip install Unix | **Bash** (`install.sh`) | deteksi OS/arch, unduh binary, taruh di PATH |
| Skrip install Windows | **PowerShell** (`install.ps1`) | padanan `curl\|sh` di Windows |
| Penyaji R2 publik (opsional) | **TypeScript/JS** (Cloudflare Worker) | bikin file R2 bisa diakses via domain sendiri |
| CI/build | **YAML** (GitHub Actions) | cross-compile + upload otomatis |

Rust yang utama; sisanya kecil dan jarang disentuh.

---

## 4. Peta modul Voca (Python) → Rust

| Voca sekarang | Fungsi | Padanan Rust (crate) |
|---|---|---|
| `provider.py` | koneksi LLM (OpenAI-compatible) | `reqwest` + `async-openai` |
| `agent.py` | loop agent, streaming, eksekusi tool | `tokio` + stream SSE |
| `tools.py` | file ops, shell, dll. | `std::fs`, `std::process`, `tokio::process` |
| `listen.py` | STT (faster-whisper) | `whisper-rs` (binding whisper.cpp) |
| `voice.py` | TTS (Piper ONNX) | `ort` (ONNX Runtime) — atau shell-out ke binary Piper dulu |
| audio in/out (`sounddevice`) | rekam/putar audio | `cpal` (lintas-OS murni) |
| `ui.py` | CLI Rich (kotak input, menu, spinner) | `crossterm` + `ratatui` |
| `lang.py` | string ID/EN | modul `lang` + `serde` (atau `.toml`/`.json`) |
| `config.py` | konfigurasi, sesi, env | `clap` (argumen) + `serde` + `dotenvy` |

---

## 5. Struktur workspace Rust (rencana)

```
voca/                      # repo Rust baru (atau folder rust/ di repo ini)
├── Cargo.toml             # workspace
├── crates/
│   ├── voca-cli/          # entrypoint: argumen, mode interaktif (bin: `voca`)
│   ├── voca-agent/        # loop agent, streaming, orkestrasi tool
│   ├── voca-llm/          # client LLM (reqwest/async-openai)
│   ├── voca-tools/        # file/shell tools
│   ├── voca-audio/        # rekam/putar (cpal)
│   ├── voca-stt/          # whisper-rs
│   ├── voca-tts/          # ort / Piper
│   ├── voca-ui/           # crossterm/ratatui
│   └── voca-core/         # config, lang, tipe bersama
├── install.sh             # installer Unix
├── install.ps1            # installer Windows
└── .github/workflows/release.yml
```

---

## 6. Rencana bertahap (jangan big-bang)

**Fase 0 — Fondasi (paling untung, paling gampang)**
- [ ] Setup workspace Rust + `clap` + `tokio`.
- [ ] `voca-llm`: panggil LLM + streaming jawaban ke terminal.
- [ ] `voca-agent`: loop tanya-jawab teks (TANPA suara dulu).
- [ ] `voca-ui`: kotak input ala Claude + spinner (port dari `ui.py`).
- [ ] `voca-tools`: 2-3 tool inti (baca/tulis file, jalankan shell).
- ✅ Sudah dapat: startup instan + 1 binary + install script. Tanpa ML.

**Fase 1 — Distribusi (target "enak install")**
- [ ] `install.sh` + `install.ps1`.
- [ ] GitHub Actions cross-compile 3 OS → upload ke R2.
- [ ] (Opsional) Cloudflare Worker + custom domain `get.voca.dev`.
- ✅ User sudah bisa `curl ... | sh` → `voca` (mode teks).

**Fase 2 — Suara (shell-out dulu, biar cepat)**
- [ ] `voca-audio` rekam mic via `cpal`.
- [ ] STT & TTS dengan **shell-out** ke binary `whisper.cpp` / `piper`.
- [ ] Unduh model dari R2 saat pertama jalan.

**Fase 3 — Suara in-process (rapi)**
- [ ] Ganti shell-out → `whisper-rs` (STT) dan `ort` (TTS) langsung di proses.

**Sepanjang jalan:** pakai test Python yang ada (`tests/`) sebagai spesifikasi perilaku
(filter halusinasi, trim history, estimasi token, dll.) untuk ditiru di test Rust.

---

## 7. Skrip install (sketsa)

`install.sh` (Unix):
```sh
#!/bin/sh
set -e
BASE="https://get.voca.dev"
OS=$(uname -s); ARCH=$(uname -m)
case "$OS-$ARCH" in
  Linux-x86_64)  TARGET="linux-x64" ;;
  Darwin-arm64)  TARGET="macos-arm64" ;;
  Darwin-x86_64) TARGET="macos-x64" ;;
  *) echo "OS/arch belum didukung: $OS-$ARCH"; exit 1 ;;
esac
BIN="$HOME/.local/bin"; mkdir -p "$BIN"
echo "Mengunduh voca ($TARGET)..."
curl -fsSL "$BASE/bin/voca-$TARGET" -o "$BIN/voca"
chmod +x "$BIN/voca"
echo "Selesai. Pastikan $BIN ada di PATH, lalu jalankan: voca"
```

`install.ps1` (Windows):
```powershell
$ErrorActionPreference = "Stop"
$base = "https://get.voca.dev"
$dir  = "$env:LOCALAPPDATA\Voca"; New-Item -ItemType Directory -Force -Path $dir | Out-Null
Invoke-WebRequest "$base/bin/voca-windows-x64.exe" -OutFile "$dir\voca.exe"
# tambahkan $dir ke PATH user
Write-Host "Selesai. Jalankan: voca"
```

---

## 8. Layout bucket R2 (rencana)

```
voca-dist/                       # bucket R2, disajikan via get.voca.dev
├── install.sh
├── install.ps1
├── bin/
│   ├── voca-linux-x64
│   ├── voca-macos-arm64
│   ├── voca-macos-x64
│   └── voca-windows-x64.exe
└── models/
    ├── whisper-small.bin        # diunduh saat pertama jalan
    ├── id_ID-news_tts-medium.onnx
    └── en_US-amy-medium.onnx
```

Catatan:
- R2 **zero egress** → unduhan binary & model gratis bandwidth.
- Bucket mentah tak otomatis publik → butuh **custom domain** atau **Worker**.
- Model diunduh on-first-run ke folder cache user (mis. `~/.cache/voca/models`).

---

## 9. GitHub Actions (sketsa `release.yml`)

```yaml
name: release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    strategy:
      matrix:
        include:
          - { os: ubuntu-latest,  target: x86_64-unknown-linux-gnu,  name: voca-linux-x64 }
          - { os: macos-latest,   target: aarch64-apple-darwin,      name: voca-macos-arm64 }
          - { os: macos-13,       target: x86_64-apple-darwin,       name: voca-macos-x64 }
          - { os: windows-latest, target: x86_64-pc-windows-msvc,    name: voca-windows-x64.exe }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: "${{ matrix.target }}" }
      - run: cargo build --release --target ${{ matrix.target }}
      # - upload artifact ke R2 (pakai rclone / aws-cli S3-compatible)
```

---

## 10. Risiko & catatan jujur

- **Native lib (whisper.cpp / ONNX Runtime)**: "single binary" bisa butuh static-link
  atau menaruh `.dll/.so/.dylib` di sebelah binary. Masih "klik-jalan" buat user.
- **Model besar** tetap diunduh terpisah (bukan masalah kode).
- **Antivirus Windows** kadang rewel pada exe baru tak ber-signature → pertimbangkan
  code signing di kemudian hari.
- **Effort STT/TTS in-process (Fase 3)** paling berat → makanya shell-out dulu di Fase 2.

---

## 11. Pertanyaan terbuka (perlu diputuskan)

1. Repo Rust **terpisah** atau folder `rust/` di repo ini? (saran: folder `rust/` dulu)
2. Domain distribusi: `get.voca.dev`? (perlu daftar domain + Cloudflare)
3. Juga sediakan jalur **`npm i -g voca`** & **`brew`**? (bisa menyusul setelah curl-install)
4. Model whisper: ukuran apa default-nya (`small`/`base`)? (trade-off akurasi vs unduhan)
