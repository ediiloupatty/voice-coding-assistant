#!/usr/bin/env bash
#
# release-local.sh — build binary Voca DI MESIN LOKAL lalu upload ke GitHub
# Release, tanpa GitHub Actions (berguna saat Actions terkunci billing).
#
#   ./release-local.sh            # ke pre-release 'nightly'
#   ./release-local.sh v0.1.0     # ke release versi (non-pre, jadi 'latest')
#
# Per-OS (deteksi otomatis dari `uname`):
#   • Linux  → voca-linux-x64  (+ voca-windows-x64.exe bila mingw terpasang)
#   • macOS  → voca-macos-arm64 + voca-macos-x64
# Jalankan di Linux DAN di sebuah Mac untuk melengkapi keempat aset (re-run aman:
# pakai --clobber, jadi aset lama ditimpa, yang sudah ada tetap).
#
# Butuh: rustup/cargo, gh (sudah `gh auth login`). Windows-cross butuh mingw:
#   Fedora: sudo dnf install mingw64-gcc mingw64-winpthreads-static
set -euo pipefail
cd "$(dirname "$0")"

TAG="${1:-nightly}"
RUST_DIR="rust"
DIST="$(mktemp -d)"
trap 'rm -rf "$DIST"' EXIT

say()  { printf '\033[1;36m%s\033[0m\n' "$*"; }
ok()   { printf '\033[1;32m✓ %s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m! %s\033[0m\n' "$*"; }
die()  { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null || die "cargo tak ditemukan — pasang Rust (https://rustup.rs)"
command -v gh    >/dev/null || die "gh tak ditemukan — pasang GitHub CLI"
gh auth status >/dev/null 2>&1 || die "gh belum login — jalankan: gh auth login"

# build <target> <asset> <nama-bin>  → cargo build --release lalu salin ke $DIST.
build() {
  local target="$1" asset="$2" binname="$3"
  say "→ build $asset  ($target)"
  rustup target add "$target" >/dev/null 2>&1 || true
  ( cd "$RUST_DIR" && cargo build --release --target "$target" )
  cp "$RUST_DIR/target/$target/release/$binname" "$DIST/$asset"
  ok "siap: $asset"
}

os="$(uname -s)"
case "$os" in
  Linux)
    build x86_64-unknown-linux-gnu voca-linux-x64 voca
    if command -v x86_64-w64-mingw32-gcc >/dev/null; then
      build x86_64-pc-windows-gnu voca-windows-x64.exe voca.exe
    else
      warn "mingw (x86_64-w64-mingw32-gcc) tak ada → lewati Windows."
      warn "  Pasang dulu: sudo dnf install mingw64-gcc mingw64-winpthreads-static"
    fi
    ;;
  Darwin)
    build aarch64-apple-darwin voca-macos-arm64 voca
    build x86_64-apple-darwin  voca-macos-x64   voca
    ;;
  *) die "OS tak didukung untuk build lokal: $os" ;;
esac

# Pastikan release ada (buat bila belum). 'nightly' = pre-release.
if ! gh release view "$TAG" >/dev/null 2>&1; then
  say "→ buat release $TAG"
  pre=""; [ "$TAG" = "nightly" ] && pre="--prerelease"
  gh release create "$TAG" $pre --title "$TAG" \
    --notes "Build lokal ($(date -u +%Y-%m-%dT%H:%MZ)) — tanpa GitHub Actions."
fi

say "→ upload aset ke release $TAG"
gh release upload "$TAG" "$DIST"/* --clobber
ok "selesai → $(gh release view "$TAG" --json url --jq .url)"
say "aset sekarang:"
gh release view "$TAG" --json assets --jq '.assets[].name' | sed 's/^/  • /'
