#!/usr/bin/env bash
#
# Pemasang Voca (core Rust + sidecar suara Python opsional).
#
#   Mode teks (kilat, 1 binary):
#     curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash
#
#   Tambah suara (TTS/STT via sidecar Python):
#     curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash -s -- --with-voice
#
# Override: VOCA_BASE_URL (sumber binary), VOCA_INSTALL_DIR (folder bin),
#           VOCA_HOME (folder sidecar suara).
set -euo pipefail

REPO="ediiloupatty/voice-coding-assistant"
BASE_URL="${VOCA_BASE_URL:-https://github.com/$REPO/releases/latest/download}"
BIN_DIR="${VOCA_INSTALL_DIR:-$HOME/.local/bin}"
VOCA_HOME="${VOCA_HOME:-$HOME/.voca}"
WITH_VOICE=0
[ "${1:-}" = "--with-voice" ] && WITH_VOICE=1

say()  { printf '\033[1;36m%s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m%s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m! %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31mx %s\033[0m\n' "$*" >&2; exit 1; }

# ── 1) Deteksi platform → nama aset rilis ──────────────────────────────────
os="$(uname -s)"; arch="$(uname -m)"
case "$os" in
  Linux)  case "$arch" in x86_64|amd64) asset="voca-linux-x64";;  *) die "arsitektur tak didukung: $arch";; esac;;
  Darwin) case "$arch" in arm64|aarch64) asset="voca-macos-arm64";; x86_64) asset="voca-macos-x64";; *) die "arsitektur tak didukung: $arch";; esac;;
  *) die "OS tak didukung: $os (Windows: pakai install.ps1)";;
esac

# ── 2) Unduh binary core (Rust) ────────────────────────────────────────────
say "Mengunduh Voca core ($asset)..."
mkdir -p "$BIN_DIR"
tmp="$(mktemp)"
curl -fsSL "$BASE_URL/$asset" -o "$tmp" || die "gagal mengunduh binary dari $BASE_URL/$asset"
chmod +x "$tmp"; mv "$tmp" "$BIN_DIR/voca"
ok "Core terpasang: $BIN_DIR/voca"

# ── 3) (opsional) Sidecar suara Python ─────────────────────────────────────
add_env() { # add_env NAMA NILAI  → tulis export ke shell rc bila belum ada
  local name="$1" val="$2" rc=""
  for f in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile"; do [ -f "$f" ] && rc="$f" && break; done
  [ -z "$rc" ] && rc="$HOME/.profile" && touch "$rc"
  grep -q "export $name=" "$rc" || { echo "export $name=\"$val\"" >> "$rc"; }
}

if [ "$WITH_VOICE" = "1" ]; then
  say "Menyiapkan sidecar suara (Python) di $VOCA_HOME ..."
  command -v python3 >/dev/null || die "python3 diperlukan untuk --with-voice."
  command -v git     >/dev/null || die "git diperlukan untuk --with-voice."

  if [ -d "$VOCA_HOME/.git" ]; then
    git -C "$VOCA_HOME" pull --ff-only
  else
    rm -rf "$VOCA_HOME"; git clone --depth 1 "https://github.com/$REPO.git" "$VOCA_HOME"
  fi

  say "  Memasang dependensi suara (bisa beberapa menit)..."
  python3 -m venv "$VOCA_HOME/.venv"
  "$VOCA_HOME/.venv/bin/pip" install -q --upgrade pip
  "$VOCA_HOME/.venv/bin/pip" install -q faster-whisper piper-tts sounddevice numpy python-dotenv
  # VAD neural Silero (wajib). torch CPU-only agar tak menarik CUDA ber-GB.
  say "  Memasang VAD Silero (torch CPU, ~200MB)..."
  "$VOCA_HOME/.venv/bin/pip" install -q torch torchaudio --index-url https://download.pytorch.org/whl/cpu
  "$VOCA_HOME/.venv/bin/pip" install -q silero-vad

  say "  Mengunduh model suara Piper (~120MB)..."
  mkdir -p "$VOCA_HOME/models"
  PB="https://huggingface.co/rhasspy/piper-voices/resolve/main"
  curl -fsSL "$PB/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx"      -o "$VOCA_HOME/models/id_ID-news_tts-medium.onnx"
  curl -fsSL "$PB/id/id_ID/news_tts/medium/id_ID-news_tts-medium.onnx.json" -o "$VOCA_HOME/models/id_ID-news_tts-medium.onnx.json"
  curl -fsSL "$PB/en/en_US/amy/medium/en_US-amy-medium.onnx"      -o "$VOCA_HOME/models/en_US-amy-medium.onnx"
  curl -fsSL "$PB/en/en_US/amy/medium/en_US-amy-medium.onnx.json" -o "$VOCA_HOME/models/en_US-amy-medium.onnx.json"

  # Beri tahu core Rust cara menemukan sidecar.
  add_env "VOCA_VOICE_PYTHON" "$VOCA_HOME/.venv/bin/python"
  add_env "VOCA_VOICE_HOME"   "$VOCA_HOME"
  ok "Sidecar suara siap. Pakai: voca --voice  /  voca --listen"
fi

# ── 4) Pastikan BIN_DIR ada di PATH ────────────────────────────────────────
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) add_env "PATH" "$BIN_DIR:\$PATH"
     warn "Buka terminal baru (atau 'source' shell rc) agar 'voca' aktif di PATH." ;;
esac

echo ""
ok "Selesai. Jalankan: voca"
echo "  (API key diminta otomatis saat pertama dijalankan, tersimpan di ~/.config/voca/)"
