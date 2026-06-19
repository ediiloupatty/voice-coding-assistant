//! Voca (Rust) — CLI asisten coding: chat teks + tool-calling + suara opsional.
//!
//! Suara ditangani oleh sidecar Python (voca.voice_server) demi kemulusan;
//! core (chat + tool) tetap Rust murni → startup instan + 1 binary.
//!
//! Flag:
//!   --voice / -v    ucapkan jawaban (TTS via sidecar)
//!   --listen / -l   input lewat mic (STT via sidecar; implikasi --voice)
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

    // Tentukan mode suara.
    let listen = args.iter().any(|a| a == "--listen" || a == "-l");
    let speak = listen || args.iter().any(|a| a == "--voice" || a == "-v");

    ui::banner(prov.name, &prov.model);

    let client = reqwest::Client::builder()
        .build()
        .context("gagal membuat HTTP client")?;

    let voice_opts = agent::VoiceOpts { speak, listen, lang };
    agent::run(client, prov, limits, voice_opts).await
}
