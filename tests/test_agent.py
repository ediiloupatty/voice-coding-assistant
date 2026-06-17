"""Tes untuk voca/agent.py — ringkas args, trim history, estimasi token, sesi."""

import io
import sys

import pytest

from voca import agent, config


# --- util tiruan untuk tes retry streaming ---------------------------------
class _Boom(Exception):
    """Error sementara palsu untuk menguji retry."""


def _chunk(teks):
    delta = type("D", (), {"content": teks, "tool_calls": None})()
    choice = type("Ch", (), {"delta": delta})()
    return type("Chunk", (), {"choices": [choice]})()


class _FlakyClient:
    """Client palsu: gagal `gagal_n` kali dulu, lalu sukses balas 'halo'."""

    def __init__(self, gagal_n):
        self.gagal_n = gagal_n
        self.calls = 0
        self.chat = self
        self.completions = self

    def create(self, **_kw):
        self.calls += 1
        if self.calls <= self.gagal_n:
            raise _Boom("koneksi putus")
        return iter([_chunk("halo")])


def _render_bold(chunks):
    """Jalankan _BoldPrinter atas potongan stream, kembalikan teks tercetak."""
    buf, old = io.StringIO(), sys.stdout
    sys.stdout = buf
    try:
        bp = agent._BoldPrinter()
        for c in chunks:
            bp.feed(c)
        bp.close()
    finally:
        sys.stdout = old
    return buf.getvalue()


def test_bold_satu_chunk():
    assert _render_bold(["ini **tebal** ya"]) == f"ini {agent._BOLD}tebal{agent._RESET} ya"


def test_bold_terpotong_antar_chunk():
    assert _render_bold(["halo *", "* x *", "* y"]) == \
        f"halo {agent._BOLD} x {agent._RESET} y"


def test_bintang_tunggal_apa_adanya():
    assert _render_bold(["a * b"]) == "a * b"


def test_bold_belum_ditutup_direset():
    assert _render_bold(["**belum tutup"]) == f"{agent._BOLD}belum tutup{agent._RESET}"


def test_ringkas_args_potong_panjang():
    out = agent._ringkas_args({"content": "x" * 200}, batas=20)
    assert "char)" in out and len(out) < 200


def test_pendekkan():
    assert agent._pendekkan("ab", 4) == "ab"
    dipendek = agent._pendekkan("abcdefgh", 4)
    assert dipendek.startswith("…") and dipendek.endswith("fgh")


def test_estimasi_token():
    msgs = [{"role": "user", "content": "a" * 35}]
    assert agent._estimasi_token(msgs) == int(35 / config.CHARS_PER_TOKEN)


def test_estimasi_token_termasuk_tool_calls():
    msgs = [{"role": "assistant", "content": "",
             "tool_calls": [{"function": {"arguments": "y" * 35}}]}]
    assert agent._estimasi_token(msgs) > 0


def test_pangkas_batas_pesan(monkeypatch):
    monkeypatch.setattr(config, "MAX_HISTORY", 4)
    monkeypatch.setattr(config, "MAX_HISTORY_TOKENS", 10 ** 9)
    msgs = [{"role": "system", "content": "s"}]
    for i in range(5):
        msgs.append({"role": "user", "content": f"u{i}"})
        msgs.append({"role": "assistant", "content": f"a{i}"})
    agent._pangkas_history(msgs)
    assert msgs[0]["role"] == "system"
    assert msgs[1]["role"] == "user"          # mulai di user -> pasangan utuh
    assert len(msgs) - 1 <= config.MAX_HISTORY


def test_pangkas_batas_token(monkeypatch):
    monkeypatch.setattr(config, "MAX_HISTORY", 10 ** 9)
    monkeypatch.setattr(config, "MAX_HISTORY_TOKENS", 30)
    monkeypatch.setattr(config, "CHARS_PER_TOKEN", 1.0)
    msgs = [{"role": "system", "content": "s"}]
    for i in range(8):
        msgs.append({"role": "user", "content": "uuuuu"})
        msgs.append({"role": "assistant", "content": "aaaaa"})
    agent._pangkas_history(msgs)
    assert agent._estimasi_token(msgs) <= 30
    assert msgs[0]["role"] == "system"


def test_sesi_simpan_muat(tmp_path, monkeypatch):
    monkeypatch.setattr(agent, "WORKSPACE", tmp_path)
    monkeypatch.setattr(config, "SESSION_ENABLED", True)
    monkeypatch.setattr(config, "SESSION_FILE", ".voca/session.json")
    data = [{"role": "system", "content": "s"}, {"role": "user", "content": "hi"}]
    agent._simpan_sesi(data)
    assert agent._muat_sesi() == data


def test_sesi_none_saat_disabled(tmp_path, monkeypatch):
    monkeypatch.setattr(agent, "WORKSPACE", tmp_path)
    monkeypatch.setattr(config, "SESSION_ENABLED", False)
    assert agent._muat_sesi() is None


def test_stream_retry_lalu_sukses(monkeypatch):
    monkeypatch.setattr(config, "VOICE_ENABLED", False)      # tanpa audio
    monkeypatch.setattr(config, "LLM_RETRY_BASE_DELAY", 0)   # tanpa jeda nyata
    monkeypatch.setattr(config, "LLM_MAX_RETRIES", 4)
    monkeypatch.setattr(agent, "_TRANSIENT_ERRORS", (_Boom,))
    client = _FlakyClient(gagal_n=2)
    narasi, tool_calls = agent._stream_satu_panggilan(client, [{"role": "user", "content": "hi"}])
    assert narasi == "halo"
    assert tool_calls == {}
    assert client.calls == 3                                  # 2 gagal + 1 sukses


def test_stream_retry_menyerah_setelah_batas(monkeypatch):
    monkeypatch.setattr(config, "VOICE_ENABLED", False)
    monkeypatch.setattr(config, "LLM_RETRY_BASE_DELAY", 0)
    monkeypatch.setattr(config, "LLM_MAX_RETRIES", 3)
    monkeypatch.setattr(agent, "_TRANSIENT_ERRORS", (_Boom,))
    client = _FlakyClient(gagal_n=99)
    with pytest.raises(_Boom):
        agent._stream_satu_panggilan(client, [{"role": "user", "content": "hi"}])
    assert client.calls == 3                                  # berhenti tepat di batas
