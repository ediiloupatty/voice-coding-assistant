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
import select
import shutil
import subprocess
import time
from pathlib import Path

from . import config
from . import ui
from .ui import console

# Folder berat/tak relevan yang dilewati saat menyusuri (list & search).
_IGNORE_DIRS = {
    ".git", "__pycache__", "node_modules", ".venv", "venv", ".voca",
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
        console.print(f"[warn]Folder saat ini tidak valid (mungkin terhapus). "
                      f"Pakai {home} sebagai folder kerja. Sebaiknya 'cd' ke folder yang ada.[/warn]")
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


# Handler konfirmasi aktif. Bisa diganti (mis. mode suara) lewat
# set_confirm_handler(). Default: keyboard (lewat helper ber-theme di ui).
_confirm_handler = ui.konfirmasi_keyboard


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


def _search_ripgrep(query: str, rel_path: str) -> str | None:
    """Cari pakai ripgrep (cepat). Return None kalau rg error -> fallback Python."""
    cmd = ["rg", "--no-heading", "--line-number", "--color", "never",
           "-S", "--max-filesize", "1M"]
    for d in _IGNORE_DIRS:
        cmd += ["--glob", f"!**/{d}/**"]
    cmd += ["--", query, rel_path]
    try:
        proc = subprocess.run(cmd, cwd=WORKSPACE, capture_output=True,
                              text=True, timeout=30)
    except Exception:
        return None
    if proc.returncode not in (0, 1):  # 0=ketemu, 1=tak ada, lainnya=error
        return None
    out = proc.stdout.splitlines()
    if not out:
        return f"Tidak ada hasil untuk '{query}'."
    hasil = [ln[:260] for ln in out[:config.SEARCH_MAX_RESULTS]]
    if len(out) > config.SEARCH_MAX_RESULTS:
        hasil.append(f"... (dipotong di {config.SEARCH_MAX_RESULTS} hasil)")
    return "\n".join(hasil)


def _search_python(query: str, base: Path) -> str:
    """Pencarian murni Python (fallback bila ripgrep tak ada)."""
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


def search_files(query: str, path: str = ".") -> str:
    """Cari teks/kode di seluruh file dalam folder kerja (seperti grep).

    Pakai ripgrep (`rg`) bila tersedia — jauh lebih cepat — kalau tidak ada
    atau gagal, jatuh ke pencarian Python.
    """
    base = _resolve_safe(path)
    if not base.exists():
        return f"Path '{path}' tidak ditemukan."

    if shutil.which("rg"):
        rel = os.path.relpath(base, WORKSPACE)
        hasil = _search_ripgrep(query, rel)
        if hasil is not None:
            return hasil
    return _search_python(query, base)


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
        console.print(f"   [muted](tidak ada perubahan isi pada '{path}')[/muted]")
        return tambah, hapus

    console.print(f"\n   [muted]{ui.TOOL}[/muted] [accent]{path}[/accent]")
    for ln in diff[:config.DIFF_MAX_LINES]:
        if ln.startswith(("+++", "---")):
            console.print(f"   [muted]{ln}[/muted]")
        elif ln.startswith("@@"):
            console.print(f"   [accent]{ln}[/accent]")
        elif ln.startswith("+"):
            console.print(f"   [success]{ln}[/success]")
        elif ln.startswith("-"):
            console.print(f"   [error]{ln}[/error]")
        else:
            console.print(f"   {ln}")
    if len(diff) > config.DIFF_MAX_LINES:
        console.print(f"   [muted]... (diff dipotong di {config.DIFF_MAX_LINES} baris)[/muted]")
    console.print(f"   [success]+{tambah}[/success] [muted]/[/muted] [error]-{hapus}[/error] baris\n")
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


def edit_file(path: str, old_string: str, new_string: str) -> str:
    """Ganti satu potongan teks di file (find/replace). MINTA KONFIRMASI dulu.

    Jauh lebih hemat daripada write_file untuk mengedit file yang sudah ada:
    cukup kirim potongan lama & penggantinya, bukan seluruh isi file.
    old_string harus cocok PERSIS dan UNIK di dalam file.
    """
    target = _resolve_safe(path)
    if not target.exists():
        return f"File '{path}' tidak ada. Pakai write_file untuk membuat file baru."
    try:
        lama = target.read_text(encoding="utf-8", errors="replace")
    except Exception as e:
        return f"Gagal membaca '{path}': {e}"

    jumlah = lama.count(old_string)
    if jumlah == 0:
        return (f"Teks tak ditemukan di '{path}'. Pastikan old_string sama persis "
                f"(termasuk spasi & indentasi).")
    if jumlah > 1:
        return (f"Teks muncul {jumlah}x di '{path}' — tidak unik. Perluas old_string "
                f"dengan konteks di sekitarnya supaya unik.")
    if old_string == new_string:
        return "Tidak ada perubahan (old_string sama dengan new_string)."

    baru = lama.replace(old_string, new_string, 1)
    tambah, hapus = _tampilkan_diff(path, lama, baru)
    if not _confirm(f"Agent ingin mengedit '{path}' (+{tambah}/-{hapus} baris). Lanjut?"):
        return f"Dibatalkan oleh user. File '{path}' tidak diubah."
    try:
        target.write_text(baru, encoding="utf-8")
        return f"Berhasil mengedit '{path}' (+{tambah}/-{hapus} baris)."
    except Exception as e:
        return f"Gagal menulis '{path}': {e}"


def _run_blocking(command: str) -> str:
    """Jalankan command tanpa streaming (fallback non-POSIX)."""
    try:
        hasil = subprocess.run(
            command, shell=True, cwd=WORKSPACE,
            capture_output=True, text=True, timeout=config.COMMAND_TIMEOUT,
        )
    except subprocess.TimeoutExpired:
        return f"Command dihentikan: melebihi {config.COMMAND_TIMEOUT} detik."
    except Exception as e:
        return f"Gagal menjalankan command: {e}"
    out = ((hasil.stdout or "") + (hasil.stderr or "")).strip() or "(tanpa output)"
    if len(out) > config.MAX_OUTPUT_CHARS:
        out = out[:config.MAX_OUTPUT_CHARS] + f"\n... (output dipotong di {config.MAX_OUTPUT_CHARS} karakter)"
    return f"(exit code {hasil.returncode})\n{out}"


def run_command(command: str) -> str:
    """Jalankan perintah terminal dengan output LIVE. MINTA KONFIRMASI dulu.

    Output ditampilkan baris-demi-baris saat berjalan (enak untuk test, build,
    install). Bisa dibatalkan dengan Ctrl+C; ada batas waktu COMMAND_TIMEOUT.
    """
    if not _confirm(f"Agent ingin menjalankan: `{command}`. Lanjut?"):
        return "Dibatalkan oleh user. Command tidak dijalankan."

    if os.name != "posix":  # streaming via select hanya andal di POSIX
        return _run_blocking(command)

    console.print(f"[muted]  $ {command}[/muted]")
    try:
        proc = subprocess.Popen(
            command, shell=True, cwd=WORKSPACE,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            text=True, bufsize=1,
        )
    except Exception as e:
        return f"Gagal menjalankan command: {e}"

    potongan, total = [], 0
    start = time.monotonic()
    try:
        while True:
            sisa = config.COMMAND_TIMEOUT - (time.monotonic() - start)
            if sisa <= 0:
                proc.kill(); proc.wait()
                ekor = "".join(potongan).strip()
                return f"Command dihentikan: melebihi {config.COMMAND_TIMEOUT} detik.\n{ekor}".strip()
            ready, _, _ = select.select([proc.stdout], [], [], min(sisa, 0.5))
            if ready:
                line = proc.stdout.readline()
                if line == "":
                    break  # EOF: proses selesai
                console.print(f"[accent]  │[/accent] {line}", end="")
                if total < config.MAX_OUTPUT_CHARS:
                    potongan.append(line)
                    total += len(line)
            elif proc.poll() is not None:
                break  # proses selesai, tak ada output baru
    except KeyboardInterrupt:
        proc.kill(); proc.wait()
        console.print(f"\n[muted]  (dibatalkan dengan Ctrl+C)[/muted]")
        return "Command dibatalkan oleh user (Ctrl+C)."
    finally:
        try:
            proc.stdout.close()
        except Exception:
            pass
    proc.wait()

    out = "".join(potongan).strip() or "(tanpa output)"
    if total >= config.MAX_OUTPUT_CHARS:
        out += f"\n... (output dipotong di {config.MAX_OUTPUT_CHARS} karakter)"
    return f"(exit code {proc.returncode})\n{out}"


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
            "name": "edit_file",
            "description": "Edit file yang SUDAH ADA dengan mengganti satu potongan "
                           "teks (find/replace). Pakai ini, BUKAN write_file, untuk "
                           "mengubah sebagian file — jauh lebih hemat & akurat. "
                           "old_string harus cocok persis dan unik.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path file yang diedit"},
                    "old_string": {"type": "string", "description": "Teks lama (persis & unik)"},
                    "new_string": {"type": "string", "description": "Teks pengganti"},
                },
                "required": ["path", "old_string", "new_string"],
            },
        },
    },
    {
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Buat file BARU atau timpa seluruh isi file. Untuk "
                           "mengubah sebagian file yang sudah ada, pakai edit_file. "
                           "Akan minta konfirmasi user.",
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
    "edit_file": edit_file,
    "write_file": write_file,
    "run_command": run_command,
}
