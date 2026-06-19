//! Voca (Rust) — CLI asisten coding: chat teks + tool-calling + suara opsional.
//!
//! Suara ditangani oleh sidecar Python (voca.voice_server) demi kemulusan;
//! core (chat + tool) tetap Rust murni → startup instan + 1 binary.
//!
//! Mode (default: SUARA penuh / hands-free — dengar mic + ucapkan jawaban):
//!   voca            mode suara penuh (listen + speak)
//!   --text / -t     mode teks murni (ketik, tanpa suara)
//!   --voice / -v    ketik input, jawaban diucapkan (speak saja)
//!   --listen / -l   sama dengan default (listen + speak)
//!   --say "teks"    ucapkan teks lalu keluar (uji sidecar suara)
//!   --version / -V  cetak versi

mod agent;
mod config;
mod llm;
mod provider;
mod tools;
mod ui;
mod voicebridge;

use anyhow::{Context, Result};
use voicebridge::VoiceBridge;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("voca {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    config::load();
    let lang = std::env::var("VOCA_LANG").unwrap_or_else(|_| "en".to_string());

    // --say "teks": uji sidecar suara lalu keluar (tak perlu API key).
    if let Some(i) = args.iter().position(|a| a == "--say") {
        let teks = args.get(i + 1).cloned().unwrap_or_default();
        if teks.is_empty() {
            ui::error("pakai: voca --say \"teks yang diucapkan\"");
            return Ok(());
        }
        match VoiceBridge::start() {
            Some(mut b) => b.speak(&teks, &lang),
            None => ui::error("sidecar suara tidak tersedia (cek instalasi Python 'voca')."),
        }
        return Ok(());
    }

    let limits = config::Limits::from_env();
    let code = provider::default_code();
    let mut prov = provider::by_code(&code).context("provider tidak dikenal")?;

    let key = config::ensure_api_key(prov.code, prov.name)?;
    prov.api_key = Some(key);

    // Tentukan mode. Default = SUARA penuh (hands-free). --text mematikannya.
    let has = |names: &[&str]| args.iter().any(|a| names.contains(&a.as_str()));
    let (listen, speak) = if has(&["--text", "-t"]) {
        (false, false) // teks murni
    } else if has(&["--voice", "-v"]) {
        (false, true) // ketik, jawaban diucapkan
    } else {
        (true, true) // default & --listen: dengar mic + ucapkan
    };

    let mode = if listen {
        "suara (hands-free)"
    } else if speak {
        "ketik + suara"
    } else {
        "teks"
    };

    // Masuk mode TUI (bar input ter-pin di dasar). Guard memulihkan terminal
    // saat keluar normal/panic.
    let (w, h) = ui::tui_enter();
    let _guard = TuiGuard(h);
    ui::banner(prov.name, &prov.model, &lang.to_uppercase(), mode);

    let client = reqwest::Client::builder()
        .build()
        .context("gagal membuat HTTP client")?;

    let voice_opts = agent::VoiceOpts { speak, listen, lang };
    agent::run(client, prov, limits, voice_opts, w, h).await
}

/// Pulihkan scroll-region/terminal saat Voca selesai (atau panic unwind).
struct TuiGuard(usize);
impl Drop for TuiGuard {
    fn drop(&mut self) {
        ui::tui_leave(self.0);
    }
}
