#!/usr/bin/env bash
#
# Pemasang AI Coding Companion (perintah: kong)
#
# Cara pakai (Linux):
#   curl -fsSL https://raw.githubusercontent.com/ediiloupatty/AI-AGENT/main/install.sh | bash
#
set -euo pipefail

REPO="https://github.com/ediiloupatty/AI-AGENT.git"
INSTALL_DIR="${KONG_HOME:-$HOME/.kong}"
BIN_DIR="$HOME/.local/bin"
MODEL_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium"
MODEL="id_ID-news_tts-medium"

say() { printf '\033[1;36m%s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m⚠️  %s\033[0m\n' "$*"; }
die() { printf '\033[1;31m❌ %s\033[0m\n' "$*" >&2; exit 1; }

say "🎙️  Memasang AI Coding Companion (kong)..."

# 1) Prasyarat wajib
command -v python3 >/dev/null || die "python3 belum terpasang."
command -v git >/dev/null || die "git belum terpasang."
command -v curl >/dev/null || die "curl belum terpasang."

# 2) Dependensi sistem (peringatan saja, tak bisa auto-install lintas distro)
for dep in ffmpeg aplay; do
  command -v "$dep" >/dev/null || warn "'$dep' belum ada — suara mungkin tak jalan. Pasang: ffmpeg & alsa-utils."
done

# 3) Unduh / perbarui kode
if [ -d "$INSTALL_DIR/.git" ]; then
  say "📦 Memperbarui kode di $INSTALL_DIR..."
  git -C "$INSTALL_DIR" pull --ff-only
else
  say "📦 Mengunduh kode ke $INSTALL_DIR..."
  rm -rf "$INSTALL_DIR"
  git clone --depth 1 "$REPO" "$INSTALL_DIR"
fi

# 4) Virtualenv + dependensi Python
say "🐍 Menyiapkan virtualenv & dependensi (bisa beberapa menit)..."
python3 -m venv "$INSTALL_DIR/.venv"
"$INSTALL_DIR/.venv/bin/pip" install -q --upgrade pip
"$INSTALL_DIR/.venv/bin/pip" install -q -r "$INSTALL_DIR/requirements.txt"

# 5) Model suara Piper
say "🔊 Mengunduh model suara Piper (~60MB)..."
mkdir -p "$INSTALL_DIR/models"
curl -fsSL "$MODEL_BASE/$MODEL.onnx"      -o "$INSTALL_DIR/models/$MODEL.onnx"
curl -fsSL "$MODEL_BASE/$MODEL.onnx.json" -o "$INSTALL_DIR/models/$MODEL.onnx.json"

# 6) Siapkan .env (kosong, untuk diisi user)
[ -f "$INSTALL_DIR/.env" ] || cp "$INSTALL_DIR/.env.example" "$INSTALL_DIR/.env"

# 7) Pasang perintah 'kong'
say "🔗 Memasang perintah 'kong' ke $BIN_DIR..."
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/kong" <<EOF
#!/usr/bin/env bash
# AI Coding Companion launcher (dibuat oleh install.sh)
ROOT="$INSTALL_DIR"
export PYTHONPATH="\$ROOT\${PYTHONPATH:+:\$PYTHONPATH}"
exec "\$ROOT/.venv/bin/python" -m companion "\$@"
EOF
chmod +x "$BIN_DIR/kong"

# 8) Pesan akhir
echo ""
say "✅ Selesai terpasang di $INSTALL_DIR"
echo ""
echo "Langkah terakhir:"
echo "  1) Isi API key Qwen di:  $INSTALL_DIR/.env"
echo "     (baris: DASHSCOPE_API_KEY=sk-xxxx)"
case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;
  *) echo "  2) Tambahkan ke ~/.zshrc atau ~/.bashrc:  export PATH=\"$BIN_DIR:\$PATH\"" ;;
esac
echo ""
echo "Lalu jalankan:  kong        (mode teks)"
echo "            atau kong --voice (hands-free)"
