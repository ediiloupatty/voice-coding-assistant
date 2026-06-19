"""Tes untuk voca/listen.py — filter halusinasi & gerbang energi (tanpa mic/model).

Di-skip otomatis kalau dependensi audio (sounddevice/faster-whisper) tak ada,
mis. di CI yang ringan.
"""

import pytest

pytest.importorskip("sounddevice")
pytest.importorskip("faster_whisper")

import numpy as np  # noqa: E402

from voca import config, listen  # noqa: E402


def test_is_halusinasi_frasa_youtube():
    assert listen._is_halusinasi("Terima kasih telah menonton.")
    assert listen._is_halusinasi("terimakasih karena menonton")
    assert listen._is_halusinasi("Thank you for watching")


def test_is_halusinasi_ucapan_asli_lolos():
    assert not listen._is_halusinasi("buatkan fungsi penjumlahan")
    assert not listen._is_halusinasi("baca file main.jsx")


def test_terlalu_hening(monkeypatch):
    monkeypatch.setattr(config, "MIN_SPEECH_RMS", 0.01)
    assert listen._terlalu_hening(np.zeros(16000, dtype="float32"))
    assert listen._terlalu_hening(None)
    assert not listen._terlalu_hening(np.full(16000, 0.2, dtype="float32"))
