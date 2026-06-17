"""
agent.py — "Otak" si AI coding companion.

Memakai model Qwen via DashScope (endpoint OpenAI-compatible).

Alur kerja:
  1. User memberi perintah (ketik atau suara).
  2. Qwen memikirkan langkah & memanggil tools (list_files, read_file,
     write_file, run_command) untuk menganalisis folder dan mengerjakan tugas.
  3. Setiap aksi yang mengubah sistem minta konfirmasi (keyboard / suara).
  4. Model menarasikan progres secara real-time (teks + suara Piper).

Dua mode: teks (default) dan hands-free (`--voice`).
"""

import json
import re
import sys

from openai import OpenAI

from . import config
from .tools import TOOLS_SCHEMA, TOOL_FUNCTIONS, WORKSPACE, set_confirm_handler
from .voice import StreamSpeaker, warmup, speak

SYSTEM_PROMPT = """Kamu adalah AI coding companion berbasis suara yang bekerja \
bersama developer, seperti rekan pair-programming yang aktif berkomunikasi.

Gaya kerja:
- Bicara dalam Bahasa Indonesia yang natural, ramah, dan ringkas.
- Sebelum bertindak, pahami dulu lingkungan kerja: gunakan list_files & read_file.
- Jelaskan langkah yang sedang kamu lakukan dan ALASANnya secara singkat,
  seolah sedang melaporkan progres ke rekan kerja secara real-time.
- Kerjakan tugas selangkah demi selangkah. Setelah selesai, simpulkan hasilnya.
- Kamu tidak perlu meminta izin di teks — sistem sudah otomatis meminta
  konfirmasi user saat kamu menulis file atau menjalankan command.

Narasimu nanti akan dibacakan dengan suara, jadi buat kalimat yang enak didengar."""


def hubungkan_tool(client, messages):
    """Loop satu giliran: panggil model, eksekusi tool, ulangi sampai selesai."""
    while True:
        stream = client.chat.completions.create(
            model=config.QWEN_MODEL,
            messages=messages,
            tools=TOOLS_SCHEMA,
            stream=True,
        )

        text_parts = []
        # Akumulasi tool call yang datang bertahap lewat stream (per index).
        tool_calls = {}
        # Speaker latar: mulai membacakan per kalimat begitu kalimat siap,
        # sambil teks berikutnya masih mengalir (lewat satu aliran, tanpa jeda).
        speaker = StreamSpeaker()

        print("\n🤖 ", end="", flush=True)
        for chunk in stream:
            if not chunk.choices:
                continue
            delta = chunk.choices[0].delta

            if getattr(delta, "content", None):
                print(delta.content, end="", flush=True)
                text_parts.append(delta.content)
                speaker.feed(delta.content)

            for tc in (getattr(delta, "tool_calls", None) or []):
                slot = tool_calls.setdefault(tc.index, {"id": "", "name": "", "args": ""})
                if tc.id:
                    slot["id"] = tc.id
                if tc.function and tc.function.name:
                    slot["name"] = tc.function.name
                if tc.function and tc.function.arguments:
                    slot["args"] += tc.function.arguments
        print()

        # Tunggu sisa narasi selesai diucapkan sebelum lanjut (mis. jalankan tool).
        speaker.close()
        narasi = "".join(text_parts)

        # Susun pesan balasan asisten (teks + permintaan tool, jika ada).
        assistant_msg = {"role": "assistant", "content": narasi}
        if tool_calls:
            assistant_msg["tool_calls"] = [
                {
                    "id": tc["id"],
                    "type": "function",
                    "function": {"name": tc["name"], "arguments": tc["args"] or "{}"},
                }
                for tc in tool_calls.values()
            ]
        messages.append(assistant_msg)

        # Tidak ada tool yang diminta -> giliran ini selesai.
        if not tool_calls:
            return

        # Eksekusi setiap tool, kirim hasilnya kembali ke model.
        for tc in tool_calls.values():
            fungsi = TOOL_FUNCTIONS.get(tc["name"])
            try:
                args = json.loads(tc["args"]) if tc["args"] else {}
            except json.JSONDecodeError:
                args = {}
            print(f"\n   🔧 {tc['name']}({args})")

            if fungsi is None:
                hasil = f"Tool tidak dikenal: {tc['name']}"
            else:
                try:
                    hasil = fungsi(**args)
                except Exception as e:
                    hasil = f"Error menjalankan {tc['name']}: {e}"

            messages.append({
                "role": "tool",
                "tool_call_id": tc["id"],
                "content": str(hasil),
            })
        # Lanjutkan loop: model lihat hasil tool lalu lanjut bekerja.


# ---------------------------------------------------------------------------
# Pengenalan ucapan ya/tidak & kata berhenti (untuk mode suara)
# ---------------------------------------------------------------------------
_KATA_YA = {"ya", "iya", "yes", "boleh", "lanjut", "setuju", "oke", "ok",
            "gas", "silakan", "jalan", "lakukan"}
_KATA_STOP = {"berhenti", "keluar", "stop", "udahan", "udah"}


def _minta_keluar(perintah: str) -> bool:
    """True kalau user jelas-jelas minta berhenti (ucapan pendek + kata stop).

    Dibatasi ucapan pendek (<=3 kata) supaya tak salah keluar saat kata 'stop'
    muncul di tengah perintah biasa, mis. 'stop server-nya lalu restart'.
    """
    kata = re.findall(r"\w+", perintah.lower())
    return len(kata) <= 3 and bool(set(kata) & _KATA_STOP)


def _voice_confirm(prompt: str) -> bool:
    """Konfirmasi via suara: AI bertanya, user menjawab 'ya'/'tidak'."""
    from .listen import listen_auto

    print(f"\n⚠️  {prompt}")
    speak(prompt + " Jawab ya atau tidak.")
    jawab = listen_auto().lower()
    print(f"📝 (suara) Jawaban: {jawab!r}")
    setuju = bool(set(re.findall(r"\w+", jawab)) & _KATA_YA)
    speak("Oke, saya lanjutkan." if setuju else "Baik, saya batalkan.")
    return setuju


# ---------------------------------------------------------------------------
# Mode interaksi
# ---------------------------------------------------------------------------
def run_text_mode(client, messages):
    """Mode teks: ketik perintah, atau 'v' + ENTER untuk bicara sekali."""
    print("=" * 60)
    print(f"🎙️  AI Coding Companion — model: {config.QWEN_MODEL}")
    print(f"📂 Folder kerja: {WORKSPACE}")
    print("Ketik perintah, atau 'v' + ENTER untuk bicara. 'keluar' untuk berhenti.")
    print("=" * 60)

    while True:
        try:
            perintah = input("\n🧑 Kamu (ketik / 'v'=bicara): ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nSampai jumpa! 👋")
            break

        if perintah.lower() in ("v", "suara", "voice"):
            try:
                from .listen import listen
                perintah = listen()
            except Exception as e:
                print(f"   [⚠️  input suara gagal: {e}]")
                continue
            print(f"📝 (suara) Kamu: {perintah}")

        if not perintah:
            continue
        if perintah.lower() in ("keluar", "exit", "quit"):
            print("Sampai jumpa! 👋")
            break

        messages.append({"role": "user", "content": perintah})
        try:
            hubungkan_tool(client, messages)
        except Exception as e:
            print(f"\n❌ Terjadi error: {e}")


def run_handsfree_mode(client, messages):
    """Mode hands-free: dengar terus otomatis, konfirmasi pakai suara."""
    from .listen import listen_auto

    set_confirm_handler(_voice_confirm)  # konfirmasi aksi lewat suara

    print("=" * 60)
    print(f"🎙️  AI Coding Companion — HANDS-FREE — model: {config.QWEN_MODEL}")
    print(f"📂 Folder kerja: {WORKSPACE}")
    print("Bicara langsung. Ucapkan 'berhenti' untuk keluar. (Ctrl+C juga bisa)")
    print("=" * 60)
    speak("Halo, saya siap membantu. Silakan bicara.")

    while True:
        print("\n🎤 Mendengarkan... (bicara, berhenti otomatis saat kamu diam)")
        try:
            perintah = listen_auto()
        except KeyboardInterrupt:
            speak("Sampai jumpa!")
            print("\nSampai jumpa! 👋")
            break

        if not perintah:
            continue  # tidak terdengar suara -> dengar lagi
        print(f"🧑 (suara) Kamu: {perintah}")

        if _minta_keluar(perintah):
            speak("Baik, sampai jumpa!")
            print("Sampai jumpa! 👋")
            break

        messages.append({"role": "user", "content": perintah})
        try:
            hubungkan_tool(client, messages)
        except Exception as e:
            print(f"\n❌ Terjadi error: {e}")


def main():
    if not config.QWEN_API_KEY:
        print("❌ DASHSCOPE_API_KEY belum diset. Salin .env.example ke .env dan isi key-mu.")
        sys.exit(1)

    client = OpenAI(api_key=config.QWEN_API_KEY, base_url=config.QWEN_BASE_URL)
    messages = [{"role": "system", "content": SYSTEM_PROMPT}]
    warmup()  # muat model suara di awal agar balasan pertama tidak tertunda

    # Mode hands-free kalau dijalankan dengan flag --voice / --suara.
    handsfree = any(a in ("--voice", "-v", "--handsfree", "--suara") for a in sys.argv[1:])
    if handsfree:
        run_handsfree_mode(client, messages)
    else:
        run_text_mode(client, messages)


if __name__ == "__main__":
    main()
