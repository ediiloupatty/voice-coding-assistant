"""
agent.py — "Otak" si Voca.

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
import shutil
import sys
import time
from pathlib import Path

from openai import (OpenAI, APIConnectionError, APITimeoutError,
                    InternalServerError, RateLimitError)

# Error sementara yang layak dicoba ulang (koneksi putus, timeout, 429, 5xx).
_TRANSIENT_ERRORS = (APIConnectionError, APITimeoutError,
                     RateLimitError, InternalServerError)

from . import config
from .tools import TOOLS_SCHEMA, TOOL_FUNCTIONS, WORKSPACE, list_files, set_confirm_handler
from .voice import StreamSpeaker, warmup, speak

# ---------------------------------------------------------------------------
# Tampilan terminal — gaya CLI minimalis & profesional
# ---------------------------------------------------------------------------
_DIM, _BOLD, _CYAN, _GREEN, _RED, _RESET = (
    "\033[2m", "\033[1m", "\033[36m", "\033[32m", "\033[31m", "\033[0m",
)


def _pendekkan(teks: str, maks: int) -> str:
    """Pangkas teks panjang dari depan, sisakan ekornya (mis. path)."""
    return teks if len(teks) <= maks else "…" + teks[-(maks - 1):]


def _kotak(rows, lebar: int = 56) -> None:
    """Cetak kotak rapi. Tiap row = (teks_polos_untuk_ukur, teks_berwarna)."""
    print(f"{_CYAN}╭{'─' * lebar}╮{_RESET}")
    for polos, warna in rows:
        pad = " " * max(lebar - 2 - len(polos), 0)
        print(f"{_CYAN}│{_RESET} {warna}{pad} {_CYAN}│{_RESET}")
    print(f"{_CYAN}╰{'─' * lebar}╯{_RESET}")


def _info(label: str, nilai: str) -> tuple[str, str]:
    """Baris info berlabel untuk banner (label redup, nilai normal)."""
    polos = f"{label:<8}{nilai}"
    warna = f"{_DIM}{label:<8}{_RESET}{nilai}"
    return polos, warna


def _banner(handsfree: bool, lebar: int = 56) -> None:
    """Banner pembuka ber-box dengan info model, folder, dan mode."""
    ws = _pendekkan(str(WORKSPACE), lebar - 2 - 8)
    rows = [
        ("Voca  voice coding assistant",
         f"{_BOLD}Voca{_RESET}{_DIM}  voice coding assistant{_RESET}"),
        ("", ""),
        _info("model", config.QWEN_MODEL),
        _info("folder", ws),
        _info("mode", "hands-free (suara)" if handsfree else "teks"),
    ]
    print()
    _kotak(rows, lebar)


def _hint(teks: str) -> None:
    """Baris petunjuk redup di bawah banner."""
    print(f"{_DIM}{teks}{_RESET}\n")


def _kotak_input(petunjuk: str = "") -> str:
    """Prompt input ber-kotak ala Claude CLI: border + '›' di dalamnya.

    Border atas-bawah membungkus baris ketik; petunjuk redup tampil di bawah.
    """
    lebar = min(shutil.get_terminal_size((80, 20)).columns, 100)
    garis = "─" * (lebar - 2)
    print(f"\n{_CYAN}╭{garis}╮{_RESET}")
    try:
        teks = input(f"{_CYAN}│{_RESET} {_GREEN}›{_RESET} ")
    finally:
        print(f"{_CYAN}╰{garis}╯{_RESET}")
        if petunjuk:
            print(f"{_DIM}  {petunjuk}{_RESET}")
    return teks.strip()


class _BoldPrinter:
    """Cetak teks streaming sambil ubah **tebal** (markdown) jadi bold ANSI asli.

    Tahan terhadap '**' yang kepotong antar-chunk: '*' di ujung ditahan dulu
    sampai karakter berikutnya datang, baru diputuskan toggle bold atau bukan.
    """

    def __init__(self):
        self._pending = ""   # '*' tunggal yang belum pasti pasangannya
        self._bold = False

    def feed(self, teks: str) -> None:
        s = self._pending + teks
        self._pending = ""
        out = []
        i = 0
        while i < len(s):
            if s[i] == "*":
                if i + 1 == len(s):           # '*' di ujung -> tunggu chunk berikut
                    self._pending = "*"
                    break
                if s[i + 1] == "*":           # '**' -> toggle bold
                    self._bold = not self._bold
                    out.append(_BOLD if self._bold else _RESET)
                    i += 2
                    continue
                out.append("*")               # '*' tunggal -> apa adanya
            else:
                out.append(s[i])
            i += 1
        if out:
            sys.stdout.write("".join(out))
            sys.stdout.flush()

    def close(self) -> None:
        if self._pending:
            sys.stdout.write(self._pending)
            self._pending = ""
        if self._bold:                        # tutup bold yang belum sempat ditutup
            sys.stdout.write(_RESET)
            self._bold = False
        sys.stdout.flush()


SYSTEM_PROMPT = """Kamu adalah Voca, asisten coding berbasis suara.

Gaya bicara:
- Santai, singkat, langsung ke inti — kayak ngobrol sama teman kerja.
- Jangan bertele-tele. Kalau bisa 1 kalimat, jangan 3.
- Pakai bahasa Indonesia yang natural, bukan bahasa buku.
- Kalau lagi ngerjain sesuatu, kasih tahu singkat aja: "Oke, saya cek dulu filenya."
- Kalau selesai, langsung kasih hasilnya. Jangan terlalu banyak basa-basi.
- Hindari kata-kata formal seperti 'saya akan melakukan', 'berikut adalah', dll.

Format teks (gaya CLI, seperti Claude di terminal):
- JANGAN pakai emoji. Satu-satunya yang boleh: tanda centang untuk menandai
  selesai. Selain itu, tidak ada emoji sama sekali.
- Untuk penekanan, pakai **teks tebal**, bukan emoji atau huruf kapital.
- Bersih dan rapi — teks polos + bold sesekali. Hindari hiasan berlebihan.

Contoh gaya yang benar:
- "Oke, saya lihat dulu strukturnya." (bukan: "Baik, saya akan menganalisis struktur direktori terlebih dahulu.")
- "Nah, ini masalahnya — di baris 12 ada typo." (bukan: "Setelah melakukan analisis, ditemukan bahwa...")
- "Udah beres. Mau dijalankan sekarang?" (bukan: "Proses telah selesai dilaksanakan.")

Tools yang ada:
- list_files: lihat struktur folder.
- search_files: cari teks/kode cepat (seperti grep) — pakai ini buat menemukan
  sesuatu SEBELUM membaca file besar, lebih hemat.
- read_file: baca file (pakai start_line & end_line untuk baca sebagian saja).
- edit_file: ubah SEBAGIAN file yang sudah ada (ganti potongan teks). PAKAI INI
  untuk mengedit, BUKAN write_file — jauh lebih hemat & akurat.
- write_file: buat file BARU atau timpa total (jarang dipakai untuk edit).
- run_command: jalankan perintah terminal (output tampil live).
Aksi yang mengubah sistem (edit/tulis/command) otomatis minta konfirmasi user.

Cara kerja (PENTING):
- Kalau disuruh ngerjain sesuatu, LANGSUNG kerjakan via tool. Jangan banyak
  ngomong dulu di depan — paling satu kalimat pendek, atau langsung action.
- JANGAN jelasin rencana panjang lebar sebelum bertindak. Kerjakan dulu.
- Kerjakan tugas sampai TUNTAS dalam satu giliran. Kalau butuh beberapa langkah
  (mis. baca file lalu edit, atau edit beberapa file), lakukan langsung
  berurutan sampai selesai. JANGAN berhenti lalu menyuruh user mengetik
  langkah berikutnya kalau langkahnya sudah jelas — itu langkahmu, bukan tugas
  user. Jangan kasih menu pilihan "ketik ini atau itu".
- Untuk mengedit file yang sudah ada: kalau isinya BELUM ada di konteks, baca
  dulu (read_file / search_files) lalu langsung edit_file — otomatis, tanpa
  nanya. TAPI kalau isinya SUDAH ada (kamu sudah pernah baca file itu di sesi
  ini, ATAU baru saja kamu tulis/edit sendiri), JANGAN baca ulang — kamu sudah
  tahu isinya, langsung edit_file saja.
- Untuk file BESAR: jangan baca seluruhnya. Pakai search_files untuk menemukan
  lokasi yang relevan, lalu read_file dengan start_line/end_line — baca bagian
  pentingnya saja, bukan seluruh file.
- Hemat langkah: jangan panggil ulang tool yang hasilnya masih kamu punya di
  percakapan ini. Baca file SEKALI; pakai terus ingatan itu sampai file berubah.
- JANGAN menempel blok/cuplikan kode di balasanmu (mis. "Contoh lokasi: ```...```"
  atau memperlihatkan kode yang mau ditulis). Langsung lakukan lewat
  edit_file/write_file — diff perubahannya sudah otomatis ditampilkan ke user.
  Cukup jelaskan dengan kata-kata singkat, bukan dengan kode.
- Cuma berhenti untuk bertanya kalau benar-benar ambigu atau ada keputusan
  berisiko/merusak. Selain itu, lanjut saja sampai tugas beres.
- Simpan penjelasan lengkap untuk DI AKHIR — setelah semua aksi selesai, baru
  rangkum singkat: apa yang diubah & langkah berikutnya kalau ada.

Pola ideal: pahami (list/search/read) -> kerjakan (edit/write/run) -> rangkum.
Semua dalam satu giliran. Ingat, kalimatmu diucapkan lewat suara, jadi ringkas."""


def _ringkas_args(args: dict, batas: int = 80) -> str:
    """Ringkas argumen tool untuk dicetak — potong nilai panjang (mis. isi file)
    agar terminal tidak banjir teks dan langsung berhenti di prompt."""
    bagian = []
    for k, v in args.items():
        s = str(v).replace("\n", "\\n")
        if len(s) > batas:
            s = s[:batas] + f"…(+{len(s) - batas} char)"
        bagian.append(f"{k}={s}")
    return ", ".join(bagian)


def _estimasi_token(messages) -> int:
    """Perkiraan kasar jumlah token (heuristik karakter/token)."""
    total = 0
    for m in messages:
        total += len(str(m.get("content") or ""))
        for tc in (m.get("tool_calls") or []):
            total += len(str(tc.get("function", {}).get("arguments") or ""))
    return int(total / config.CHARS_PER_TOKEN)


def _pangkas_history(messages):
    """Batasi history biar hemat token & tak overflow context.

    Dua lapis: (1) batas jumlah pesan, (2) batas estimasi token. Pemotongan
    selalu per-giliran utuh dan menyisakan system di depan, sehingga pasangan
    tool_call/tool tak terputus (kalau terputus, DashScope menolak request).
    """
    if not messages:
        return
    system = messages[0]

    # Lapis 1: batas jumlah pesan.
    if len(messages) - 1 > config.MAX_HISTORY:  # -1 untuk pesan system
        ekor = messages[-config.MAX_HISTORY:]
        while ekor and ekor[0].get("role") != "user":
            ekor.pop(0)
        messages[:] = [system] + ekor

    # Lapis 2: batas token — buang giliran terlama (mulai dari pesan 'user').
    while _estimasi_token(messages) > config.MAX_HISTORY_TOKENS and len(messages) > 2:
        del messages[1]
        while len(messages) > 2 and messages[1].get("role") != "user":
            del messages[1]


# ---------------------------------------------------------------------------
# Simpan & lanjutkan sesi (resume)
# ---------------------------------------------------------------------------
def _session_path() -> Path:
    return WORKSPACE / config.SESSION_FILE


def _simpan_sesi(messages) -> None:
    """Simpan history ke file sesi (diam-diam; gagal tak mengganggu kerja)."""
    if not config.SESSION_ENABLED:
        return
    try:
        p = _session_path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(messages, ensure_ascii=False), encoding="utf-8")
    except Exception:
        pass


def _muat_sesi():
    """Muat sesi tersimpan kalau ada & valid; else None."""
    if not config.SESSION_ENABLED:
        return None
    try:
        p = _session_path()
        if not p.exists():
            return None
        data = json.loads(p.read_text(encoding="utf-8"))
        if isinstance(data, list) and any(m.get("role") == "user" for m in data):
            return data
    except Exception:
        pass
    return None


def _tanya_resume(messages) -> bool:
    """Tanya user apakah mau lanjut dari sesi sebelumnya."""
    giliran = sum(1 for m in messages if m.get("role") == "user")
    try:
        jawab = input(
            f"{_DIM}Ada sesi sebelumnya ({giliran} giliran). Lanjutkan? [y/N]{_RESET} "
        ).strip().lower()
    except (EOFError, KeyboardInterrupt):
        return False
    return jawab in ("y", "yes", "ya")


def _stream_satu_panggilan(client, messages):
    """Satu panggilan LLM streaming, dengan retry+backoff saat error koneksi.

    Aman untuk dicoba ulang: bila gagal SEBELUM ada teks tercetak/terucap,
    panggilan diulang dari awal. Kalau sudah terlanjur keluar sebagian (atau
    retry habis), error dilempar ke pemanggil. Return (narasi, tool_calls).
    """
    delay = config.LLM_RETRY_BASE_DELAY
    for percobaan in range(1, config.LLM_MAX_RETRIES + 1):
        text_parts = []
        tool_calls = {}          # tool call datang bertahap lewat stream (per index)
        sudah_keluar = False     # sudah ada teks tercetak/terucap?
        speaker = StreamSpeaker()
        pencetak = _BoldPrinter()  # **tebal** -> bold ANSI saat dicetak
        try:
            stream = client.chat.completions.create(
                model=config.QWEN_MODEL,
                messages=messages,
                tools=TOOLS_SCHEMA,
                temperature=config.QWEN_TEMPERATURE,
                stream=True,
            )
            print("\n", end="", flush=True)
            for chunk in stream:
                if not chunk.choices:
                    continue
                delta = chunk.choices[0].delta

                if getattr(delta, "content", None):
                    sudah_keluar = True
                    pencetak.feed(delta.content)
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
            pencetak.close()
            print()
            speaker.close()
            return "".join(text_parts), tool_calls

        except _TRANSIENT_ERRORS as e:
            pencetak.close()
            try:
                speaker.close()
            except Exception:
                pass
            # Sudah terlanjur keluar sebagian, atau percobaan terakhir -> menyerah.
            if sudah_keluar or percobaan == config.LLM_MAX_RETRIES:
                raise
            print(f"{_DIM}  koneksi bermasalah ({type(e).__name__}), "
                  f"coba lagi {percobaan}/{config.LLM_MAX_RETRIES - 1} "
                  f"dalam {delay:.0f}s…{_RESET}")
            time.sleep(delay)
            delay = min(delay * 2, 30)


def hubungkan_tool(client, messages):
    """Loop satu giliran: panggil model, eksekusi tool, ulangi sampai selesai."""
    _pangkas_history(messages)
    for _ in range(config.MAX_TOOL_ITERS):
        narasi, tool_calls = _stream_satu_panggilan(client, messages)

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
            print(f"\n{_DIM}  › {tc['name']}({_ringkas_args(args)}){_RESET}")

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

    # Batas iterasi tercapai tanpa selesai -> stop biar tak muter & boros token.
    print(f"\n{_DIM}  batas {config.MAX_TOOL_ITERS} langkah tercapai, berhenti dulu.{_RESET}")
    pesan_stop = ("Ini butuh banyak langkah, aku berhenti dulu biar nggak muter. "
                  "Kasih tahu mau lanjut ke bagian mana.")
    messages.append({"role": "assistant", "content": pesan_stop})
    speak(pesan_stop)


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

    print(f"\n{_CYAN}?{_RESET} {prompt}")
    speak(prompt + " Jawab ya atau tidak.")
    jawab = listen_auto().lower()
    print(f"{_DIM}(suara) jawaban:{_RESET} {jawab!r}")
    setuju = bool(set(re.findall(r"\w+", jawab)) & _KATA_YA)
    speak("Oke, saya lanjutkan." if setuju else "Baik, saya batalkan.")
    return setuju


# ---------------------------------------------------------------------------
# Mode interaksi
# ---------------------------------------------------------------------------
def run_text_mode(client, messages):
    """Mode teks: ketik perintah, atau 'v' + ENTER untuk bicara sekali."""
    _banner(handsfree=False)

    while True:
        try:
            perintah = _kotak_input("'v' + Enter untuk bicara  ·  'keluar' untuk berhenti")
        except (EOFError, KeyboardInterrupt):
            print(f"\n{_DIM}Sampai jumpa.{_RESET}")
            break

        if perintah.lower() in ("v", "suara", "voice"):
            try:
                from .listen import listen
                perintah = listen()
            except Exception as e:
                print(f"{_DIM}  input suara gagal: {e}{_RESET}")
                continue
            print(f"{_DIM}(suara){_RESET} {perintah}")

        if not perintah:
            continue
        if perintah.lower() in ("keluar", "exit", "quit"):
            print(f"{_DIM}Sampai jumpa.{_RESET}")
            break

        messages.append({"role": "user", "content": perintah})
        try:
            hubungkan_tool(client, messages)
        except Exception as e:
            print(f"\n{_RED}Error:{_RESET} {e}")
        _simpan_sesi(messages)


def run_handsfree_mode(client, messages):
    """Mode hands-free: dengar otomatis, ATAU ketik kapan saja (tekan ENTER)."""
    from .listen import listen_auto_atau_ketik

    set_confirm_handler(_voice_confirm)  # konfirmasi aksi lewat suara

    _banner(handsfree=True)
    _hint("ngomong langsung, atau tekan ENTER untuk ketik  ·  'berhenti' = keluar  ·  Ctrl+C")
    speak("Halo, saya siap membantu. Silakan bicara, atau ketik kalau mau.")

    while True:
        print(f"\n{_DIM}mendengarkan…  (ngomong, atau tekan ENTER untuk ketik){_RESET}")
        try:
            jenis, perintah = listen_auto_atau_ketik()
        except KeyboardInterrupt:
            speak("Sampai jumpa!")
            print(f"\n{_DIM}Sampai jumpa.{_RESET}")
            break

        # User tekan ENTER tanpa langsung mengetik -> tampilkan kotak input.
        if jenis == "ketik" and not perintah:
            try:
                perintah = _kotak_input("'berhenti' untuk keluar  ·  Enter kosong = batal")
            except (EOFError, KeyboardInterrupt):
                continue

        if not perintah:
            continue  # tidak terdengar suara / ketikan kosong -> dengar lagi
        label = "(ketik)" if jenis == "ketik" else "(suara)"
        print(f"{_DIM}{label}{_RESET} {perintah}")

        if _minta_keluar(perintah):
            speak("Baik, sampai jumpa!")
            print(f"{_DIM}Sampai jumpa.{_RESET}")
            break

        messages.append({"role": "user", "content": perintah})
        try:
            hubungkan_tool(client, messages)
        except Exception as e:
            print(f"\n{_RED}Error:{_RESET} {e}")
        _simpan_sesi(messages)


def main():
    if not config.QWEN_API_KEY:
        print(f"{_RED}Error:{_RESET} DASHSCOPE_API_KEY belum diset. "
              f"Salin .env.example ke .env, lalu isi key-mu.")
        sys.exit(1)

    client = OpenAI(api_key=config.QWEN_API_KEY, base_url=config.QWEN_BASE_URL)
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "system",
         "content": f"Struktur folder kerja saat ini ({WORKSPACE}):\n{list_files('.')}"},
    ]

    # Lanjutkan sesi sebelumnya kalau ada & user setuju.
    tersimpan = _muat_sesi()
    if tersimpan and _tanya_resume(tersimpan):
        messages = tersimpan
        print(f"{_DIM}Sesi dilanjutkan.{_RESET}")

    warmup()  # muat model suara di awal agar balasan pertama tidak tertunda

    # Mode hands-free kalau dijalankan dengan flag --voice / --suara.
    handsfree = any(a in ("--voice", "-v", "--handsfree", "--suara") for a in sys.argv[1:])
    if handsfree:
        run_handsfree_mode(client, messages)
    else:
        run_text_mode(client, messages)


if __name__ == "__main__":
    main()