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
import select
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


def _stdin_has_input() -> bool:
    """True bila ada baris menunggu di stdin (perintah dari core Rust).

    Dipakai saat sedang `listen`: bila core mengirim apa pun (mis. {"cmd":"cancel"}
    ketika user menekan `t`), konsumsi barisnya & batalkan perekaman. Hanya andal
    di POSIX. Perintah cancel sengaja TIDAK dibalas agar protokol tetap sinkron.
    """
    try:
        if select.select([sys.stdin], [], [], 0)[0]:
            sys.stdin.readline()  # konsumsi baris cancel
            return True
    except Exception:
        pass
    return False


def main() -> None:
    # Hangatkan SEMUA model di awal supaya user langsung bisa bicara tanpa jeda:
    # TTS (piper) + STT (whisper). Status tampil di terminal (stderr).
    print("  memuat model suara (TTS + STT), sebentar…", file=sys.stderr, flush=True)
    try:
        voice.warmup()        # Piper (TTS)
    except Exception:
        pass
    try:
        listen._get_model()   # Whisper (STT) — preload, tak lagi lazy
    except Exception:
        pass
    # Silero VAD WAJIB (tak ada fallback). Bila gagal dimuat → cetak instruksi
    # pasang yang jelas lalu keluar; core Rust akan lanjut ke mode teks.
    try:
        listen.preload_vad()
    except Exception:
        print("  \033[1;31m✗ suara dimatikan — VAD Silero belum terpasang.\033[0m",
              file=sys.stderr, flush=True)
        sys.exit(1)
    print("  \033[32m✓ model suara siap.\033[0m", file=sys.stderr, flush=True)
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
                barged = voice.speak(req.get("text", ""))
                _send({"ok": True, "barged": bool(barged)})
            elif cmd == "listen":
                print("  [mic] mendengarkan…", file=sys.stderr, flush=True)
                # Lapor status suara real-time ke core (indikator "mendengar kamu").
                def _on_vad(speech: bool) -> None:
                    _send({"event": "vad", "speech": bool(speech)})
                text = listen.listen_auto(should_cancel=_stdin_has_input, on_vad=_on_vad)
                print(f"  [mic] transkripsi: {text!r}", file=sys.stderr, flush=True)
                _send({"ok": True, "text": text})
            elif cmd == "cancel":
                # No-op di luar listen (saat listen, dikonsumsi _stdin_has_input).
                # Sengaja tak membalas: cancel bersifat fire-and-forget.
                pass
            else:
                _send({"ok": False, "error": f"unknown cmd: {cmd}"})
        except Exception as e:
            _send({"ok": False, "error": str(e)})


if __name__ == "__main__":
    main()
