//! voicebridge.rs — jembatan ke sidecar suara Python (arsitektur hybrid).
//!
//! Core Rust menjalankan satu proses Python (`voca.voice_server`) yang menjaga
//! model TTS/STT tetap warm, lalu bertukar perintah lewat protokol JSON
//! per-baris. Ini memberi suara yang mulus tanpa membebani build Rust dengan
//! FFI ML (whisper/piper) yang rapuh.
//!
//! Konfigurasi (env):
//!   VOCA_VOICE_PYTHON  executable python (default: "python3")
//!   VOCA_VOICE_HOME    folder berisi paket `voca` (di-set sebagai cwd +
//!                      PYTHONPATH; berguna saat dijalankan dari mana saja)

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

use crate::ui;

pub struct VoiceBridge {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl VoiceBridge {
    /// Jalankan sidecar Python & tunggu sampai siap. None kalau gagal start.
    pub fn start() -> Option<Self> {
        let py = std::env::var("VOCA_VOICE_PYTHON").unwrap_or_else(|_| "python3".to_string());
        let mut cmd = Command::new(&py);
        cmd.args(["-m", "voca.voice_server"]);
        if let Ok(home) = std::env::var("VOCA_VOICE_HOME") {
            cmd.current_dir(&home).env("PYTHONPATH", &home);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // status UI Python → terminal

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                ui::warn(&format!("gagal start sidecar suara ({py}): {e}"));
                return None;
            }
        };
        let stdin = child.stdin.take()?;
        let mut stdout = BufReader::new(child.stdout.take()?);

        // Tunggu baris kesiapan {"ready":true}.
        let mut line = String::new();
        match stdout.read_line(&mut line) {
            Ok(0) | Err(_) => {
                ui::warn("sidecar suara tidak merespons (cek instalasi Python 'voca').");
                let _ = child.kill();
                return None;
            }
            Ok(_) => {}
        }

        Some(VoiceBridge { child, stdin, stdout })
    }

    fn request(&mut self, req: Value) -> Option<Value> {
        self.stdin.write_all(req.to_string().as_bytes()).ok()?;
        self.stdin.write_all(b"\n").ok()?;
        self.stdin.flush().ok()?;
        let mut resp = String::new();
        match self.stdout.read_line(&mut resp) {
            Ok(0) | Err(_) => None,
            Ok(_) => serde_json::from_str(resp.trim()).ok(),
        }
    }

    /// Ucapkan teks (blokir sampai selesai diucapkan).
    pub fn speak(&mut self, text: &str, lang: &str) {
        let _ = self.request(json!({"cmd": "speak", "text": text, "lang": lang}));
    }

    /// Dengarkan mic (VAD, berhenti saat hening) → teks. "" kalau hening/gagal.
    pub fn listen(&mut self, lang: &str) -> String {
        match self.request(json!({"cmd": "listen", "lang": lang})) {
            Some(v) => v.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string(),
            None => String::new(),
        }
    }
}

impl Drop for VoiceBridge {
    fn drop(&mut self) {
        // Tutup stdin → sidecar keluar sendiri; pastikan tak jadi zombie.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
