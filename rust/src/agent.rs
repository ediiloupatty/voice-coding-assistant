//! agent.rs — loop percakapan teks (Fase 0: belum ada tool & suara).
//!
//! Menjaga riwayat, memanggil LLM streaming, dan memangkas history agar tak
//! membengkak (port sederhana dari _pangkas_history di voca/agent.py).

use anyhow::Result;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

use crate::config::Limits;
use crate::llm::{self, Message};
use crate::provider::Provider;
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
    provider: Provider,
    limits: Limits,
    voice: VoiceOpts,
) -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    let mut messages: Vec<Message> = vec![Message::new("system", SYSTEM_PROMPT)];
    let tools_schema = tools::tools_schema();

    // Sidecar suara Python (hanya kalau mode suara diminta).
    let mut bridge: Option<VoiceBridge> = if voice.speak || voice.listen {
        let b = VoiceBridge::start();
        if b.is_none() {
            ui::warn("mode suara dimatikan — lanjut sebagai teks.");
        }
        b
    } else {
        None
    };
    let voice_listen = voice.listen && bridge.is_some();
    let voice_speak = voice.speak && bridge.is_some();

    loop {
        // Ambil input: dari mic (mode dengar) atau ketik.
        let teks: String = if voice_listen {
            ui::info("dengar… (bicara, berhenti otomatis saat hening)");
            let t = bridge.as_mut().unwrap().listen(&voice.lang);
            if t.is_empty() {
                ui::info("(tidak terdengar — coba lagi)");
                continue;
            }
            println!("  {t}");
            t
        } else {
            match rl.readline(&ui::input_prompt()) {
                Ok(l) => l.trim().to_string(),
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    ui::info("sampai jumpa.");
                    break;
                }
                Err(e) => {
                    ui::error(&format!("input error: {e}"));
                    break;
                }
            }
        };

        if teks.is_empty() {
            continue;
        }
        if teks == "/exit" || teks == "/quit" {
            ui::info("sampai jumpa.");
            break;
        }
        if !voice_listen {
            let _ = rl.add_history_entry(&teks);
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
