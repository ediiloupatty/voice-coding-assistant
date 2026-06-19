"""
voice_server.py — sidecar suara untuk core Rust (arsitektur hybrid).

Core Rust menjalankan sidecar ini SEKALI; model tetap "warm" di memori sehingga
TTS/STT berikutnya mulus (tak ada loading ulang). Komunikasi lewat protokol
JSON per-baris:

  stdin  (Rust → Python):
    {"cmd":"ping"}
    {"cmd":"speak","text":"...","lang":"id"}
    {"cmd":"listen","lang":"id"}
  stdout (Python → Rust):
    {"ready":true}              # sekali, saat siap
    {"ok":true}                 # balasan speak/ping
    {"ok":true,"text":"..."}    # balasan listen (hasil transkripsi)
    {"ok":false,"error":"..."}  # bila gagal

Semua output UI (Rich) dari modul voice/listen dialihkan ke STDERR supaya STDOUT
tetap bersih khusus protokol. Pengguna tetap melihat status ("merekam…", dll.)
karena stderr diwarisi oleh terminal.
"""
import json
import os
import sys

# --- Pisahkan kanal protokol (stdout asli) dari output UI ------------------
# Duplikat stdout ASLI (pipe ke Rust) untuk protokol, lalu arahkan fd 1 ke
# stderr agar semua print/Rich tampil di terminal, bukan mengotori protokol.
_proto = os.fdopen(os.dup(1), "w", buffering=1)
os.dup2(2, 1)
sys.stdout = sys.stderr

# Impor SETELAH pengalihan agar pesan saat import pun tak mengotori protokol.
from voca import lang as lang_mod  # noqa: E402
from voca import listen, voice  # noqa: E402


def _send(obj) -> None:
    _proto.write(json.dumps(obj) + "\n")
    _proto.flush()


def main() -> None:
    # Hangatkan model TTS supaya ucapan pertama tak terasa lambat.
    try:
        voice.warmup()
    except Exception:
        pass
    _send({"ready": True})

    while True:
        line = sys.stdin.readline()
        if not line:  # EOF: core Rust menutup koneksi → keluar.
            break
        line = line.strip()
        if not line:
            continue

        try:
            req = json.loads(line)
        except Exception:
            _send({"ok": False, "error": "bad json"})
            continue

        if req.get("lang"):
            lang_mod.set(req["lang"])
        cmd = req.get("cmd")

        try:
            if cmd == "ping":
                _send({"ok": True})
            elif cmd == "speak":
                voice.speak(req.get("text", ""))
                _send({"ok": True})
            elif cmd == "listen":
                text = listen.listen_auto()
                _send({"ok": True, "text": text})
            else:
                _send({"ok": False, "error": f"unknown cmd: {cmd}"})
        except Exception as e:
            _send({"ok": False, "error": str(e)})


if __name__ == "__main__":
    main()
