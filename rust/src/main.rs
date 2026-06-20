mod agent;
mod app;
mod config;
mod llm;
mod provider;
mod tools;
mod ui;
mod voicebridge;

use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream};
use futures_util::StreamExt;
use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use ratatui::crossterm::execute;
use std::env;
use std::panic;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, MissedTickBehavior};

use crate::app::{App, AppEvent};
use crate::voicebridge::VoiceHandle;

#[tokio::main]
async fn main() -> Result<()> {
    // Pulihkan terminal saat panic agar tidak corrupt
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = ratatui::restore();
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        original_hook(info);
    }));

    // Flag versi
    let args: Vec<String> = env::args().collect();
    if args.get(1).map_or(false, |a| matches!(a.as_str(), "--version" | "-v")) {
        println!("Voca v{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Muat .env + config user (~/.config/voca/config.json)
    config::load();

    // ── Parse CLI ────────────────────────────────────────────────────────────
    let mut voice_listen = true;
    let mut voice_speak  = true;
    // Bahasa default Inggris (selaras dengan UI). Bisa di-override via env
    // VOCA_LANG=id atau argumen --lang id, atau /lan saat runtime.
    let mut voice_lang   = env::var("VOCA_LANG").unwrap_or_else(|_| "en".to_string());
    let mut initial_text: Option<String> = None;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--no-voice" | "--text-only" => { voice_listen = false; voice_speak = false; }
            "--voice"  => { voice_listen = true; voice_speak = true; }
            "--listen" => voice_listen = true,
            "--speak" | "--say" => voice_speak = true,
            "--lang" if i + 1 < args.len() => { voice_lang = args[i + 1].clone(); i += 1; }
            "-t" | "--text" if i + 1 < args.len() => { initial_text = Some(args[i + 1].clone()); i += 1; }
            _ => {}
        }
        i += 1;
    }

    // ── Provider LLM ─────────────────────────────────────────────────────────
    let code = env::var("VOCA_MODEL")
        .or_else(|_| env::var("VOCA_PROVIDER"))
        .unwrap_or_else(|_| "qwen".to_string());

    let mut prov = provider::by_code(&code)
        .unwrap_or_else(|| provider::all().into_iter().next().unwrap());
    prov.api_key = Some(config::ensure_api_key(prov.code, prov.name)?);

    let limits = config::Limits::from_env();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;

    // ── Trust folder (gerbang keamanan) ──────────────────────────────────────
    // Cek status; bila belum tepercaya, modal "Trust this folder?" ditampilkan
    // di dalam TUI (lihat di bawah) — bukan prompt teks polos.
    let trusted = config::is_trusted_cwd();

    // ── Channel & State ──────────────────────────────────────────────────────
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    let voice_handle = if voice_listen || voice_speak {
        // Sidecar memuat model TTS/STT dulu (bisa beberapa detik di run pertama).
        // TUI belum aktif, jadi beri umpan balik sederhana di sini.
        println!("  Loading voice models (first run may take a moment)…");
        VoiceHandle::start(tx.clone())
    } else {
        None
    };

    let voice_opts = app::VoiceOpts { listen: voice_listen, speak: voice_speak, lang: voice_lang };
    let mut app = App::new(prov, voice_opts, limits, client, tx.clone());
    app.voice_handle = voice_handle;
    app.trusted = trusted;

    // Mode suara hands-free → tegaskan gaya obrolan dua arah yang ringkas.
    if app.voice.listen {
        app.llm_messages.push(crate::llm::Message::new(
            "system",
            "You are in hands-free VOICE mode right now. Be extra brief: answer in 1–2 \
             spoken sentences, ask at most one question per turn, and never read out \
             code or long lists — summarize instead. Keep the conversation flowing.",
        ));
    }

    // Konteks proyek (VOCA.md/AGENTS.md/CLAUDE.md/README.md) → jadi pesan system
    // tambahan agar Voca paham proyek ini, mirip CLAUDE.md di Claude Code.
    // HANYA dimuat bila folder tepercaya (cegah prompt-injection dari repo asing).
    if app.trusted {
        if let Some((file, ctx)) = config::load_project_context() {
            app.llm_messages.push(crate::llm::Message::new(
                "system",
                format!("Project context from {file} (use it to understand this project):\n\n{ctx}"),
            ));
            app.push_system(&format!("◉ Loaded project context from {file}"));
        }
    }

    // Banner, lalu status sidecar
    app.push_banner();
    // Folder belum tepercaya → tampilkan popup trust di dalam TUI (lewat MenuState,
    // seragam dengan menu lain). Keputusan ditangani di agent (handler MenuKind::Trust).
    if !app.trusted {
        use crate::app::{MenuKind, MenuState};
        let path = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        app.menu = Some(MenuState {
            title: "⚠ Trust this folder?".into(),
            subtitle: Some(path),
            items: vec![
                "Trust this folder".into(),
                "Don't trust  (restricted: no context, confirm every command)".into(),
            ],
            selected: 0, // default = trust (Enter langsung percayai)
            kind: MenuKind::Trust,
            danger: false,
        });
        app.input_mode = crate::app::InputMode::Menu;
    }
    if app.voice_handle.is_none() && (app.voice.listen || app.voice.speak) {
        app.push_system(
            "⌨  Voice sidecar unavailable — continuing in text mode.\n\
             ·  Start it first: python3 -m voca.voice_server"
        );
        app.voice.listen = false;
        app.voice.speak  = false;
    } else if app.voice_handle.is_some() {
        app.push_system("◉ Voice sidecar ready — just speak, or press t to type.");
    }

    // Pesan awal dari argumen -t
    if let Some(text) = initial_text {
        let _ = tx.send(AppEvent::VoiceResult(text));
    } else if app.trusted && app.voice.listen && app.voice_handle.is_some() {
        // Hands-free: langsung buka telinga tanpa perlu tekan Enter.
        // (Kalau folder belum tepercaya, auto-listen menunggu sampai modal trust
        // dijawab — lihat handler InputMode::Trust di agent.)
        let _ = tx.send(AppEvent::StartListening);
    }

    // ── TUI ──────────────────────────────────────────────────────────────────
    // Mouse capture AKTIF → roda mouse bisa scroll chat (lihat handle_mouse).
    // Konsekuensi: seleksi teks pakai Shift+drag (klik-drag polos direbut app).
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);

    // Tick setiap 80 ms untuk animasi spinner; lewati tick yang terlewat
    let mut tick_interval = interval(Duration::from_millis(80));
    tick_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut crossterm_events = EventStream::new();

    loop {
        if app.should_quit { break; }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        tokio::select! {
            biased;

            // Prioritas: event dari LLM/voice tasks (LlmChunk, VoiceResult, …)
            Some(event) = rx.recv() => {
                agent::handle_event(&mut app, event);
            }

            // Keyboard/mouse diproses langsung (tanpa round-trip lewat tx)
            Some(Ok(event)) = crossterm_events.next() => {
                match event {
                    CrosstermEvent::Key(k) => {
                        agent::handle_event(&mut app, AppEvent::Key(k));
                    }
                    CrosstermEvent::Mouse(m) => {
                        agent::handle_event(&mut app, AppEvent::Mouse(m));
                    }
                    CrosstermEvent::Resize(w, h) => {
                        agent::handle_event(&mut app, AppEvent::Resize(w, h));
                    }
                    _ => {}
                }
            }

            // Tick animasi (prioritas terendah)
            _ = tick_interval.tick() => {
                agent::handle_event(&mut app, AppEvent::Tick);
            }
        }
    }

    // ── Cleanup ──────────────────────────────────────────────────────────────
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();

    Ok(())
}
