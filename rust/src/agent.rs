use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::backend::crossterm::EventHandler;

use crate::app::{App, AppEvent, InputMode, MenuKind, MenuState};
use crate::llm::{Message, ToolCall};
use crate::{llm, provider, tools};

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn handle_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Key(key)         => handle_key(app, key),
        AppEvent::Mouse(mouse)     => handle_mouse(app, mouse),
        AppEvent::Resize(_, _)     => {}

        AppEvent::Tick => {
            app.spinner_frame = app.spinner_frame.wrapping_add(1);
        }

        // ── LLM streaming ────────────────────────────────────────────────────
        // Abaikan event LLM yang datang setelah Esc-interrupt (sudah dibatalkan).
        AppEvent::LlmChunk(chunk) => {
            if app.interrupted { return; }
            app.append_chunk(&chunk);
            if app.is_at_bottom { app.scroll_to_bottom(); }
        }
        AppEvent::LlmComplete(text, tool_calls) => {
            if app.interrupted { return; }
            handle_llm_complete(app, text, tool_calls);
        }
        AppEvent::LlmError(msg) => {
            if app.interrupted { return; }
            app.finish_streaming();
            app.push_system(&format!("❌ {msg}"));
        }

        // ── Voice ────────────────────────────────────────────────────────────
        // Mode hands-free: mic SELALU mendengarkan otomatis (loop). Worker thread
        // di voicebridge yang blocking; main-loop tetap hidup sehingga `t` (beralih
        // ketik) responsif. Tak perlu tekan Enter untuk mulai bicara.
        AppEvent::StartListening => {
            arm_listen(app);
        }
        AppEvent::VoiceResult(text) => {
            // Half-duplex: HANYA terima hasil bila kita memang sedang `Listening`.
            // Hasil yang tiba di mode lain = siklus basi / dibatalkan (user menekan
            // `t`, atau LLM/TTS sedang jalan) → dibuang, TIDAK diproses & TIDAK
            // memicu dengar ulang. Inilah yang mencegah suara "nyasar masuk".
            if app.input_mode != InputMode::Listening {
                return;
            }
            app.input_mode = InputMode::Normal;
            app.vad_speech = false;
            let text = text.trim().to_string();
            if !text.is_empty() {
                process_user_input(app, &text);
            }
            // Belum mulai proses LLM (mis. ucapan kosong / perintah cepat)?
            // Langsung pasang telinga lagi supaya tetap hands-free.
            if app.input_mode == InputMode::Normal {
                arm_listen(app);
            }
        }
        AppEvent::VoiceSpeak(text) => {
            // Non-blocking: worker memutar TTS lalu mengirim SpeakDone.
            if let Some(h) = &app.voice_handle {
                // Tandai Speaking HANYA bila ini narasi penutup giliran (Normal).
                // Saat masih ada tool/iterasi lanjut, input_mode = Processing →
                // jangan ditimpa, biar telinga tetap tertutup sampai giliran tuntas.
                if app.input_mode == InputMode::Normal {
                    app.input_mode = InputMode::Speaking;
                }
                h.speak(&text, &app.voice.lang);
            }
        }
        AppEvent::SpeakDone => {
            // Buka telinga lagi HANYA kalau TTS penutup giliran barusan selesai.
            // (Narasi tengah saat tool berjalan tak menyetel Speaking → diabaikan,
            // giliran berikutnya yang akan memasang telinga.)
            if app.input_mode == InputMode::Speaking {
                app.input_mode = InputMode::Normal;
                arm_listen(app);
            }
        }
        AppEvent::VadState(speech) => {
            // Indikator real-time: hanya relevan saat memang sedang mendengarkan.
            app.vad_speech = speech && app.input_mode == InputMode::Listening;
        }
        AppEvent::BargeIn => {
            // User menyela saat asisten bicara (TTS) → hentikan & langsung dengarkan.
            if app.input_mode == InputMode::Speaking {
                app.input_mode = InputMode::Normal;
                arm_listen(app);
            }
        }

        // ── Tools ─────────────────────────────────────────────────────────────
        // Perintah async (run_command) selesai: tampilkan output prosesnya di
        // layar, simpan ke riwayat, lalu lanjutkan antrean tool.
        AppEvent::ToolDone(id, output) => {
            let shown = clip_output(&output, 24);
            if !shown.trim().is_empty() {
                app.push_tool(&shown);
            }
            app.llm_messages.push(Message::tool_result(&id, output));
            step_tools(app);
        }
        AppEvent::ConfirmAnswer(yes) => {
            app.input_mode = InputMode::Normal;
            if app.pending_tools.is_empty() {
                return; // tak ada batch tertunda
            }
            if yes {
                // Setujui SELURUH batch sekali → jalankan semuanya.
                app.batch_confirmed = true;
                run_pending_tools(app);
            } else {
                // Tolak seluruh batch: beri tahu LLM lewat tool_result tiap aksi
                // agar bisa menyesuaikan rencana, lalu lanjut.
                app.push_system("✗ declined");
                let batch: Vec<ToolCall> = app.pending_tools.drain(..).collect();
                for tc in &batch {
                    app.llm_messages.push(Message::tool_result(
                        &tc.id,
                        "(the user declined this action — do not retry it; adjust your plan)".to_string(),
                    ));
                }
                start_llm_turn(app);
            }
        }
        AppEvent::ConfirmAlways => {
            app.input_mode = InputMode::Normal;
            if app.pending_tools.is_empty() {
                return;
            }
            // Izinkan batch ini untuk sesi sekarang, lalu jalankan.
            allow_batch(app);
            app.batch_confirmed = true;
            app.push_system("✓ always allow (this session)");
            run_pending_tools(app);
        }
    }
}

// ── Keyboard ─────────────────────────────────────────────────────────────────

fn handle_key(app: &mut App, key: KeyEvent) {
    // Global: Ctrl-C → tekan SEKALI aman (peringatan), tekan LAGI untuk keluar.
    // Saat keluar, VoiceHandle di-drop → sidecar suara dimatikan (audio berhenti).
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        if app.quit_pending {
            app.should_quit = true;
        } else {
            app.quit_pending = true;
            app.push_system("⚠ Press Ctrl+C again to quit.");
            // Hentikan dengar yang sedang berjalan (aman, tak menutup app).
            if let Some(h) = &app.voice_handle {
                h.cancel();
            }
        }
        return;
    }
    // Tombol lain → batalkan "arming" keluar (Ctrl+C harus dua kali berturut).
    app.quit_pending = false;

    // Global: scroll
    match key.code {
        KeyCode::PageUp   => { app.scroll_up(10); return; }
        KeyCode::PageDown => { app.scroll_down(10); return; }
        KeyCode::End      => { app.scroll_to_bottom(); return; }
        _ => {}
    }

    match &app.input_mode.clone() {
        InputMode::Normal => {
            // Palet slash command terbuka? (input diawali "/", belum ada argumen)
            let palette = crate::app::slash_matches(app.input.value());
            let palette_open = !palette.is_empty();
            let sel = app.slash_sel.min(palette.len().saturating_sub(1));

            match key.code {
                KeyCode::Enter => {
                    // Palet terbuka → jalankan command yang sedang disorot.
                    let text = if palette_open {
                        palette[sel].0.to_string()
                    } else {
                        app.input.value().to_string()
                    };
                    app.input.reset();
                    app.slash_sel = 0;
                    if !text.is_empty() {
                        app.voice_text_mode = false;
                        push_history(app, &text);
                        process_user_input(app, &text);
                        // Perintah cepat (bukan LLM) → kembali hands-free.
                        if app.input_mode == InputMode::Normal {
                            arm_listen(app);
                        }
                    }
                }

                // Saat palet terbuka, ↑/↓ memilih item; Tab melengkapi ketikan.
                KeyCode::Up if palette_open => {
                    let n = palette.len();
                    app.slash_sel = (sel + n - 1) % n;
                }
                KeyCode::Down if palette_open => {
                    let n = palette.len();
                    app.slash_sel = (sel + 1) % n;
                }
                KeyCode::Tab if palette_open => {
                    set_input(app, &format!("{} ", palette[sel].0));
                    app.slash_sel = 0;
                }

                // ↑/↓ (palet tertutup) → telusuri riwayat ketikan.
                KeyCode::Up   => history_prev(app),
                KeyCode::Down => history_next(app),

                // Esc: tutup palet bila terbuka, jika tidak & di voice text-mode →
                // batal ketik, kembali mendengarkan.
                KeyCode::Esc if palette_open => {
                    app.input.reset();
                    app.slash_sel = 0;
                    if app.voice_text_mode { app.voice_text_mode = false; arm_listen(app); }
                }
                KeyCode::Esc if app.voice_text_mode => {
                    app.voice_text_mode = false;
                    app.input.reset();
                    arm_listen(app);
                }

                _ => {
                    app.history_pos = None; // ngetik baru → keluar dari mode riwayat
                    app.slash_sel = 0;      // filter berubah → mulai dari item teratas
                    app.input.handle_event(&crossterm::event::Event::Key(key));
                }
            }
        }

        // Mic sedang mendengarkan (hands-free).
        //   `t` → beralih ke mode ketik (form kosong).
        //   `/` → buka form langsung dengan "/" terketik (slash command tanpa `t` dulu).
        InputMode::Listening => match key.code {
            KeyCode::Char('t') | KeyCode::Char('T') => {
                enter_text_mode(app);
            }
            KeyCode::Char('/') => {
                enter_text_mode(app);
                // Ketikkan "/" ke input agar user lanjut /model, /lan, dll.
                app.input.handle_event(&crossterm::event::Event::Key(key));
            }
            _ => {} // tombol lain diabaikan — biarkan mic tetap merekam
        },

        // Semua popup (model, bahasa, trust, konfirmasi) ditangani seragam di sini.
        InputMode::Menu => {
            if let Some(mut menu) = app.menu.take() {
                let n = menu.items.len();
                match key.code {
                    KeyCode::Up   | KeyCode::Char('k') => {
                        menu.selected = (menu.selected + n - 1) % n;
                        app.menu = Some(menu);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        menu.selected = (menu.selected + 1) % n;
                        app.menu = Some(menu);
                    }
                    KeyCode::Enter => {
                        app.input_mode = InputMode::Normal;
                        commit_menu(app, &menu);
                    }
                    KeyCode::Esc | KeyCode::Char('q') => {
                        app.input_mode = InputMode::Normal;
                        cancel_menu(app, &menu);
                    }
                    _ => { app.menu = Some(menu); }
                }
            }
        }

        InputMode::Processing => {
            // Esc → hentikan generasi LLM yang sedang stream. (Saat run_command
            // berjalan, is_streaming=false → Esc diabaikan, perintah dibiarkan tuntas.)
            if key.code == KeyCode::Esc && app.is_streaming {
                interrupt_generation(app);
            }
        }
        InputMode::Speaking => {
            // Abaikan input saat TTS membacakan jawaban.
        }
    }
}

fn handle_mouse(app: &mut App, mouse: crossterm::event::MouseEvent) {
    use crossterm::event::MouseEventKind;
    match mouse.kind {
        MouseEventKind::ScrollUp   => app.scroll_up(3),
        MouseEventKind::ScrollDown => app.scroll_down(3),
        _ => {}
    }
}

// ── User input → LLM ─────────────────────────────────────────────────────────

/// Buka telinga: mulai satu siklus dengar bila mode suara aktif & kita memang
/// sedang menganggur (Normal). No-op saat user sedang mengetik (voice_text_mode),
/// saat LLM bekerja (Processing), atau saat sudah mendengarkan.
fn arm_listen(app: &mut App) {
    if app.voice.listen
        && !app.voice_text_mode
        && app.voice_handle.is_some()
        && app.input_mode == InputMode::Normal
        && app.input.value().is_empty()   // jangan buka mic kalau user sedang ngetik
    {
        app.input_mode = InputMode::Listening;
        app.vad_speech = false;
        let lang = app.voice.lang.clone();
        if let Some(h) = &app.voice_handle {
            h.listen(&lang);
        }
    }
}

/// Beralih dari mode dengar (hands-free) ke mode ketik: tampilkan form kosong
/// dan hentikan siklus dengar yang sedang berjalan.
fn enter_text_mode(app: &mut App) {
    app.voice_text_mode = true;
    app.input_mode = InputMode::Normal;
    app.input.reset();
    if let Some(h) = &app.voice_handle { h.cancel(); }
}

/// Kembali ke mode dengar setelah selesai dengan form/menu (mis. usai /model).
/// No-op kalau mode suara mati.
fn return_to_voice(app: &mut App) {
    app.voice_text_mode = false;
    arm_listen(app);
}

fn process_user_input(app: &mut App, teks: &str) {
    let teks = teks.trim();
    if teks.is_empty() { return; }

    // Slash commands
    match teks {
        "/exit" | "/quit" => { app.should_quit = true; return; }
        "/help" => {
            let mut s = String::from("Commands:");
            for (name, args, desc) in crate::app::SLASH_COMMANDS {
                s.push_str(&format!("\n  {name} {args}  — {desc}"));
            }
            app.push_system(&s);
            return;
        }
        "/trust" => { trust_folder(app); return; }
        "/undo"  => { undo_last(app); return; }
        _ => {}
    }
    if let Some(arg) = teks.strip_prefix("/model") {
        switch_model(app, arg.trim());
        return;
    }
    if let Some(arg) = teks.strip_prefix("/lan") {
        switch_lang(app, arg.trim_start_matches('g').trim());
        return;
    }

    // Quick shortcuts (ganti model/bahasa dengan kata natural)
    if let Some((kind, val)) = detect_quick_command(teks) {
        match kind {
            "model" => { switch_model(app, val); return; }
            _       => { switch_lang(app, val); return; }
        }
    }

    // Pesan biasa → kirim ke LLM. Reset penghitung iterasi tool untuk giliran baru.
    // @file mention: isi file ditempel ke pesan LLM (tampilan tetap teks asli).
    let augmented = expand_file_mentions(app, teks);
    app.push_user(teks);
    app.llm_messages.push(Message::new("user", augmented));
    app.tool_iters = 0;
    start_llm_turn(app);
}

// ── Input history (↑/↓) ──────────────────────────────────────────────────────

fn push_history(app: &mut App, text: &str) {
    if app.input_history.last().map(|s| s.as_str()) != Some(text) {
        app.input_history.push(text.to_string());
    }
    app.history_pos = None;
}

fn set_input(app: &mut App, value: &str) {
    app.input = tui_input::Input::new(value.to_string());
}

fn history_prev(app: &mut App) {
    if app.input_history.is_empty() { return; }
    let new = match app.history_pos {
        None      => app.input_history.len() - 1,
        Some(0)   => 0,
        Some(i)   => i - 1,
    };
    app.history_pos = Some(new);
    let v = app.input_history[new].clone();
    set_input(app, &v);
}

fn history_next(app: &mut App) {
    match app.history_pos {
        Some(i) if i + 1 < app.input_history.len() => {
            app.history_pos = Some(i + 1);
            let v = app.input_history[i + 1].clone();
            set_input(app, &v);
        }
        Some(_) => { app.history_pos = None; set_input(app, ""); }
        None => {}
    }
}

// ── @file mention ────────────────────────────────────────────────────────────

/// Cari token `@path` di teks; baca isinya & tempelkan ke pesan yang dikirim ke
/// LLM (teks asli tetap ditampilkan apa adanya ke user).
fn expand_file_mentions(app: &mut App, teks: &str) -> String {
    let mut augmented = teks.to_string();
    let mut included: Vec<String> = Vec::new();
    for tok in teks.split_whitespace() {
        let path = tok.trim_start_matches('@');
        if tok.starts_with('@') && !path.is_empty() {
            let args = serde_json::json!({ "path": path }).to_string();
            let content = tools::dispatch("read_file", &args);
            augmented.push_str(&format!("\n\n[File @{path}]:\n{content}"));
            included.push(path.to_string());
        }
    }
    if !included.is_empty() {
        app.push_system(&format!("📎 included: {}", included.join(", ")));
    }
    augmented
}

/// `/undo`: kembalikan perubahan file terakhir (edit_file/write_file) yang dibuat Voca.
fn undo_last(app: &mut App) {
    match app.undo_stack.pop() {
        Some((path, before)) => match tools::restore(&path, &before) {
            Ok(_) => {
                let what = if before.is_some() { "Reverted" } else { "Removed (was new)" };
                app.push_system(&format!("↩ {what}: {path}"));
            }
            Err(e) => app.push_system(&format!("❌ Undo failed: {e}")),
        },
        None => app.push_system("Nothing to undo."),
    }
}

fn start_llm_turn(app: &mut App) {
    app.start_streaming("thinking...");
    app.interrupted = false; // giliran baru → terima event LLM lagi
    let tx     = app.tx.clone();
    let client = app.client.clone();
    let prov   = app.provider.clone();
    let limits = app.limits.clone();
    let msgs   = app.llm_messages.clone();
    let tools  = app.tools_schema.clone();

    let handle = tokio::spawn(async move {
        llm::stream_to_channel(client, prov, limits, msgs, tools, tx).await;
    });
    app.llm_task = Some(handle); // disimpan agar bisa di-abort (Esc-interrupt)
}

/// Esc saat LLM bekerja → hentikan generasi seketika (abort task + tutup koneksi).
fn interrupt_generation(app: &mut App) {
    if let Some(h) = app.llm_task.take() {
        h.abort();
    }
    app.interrupted = true; // abaikan LlmChunk/LlmComplete yang masih nyangkut
    app.pending_tools.clear();
    app.batch_confirmed = false;
    app.finish_streaming(); // simpan teks parsial yang sudah masuk (kalau ada)
    app.push_system("⛔ interrupted");
    arm_listen(app);
}

fn handle_llm_complete(app: &mut App, narasi: String, tool_calls: Vec<ToolCall>) {
    // finish_streaming() push bubble VOCA → draw() menampilkannya
    app.finish_streaming();
    app.llm_messages.push(Message::assistant(narasi.clone(), tool_calls.clone()));

    // Jadwalkan TTS setelah draw() berikutnya:
    // urutan event → LlmComplete (finish_streaming+push) → draw() tampilkan teks
    // → VoiceSpeak → worker TTS → SpeakDone → buka telinga lagi.
    let will_speak = app.voice.speak && app.voice_handle.is_some() && !narasi.is_empty();
    if will_speak {
        let _ = app.tx.send(AppEvent::VoiceSpeak(narasi.clone()));
    }

    if tool_calls.is_empty() {
        trim_history(&mut app.llm_messages, app.limits.max_history);
        // Giliran selesai. Kalau tak ada TTS (SpeakDone takkan datang),
        // langsung pasang telinga lagi agar tetap hands-free.
        if !will_speak {
            arm_listen(app);
        }
        return;
    }

    // Penjaga: hentikan kalau sudah terlalu banyak langkah tool dalam satu giliran.
    app.tool_iters += 1;
    if app.tool_iters > app.limits.max_tool_iters {
        // Tetap balas hasil "kosong" untuk tiap tool_call agar riwayat pesan valid
        // (API menolak assistant tool_calls tanpa tool responses).
        for tc in &tool_calls {
            app.llm_messages.push(Message::tool_result(
                &tc.id,
                "(skipped: reached the tool-step safety limit)".to_string(),
            ));
        }
        app.push_system(&format!(
            "⚠ Stopped after {} tool steps (safety limit). Ask me to continue if needed.",
            app.limits.max_tool_iters
        ));
        trim_history(&mut app.llm_messages, app.limits.max_history);
        if !will_speak {
            arm_listen(app);
        }
        return;
    }

    // Antrekan SELURUH batch tool dari langkah ini. Kalau ada yang mengubah
    // file/sistem → satu konfirmasi untuk seluruh batch (lihat run_pending_tools).
    app.pending_tools = tool_calls.into_iter().collect();
    app.batch_confirmed = false;
    run_pending_tools(app);
}

/// Jalankan batch tool. Kalau batch berisi tool pengubah dan belum disetujui &
/// bukan mode auto → tampilkan preview SEMUA aksi + minta SATU konfirmasi y/N.
/// Kalau aman / sudah disetujui → eksekusi semuanya berurutan lalu lanjut LLM.
fn run_pending_tools(app: &mut App) {
    // Mode YOLO: lewati konfirmasi (VOCA_AUTO_APPROVE=1) — TAPI hanya di folder
    // tepercaya. Folder tak tepercaya selalu minta konfirmasi.
    let auto = app.trusted
        && std::env::var("VOCA_AUTO_APPROVE").as_deref() == Ok("1");

    let has_mutating = app
        .pending_tools
        .iter()
        .any(|tc| tools::is_mutating(&tc.function.name));

    // Butuh konfirmasi batch? (lewati bila semua aksi sudah di-"always allow")
    if has_mutating && !auto && !app.batch_confirmed && !batch_all_allowed(app) {
        // Preview tiap aksi yang akan dijalankan (sekali untuk seluruh batch).
        let batch: Vec<_> = app.pending_tools.iter().cloned().collect();
        for tc in &batch {
            let prev = tools::preview(&tc.function.name, &tc.function.arguments);
            if !prev.is_empty() {
                app.push_tool(&prev);
            }
        }
        app.menu = Some(build_confirm_menu(&batch));
        app.input_mode = InputMode::Menu;
        return; // tunggu satu jawaban untuk semua
    }

    // Aman / disetujui → proses antrean.
    step_tools(app);
}

/// True bila SEMUA aksi pengubah di batch sudah diizinkan lewat allowlist sesi
/// ("always allow"): program run_command terdaftar, dan/atau izin tulis file aktif.
fn batch_all_allowed(app: &App) -> bool {
    app.pending_tools.iter().all(|tc| match tc.function.name.as_str() {
        "run_command" => tools::cmd_program(&tc.function.arguments)
            .map(|p| app.allowed_cmds.contains(&p))
            .unwrap_or(false),
        "edit_file" | "write_file" => app.allow_writes,
        _ => true, // tool baca selalu boleh
    })
}

/// Catat batch ini ke allowlist sesi (dipakai saat user memilih "always allow").
fn allow_batch(app: &mut App) {
    let batch: Vec<ToolCall> = app.pending_tools.iter().cloned().collect();
    for tc in &batch {
        match tc.function.name.as_str() {
            "run_command" => {
                if let Some(p) = tools::cmd_program(&tc.function.arguments) {
                    app.allowed_cmds.insert(p);
                }
            }
            "edit_file" | "write_file" => app.allow_writes = true,
            _ => {}
        }
    }
}

/// Proses antrean tool satu per satu. Tool cepat (file ops) jalan sinkron; namun
/// `run_command` dijalankan ASYNC (spawn_blocking) supaya UI tak beku & spinner
/// tetap animasi — hasilnya datang sebagai AppEvent::ToolDone.
fn step_tools(app: &mut App) {
    while let Some(tc) = app.pending_tools.front().cloned() {
        if tc.function.name == "run_command" {
            app.pending_tools.pop_front();
            let summary = tools::summarize_args(&tc.function.arguments);
            app.push_tool(&format!("◆ run_command {summary}"));
            // Indikator loading di bar input selama perintah berjalan.
            app.input_mode = InputMode::Processing;
            app.spinner_msg = format!("running: {summary}");
            app.spinner_frame = 0;

            let tx   = app.tx.clone();
            let id   = tc.id.clone();
            let name = tc.function.name.clone();
            let args = tc.function.arguments.clone();
            tokio::task::spawn_blocking(move || {
                let out = tools::dispatch(&name, &args);
                let _ = tx.send(AppEvent::ToolDone(id, out));
            });
            return; // tunggu ToolDone sebelum lanjut
        }

        app.pending_tools.pop_front();
        exec_tool(app, &tc);
    }

    // Semua tool batch selesai → lanjut iterasi LLM.
    start_llm_turn(app);
}

/// Potong output panjang untuk ditampilkan di chat (riwayat LLM tetap penuh).
fn clip_output(out: &str, max_lines: usize) -> String {
    let total = out.lines().count();
    let mut s = out.lines().take(max_lines).collect::<Vec<_>>().join("\n");
    if total > max_lines {
        s.push_str(&format!("\n… (+{} more lines)", total - max_lines));
    }
    s
}

/// Popup konfirmasi bergaya Claude: judul = label aksi, subjudul = perintah/preview,
/// lalu pilihan Yes / "don't ask again …" / No (cursor-select, item bernomor).
fn build_confirm_menu(batch: &[ToolCall]) -> MenuState {
    let risky = batch
        .iter()
        .any(|tc| tools::is_risky(&tc.function.name, &tc.function.arguments));

    let runs  = batch.iter().filter(|tc| tc.function.name == "run_command").count();
    let edits = batch.iter()
        .filter(|tc| matches!(tc.function.name.as_str(), "edit_file" | "write_file"))
        .count();

    // Judul kotak (label aksi).
    let title = match (runs, edits) {
        (1, 0) => "Bash command".to_string(),
        (r, 0) => format!("{r} bash commands"),
        (0, 1) => "Edit file".to_string(),
        (0, e) => format!("{e} file changes"),
        _      => "Run actions".to_string(),
    };

    // Subjudul = preview satu-baris dari aksi pengubah pertama (mis. "$ cargo build").
    let subtitle = batch.iter()
        .find(|tc| tools::is_mutating(&tc.function.name))
        .map(|tc| {
            let first = tools::preview(&tc.function.name, &tc.function.arguments)
                .lines().next().unwrap_or("").to_string();
            if batch.len() > 1 { format!("{first}   (+{} more)", batch.len() - 1) } else { first }
        });

    // Opsi "always allow" — sebut nama program bila batch satu jenis perintah.
    let always = if runs >= 1 && edits == 0 {
        let mut progs: Vec<String> = batch.iter()
            .filter_map(|tc| tools::cmd_program(&tc.function.arguments))
            .collect();
        progs.sort();
        progs.dedup();
        match progs.as_slice() {
            [p] => format!("Yes, and don't ask again for: {p} *"),
            _   => "Yes, allow these commands this session".to_string(),
        }
    } else {
        "Yes, and don't ask again this session".to_string()
    };

    MenuState {
        title,
        subtitle,
        items: vec!["Yes".into(), always, "No".into()],
        selected: if risky { 2 } else { 0 }, // risky → default No agar tak ke-Enter
        kind: MenuKind::Confirm,
        danger: risky,
    }
}

/// Jalankan satu tool: tampilkan barisnya, dispatch, simpan hasil ke riwayat.
fn exec_tool(app: &mut App, tc: &ToolCall) {
    // Untuk edit/write: simpan kondisi file SEBELUM diubah agar bisa /undo.
    if matches!(tc.function.name.as_str(), "edit_file" | "write_file") {
        if let Some(snap) = tools::snapshot_before(&tc.function.arguments) {
            app.undo_stack.push(snap);
        }
    }
    let summary = tools::summarize_args(&tc.function.arguments);
    app.push_tool(&format!("◆ {} {}", tc.function.name, summary));
    let result = tools::dispatch(&tc.function.name, &tc.function.arguments);
    app.llm_messages.push(Message::tool_result(&tc.id, result));
}

// ── Menu (popup) commit / cancel ──────────────────────────────────────────────

/// Terapkan pilihan menu yang dikonfirmasi (Enter). input_mode sudah jadi Normal.
fn commit_menu(app: &mut App, menu: &MenuState) {
    match menu.kind {
        MenuKind::Model => {
            let all = provider::all();
            apply_provider(all[menu.selected].code, app);
            return_to_voice(app); // usai ganti model → balik mendengarkan (bila mode suara)
        }
        MenuKind::Language => {
            let langs = ["id", "en"];
            apply_language(langs[menu.selected], app);
            return_to_voice(app);
        }
        MenuKind::Trust => {
            if menu.selected == 0 { trust_folder(app); } else { decline_trust(app); }
            arm_listen(app);
        }
        MenuKind::Confirm => match menu.selected {
            0 => { let _ = app.tx.send(AppEvent::ConfirmAnswer(true)); }   // yes
            1 => { let _ = app.tx.send(AppEvent::ConfirmAlways); }          // always
            _ => { let _ = app.tx.send(AppEvent::ConfirmAnswer(false)); }   // no
        },
    }
}

/// Batalkan menu (Esc / q). Trust → mode terbatas; konfirmasi → tolak batch.
fn cancel_menu(app: &mut App, menu: &MenuState) {
    match menu.kind {
        MenuKind::Trust   => { decline_trust(app); arm_listen(app); }
        MenuKind::Confirm => { let _ = app.tx.send(AppEvent::ConfirmAnswer(false)); }
        // Model/Language dibatalkan → balik mendengarkan (bila mode suara).
        _                 => { app.push_system("(cancelled)"); return_to_voice(app); }
    }
}

/// Folder tetap tak tepercaya: mode terbatas (tanpa konteks, konfirmasi tiap perintah).
fn decline_trust(app: &mut App) {
    app.trusted = false;
    app.push_system(
        "🔒 Untrusted folder — project context not loaded; every command needs \
         confirmation. Type /trust to trust this folder.",
    );
}

// ── Slash command helpers ─────────────────────────────────────────────────────

fn switch_model(app: &mut App, arg: &str) {
    if arg.is_empty() {
        let all = provider::all();
        let cur = all.iter().position(|p| p.code == app.provider.code).unwrap_or(0);
        let items = all.iter().map(|p| {
            let dot  = if p.api_key.is_some() { "●" } else { "○" };
            let flag = if p.code == app.provider.code { " ←" } else { "" };
            format!("{:<11} {}  {dot}{flag}", p.code, p.model)
        }).collect();
        app.menu = Some(MenuState::choice("SELECT MODEL", items, cur, MenuKind::Model));
        app.input_mode = InputMode::Menu;
    } else {
        apply_provider(arg, app);
    }
}

fn apply_provider(code: &str, app: &mut App) {
    match provider::by_code(code) {
        Some(mut p) => match crate::config::ensure_api_key(p.code, p.name) {
            Ok(k) => {
                p.api_key = Some(k);
                app.push_system(&format!("✓ model: {} ({})", p.name, p.model));
                app.provider = p;
            }
            Err(e) => app.push_system(&format!("❌ {e}")),
        },
        None => app.push_system("❌ unknown provider (qwen / openai / openrouter / deepseek)"),
    }
}

fn switch_lang(app: &mut App, arg: &str) {
    const LANGS: [(&str, &str); 2] = [("id", "Indonesia"), ("en", "English")];
    if arg == "id" || arg == "en" {
        apply_language(arg, app);
    } else if arg.is_empty() {
        let cur   = LANGS.iter().position(|(c, _)| *c == app.voice.lang).unwrap_or(0);
        let items = LANGS.iter().map(|(c, name)| {
            let flag = if *c == app.voice.lang { " ←" } else { "" };
            format!("{name}{flag}")
        }).collect();
        app.menu = Some(MenuState::choice("SELECT LANGUAGE", items, cur, MenuKind::Language));
        app.input_mode = InputMode::Menu;
    } else {
        let new = if app.voice.lang == "id" { "en" } else { "id" };
        apply_language(new, app);
    }
}

fn apply_language(lang: &str, app: &mut App) {
    app.voice.lang = lang.to_string();
    app.push_system(&format!("✓ language: {}", lang.to_uppercase()));
}

/// `/trust`: percayai folder kerja saat ini (permanen). Setelah dipercaya,
/// konteks proyek dimuat & auto-approve diizinkan lagi.
fn trust_folder(app: &mut App) {
    if app.trusted {
        app.push_system("✓ This folder is already trusted.");
        return;
    }
    let _ = crate::config::trust_cwd();
    app.trusted = true;
    // Muat konteks proyek sekarang (tadi dilewati karena belum tepercaya).
    if let Some((file, ctx)) = crate::config::load_project_context() {
        app.llm_messages.push(Message::new(
            "system",
            format!("Project context from {file} (use it to understand this project):\n\n{ctx}"),
        ));
        app.push_system(&format!("✓ Folder trusted. Loaded project context from {file}."));
    } else {
        app.push_system("✓ Folder trusted.");
    }
}

fn trim_history(messages: &mut Vec<Message>, max: usize) {
    // messages[0] = system prompt, jangan dihapus
    while messages.len().saturating_sub(1) > max {
        messages.remove(1);
        // Jaga agar setelah remove[1] selanjutnya adalah pesan user
        while messages.len() > 1 && messages[1].role != "user" {
            messages.remove(1);
        }
    }
}

fn detect_quick_command(teks: &str) -> Option<(&'static str, &'static str)> {
    let t: String = teks
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let t = t.trim();
    if t.is_empty() || t.split_whitespace().count() > 3 { return None; }
    match t {
        "qwen" | "kuen" | "kwen"                       => Some(("model", "qwen")),
        "openai" | "open ai" | "gpt" | "chatgpt"       => Some(("model", "openai")),
        "openrouter" | "open router" | "router"         => Some(("model", "openrouter")),
        "deepseek" | "deep seek" | "dipsik"             => Some(("model", "deepseek")),
        "english" | "bahasa inggris" | "inggris"        => Some(("lan", "en")),
        "indonesia" | "bahasa indonesia"                => Some(("lan", "id")),
        _ => None,
    }
}
