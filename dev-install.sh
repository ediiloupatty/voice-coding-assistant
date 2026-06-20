#!/usr/bin/env bash
#
# Build & pasang Voca dari sumber lokal (untuk pengembangan).
#
#   ./dev-install.sh            # build release + salin ke ~/.local/bin/voca
#   ./dev-install.sh --debug    # build debug (lebih cepat, binary besar)
#   ./dev-install.sh --run      # build + pasang, lalu langsung jalankan voca
#
# Override: VOCA_INSTALL_DIR (folder bin tujuan, default ~/.local/bin).
set -euo pipefail

# Jalankan dari folder skrip ini, apa pun cwd pemanggil.
cd "$(dirname "$0")"

BIN_DIR="${VOCA_INSTALL_DIR:-$HOME/.local/bin}"
PROFILE="release"
RUN=0
for arg in "$@"; do
  case "$arg" in
    --debug) PROFILE="debug";;
    --run)   RUN=1;;
    -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0;;
    *) printf '\033[1;31mx argumen tak dikenal: %s\033[0m\n' "$arg" >&2; exit 1;;
  esac
done

say()  { printf '\033[1;36m%s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m%s\033[0m\n' "$*"; }
die()  { printf '\033[1;31mx %s\033[0m\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || die "cargo tidak ditemukan — pasang Rust dulu (https://rustup.rs)"

# ── 1) Build ───────────────────────────────────────────────────────────────
if [ "$PROFILE" = "release" ]; then
  say "→ cargo build --release"
  ( cd rust && cargo build --release )
  SRC="rust/target/release/voca"
else
  say "→ cargo build (debug)"
  ( cd rust && cargo build )
  SRC="rust/target/debug/voca"
fi
[ -x "$SRC" ] || die "binary tak ada: $SRC"

# ── 2) Pasang ──────────────────────────────────────────────────────────────
mkdir -p "$BIN_DIR"
install -m 0755 "$SRC" "$BIN_DIR/voca"
ok "✓ terpasang: $BIN_DIR/voca  ($PROFILE)"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf '\033[1;33m! %s belum di PATH — tambahkan: export PATH="%s:$PATH"\033[0m\n' "$BIN_DIR" "$BIN_DIR";;
esac

# ── 3) Jalankan (opsional) ─────────────────────────────────────────────────
[ "$RUN" = "1" ] && { say "→ menjalankan voca"; exec "$BIN_DIR/voca"; }
