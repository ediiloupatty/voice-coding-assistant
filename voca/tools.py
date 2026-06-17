"""
tools.py — "Tangan" si agent.

Berisi fungsi-fungsi yang bisa dipanggil model untuk berinteraksi dengan
lingkungan kerja: melihat struktur folder, membaca file, menulis file, dan
menjalankan perintah terminal.

Semua aksi yang mengubah sistem (menulis file, menjalankan command) akan
MEMINTA KONFIRMASI ke user terlebih dahulu sebelum dieksekusi.
"""

import difflib
import os
import subprocess
from pathlib import Path

from . import config

# Kode warna ANSI untuk tampilan diff di terminal.
_HIJAU, _MERAH, _CYAN, _TEBAL, _RESET = (
    "\033[32m", "\033[31m", "\033[36m", "\033[1m", "\033[0m",
)

# Folder berat/tak relevan yang dilewati saat menyusuri (list & search).
_IGNORE_DIRS = {
    ".git", "__pycache__", "node_modules", ".venv", "venv",
    ".mypy_cache", ".pytest_cache", "dist", "build", ".next", ".idea",
}


def _detect_workspace() -> Path:
    """Folder kerja agent = folder tempat program dijalankan.

    Kalau folder itu sudah tidak valid (mis. terhapus/di-rename saat terminal
    masih berada di dalamnya), jangan crash — pakai HOME sebagai cadangan.
    """
    try:
        return Path(os.getcwd()).resolve()
    except (FileNotFoundError, OSError):
        home = Path.home()
        print(f"Folder saat ini tidak valid (mungkin terhapus). "
              f"Pakai {home} sebagai folder kerja. Sebaiknya 'cd' ke folder yang ada.")
        return home


# Semua operasi file dibatasi di dalam folder ini demi keamanan.
WORKSPACE = _detect_workspace()


# ---------------------------------------------------------------------------
# Util keamanan: pastikan path tetap di dalam workspace
# ---------------------------------------------------------------------------
def _resolve_safe(path: str) -> Path:
    """Ubah path relatif jadi absolut & cegah keluar dari WORKSPACE."""
    target = (WORKSPACE / path).resolve()
    if WORKSPACE not in target.parents and target != WORKSPACE:
        raise ValueError(f"Akses ditolak: '{path}' di luar folder kerja.")
    return target


def _keyboard_confirm(prompt: str) -> bool:
    """Konfirmasi default: ketik y/n di keyboard."""
    jawab = input(f"\n{prompt} [y/N]: ").strip().lower()
    return jawab in ("y", "yes", "ya")


# Handler konfirmasi aktif. Bisa diganti (mis. mode suara) lewat
# set_confirm_handler(). Default: keyboard.
_confirm_handler = _keyboard_confirm


def set_confirm_handler(fn) -> None:
    """Ganti cara konfirmasi (mis. konfirmasi via suara untuk mode hands-free)."""
    global _confirm_handler
    _confirm_handler = fn


def _confirm(prompt: str) -> bool:
    """Minta persetujuan user lewat handler yang aktif. Return True jika setuju."""
    return _confirm_handler(prompt)


# ---------------------------------------------------------------------------
# Implementasi tool
# ---------------------------------------------------------------------------
def list_files(path: str = ".") -> str:
    """Tampilkan daftar file & folder (rekursif, ringkas, dibatasi)."""
    base = _resolve_safe(path)
    if not base.exists():
        return f"Path '{path}' tidak ditemukan."

    baris = []
    terpotong = False
    for root, dirs, files in os.walk(base):
        dirs[:] = [d for d in sorted(dirs) if d not in _IGNORE_DIRS]
        rel_root = os.path.relpath(root, WORKSPACE)
        depth = 0 if rel_root == "." else rel_root.count(os.sep) + 1
        if depth > config.LIST_MAX_DEPTH:
            dirs[:] = []  # jangan turun lebih dalam dari batas
            continue
        indent = "  " * depth
        if rel_root != ".":
            baris.append(f"{indent}{os.path.basename(root)}/")
        for f in sorted(files):
            baris.append(f"{indent}  {f}")
        if len(baris) >= config.LIST_MAX_ENTRIES:
            terpotong = True
            break

    if not baris:
        return "(folder kosong)"
    hasil = "\n".join(baris[:config.LIST_MAX_ENTRIES])
    if terpotong:
        hasil += (f"\n... (dipotong di {config.LIST_MAX_ENTRIES} baris — "
                  f"pakai path lebih spesifik atau search_files untuk fokus)")
    return hasil


def read_file(path: str, start_line: int = None, end_line: int = None) -> str:
    """Baca isi file teks. Bisa baca sebagian lewat start_line & end_line."""
    target = _resolve_safe(path)
    if not target.exists():
        return f"File '{path}' tidak ditemukan."
    try:
        teks = target.read_text(encoding="utf-8", errors="replace")
    except Exception as e:
        return f"Gagal membaca '{path}': {e}"

    # Baca rentang baris tertentu (bernomor, memudahkan rujukan).
    if start_line is not None or end_line is not None:
        semua = teks.splitlines()
        a = max((start_line or 1) - 1, 0)
        b = end_line if end_line is not None else len(semua)
        potong = semua[a:b]
        if not potong:
            return "(rentang baris kosong)"
        return "\n".join(f"{a + i + 1}: {ln}" for i, ln in enumerate(potong))

    # Baca utuh, tapi potong kalau kelewat besar agar context tak meledak.
    if len(teks) > config.MAX_READ_CHARS:
        return (teks[:config.MAX_READ_CHARS]
                + f"\n\n... (file dipotong di {config.MAX_READ_CHARS} dari {len(teks)} karakter — "
                  f"pakai start_line & end_line untuk baca bagian tertentu)")
    return teks


def search_files(query: str, path: str = ".") -> str:
    """Cari teks/kode di seluruh file dalam folder kerja (seperti grep)."""
    base = _resolve_safe(path)
    if not base.exists():
        return f"Path '{path}' tidak ditemukan."

    q = query.lower()
    hasil = []
    for root, dirs, files in os.walk(base):
        dirs[:] = [d for d in sorted(dirs) if d not in _IGNORE_DIRS]
        for f in sorted(files):
            fp = Path(root) / f
            try:
                if fp.stat().st_size > 1_000_000:  # lewati file >1 MB
                    continue
                isi = fp.read_text(encoding="utf-8", errors="ignore")
            except Exception:
                continue
            for i, ln in enumerate(isi.splitlines(), 1):
                if q in ln.lower():
                    rel = os.path.relpath(fp, WORKSPACE)
                    hasil.append(f"{rel}:{i}: {ln.strip()[:200]}")
                    if len(hasil) >= config.SEARCH_MAX_RESULTS:
                        hasil.append(f"... (dipotong di {config.SEARCH_MAX_RESULTS} hasil)")
                        return "\n".join(hasil)
    return "\n".join(hasil) if hasil else f"Tidak ada hasil untuk '{query}'."


def _tampilkan_diff(path: str, lama: str, baru: str) -> tuple[int, int]:
    """Cetak perbedaan ke terminal (hijau = ditambah, merah = dihapus).

    Mengembalikan (jumlah_tambah, jumlah_hapus) supaya bisa dilaporkan.
    """
    diff = list(difflib.unified_diff(
        lama.splitlines(), baru.splitlines(),
        fromfile=f"a/{path}", tofile=f"b/{path}", lineterm="",
    ))
    tambah = sum(1 for d in diff if d.startswith("+") and not d.startswith("+++"))
    hapus = sum(1 for d in diff if d.startswith("-") and not d.startswith("---"))

    if not config.SHOW_DIFF:
        return tambah, hapus
    if not diff:
        print(f"   (tidak ada perubahan isi pada '{path}')")
        return tambah, hapus

    print(f"\n   ── Perubahan '{path}' ──")
    for ln in diff[:config.DIFF_MAX_LINES]:
        if ln.startswith(("+++", "---")):
            print(f"   {_TEBAL}{ln}{_RESET}")
        elif ln.startswith("@@"):
            print(f"   {_CYAN}{ln}{_RESET}")
        elif ln.startswith("+"):
            print(f"   {_HIJAU}{ln}{_RESET}")
        elif ln.startswith("-"):
            print(f"   {_MERAH}{ln}{_RESET}")
        else:
            print(f"   {ln}")
    if len(diff) > config.DIFF_MAX_LINES:
        print(f"   ... (diff dipotong di {config.DIFF_MAX_LINES} baris)")
    print(f"   {_HIJAU}+{tambah}{_RESET} / {_MERAH}-{hapus}{_RESET} baris\n")
    return tambah, hapus


def write_file(path: str, content: str) -> str:
    """Tulis/timpa isi file. Tampilkan diff lalu MINTA KONFIRMASI dulu."""
    target = _resolve_safe(path)
    ada = target.exists()
    aksi = "menimpa" if ada else "membuat"

    lama = ""
    if ada:
        try:
            lama = target.read_text(encoding="utf-8", errors="replace")
        except Exception:
            lama = ""

    tambah, hapus = _tampilkan_diff(path, lama, content)

    if not _confirm(f"Agent ingin {aksi} file '{path}' (+{tambah}/-{hapus} baris). Lanjut?"):
        return f"Dibatalkan oleh user. File '{path}' tidak diubah."
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        return f"Berhasil {aksi} file '{path}' (+{tambah}/-{hapus} baris)."
    except Exception as e:
        return f"Gagal menulis '{path}': {e}"


def run_command(command: str) -> str:
    """Jalankan perintah terminal. MINTA KONFIRMASI dulu."""
    if not _confirm(f"Agent ingin menjalankan: `{command}`. Lanjut?"):
        return "Dibatalkan oleh user. Command tidak dijalankan."
    try:
        hasil = subprocess.run(
            command, shell=True, cwd=WORKSPACE,
            capture_output=True, text=True, timeout=120,
        )
        out = ((hasil.stdout or "") + (hasil.stderr or "")).strip() or "(tanpa output)"
        if len(out) > config.MAX_OUTPUT_CHARS:
            out = out[:config.MAX_OUTPUT_CHARS] + f"\n... (output dipotong di {config.MAX_OUTPUT_CHARS} karakter)"
        return f"(exit code {hasil.returncode})\n{out}"
    except subprocess.TimeoutExpired:
        return "Command dihentikan: melebihi batas waktu 120 detik."
    except Exception as e:
        return f"Gagal menjalankan command: {e}"


# ---------------------------------------------------------------------------
# Skema tool format OpenAI / function calling (dipakai di agent.py)
# ---------------------------------------------------------------------------
TOOLS_SCHEMA = [
    {
        "type": "function",
        "function": {
            "name": "list_files",
            "description": "Lihat struktur folder & file di dalam folder kerja. "
                           "Gunakan ini lebih dulu untuk memahami lingkungan kerja.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path folder (default '.')"}
                },
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "search_files",
            "description": "Cari teks/kode di seluruh folder kerja (seperti grep). "
                           "Pakai ini untuk menemukan di mana sesuatu didefinisikan "
                           "SEBELUM membaca file besar — jauh lebih hemat.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Teks/kata kunci yang dicari"},
                    "path": {"type": "string", "description": "Folder pencarian (default '.')"},
                },
                "required": ["query"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Baca isi sebuah file teks. Untuk file besar, baca "
                           "sebagian saja lewat start_line & end_line.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path file yang dibaca"},
                    "start_line": {"type": "integer", "description": "Baris awal (opsional, mulai dari 1)"},
                    "end_line": {"type": "integer", "description": "Baris akhir (opsional)"},
                },
                "required": ["path"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Tulis atau timpa sebuah file. Akan minta konfirmasi user.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path file tujuan"},
                    "content": {"type": "string", "description": "Isi file"},
                },
                "required": ["path", "content"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "run_command",
            "description": "Jalankan perintah terminal di folder kerja. Akan minta konfirmasi user.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Perintah shell yang dijalankan"}
                },
                "required": ["command"],
            },
        },
    },
]

# Pemetaan nama tool -> fungsi Python-nya
TOOL_FUNCTIONS = {
    "list_files": list_files,
    "search_files": search_files,
    "read_file": read_file,
    "write_file": write_file,
    "run_command": run_command,
}
