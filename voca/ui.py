"""
ui.py — Sistem desain & seluruh presentasi terminal Voca.

Semua hal yang berkaitan dengan TAMPILAN dikumpulkan di sini (warna, simbol,
banner, kolom input, bar status, spinner, prompt) supaya agent.py fokus ke
logika. Gaya: minimalis-modern, satu aksen (teal), garis tipis, banyak ruang.

Dua jalur render:
  - Rich (lewat `console`) untuk teks/markdown biasa — pakai nama style semantik.
  - ANSI mentah untuk drawing yang butuh kontrol kursor (kolom input full-width
    & bar status di scroll-region mode hands-free).
"""

import shutil
from pathlib import Path

from rich.console import Console
from rich.prompt import Confirm
from rich.spinner import Spinner
from rich.text import Text
from rich.theme import Theme

# ---------------------------------------------------------------------------
# Palet — ANSI mentah (untuk drawing scroll-region) + Rich Theme (untuk markup)
# ---------------------------------------------------------------------------
RESET = "\033[0m"
BOLD = "\033[1m"
NOBOLD = "\033[22m"
DIM = "\033[2m"

ACCENT = "\033[38;5;30m"     # teal — warna utama/brand
ACCENT_HI = "\033[38;5;37m"  # teal terang — sigil & judul
MUTED = "\033[38;5;245m"     # abu-abu — teks sekunder/petunjuk
WARN = "\033[38;5;172m"      # amber — proses/peringatan

# Kolom input abu-abu muda (TIDAK diubah — sesuai permintaan user).
BG_INPUT = "\033[48;5;254m"  # latar abu-abu muda
FG_INPUT = "\033[38;5;236m"  # teks gelap agar terbaca di atas abu-abu

# Style semantik untuk Rich. Pakai color(N) agar sama persis dgn ANSI 256 di atas.
THEME = Theme({
    "accent":        "color(30)",
    "accent.hi":     "color(37)",
    "muted":         "color(245)",
    "success":       "color(28)",
    "warn":          "color(172)",
    "error":         "color(160)",
    "rule.line":     "color(30)",
    "markdown.code": "bold color(30)",   # inline `code` — tanpa background gelap
})

console = Console(theme=THEME)

# ---------------------------------------------------------------------------
# Simbol (bukan emoji — aman dgn aturan no-emoji)
# ---------------------------------------------------------------------------
SIGIL = "◆"    # penanda brand Voca
PROMPT = "›"   # prompt input
TOOL = "▸"     # baris pemanggilan tool
RULE = "─"     # garis pemisah tipis
DOT = "·"      # pemisah antar-info
ASK = "?"      # tanda tanya konfirmasi
ERR = "✕"      # tanda error


# ---------------------------------------------------------------------------
# Util kecil
# ---------------------------------------------------------------------------
def _pendekkan(teks: str, maks: int) -> str:
    """Pangkas teks panjang dari depan, sisakan ekornya (mis. path)."""
    return teks if len(teks) <= maks else "…" + teks[-(maks - 1):]


def _colhome(path) -> str:
    """Ganti $HOME di awal path dengan '~' biar ringkas."""
    s = str(path)
    try:
        home = str(Path.home())
        if s.startswith(home):
            return "~" + s[len(home):]
    except Exception:
        pass
    return s


def _lebar() -> int:
    return shutil.get_terminal_size().columns


# ---------------------------------------------------------------------------
# Komponen statis (Rich)
# ---------------------------------------------------------------------------
def banner(model: str, workspace, mode: str) -> None:
    """Banner pembuka minimalis: sigil + wordmark, garis tipis, lalu info ringkas."""
    lebar = 44
    ws = _pendekkan(_colhome(workspace), lebar - 8)
    console.print()
    console.print(f"[accent.hi]{SIGIL}[/] [bold accent]VOCA[/]  [muted]{DOT}  voice coding assistant[/]")
    console.print(f"[accent]{RULE * lebar}[/]")
    for label, val in (("model", model), ("folder", ws), ("mode", mode)):
        console.print(f"[muted]{label:<6}[/]  {val}")
    console.print()


def hint(teks: str) -> None:
    """Satu baris petunjuk redup di bawah banner."""
    console.print(f"[muted]{teks}[/]\n")


def info(teks: str) -> None:
    """Baris info redup (status ringan, mis. 'Sesi dilanjutkan')."""
    console.print(f"[muted]{teks}[/]")


def error(msg) -> None:
    """Baris error yang bersih: ✕ pesan."""
    console.print(f"\n[error]{ERR}[/] {msg}")


def selesai(teks: str = "sampai jumpa") -> None:
    """Baris penutup."""
    console.print(f"\n[muted]{SIGIL} {teks}[/]")


def header_jawaban() -> None:
    """Penanda di atas tiap jawaban asisten: '◆ voca'."""
    console.print(f"[accent.hi]{SIGIL}[/] [muted]voca[/]")


def baris_tool(nama: str, ringkas: str) -> None:
    """Baris pemanggilan tool: '▸ nama · args'."""
    sep = f" [muted]{DOT}[/] " if ringkas else ""
    console.print(f"  [muted]{TOOL}[/] [accent]{nama}[/]{sep}[muted]{ringkas}[/]")


def spinner_berpikir(teks: str = "berpikir…"):
    """Factory spinner 'sedang berpikir' (dipakai dgn rich.live.Live)."""
    return Spinner("dots", text=Text(teks, style="muted"), style="accent")


# ---------------------------------------------------------------------------
# Prompt & konfirmasi
# ---------------------------------------------------------------------------
def tanya_resume(giliran: int) -> bool:
    """Tanya user apakah mau lanjut dari sesi sebelumnya."""
    try:
        console.print()
        return Confirm.ask(
            f"[muted]Ada sesi sebelumnya ({giliran} giliran). Lanjutkan?[/]",
            default=False,
        )
    except (EOFError, KeyboardInterrupt):
        return False


def tanya_konfirmasi_suara(prompt: str) -> None:
    """Cetak pertanyaan konfirmasi (jawaban via suara ditangani pemanggil)."""
    console.print(f"\n[warn]{ASK}[/] {prompt}")


def konfirmasi_keyboard(prompt: str) -> bool:
    """Konfirmasi y/n via keyboard (handler default aksi yang mengubah sistem)."""
    try:
        console.print()
        return Confirm.ask(f"[warn]{ASK}[/] {prompt}", default=False)
    except (EOFError, KeyboardInterrupt):
        return False


# ---------------------------------------------------------------------------
# Kolom input abu-abu (ANSI mentah, full-width)
# ---------------------------------------------------------------------------
def _baris_input_grey(W: int) -> str:
    """Segmen ANSI: isi baris penuh abu-abu, balik ke awal, cetak prompt '›'."""
    return (
        f"{BG_INPUT}{FG_INPUT}{' ' * W}\r"     # latar abu-abu mentok kiri-kanan
        f"{BOLD}{ACCENT} {PROMPT} {NOBOLD}{FG_INPUT}"  # prompt aksen, lalu teks gelap
    )


def kotak_input(petunjuk: str = "") -> str:
    """Prompt input full-width: kolom abu-abu ujung-ke-ujung + padding atas/bawah."""
    W = _lebar()
    blank = f"{BG_INPUT}{' ' * W}{RESET}"
    try:
        console.print()
        if petunjuk:
            console.print(f"[muted]{petunjuk}[/]")
        print(blank, flush=True)                       # padding atas
        print(_baris_input_grey(W), end="", flush=True)
        teks = input()                                 # user mengetik di atas abu-abu
        print(blank, flush=True)                       # padding bawah
    except (EOFError, KeyboardInterrupt):
        print(RESET, end="", flush=True)
        raise
    print(RESET, end="", flush=True)                   # reset warna setelah Enter
    return teks.strip()


def pesan_user(teks: str) -> None:
    """Tampilkan ucapan/ketikan user dalam bar abu-abu full-width (echo)."""
    W = _lebar()
    blank = f"{BG_INPUT}{' ' * W}{RESET}"
    pad = " " * max(0, W - len(f" {PROMPT} {teks}"))
    print()
    print(blank, flush=True)                           # padding atas
    print(
        f"{BG_INPUT}{FG_INPUT}{BOLD}{ACCENT} {PROMPT} {NOBOLD}{FG_INPUT}{teks}{pad}{RESET}",
        flush=True,
    )
    print(blank, flush=True)                           # padding bawah


# ---------------------------------------------------------------------------
# Bar status bawah (mode hands-free, di dalam scroll-region)
# ---------------------------------------------------------------------------
# mode -> (warna_label, label, hint)
_BAR = {
    "dengerin":    (ACCENT,    "SUARA",    f"ngomong langsung  {DOT}  ENTER ketik  {DOT}  ^C keluar"),
    "transkripsi": (WARN,      "PROSES",   "memproses suara…"),
    "berpikir":    (ACCENT_HI, "BERPIKIR", "menyusun jawaban…"),
}


def status_bar(H: int, W: int, mode: str = "dengerin") -> None:
    """Gambar bar status 3-baris paling bawah: rule, kolom input, label mode.

    mode: 'dengerin' (siap menerima) | 'transkripsi' | 'berpikir'.
    """
    warna, label, teks_hint = _BAR.get(mode, _BAR["dengerin"])

    # Reset warna dulu, lalu bersihkan dari H-2 ke bawah.
    print(f"{RESET}\033[{H-2};1H\033[J", end="", flush=True)
    # H-2: garis pemisah tipis (teal redup).
    print(f"{DIM}{ACCENT}{RULE * (W - 1)}{RESET}", end="", flush=True)

    if mode == "dengerin":
        # H-1: kolom input abu-abu (tempat user mengetik).
        print(f"\033[{H-1};1H{_baris_input_grey(W)}", end="", flush=True)
        # H: label mode + petunjuk.
        print(
            f"{RESET}\033[{H};1H {warna}{BOLD}{label}{RESET}  {MUTED}{teks_hint}{RESET}",
            end="", flush=True,
        )
        # Aktifkan kembali abu-abu & taruh kursor di awal area ketik (kolom 4).
        print(f"\033[{H-1};4H{BG_INPUT}{FG_INPUT}", end="", flush=True)
    else:
        # Status (transkripsi/berpikir): tanpa kolom abu-abu.
        print(
            f"\033[{H-1};1H {warna}{BOLD}{PROMPT}{RESET}  {MUTED}{teks_hint}{RESET}",
            end="", flush=True,
        )
        print(f"\033[{H};1H {warna}{BOLD}{label}{RESET}", end="", flush=True)
        print(f"\033[{H-1};4H", end="", flush=True)


# ---------------------------------------------------------------------------
# Layar terpisah (alternate screen buffer)
# ---------------------------------------------------------------------------
def buka_layar() -> None:
    """Masuk ke 'layar baru' (alternate screen) seperti vim/htop/less.

    Isi terminal sebelumnya disembunyikan selama Voca jalan, lalu dikembalikan
    utuh saat Voca ditutup.

    - ?1049h : aktifkan alternate screen.
    - ?1007l : matikan 'alternate scroll' — supaya scroll mouse TIDAK dikirim
      sebagai tombol panah (kalau aktif, muncul ^[[A / ^[[B di kolom input).
    """
    print("\033[?1049h\033[?1007l\033[2J\033[H", end="", flush=True)


def tutup_layar() -> None:
    """Keluar dari alternate screen: reset warna & scroll-region, pulihkan
    alternate scroll, lalu kembali ke layar terminal semula."""
    print("\033[0m\033[r\033[?1007h\033[?1049l", end="", flush=True)


if __name__ == "__main__":
    # Smoke test visual (tanpa API): python -m voca.ui
    buka_layar()
    try:
        banner("qwen-plus", Path.home() / "edi/project/pribadi/tts", "hands-free")
        hint(f"ngomong langsung  {DOT}  ENTER ketik  {DOT}  'berhenti' keluar")
        pesan_user("kamu siapa sih?")
        header_jawaban()
        console.print("Saya Voca, asisten coding berbasis suara.")
        baris_tool("read_file", "path=agent.py")
        baris_tool("run_command", "cmd=pytest -q")
        console.input("\n[muted]ENTER untuk keluar demo…[/]")
    finally:
        tutup_layar()
