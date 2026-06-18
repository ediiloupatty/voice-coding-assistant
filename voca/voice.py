"""
voice.py — "Mulut" si agent (Text-to-Speech).

Mesin utama: Piper (neural TTS LOKAL/offline, cepat, tanpa jeda jaringan).
Fallback: gTTS (online) bila model Piper tidak tersedia.

Sebelum dibacakan, teks dibersihkan dulu dari emoji, markdown, blok kode, dan
URL supaya yang terdengar hanya kalimat yang natural.

Dependensi sistem: aplay (alsa-utils) untuk memutar audio mentah dari Piper,
dan ffplay (ffmpeg) untuk fallback gTTS. Pitch-shift pakai ffmpeg rubberband.
"""

import os
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import threading

from . import config
from . import lang

_voice = None       # model Piper di-cache setelah dimuat sekali
_voice_path = None  # path model yang sedang dimuat (untuk deteksi ganti bahasa)
_syn = None         # SynthesisConfig di-cache


def _get_syn_config():
    """Konfigurasi sintesis Piper (tempo & volume) untuk kesan lebih lembut."""
    global _syn
    if _syn is None:
        from piper import SynthesisConfig
        _syn = SynthesisConfig(length_scale=config.VOICE_SPEED, volume=config.VOICE_VOLUME)
    return _syn


def _get_voice():
    """Muat model Piper untuk bahasa aktif (lazy + cache, muat ulang bila ganti)."""
    global _voice, _voice_path
    target = lang.piper_model()
    if _voice is None or _voice_path != target:
        from piper import PiperVoice
        _voice = PiperVoice.load(target)
        _voice_path = target
    return _voice


def _piper_available() -> bool:
    """True kalau model Piper untuk bahasa aktif benar-benar ada di disk."""
    p = lang.piper_model()
    return bool(p) and os.path.exists(p)


def _pitch_aktif() -> bool:
    """Pitch-shift dipakai hanya kalau diminta DAN ffmpeg tersedia."""
    return abs(config.VOICE_PITCH - 1.0) >= 0.01 and shutil.which("ffmpeg") is not None


def _ffmpeg_pitch_cmd(sr: int):
    """Perintah ffmpeg rubberband: geser nada, jaga timbre (formant=preserved)."""
    return ["ffmpeg", "-loglevel", "quiet",
            "-f", "s16le", "-ar", str(sr), "-ac", "1", "-i", "pipe:0",
            "-af", f"rubberband=pitch={config.VOICE_PITCH}:formant=preserved",
            "-f", "s16le", "-ar", str(sr), "-ac", "1", "pipe:1"]


class _AplayPlayer:
    """Pemutar Linux: tulis PCM ke aplay (opsional lewat ffmpeg untuk pitch)."""

    def __init__(self, sr: int):
        aplay = ["aplay", "-q", "-r", str(sr), "-f", "S16_LE", "-c", "1", "-t", "raw", "-"]
        if _pitch_aktif():
            self._ff = subprocess.Popen(_ffmpeg_pitch_cmd(sr),
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            self._aplay = subprocess.Popen(aplay, stdin=self._ff.stdout)
            self._ff.stdout.close()
            self._in, self._procs = self._ff.stdin, [self._ff, self._aplay]
        else:
            self._aplay = subprocess.Popen(aplay, stdin=subprocess.PIPE)
            self._in, self._procs = self._aplay.stdin, [self._aplay]

    def write(self, data: bytes):
        self._in.write(data)

    def close(self):
        try:
            self._in.close()
        except Exception:
            pass
        for p in self._procs:
            p.wait()


class _SoundDevicePlayer:
    """Pemutar lintas-platform (Windows/macOS) lewat sounddevice (PortAudio).

    Pitch-shift opsional: PCM -> ffmpeg -> dibaca thread -> stream audio.
    """

    def __init__(self, sr: int):
        import sounddevice as sd
        self._stream = sd.RawOutputStream(samplerate=sr, channels=1, dtype="int16")
        self._stream.start()
        self._ff = None
        if _pitch_aktif():
            self._ff = subprocess.Popen(_ffmpeg_pitch_cmd(sr),
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            self._reader = threading.Thread(target=self._pump, daemon=True)
            self._reader.start()

    def _pump(self):
        while True:
            data = self._ff.stdout.read(4096)
            if not data:
                break
            self._stream.write(data)

    def write(self, data: bytes):
        if self._ff:
            self._ff.stdin.write(data)
        else:
            self._stream.write(data)

    def close(self):
        if self._ff:
            try:
                self._ff.stdin.close()
            except Exception:
                pass
            self._reader.join()
            self._ff.wait()
        self._stream.stop()
        self._stream.close()


def _open_player(sr: int):
    """Pilih pemutar audio sesuai OS. Punya .write(bytes) dan .close()."""
    if sys.platform.startswith("linux") and shutil.which("aplay") is not None:
        return _AplayPlayer(sr)
    return _SoundDevicePlayer(sr)


# Ejaan fonetik kata Inggris umum -> agar Piper (model Indonesia) membacanya
# mendekati lafal Inggris. Hanya untuk SUARA; teks di layar tidak diubah.
_PHONETIC = {
    "file": "fail", "files": "fail",
    "error": "eror", "errors": "eror",
    "bug": "bag", "bugs": "bag", "debug": "dibag",
    "code": "kod", "coding": "koding",
    "commit": "komit", "commits": "komit",
    "command": "komand", "commands": "komand",
    "function": "fangsyen", "functions": "fangsyen",
    "update": "apdet", "updates": "apdet",
    "install": "instol", "deploy": "diploi",
    "python": "paiton", "cache": "kesh", "queue": "kiyu",
    "request": "rikues", "response": "rispons",
    "return": "ritern", "value": "valyu", "values": "valyu",
    "merge": "merj", "branch": "brench", "branches": "brenches",
    "save": "seiv", "default": "difolt", "range": "reinj",
    "true": "tru", "false": "fols", "null": "nal",
    "username": "yusernem", "password": "paswor",
}
_PHONETIC_RE = re.compile(
    r"\b(" + "|".join(sorted(_PHONETIC, key=len, reverse=True)) + r")\b",
    re.IGNORECASE,
)


def _eja_inggris(teks: str) -> str:
    """Ganti kata Inggris umum dengan ejaan fonetik Indonesia (per kata utuh)."""
    return _PHONETIC_RE.sub(lambda m: _PHONETIC[m.group(0).lower()], teks)


def _bersihkan_teks(teks: str) -> str:
    """Buang elemen yang tidak enak dibacakan: kode, emoji, markdown, URL."""
    teks = re.sub(r"```.*?```", " ", teks, flags=re.DOTALL)   # blok kode
    teks = teks.replace("`", "")                               # inline code (keep text, strip backticks)
    teks = re.sub(r"https?://\S+", " ", teks)                  # URL
    teks = re.sub(r"[#*_>`]", " ", teks)                       # penanda markdown
    teks = re.sub(r"^\s*[-•]\s*", "", teks, flags=re.MULTILINE)
    teks = re.sub(                                             # emoji & simbol
        r"[\U0001F000-\U0001FAFF\U00002600-\U000027BF\U0001F1E6-\U0001F1FF←-⇿⌀-⏿]",
        " ", teks,
    )
    teks = re.sub(r"\s+", " ", teks).strip()                  # rapikan spasi
    if lang.phonetic():
        teks = _eja_inggris(teks)                             # lafalkan kata Inggris
    return teks


def _piper_stream_play(teks: str) -> None:
    """Sintesis dengan Piper dan alirkan audio ke speaker, mulus tanpa jeda.

    Audio mentah (PCM 16-bit) ditulis ke pemutar potongan demi potongan begitu
    Piper menghasilkannya — suara mulai terdengar dalam ~0.2-0.4 detik.
    """
    voice = _get_voice()
    sr = voice.config.sample_rate

    player = _open_player(sr)
    try:
        for chunk in voice.synthesize(teks, _get_syn_config()):
            player.write(chunk.audio_int16_bytes)
    finally:
        player.close()


def _gtts_play(teks: str) -> None:
    """Fallback online: gTTS -> simpan mp3 -> putar dengan ffplay."""
    from gtts import gTTS

    path = None
    try:
        with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as f:
            path = f.name
        gTTS(text=teks, lang=lang.gtts()).save(path)
        subprocess.run(
            ["ffplay", "-nodisp", "-autoexit", "-loglevel", "quiet", path],
            check=False,
        )
    finally:
        if path:
            import os
            try:
                os.remove(path)
            except Exception:
                pass


def warmup() -> None:
    """Muat model Piper lebih awal supaya balasan pertama tidak tertunda."""
    if not config.VOICE_ENABLED:
        return
    try:
        _get_voice()
    except Exception:
        pass  # nanti fallback ke gTTS saat speak() dipanggil


def speak(teks: str) -> None:
    """Bacakan teks dengan suara. Aman: tidak menghentikan agent kalau gagal."""
    if not config.VOICE_ENABLED:
        return
    bersih = _bersihkan_teks(teks)
    if not bersih:
        return

    try:
        # Utama: Piper lokal (kalau model bahasa ini ada). Selain itu langsung gTTS.
        if _piper_available():
            try:
                _piper_stream_play(bersih)
                return
            except KeyboardInterrupt:
                raise
            except Exception as e:
                print(f"   [Piper gagal ({e}), pakai gTTS...]")
        _gtts_play(bersih)
    except KeyboardInterrupt:
        # Tekan Ctrl+C saat bicara = lewati suara, JANGAN matikan agent.
        print("\n   [narasi suara dilewati]")
    except Exception as e:
        # Suara cuma "kulit" — kalau gagal, agent tetap lanjut bekerja.
        print(f"   [TTS gagal, lanjut tanpa suara: {e}]")


# ---------------------------------------------------------------------------
# Speaker latar: bicara per kalimat SAMBIL teks mengalir, lewat SATU aliran
# aplay yang sama supaya tanpa jeda (Piper menyintesis lebih cepat dari main).
# ---------------------------------------------------------------------------
# Akhir kalimat: titik / tanya / seru / baris baru.
_SENT_END = set(".!?\n")
# Kalau kalimat kepanjangan tanpa tanda baca, paksa potong setelah sekian kata
# supaya potongan pertama tetap mulai cepat. (Bisa diubah sesuai selera.)
_FALLBACK_WORDS = 12


def _potong_kalimat(buf: str):
    """Ambil satu kalimat (atau ~_FALLBACK_WORDS kata) dari buffer.

    Return (potongan_atau_None, sisa_buffer).
    """
    for i, ch in enumerate(buf):
        if ch in _SENT_END:
            return buf[: i + 1], buf[i + 1:]
    kata = buf.split(" ")
    if len(kata) > _FALLBACK_WORDS:
        head = " ".join(kata[:_FALLBACK_WORDS])
        return head, buf[len(head):]
    return None, buf


class StreamSpeaker:
    """Membacakan teks per kalimat sambil teks masih mengalir, tanpa jeda.

    Semua audio ditulis ke SATU proses aplay → potongan menyambung mulus.

        sp = StreamSpeaker()
        sp.feed("Oke, saya cek dulu. ")   # mulai bicara begitu kalimat siap
        sp.feed("Lalu kerjakan tugasmu.")
        sp.close()                         # tunggu sampai selesai
    """

    def __init__(self):
        self.enabled = config.VOICE_ENABLED
        self._use_gtts = False  # mode fallback: kumpulkan teks, ucapkan via gTTS di close
        self._text = []         # penampung teks untuk mode gTTS
        self._buf = ""
        self._in_code = False   # sedang di dalam blok ``` ?
        self._fence_buf = ""    # tahan backtick yang mungkin kepotong antar-chunk
        if not self.enabled:
            return
        if not _piper_available():
            # Bahasa tanpa model Piper (mis. English) -> pakai gTTS saat close().
            self._use_gtts = True
            return
        try:
            self._voice = _get_voice()
        except Exception:
            # Model Piper gagal dimuat -> matikan speaker latar (agent tetap jalan).
            self.enabled = False
            return
        sr = self._voice.config.sample_rate
        self._player = _open_player(sr)
        self._q: "queue.Queue[str | None]" = queue.Queue()
        self._thread = threading.Thread(target=self._worker, daemon=True)
        self._thread.start()

    def _worker(self):
        while True:
            kalimat = self._q.get()
            if kalimat is None:
                break
            try:
                for audio in self._voice.synthesize(kalimat, _get_syn_config()):
                    self._player.write(audio.audio_int16_bytes)
            except Exception:
                pass  # satu kalimat gagal -> lewati, jangan ganggu kerja agent

    def _enqueue(self, teks: str):
        bersih = _bersihkan_teks(teks)
        if bersih:
            self._q.put(bersih)

    def _buang_kode(self, teks: str) -> str:
        """Buang isi blok ``` dari aliran (stateful) supaya kode tak diucapkan.

        Blok kode sering berisi banyak baris; pemotong kalimat memecahnya per
        baris sehingga penyaring biasa tak melihat pasangan ``` utuh. Di sini
        status 'dalam kode' dijaga lintas-chunk, jadi seluruh isi blok dilewati.
        """
        s = self._fence_buf + teks
        self._fence_buf = ""
        out = []
        i, n = 0, len(s)
        while i < n:
            if s[i] == "`":
                j = i
                while j < n and s[j] == "`":
                    j += 1
                count = j - i
                if j == n and count < 3:           # mungkin ``` kepotong -> tahan
                    self._fence_buf = "`" * count
                    break
                if count >= 3:                      # pagar blok kode -> toggle
                    self._in_code = not self._in_code
                    i = j
                    continue
                if not self._in_code:               # 1-2 backtick: biarkan (inline)
                    out.append("`" * count)
                i = j
                continue
            if not self._in_code:
                out.append(s[i])
            i += 1
        return "".join(out)

    def feed(self, teks: str):
        """Suapkan potongan teks dari stream; kalimat utuh langsung diucapkan."""
        if not self.enabled:
            return
        if self._use_gtts:
            self._text.append(teks)      # kumpulkan dulu; diucapkan sekali di close()
            return
        prosa = self._buang_kode(teks)   # buang blok kode sebelum diucapkan
        if not prosa:
            return
        self._buf += prosa
        while True:
            kalimat, self._buf = _potong_kalimat(self._buf)
            if kalimat is None:
                break
            self._enqueue(kalimat)

    def close(self):
        """Ucapkan sisa buffer lalu tunggu seluruh suara selesai."""
        if not self.enabled:
            return
        if self._use_gtts:
            # Mode fallback: bersihkan seluruh teks lalu ucapkan sekali via gTTS.
            full = _bersihkan_teks("".join(self._text))
            self._text = []
            if full:
                try:
                    _gtts_play(full)
                except Exception:
                    pass  # gagal suara tak boleh mengganggu kerja agent
            return
        if self._buf.strip():
            self._enqueue(self._buf)
        self._buf = ""
        self._q.put(None)
        self._thread.join()
        self._player.close()


if __name__ == "__main__":
    # Tes cepat: python -m voca.voice
    speak("Halo! Saya Voca, coding assistant kamu. Sekarang suara saya berjalan "
          "lokal dengan Piper, jadi jauh lebih cepat dan mengalir tanpa jeda.")
