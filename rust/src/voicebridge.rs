use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppEvent;

/// Perintah yang dikirim main-loop ke worker thread suara.
enum VoiceCmd {
    Listen(String),        // lang
    Speak(String, String), // text, lang
}

/// Handle non-blocking ke sidecar suara.
///
/// Sidecar Python (`voca.voice_server`) dijalankan sekali; sebuah OS-thread
/// khusus (`worker_loop`) yang memegang pipa stdout-nya menjalankan listen/speak
/// secara serial. Karena listen/speak berjalan di thread terpisah, main-loop TUI
/// tetap hidup & responsif terhadap keyboard selama mic merekam — itulah yang
/// memungkinkan tombol `t` (beralih ketik) bekerja saat sedang mendengarkan.
///
/// Hasil dikirim balik lewat `AppEvent` (VoiceResult / SpeakDone).
pub struct VoiceHandle {
    cmd_tx: Sender<VoiceCmd>,
    stdin:  Arc<Mutex<ChildStdin>>,
    child:  Child,
}

impl VoiceHandle {
    /// Jalankan sidecar & worker thread. `None` jika proses gagal start.
    pub fn start(app_tx: UnboundedSender<AppEvent>) -> Option<Self> {
        let py = std::env::var("VOCA_VOICE_PYTHON")
            .unwrap_or_else(|_| "python3".to_string());

        let mut cmd = Command::new(&py);
        cmd.args(["-m", "voca.voice_server"]);

        // Cari direktori yang mengandung paket `voca/` mulai dari CWD ke atas
        let home = std::env::var("VOCA_VOICE_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| find_voca_root(&std::env::current_dir().ok()?));

        if let Some(ref h) = home {
            cmd.current_dir(h).env("PYTHONPATH", h);
        }

        // PENTING: stderr sidecar JANGAN diwariskan ke terminal — TUI ratatui
        // memakai layar penuh, jadi log Python ("[mic] mendengarkan…", dst.) akan
        // membocori & merusak tampilan. Arahkan ke file log agar tetap bisa
        // di-debug tanpa mengganggu UI.
        let log_dir = dirs::cache_dir()
            .map(|d| d.join("voca"))
            .unwrap_or_else(std::env::temp_dir);
        let _ = std::fs::create_dir_all(&log_dir);
        let stderr_cfg = std::fs::File::create(log_dir.join("voice_sidecar.log"))
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());

        cmd.stdin(Stdio::piped())
           .stdout(Stdio::piped())
           .stderr(stderr_cfg);

        let mut child  = cmd.spawn().ok()?;
        let stdin      = child.stdin.take()?;
        let mut stdout = BufReader::new(child.stdout.take()?);

        // Tunggu baris `{"ready":true}`
        let mut line = String::new();
        match stdout.read_line(&mut line) {
            Ok(0) | Err(_) => { let _ = child.kill(); return None; }
            Ok(_) => {}
        }

        let stdin = Arc::new(Mutex::new(stdin));
        let (cmd_tx, cmd_rx) = mpsc::channel::<VoiceCmd>();

        let stdin_worker = stdin.clone();
        thread::spawn(move || worker_loop(cmd_rx, stdin_worker, stdout, app_tx));

        Some(VoiceHandle { cmd_tx, stdin, child })
    }

    /// Minta worker mulai satu siklus dengar (VAD: mulai saat ada suara,
    /// berhenti saat hening). Hasil datang sebagai `AppEvent::VoiceResult`.
    pub fn listen(&self, lang: &str) {
        let _ = self.cmd_tx.send(VoiceCmd::Listen(lang.to_string()));
    }

    /// Minta worker mengucapkan teks (TTS). Selesai → `AppEvent::SpeakDone`.
    pub fn speak(&self, text: &str, lang: &str) {
        let _ = self.cmd_tx.send(VoiceCmd::Speak(text.to_string(), lang.to_string()));
    }

    /// Batalkan listen yang sedang berjalan (dipakai saat user menekan `t`).
    ///
    /// Ditulis langsung ke stdin sidecar (bukan lewat channel) supaya menembus
    /// worker yang sedang blok membaca respons. Fire-and-forget: sidecar TIDAK
    /// membalas perintah ini, jadi tak mengganggu sinkronisasi protokol.
    pub fn cancel(&self) {
        if let Ok(mut s) = self.stdin.lock() {
            let _ = s.write_all(b"{\"cmd\":\"cancel\"}\n");
            let _ = s.flush();
        }
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ── Worker thread ────────────────────────────────────────────────────────────

fn worker_loop(
    cmd_rx: Receiver<VoiceCmd>,
    stdin:  Arc<Mutex<ChildStdin>>,
    mut stdout: BufReader<ChildStdout>,
    app_tx: UnboundedSender<AppEvent>,
) {
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            VoiceCmd::Listen(lang) => {
                if !write_cmd(&stdin, &json!({ "cmd": "listen", "lang": lang })) { break; }
                // Baca baris demi baris: pesan {"event":"vad"} = status real-time,
                // baris final memuat "text". Lanjut sampai dapat respons final.
                loop {
                    match read_value(&mut stdout) {
                        None => {
                            let _ = app_tx.send(AppEvent::VoiceResult(String::new()));
                            break;
                        }
                        Some(v) => {
                            if v.get("event").and_then(|e| e.as_str()) == Some("vad") {
                                let speech = v.get("speech").and_then(|b| b.as_bool()).unwrap_or(false);
                                let _ = app_tx.send(AppEvent::VadState(speech));
                                continue;
                            }
                            let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();
                            let _ = app_tx.send(AppEvent::VoiceResult(text));
                            break;
                        }
                    }
                }
            }
            VoiceCmd::Speak(text, lang) => {
                if !write_cmd(&stdin, &json!({ "cmd": "speak", "text": text, "lang": lang })) { break; }
                let resp = read_value(&mut stdout); // {"ok":true,"barged":bool}
                let barged = resp
                    .as_ref()
                    .and_then(|v| v.get("barged"))
                    .and_then(|b| b.as_bool())
                    .unwrap_or(false);
                if barged {
                    let _ = app_tx.send(AppEvent::BargeIn);
                } else {
                    let _ = app_tx.send(AppEvent::SpeakDone);
                }
            }
        }
    }
}

fn write_cmd(stdin: &Arc<Mutex<ChildStdin>>, v: &Value) -> bool {
    match stdin.lock() {
        Ok(mut s) => {
            s.write_all(v.to_string().as_bytes()).is_ok()
                && s.write_all(b"\n").is_ok()
                && s.flush().is_ok()
        }
        Err(_) => false,
    }
}

fn read_value(stdout: &mut BufReader<ChildStdout>) -> Option<Value> {
    let mut resp = String::new();
    match stdout.read_line(&mut resp) {
        Ok(0) | Err(_) => None,
        Ok(_)          => serde_json::from_str(resp.trim()).ok(),
    }
}

/// Naik hingga 5 level dari `start` mencari direktori yang punya `voca/__init__.py`.
fn find_voca_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..5 {
        if dir.join("voca").join("__init__.py").exists() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}
