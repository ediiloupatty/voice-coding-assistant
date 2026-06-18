"""
agent.py — "Otak" si Voca.

Memakai model Qwen via DashScope (endpoint OpenAI-compatible).

Alur kerja:
  1. User memberi perintah (ketik atau suara).
  2. Qwen memikirkan langkah & memanggil tools (list_files, read_file,
     write_file, run_command) untuk menganalisis folder dan mengerjakan tugas.
  3. Setiap aksi yang mengubah sistem minta konfirmasi (keyboard / suara).
  4. Model menarasikan progres secara real-time (teks + suara Piper).

Dua mode: hands-free (default) dan teks murni (`--text`).
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
from . import lang
from .tools import TOOLS_SCHEMA, TOOL_FUNCTIONS, WORKSPACE, list_files, set_confirm_handler
from .voice import StreamSpeaker, warmup, speak

from rich.markdown import Markdown
from rich.live import Live

from . import ui
from .ui import console


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
- EKSPLORASI DULU sebelum memilih aset: kalau user minta ganti/pakai sesuatu
  yang ada pilihannya di folder (icon, gambar, suara, tema, font, dsb.) dan
  kamu BELUM tahu isinya — WAJIB list_files folder itu dulu. Baru setelah tahu
  daftar pilihannya, pilih yang paling cocok atau tanyakan kalau benar-benar
  tidak jelas. JANGAN menebak nama file atau langsung edit tanpa cek dulu.
  Contoh: user bilang "ganti icon, ada di folder assets/icons" → list_files
  assets/icons dulu, lihat apa yang ada, baru edit.
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

        # Fase 1: spinner "berpikir" (transient — hilang begitu jawaban mulai).
        console.print()
        spinner = Live(ui.spinner_berpikir(), console=console,
                       refresh_per_second=12, transient=True)
        spinner.start()
        body = None              # Live untuk isi jawaban (Markdown), dibuat saat token pertama

        def _tutup_live():
            (body or spinner).stop()

        try:
            stream = client.chat.completions.create(
                model=config.QWEN_MODEL,
                messages=messages,
                tools=TOOLS_SCHEMA,
                temperature=config.QWEN_TEMPERATURE,
                stream=True,
            )
            for chunk in stream:
                if not chunk.choices:
                    continue
                delta = chunk.choices[0].delta

                if getattr(delta, "content", None):
                    if not sudah_keluar:
                        # Token pertama: buang spinner, cetak penanda, mulai render isi.
                        spinner.stop()
                        ui.header_jawaban()
                        body = Live(Markdown(""), console=console,
                                    refresh_per_second=15, transient=False)
                        body.start()
                    sudah_keluar = True
                    text_parts.append(delta.content)
                    speaker.feed(delta.content)
                    body.update(Markdown("".join(text_parts)))

                for tc in (getattr(delta, "tool_calls", None) or []):
                    slot = tool_calls.setdefault(tc.index, {"id": "", "name": "", "args": ""})
                    if tc.id:
                        slot["id"] = tc.id
                    if tc.function and tc.function.name:
                        slot["name"] = tc.function.name
                    if tc.function and tc.function.arguments:
                        slot["args"] += tc.function.arguments

            _tutup_live()
            speaker.close()
            return "".join(text_parts), tool_calls

        except _TRANSIENT_ERRORS as e:
            _tutup_live()
            try:
                speaker.close()
            except Exception:
                pass
            # Sudah terlanjur keluar sebagian, atau percobaan terakhir -> menyerah.
            if sudah_keluar or percobaan == config.LLM_MAX_RETRIES:
                raise
            ui.info(f"  koneksi bermasalah ({type(e).__name__}), "
                    f"coba lagi {percobaan}/{config.LLM_MAX_RETRIES - 1} "
                    f"dalam {delay:.0f}s…")
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
            console.print()
            ui.baris_tool(tc["name"], _ringkas_args(args))

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
    ui.info(f"\n  batas {config.MAX_TOOL_ITERS} langkah tercapai, berhenti dulu.")
    pesan_stop = ("Ini butuh banyak langkah, aku berhenti dulu biar nggak muter. "
                  "Kasih tahu mau lanjut ke bagian mana.")
    messages.append({"role": "assistant", "content": pesan_stop})
    speak(pesan_stop)


# ---------------------------------------------------------------------------
# Pengenalan ucapan ya/tidak & kata berhenti (untuk mode suara)
# ---------------------------------------------------------------------------
# Kata-kata dibuat gabungan ID+EN supaya konfirmasi & 'berhenti' jalan di kedua bahasa.
_KATA_YA = {"ya", "iya", "yes", "boleh", "lanjut", "setuju", "oke", "ok",
            "gas", "silakan", "jalan", "lakukan",
            "yeah", "yep", "yup", "sure", "go", "proceed", "okay"}
_KATA_TIDAK = {"tidak", "jangan", "batal", "bukan", "no", "nggak", "ngga", "gk", "batalkan",
               "nope", "cancel", "dont", "stop"}
_KATA_STOP = {"berhenti", "keluar", "stop", "udahan", "udah", "quit", "exit", "bye"}


def _set_bahasa(code: str, messages) -> None:
    """Ganti bahasa aktif: perbarui directive untuk LLM lalu beri tahu user (teks + suara)."""
    lang.set(code)
    messages[0]["content"] = SYSTEM_PROMPT + "\n\n" + lang.directive()
    pesan = lang.switched_msg()
    console.print(f"\n[accent.hi]{ui.SIGIL}[/] {pesan}")
    speak(pesan)


def _cek_ganti_bahasa(perintah: str, messages) -> bool:
    """Tangani perintah ganti bahasa. Return True kalau perintah memang soal bahasa
    (caller harus skip giliran, tidak mengirim ke LLM)."""
    code = lang.detect_command(perintah)
    if not code:
        return False
    if code != lang.code():
        _set_bahasa(code, messages)
    else:
        console.print(f"[muted]Sudah pakai {lang.name()}.[/muted]")
    return True


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

    ui.tanya_konfirmasi_suara(prompt)
    speak(prompt + " Jawab ya atau tidak.")
    jawab = listen_auto().lower()
    ui.info(f"(suara) jawaban: {jawab!r}")
    
    kata_jawaban = set(re.findall(r"\w+", jawab))
    if kata_jawaban & _KATA_TIDAK:
        setuju = False
    else:
        setuju = bool(kata_jawaban & _KATA_YA)
        
    speak("Oke, saya lanjutkan." if setuju else "Baik, saya batalkan.")
    return setuju


# ---------------------------------------------------------------------------
# Mode interaksi
# ---------------------------------------------------------------------------
def run_text_mode(client, messages):
    """Mode teks: ketik perintah, atau 'v' + ENTER untuk bicara sekali."""
    ui.banner(config.QWEN_MODEL, WORKSPACE, "teks")

    while True:
        try:
            perintah = ui.kotak_input(
                f"'v' + Enter untuk bicara  {ui.DOT}  'english'/'indonesia' ganti bahasa"
                f"  {ui.DOT}  'keluar' berhenti")
        except (EOFError, KeyboardInterrupt):
            ui.selesai()
            break

        if perintah.lower() in ("v", "suara", "voice"):
            try:
                from .listen import listen
                perintah = listen()
            except Exception as e:
                ui.info(f"  input suara gagal: {e}")
                continue
            if perintah:
                ui.pesan_user(perintah)

        if not perintah:
            continue
        if _cek_ganti_bahasa(perintah, messages):
            _simpan_sesi(messages)
            continue
        if perintah.lower() in ("keluar", "exit", "quit"):
            ui.selesai()
            break

        messages.append({"role": "user", "content": perintah})
        try:
            hubungkan_tool(client, messages)
        except Exception as e:
            ui.error(e)
        _simpan_sesi(messages)


def run_handsfree_mode(client, messages):
    """Mode hands-free: dengar otomatis, ATAU ketik kapan saja (tekan ENTER)."""
    from .listen import _rekam_atau_ketik, transcribe

    set_confirm_handler(_voice_confirm)  # konfirmasi aksi lewat suara

    ui.banner(config.QWEN_MODEL, WORKSPACE, "hands-free (suara)")
    ui.hint(f"ngomong / ketik perintah  {ui.DOT}  'english'/'indonesia' ganti bahasa"
            f"  {ui.DOT}  'berhenti' keluar")
    speak("Halo, saya siap membantu. Silakan bicara, atau ketik kalau mau.")

    H = max(15, shutil.get_terminal_size().lines)
    W = shutil.get_terminal_size().columns

    try:
        # Pindahkan kursor ke bottom of scroll region (H-3)
        print(f"\033[1;{H-3}r", end="", flush=True)
        print(f"\033[{H-3};1H", end="", flush=True)

        while True:
            # Dapatkan ukuran terminal terbaru jika di-resize
            H = max(15, shutil.get_terminal_size().lines)
            W = shutil.get_terminal_size().columns

            # Pastikan scrolling region aktif untuk H-3
            print(f"\033[1;{H-3}r", end="", flush=True)

            # Gambar bar status di bagian paling bawah
            ui.status_bar(H, W, "dengerin")

            try:
                jenis, data = _rekam_atau_ketik()
            except KeyboardInterrupt:
                break

            # Reset warna (grey bg dari input bar) & kembalikan kursor ke scroll region
            print(f"\033[0m\033[{H-3};1H", end="", flush=True)

            if jenis == "ketik":
                perintah = data
            elif jenis == "suara":
                if data is None:
                    continue
                ui.status_bar(H, W, "transkripsi")
                perintah = transcribe(data)
            else:
                continue

            if not perintah:
                continue

            # Cetak perintah user di scroll region (echo)
            ui.pesan_user(perintah)

            # Ganti bahasa? tangani langsung tanpa lewat LLM.
            if _cek_ganti_bahasa(perintah, messages):
                _simpan_sesi(messages)
                continue

            if _minta_keluar(perintah):
                speak("Baik, sampai jumpa!")
                ui.selesai()
                break

            ui.status_bar(H, W, "berpikir")
            # Kembalikan kursor ke scroll region
            print(f"\033[{H-3};1H", end="", flush=True)

            messages.append({"role": "user", "content": perintah})
            try:
                hubungkan_tool(client, messages)
            except Exception as e:
                ui.error(e)
            _simpan_sesi(messages)

    finally:
        # Reset warna + scroll region ke normal, kursor ke baris terakhir
        print(f"\033[0m\033[r\033[{H};1H\n", end="", flush=True)


def main():
    if not config.QWEN_API_KEY:
        ui.error("DASHSCOPE_API_KEY belum diset. Salin .env.example ke .env, lalu isi key-mu.")
        sys.exit(1)

    client = OpenAI(api_key=config.QWEN_API_KEY, base_url=config.QWEN_BASE_URL)
    messages = [
        {"role": "system", "content": SYSTEM_PROMPT + "\n\n" + lang.directive()},
        {"role": "system",
         "content": f"Struktur folder kerja saat ini ({WORKSPACE}):\n{list_files('.')}"},
    ]

    # Secara default jalankan mode hands-free (suara).
    # Gunakan flag --text untuk mode ketik murni jika dibutuhkan.
    text_mode = any(a in ("--text", "-t") for a in sys.argv[1:])

    # Jalankan seluruh sesi di layar terpisah agar terminal tetap bersih.
    ui.buka_layar()
    try:
        # Lanjutkan sesi sebelumnya kalau ada & user setuju.
        tersimpan = _muat_sesi()
        if tersimpan:
            giliran = sum(1 for m in tersimpan if m.get("role") == "user")
            if ui.tanya_resume(giliran):
                messages = tersimpan
                ui.info("Sesi dilanjutkan.")

        warmup()  # muat model suara di awal agar balasan pertama tidak tertunda

        if text_mode:
            run_text_mode(client, messages)
        else:
            run_handsfree_mode(client, messages)
    except KeyboardInterrupt:
        pass  # Ctrl+C = keluar bersih, tanpa traceback berantakan
    finally:
        ui.tutup_layar()  # kembalikan terminal ke kondisi semula


if __name__ == "__main__":
    main()