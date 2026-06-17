"""
tools.py — "Tangan" si agent.

Berisi fungsi-fungsi yang bisa dipanggil model untuk berinteraksi dengan
lingkungan kerja: melihat struktur folder, membaca file, menulis file, dan
menjalankan perintah terminal.

Semua aksi yang mengubah sistem (menulis file, menjalankan command) akan
MEMINTA KONFIRMASI ke user terlebih dahulu sebelum dieksekusi.
"""

import os
import subprocess
from pathlib import Path


def _detect_workspace() -> Path:
    """Folder kerja agent = folder tempat program dijalankan.

    Kalau folder itu sudah tidak valid (mis. terhapus/di-rename saat terminal
    masih berada di dalamnya), jangan crash — pakai HOME sebagai cadangan.
    """
    try:
        return Path(os.getcwd()).resolve()
    except (FileNotFoundError, OSError):
        home = Path.home()
        print(f"⚠️  Folder saat ini tidak valid (mungkin terhapus). "
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
    jawab = input(f"\n⚠️  {prompt} [y/N]: ").strip().lower()
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
    """Tampilkan daftar file & folder (rekursif, ringkas)."""
    base = _resolve_safe(path)
    if not base.exists():
        return f"Path '{path}' tidak ditemukan."

    baris = []
    for root, dirs, files in os.walk(base):
        # Lewati folder berat/tak relevan
        dirs[:] = [d for d in dirs if d not in (".git", "__pycache__", "node_modules", ".venv")]
        rel_root = os.path.relpath(root, WORKSPACE)
        depth = 0 if rel_root == "." else rel_root.count(os.sep) + 1
        indent = "  " * depth
        if rel_root != ".":
            baris.append(f"{indent}{os.path.basename(root)}/")
        for f in sorted(files):
            baris.append(f"{indent}  {f}")
    return "\n".join(baris) if baris else "(folder kosong)"


def read_file(path: str) -> str:
    """Baca isi sebuah file teks."""
    target = _resolve_safe(path)
    if not target.exists():
        return f"File '{path}' tidak ditemukan."
    try:
        return target.read_text(encoding="utf-8")
    except Exception as e:
        return f"Gagal membaca '{path}': {e}"


def write_file(path: str, content: str) -> str:
    """Tulis/timpa isi file. MINTA KONFIRMASI dulu."""
    target = _resolve_safe(path)
    aksi = "menimpa" if target.exists() else "membuat"
    if not _confirm(f"Agent ingin {aksi} file '{path}' ({len(content)} karakter). Lanjut?"):
        return f"Dibatalkan oleh user. File '{path}' tidak diubah."
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")
        return f"Berhasil {aksi} file '{path}'."
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
        out = (hasil.stdout or "") + (hasil.stderr or "")
        return f"(exit code {hasil.returncode})\n{out.strip() or '(tanpa output)'}"
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
            "name": "read_file",
            "description": "Baca isi sebuah file teks.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path file yang dibaca"}
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
    "read_file": read_file,
    "write_file": write_file,
    "run_command": run_command,
}
