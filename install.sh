#!/usr/bin/env bash
#
# Pemasang Voca — AI Coding Assistant (perintah: voca)
#
# Cara pakai (Linux):
#   curl -fsSL https://raw.githubusercontent.com/ediiloupatty/voice-coding-assistant/main/install.sh | bash
#
set -euo pipefail

REPO="https://github.com/ediiloupatty/voice-coding-assistant.git"
INSTALL_DIR="${VOCA_HOME:-$HOME/.voca}"
BIN_DIR="$HOME/.local/bin"
MODEL_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/id/id_ID/news_tts/medium"
MODEL="id_ID-news_tts-medium"
# Model suara English (lokal, untuk fitur ganti bahasa).
MODEL_EN_BASE="https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium"
MODEL_EN="en_US-amy-medium"

say()  { printf '\033[1;36m%s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m%s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m⚠️  %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31m❌ %s\033[0m\n' "$*" >&2; exit 1; }

say "🎙️  Memasang Voca — AI Coding Assistant..."

# ─────────────────────────────────────────────────────────────
# 1) Prasyarat wajib
# ─────────────────────────────────────────────────────────────
command -v python3 >/dev/null || die "python3 belum terpasang."
command -v git     >/dev/null || die "git belum terpasang."
command -v curl    >/dev/null || die "curl belum terpasang."

# ─────────────────────────────────────────────────────────────
# 2) Dependensi sistem (peringatan saja, tak bisa auto-install lintas distro)
# ─────────────────────────────────────────────────────────────
for dep in ffmpeg aplay; do
  command -v "$dep" >/dev/null \
    || warn "'$dep' belum ada — suara mungkin tak jalan. Pasang: sudo apt install ffmpeg alsa-utils"
done

# ─────────────────────────────────────────────────────────────
# 3) Unduh / perbarui kode
# ─────────────────────────────────────────────────────────────
if [ -d "$INSTALL_DIR/.git" ]; then
  say "📦 Memperbarui kode di $INSTALL_DIR..."
  git -C "$INSTALL_DIR" pull --ff-only
else
  say "📦 Mengunduh kode ke $INSTALL_DIR..."
  rm -rf "$INSTALL_DIR"
  git clone --depth 1 "$REPO" "$INSTALL_DIR"
fi

# ─────────────────────────────────────────────────────────────
# 4) Virtualenv + dependensi Python
# ─────────────────────────────────────────────────────────────
say "🐍 Menyiapkan virtualenv & dependensi Python (bisa beberapa menit)..."
python3 -m venv "$INSTALL_DIR/.venv"
"$INSTALL_DIR/.venv/bin/pip" install -q --upgrade pip
"$INSTALL_DIR/.venv/bin/pip" install -q -r "$INSTALL_DIR/requirements.txt"

# ─────────────────────────────────────────────────────────────
# 5) Model suara Piper (~60 MB)
# ─────────────────────────────────────────────────────────────
say "🔊 Mengunduh model suara Piper Indonesia (~60MB)..."
mkdir -p "$INSTALL_DIR/models"
curl -fsSL "$MODEL_BASE/$MODEL.onnx"      -o "$INSTALL_DIR/models/$MODEL.onnx"
curl -fsSL "$MODEL_BASE/$MODEL.onnx.json" -o "$INSTALL_DIR/models/$MODEL.onnx.json"

say "🔊 Mengunduh model suara Piper English (~60MB)..."
curl -fsSL "$MODEL_EN_BASE/$MODEL_EN.onnx"      -o "$INSTALL_DIR/models/$MODEL_EN.onnx"
curl -fsSL "$MODEL_EN_BASE/$MODEL_EN.onnx.json" -o "$INSTALL_DIR/models/$MODEL_EN.onnx.json"

# ─────────────────────────────────────────────────────────────
# 6) Siapkan .env — ATAU tanya API key jika belum ada
# ─────────────────────────────────────────────────────────────
ENV_FILE="$INSTALL_DIR/.env"

# Salin template jika .env belum ada sama sekali
[ -f "$ENV_FILE" ] || cp "$INSTALL_DIR/.env.example" "$ENV_FILE"

# Cek apakah API key sudah terisi (bukan placeholder)
CURRENT_KEY="$(grep -E '^DASHSCOPE_API_KEY=' "$ENV_FILE" | cut -d= -f2- | tr -d '[:space:]')"
if [ -z "$CURRENT_KEY" ] || [[ "$CURRENT_KEY" == sk-xxx* ]]; then
  echo ""
  say "🔑 Masukkan API key Qwen / DashScope kamu."
  echo "   (Daftar gratis di: https://dashscope.aliyun.com)"
  echo "   Key tidak akan ditampilkan saat diketik."
  echo ""

  # Baca dari /dev/tty agar tetap bisa input keyboard walau script di-pipe lewat curl | bash
  while true; do
    printf '\033[1;33m   API Key (sk-...): \033[0m'
    read -r -s DASHSCOPE_API_KEY < /dev/tty
    echo ""  # newline setelah input tersembunyi

    # Validasi minimal: tidak kosong & dimulai dengan "sk-"
    if [[ -n "$DASHSCOPE_API_KEY" && "$DASHSCOPE_API_KEY" == sk-* ]]; then
      break
    else
      warn "Key tidak valid. Harus dimulai dengan 'sk-'. Coba lagi."
    fi
  done

  # Tulis key ke .env (ganti baris DASHSCOPE_API_KEY yang sudah ada)
  if grep -q '^DASHSCOPE_API_KEY=' "$ENV_FILE"; then
    sed "s|^DASHSCOPE_API_KEY=.*|DASHSCOPE_API_KEY=$DASHSCOPE_API_KEY|" "$ENV_FILE" > "$ENV_FILE.tmp"
    mv "$ENV_FILE.tmp" "$ENV_FILE"
  else
    echo "DASHSCOPE_API_KEY=$DASHSCOPE_API_KEY" >> "$ENV_FILE"
  fi

  ok "   ✅ API key tersimpan di $ENV_FILE"
else
  ok "   ✅ API key sudah terisi, melewati langkah ini."
fi

# ─────────────────────────────────────────────────────────────
# 7) Pasang perintah 'voca'
# ─────────────────────────────────────────────────────────────
say "🔗 Memasang perintah 'voca' ke $BIN_DIR..."
mkdir -p "$BIN_DIR"
cat > "$BIN_DIR/voca" <<EOF
#!/usr/bin/env bash
# Voca launcher (dibuat oleh install.sh)
ROOT="$INSTALL_DIR"
export PYTHONPATH="\$ROOT\${PYTHONPATH:+:\$PYTHONPATH}"
exec "\$ROOT/.venv/bin/python" -m voca "\$@"
EOF
chmod +x "$BIN_DIR/voca"

# ─────────────────────────────────────────────────────────────
# 8) Auto-tambahkan PATH ke shell rc (jika belum ada)
# ─────────────────────────────────────────────────────────────
PATH_EXPORT="export PATH=\"$BIN_DIR:\$PATH\""
SHELL_RC=""

case ":$PATH:" in
  *":$BIN_DIR:"*) : ;;  # sudah ada di PATH, skip
  *)
    if [ -f "$HOME/.zshrc" ]; then
      SHELL_RC="$HOME/.zshrc"
    elif [ -f "$HOME/.bashrc" ]; then
      SHELL_RC="$HOME/.bashrc"
    elif [ -f "$HOME/.profile" ]; then
      SHELL_RC="$HOME/.profile"
    fi

    if [ -n "$SHELL_RC" ]; then
      if ! grep -qF "$BIN_DIR" "$SHELL_RC"; then
        echo "" >> "$SHELL_RC"
        echo "# Voca — AI Coding Assistant" >> "$SHELL_RC"
        echo "$PATH_EXPORT" >> "$SHELL_RC"
        ok "   ✅ PATH ditambahkan ke $SHELL_RC"
      fi
      export PATH="$BIN_DIR:$PATH"
    else
      warn "Tidak ditemukan shell rc. Tambahkan manual: $PATH_EXPORT"
    fi
    ;;
esac

# ─────────────────────────────────────────────────────────────
# 9) Pesan akhir
# ─────────────────────────────────────────────────────────────
echo ""
ok "✅ Instalasi selesai! Voca siap dipakai."
echo ""
echo "  Jalankan sekarang:"
echo ""
echo "    voca              → mode hands-free (default, bicara langsung)"
echo "    voca --text       → mode teks murni (tanpa suara/STT)"
echo ""
echo "  Setting suara & model ada di: $ENV_FILE"
echo ""
