//! agent.rs — loop percakapan teks (Fase 0: belum ada tool & suara).
//!
//! Menjaga riwayat, memanggil LLM streaming, dan memangkas history agar tak
//! membengkak (port sederhana dari _pangkas_history di voca/agent.py).

use anyhow::Result;

use crate::config::{self, Limits};
use crate::llm::{self, Message};
use crate::provider::{self, Provider};
use crate::voicebridge::VoiceBridge;
use crate::{tools, ui};

const SYSTEM_PROMPT: &str =
    "Kamu Voca, asisten coding yang ringkas dan membantu. Kamu punya tool untuk \
     melihat folder, mencari, membaca, menulis/mengedit file, dan menjalankan \
     perintah. Pakai tool seperlunya, lalu jawab dalam bahasa pengguna.";

/// Pangkas history agar jumlah pesan (di luar system) tak melebihi max_history.
/// Dimulai dari pesan 'user' supaya pasangan user/assistant tetap utuh.
fn trim_history(messages: &mut Vec<Message>, max_history: usize) {
    // messages[0] selalu system.
    while messages.len() - 1 > max_history {
        // Buang pesan tertua setelah system.
        messages.remove(1);
        // Pastikan pesan pertama setelah system adalah 'user'.
        while messages.len() > 1 && messages[1].role != "user" {
            messages.remove(1);
        }
    }
}

/// Opsi suara untuk satu sesi.
pub struct VoiceOpts {
    pub speak: bool,  // ucapkan jawaban (TTS via sidecar)
    pub listen: bool, // ambil input lewat mic (STT via sidecar)
    pub lang: String, // "en" / "id"
}

/// Jalankan REPL sampai user keluar (mode teks &/ suara).
pub async fn run(
    client: reqwest::Client,
    mut provider: Provider,
    limits: Limits,
    mut voice: VoiceOpts,
    w: usize,
    h: usize,
) -> Result<()> {
    let mut messages: Vec<Message> = vec![Message::new("system", SYSTEM_PROMPT)];
    let tools_schema = tools::tools_schema();

    // Sidecar suara Python (hanya kalau mode suara diminta).
    let mut bridge: Option<VoiceBridge> = if voice.speak || voice.listen {
        ui::info("menyiapkan suara (memuat model)…");
        let b = VoiceBridge::start();
        match &b {
            Some(_) => ui::info("✓ siap — silakan bicara."),
            None => ui::warn("mode suara dimatikan — lanjut sebagai teks."),
        }
        b
    } else {
        None
    };
    let voice_listen = voice.listen && bridge.is_some();
    let voice_speak = voice.speak && bridge.is_some();

    loop {
        // Ambil input: dari mic (mode dengar) atau ketik — bar ter-pin di bawah.
        let teks: String = if voice_listen {
            ui::draw_bar(w, h, "● SUARA", "bicara · ucap 'openai'/'english' utk ganti · /exit", &voice.lang);
            ui::park_in_bar(h); // kursor "terkunci" di kotak input saat menunggu suara
            let t = bridge.as_mut().unwrap().listen(&voice.lang);
            if t.is_empty() {
                continue;
            }
            ui::to_scroll(h); // pindah ke area gulir untuk mencetak echo + jawaban
            ui::user_echo(&t);
            t
        } else {
            ui::draw_bar(w, h, "⌨ KETIK", "/model · /lan · /exit", &voice.lang);
            match ui::read_line_bar(w, h) {
                Some(l) => l,
                None => {
                    ui::bye("sampai jumpa");
                    break;
                }
            }
        };

        if teks.is_empty() {
            continue;
        }
        // --- Perintah slash (tidak dikirim ke LLM) ---------------------------
        if teks == "/exit" || teks == "/quit" {
            ui::bye("sampai jumpa");
            break;
        }
        if teks == "/help" {
            ui::info("perintah: /model [nama] · /lan [id|en] · /exit");
            continue;
        }
        if let Some(rest) = teks.strip_prefix("/model") {
            switch_model(rest.trim(), &mut provider);
            continue;
        }
        if let Some(rest) = teks.strip_prefix("/lan") {
            // terima /lan maupun /lang
            switch_lang(rest.trim_start_matches('g').trim(), &mut voice);
            continue;
        }
        // Perintah singkat lewat SUARA (ucapkan "openai", "english", dll).
        if let Some((kind, val)) = detect_quick_command(&teks) {
            match kind {
                "model" => switch_model(val, &mut provider),
                _ => switch_lang(val, &mut voice),
            }
            continue;
        }

        messages.push(Message::new("user", &teks));
        match handle_turn(&client, &provider, &limits, &tools_schema, &mut messages).await {
            Ok(narasi) => {
                if voice_speak && !narasi.is_empty() {
                    bridge.as_mut().unwrap().speak(&narasi, &voice.lang);
                }
            }
            Err(e) => ui::error(&format!("{e}")),
        }
        trim_history(&mut messages, limits.max_history);
    }
    Ok(())
}

/// `/model [kode]` — tampilkan daftar provider, atau pindah ke salah satunya.
fn switch_model(arg: &str, provider: &mut Provider) {
    if arg.is_empty() {
        ui::info("provider tersedia:");
        for p in provider::all() {
            let aktif = if p.code == provider.code { "  (aktif)" } else { "" };
            let key = if p.api_key.is_some() { "●" } else { "○" };
            ui::info(&format!("  {key} {:<11} {}{aktif}", p.code, p.model));
        }
        ui::info("pakai: /model <qwen|openai|openrouter|deepseek>");
        return;
    }
    match provider::by_code(arg) {
        Some(mut p) => match config::ensure_api_key(p.code, p.name) {
            Ok(k) => {
                p.api_key = Some(k);
                ui::info(&format!("✓ pindah ke {} ({})", p.name, p.model));
                *provider = p;
            }
            Err(e) => ui::error(&format!("{e}")),
        },
        None => ui::error("provider tak dikenal (qwen/openai/openrouter/deepseek)"),
    }
}

/// Deteksi perintah singkat (≤3 kata) dari ucapan/ketikan: ganti model/bahasa.
/// Hanya memicu kalau SELURUH teks cocok, agar tak salah picu di tengah kalimat.
fn detect_quick_command(teks: &str) -> Option<(&'static str, &'static str)> {
    let t: String = teks
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let t = t.trim();
    if t.is_empty() || t.split_whitespace().count() > 3 {
        return None;
    }
    match t {
        // Tampilkan daftar model (arg kosong → switch_model menampilkan list).
        "model" | "ganti model" | "pilih model" | "list model" | "models" => {
            Some(("model", ""))
        }
        "qwen" | "kuen" | "kwen" => Some(("model", "qwen")),
        "openai" | "open ai" | "gpt" | "chatgpt" => Some(("model", "openai")),
        "openrouter" | "open router" | "router" => Some(("model", "openrouter")),
        "deepseek" | "deep seek" | "dipsik" => Some(("model", "deepseek")),
        "bahasa" | "ganti bahasa" | "language" => Some(("lan", "")), // toggle id/en
        "english" | "bahasa inggris" | "inggris" => Some(("lan", "en")),
        "indonesia" | "bahasa indonesia" => Some(("lan", "id")),
        _ => None,
    }
}

/// `/lan [id|en]` — set bahasa suara; tanpa argumen = toggle.
fn switch_lang(arg: &str, voice: &mut VoiceOpts) {
    let new = match arg {
        "id" | "en" => arg.to_string(),
        _ => if voice.lang == "id" { "en".to_string() } else { "id".to_string() },
    };
    ui::info(&format!("bahasa: {}", new.to_uppercase()));
    voice.lang = new;
}

/// Satu giliran: panggil model, eksekusi tool yang diminta, ulangi sampai
/// model selesai (tak minta tool lagi) atau batas iterasi tercapai.
async fn handle_turn(
    client: &reqwest::Client,
    provider: &Provider,
    limits: &Limits,
    tools_schema: &serde_json::Value,
    messages: &mut Vec<Message>,
) -> Result<String> {
    for _ in 0..limits.max_tool_iters {
        let (narasi, tool_calls) =
            llm::stream_once(client, provider, limits, messages, tools_schema).await?;

        messages.push(Message::assistant(narasi.clone(), tool_calls.clone()));

        // Tidak ada tool yang diminta → giliran selesai; kembalikan narasinya.
        if tool_calls.is_empty() {
            return Ok(narasi);
        }

        // Eksekusi tiap tool, kirim hasilnya kembali ke model.
        for tc in &tool_calls {
            ui::tool_line(&tc.function.name, &tools::summarize_args(&tc.function.arguments));
            let hasil = tools::dispatch(&tc.function.name, &tc.function.arguments);
            messages.push(Message::tool_result(&tc.id, hasil));
        }
    }
    ui::info(&format!(
        "batas {} langkah tercapai, berhenti dulu.",
        limits.max_tool_iters
    ));
    Ok(String::new())
}
