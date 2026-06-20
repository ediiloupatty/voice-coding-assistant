"""
ui.py — Sisa kecil presentasi terminal untuk sidecar suara.

Setelah core pindah ke Rust (rust/src/ui.rs yang menggambar seluruh antarmuka),
modul ini menyusut drastis: hanya `info()` & `error()` yang masih dipakai oleh
`listen.py` untuk mencetak status ringan ke STDERR (lihat voice_server.py).

Seluruh komponen UI lama (banner, kolom input, bar status, menu panah
pilih_model/pilih_bahasa, alternate-screen, baca-tombol) sudah ditangani Rust
dan dihapus dari sini.
"""

from rich.console import Console
from rich.theme import Theme

# Style semantik minimal yang masih dipakai info()/error().
THEME = Theme({
    "muted": "color(244)",   # teks sekunder/status redup
    "error": "color(197)",   # pesan error
})

console = Console(theme=THEME)

ERR = "✕"   # tanda error


def info(teks: str) -> None:
    """Baris info redup (status ringan, mis. 'mentranskripsi…')."""
    console.print(f"[muted]{teks}[/]")


def error(msg) -> None:
    """Baris error yang bersih: ✕ pesan."""
    console.print(f"\n[error]{ERR}[/] {msg}")
