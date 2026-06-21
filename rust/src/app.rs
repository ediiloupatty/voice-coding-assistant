use std::collections::{HashSet, VecDeque};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tui_input::Input;

use crate::config::Limits;
use crate::llm::ToolCall;
use crate::provider::Provider;

pub const SYSTEM_PROMPT: &str = "\
You are Voca, an autonomous coding assistant working inside the user's project \
folder. You can read, search, create, and modify files and run shell commands \
to actually get work done — not just describe it.

Tools available:
- list_files: see the folder/file structure. Use it first to orient yourself.
- search_files: grep for text/code to locate things before reading big files.
- read_file: read a file (use start_line/end_line for large files).
- edit_file: change part of an existing file via exact find/replace (preferred for edits).
- write_file: create a new file or fully overwrite one.
- run_command: run a terminal command. Use this to MOVE/RENAME (`mv`), DELETE (`rm`), \
create folders (`mkdir`), run tests/build, git, etc.

How to work (be agentic — keep going until the task is actually done):
1. Understand the request. If it refers to existing code, EXPLORE first \
(list_files / search_files / read_file) instead of guessing.
2. Make a short plan, then ACT with tools, one concrete step at a time.
3. Prefer edit_file for small changes; read the file before editing so old_string \
matches exactly. Keep changes minimal and match the surrounding style.
4. After editing, VERIFY when it makes sense (re-read the section, or run the \
build/test/command).
5. Be careful with destructive commands (rm, overwrite, git reset). Only do what \
the user asked.

Dependencies — whenever you write or run code that needs external libraries/modules \
or CLI tools, make sure they actually exist before relying on them:
- FIRST verify each dependency is installed, e.g. run `python3 -c \"import NAME\"`, \
`pip show NAME`, `node -e \"require('NAME')\"`, or `command -v TOOL`.
- If something is missing, install it with the RIGHT tool for this project: pip/pip3 \
(prefer an active virtualenv or an existing requirements.txt), npm/yarn when there's a \
package.json, or the system package manager (apt/dnf/brew) for CLI tools. Do the install \
through run_command.
- When several libraries could do the job, pick the most standard, well-maintained, \
lightweight one — and say which you picked in a few words.
- Never assume a package is present: check, then install or adapt. After installing, \
re-run to confirm it works.

Communication — this is a live, SPOKEN conversation. Talk like a real back-and-forth, \
not a lecture:
- Keep replies SHORT — usually 1–2 sentences. Lead with the point.
- Cut filler: no pleasantries, no disclaimers, no restating the question, no \
\"let me explain…\". Skip the obvious.
- When you need information, ask ONE short question, then STOP and wait — don't dump a \
list of options or a multi-step plan in one breath.
- Describe actions in a few words (\"checking the file…\", \"done, added the function\"), \
not paragraphs. Save long detail/code for when it's actually asked for.
- It's a dialogue: say one thing, let the user respond, build from there.
Reply in the language the user is using.";

// ─── Events ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppEvent {
    Key(crossterm::event::KeyEvent),
    Mouse(crossterm::event::MouseEvent),
    Resize(u16, u16),
    Tick,

    // LLM streaming
    LlmChunk(String),
    LlmComplete(String, Vec<ToolCall>),
    LlmError(String),

    // Live model listing (picker "↻ live")
    ModelsFetched(String, Vec<String>), // (kode provider, daftar id model)
    ModelsFetchError(String),

    // Voice
    StartListening,
    VoiceResult(String),
    VoiceSpeak(String),
    SpeakDone,
    VadState(bool),  // sidecar: True saat suara user terdeteksi (indikator real-time)
    BargeIn,         // sidecar: user menyela saat TTS bicara → mulai dengar

    // Tools
    ToolDone(String, String), // (tool_call_id, output) — hasil run_command async
    ConfirmAnswer(bool),
    ConfirmAlways,            // "always allow" batch ini untuk sesi sekarang
}

// ─── Chat ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ChatBubble {
    pub role: BubbleRole,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BubbleRole {
    System,
    User,
    Assistant,
    Tool,
}

// ─── Modes & Options ────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Listening,
    Processing,
    Speaking,   // TTS sedang membacakan jawaban — mic tertutup (half-duplex)
    Menu,       // semua popup (model, bahasa, trust, konfirmasi) lewat MenuState
}

#[derive(Clone, Debug)]
pub struct VoiceOpts {
    pub speak: bool,
    pub listen: bool,
    pub lang: String,
}

#[derive(Clone, Debug)]
pub struct MenuState {
    pub title: String,
    pub subtitle: Option<String>, // baris konteks opsional (mis. path folder)
    pub items: Vec<String>,
    pub selected: usize,
    pub kind: MenuKind,
    pub danger: bool,             // true → border merah + default ke opsi aman
}

impl MenuState {
    /// Menu pilihan biasa (model, bahasa) — netral, default ke item pertama.
    pub fn choice(title: &str, items: Vec<String>, selected: usize, kind: MenuKind) -> Self {
        MenuState { title: title.into(), subtitle: None, items, selected, kind, danger: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuKind {
    Model,
    ModelPick(String), // langkah-2: pilih model dalam provider (String = kode provider)
    ModelLive(String), // daftar model live dari server (String = kode provider)
    Language,
    Trust,    // "Trust this folder?" — pilih: trust / restricted
    Confirm,  // konfirmasi batch tool — pilih: yes / always / no
}

// ─── Slash command palette ──────────────────────────────────────────────────
//
// (nama, argumen, deskripsi) — dipakai palet popup saat user mengetik "/", dan
// juga oleh /help. Satu sumber kebenaran agar tak ada daftar ganda.
pub type SlashCmd = (&'static str, &'static str, &'static str);

pub const SLASH_COMMANDS: &[SlashCmd] = &[
    ("/model", "[provider] [model-id]", "switch provider (+ optional model id)"),
    ("/lan",   "[id|en]", "switch language"),
    ("/trust", "",        "trust this folder"),
    ("/undo",  "",        "undo last file change"),
    ("/help",  "",        "list all commands"),
    ("/exit",  "",        "quit voca"),
];

/// Command yang cocok dengan teks input saat ini. Palet hanya muncul saat input
/// diawali "/" dan belum berisi argumen (belum ada spasi).
pub fn slash_matches(input: &str) -> Vec<&'static SlashCmd> {
    if !input.starts_with('/') || input.contains(' ') {
        return Vec::new();
    }
    SLASH_COMMANDS.iter().filter(|(name, _, _)| name.starts_with(input)).collect()
}

// ─── App State ──────────────────────────────────────────────────────────────

pub struct App {
    // Chat history
    pub messages: Vec<ChatBubble>,
    pub current_stream: String,
    pub is_streaming: bool,
    pub scroll_offset: u16,
    pub is_at_bottom: bool,
    pub total_lines: u16,
    pub chat_height: u16, // tinggi viewport chat (di-set render_chat tiap frame)

    // Input
    pub input: Input,
    pub input_mode: InputMode,
    pub menu: Option<MenuState>,
    // Riwayat ketikan untuk navigasi ↑/↓ (history_pos None = sedang mengetik baru).
    pub input_history: Vec<String>,
    pub history_pos: Option<usize>,
    // Item terpilih di palet slash command (popup yang muncul saat input diawali "/").
    pub slash_sel: usize,

    // Config
    pub provider: Provider,
    pub voice: VoiceOpts,
    pub limits: Limits,

    // Animation
    pub spinner_frame: usize,
    pub spinner_msg: String,

    // Voice text override: true saat user tekan 't' di mic-mode untuk beralih ketik
    pub voice_text_mode: bool,
    // True saat sidecar melaporkan suara user sedang terdeteksi (indikator listening).
    pub vad_speech: bool,

    // Berapa kali tool dieksekusi dalam giliran sekarang (reset tiap pesan user baru).
    // Penjaga agar agent tak ngeloop tanpa henti (lihat Limits::max_tool_iters).
    pub tool_iters: usize,

    // Antrean tool batch dari satu langkah LLM. Kalau ada tool yang MENGUBAH
    // file/sistem, seluruh batch ditahan untuk SATU konfirmasi (bukan per tool).
    // `batch_confirmed` = user sudah menyetujui batch yang sedang diantre.
    pub pending_tools: VecDeque<crate::llm::ToolCall>,
    pub batch_confirmed: bool,

    // Tugas LLM yang sedang stream (untuk Esc-interrupt: abort).
    pub llm_task: Option<JoinHandle<()>>,
    // True setelah Esc-interrupt: abaikan event LLM yang masih nyangkut di antrean.
    pub interrupted: bool,
    // Undo: tumpukan (path absolut, isi-sebelum | None bila file baru).
    pub undo_stack: Vec<(String, Option<String>)>,
    // "Always allow" sesi ini: program run_command yang diizinkan + izin tulis file.
    pub allowed_cmds: HashSet<String>,
    pub allow_writes: bool,

    // Runtime
    pub should_quit: bool,
    pub quit_pending: bool, // Ctrl+C pertama "mengarmkan" keluar; kedua benar keluar
    pub tx: mpsc::UnboundedSender<AppEvent>,
    pub llm_messages: Vec<crate::llm::Message>,
    pub tools_schema: serde_json::Value,
    pub voice_handle: Option<crate::voicebridge::VoiceHandle>,
    pub client: reqwest::Client,

    // Folder kerja tepercaya? Bila tidak: konteks proyek tak di-auto-load dan
    // semua perintah pengubah WAJIB konfirmasi (VOCA_AUTO_APPROVE diabaikan).
    pub trusted: bool,
}

impl App {
    pub fn new(
        provider: Provider,
        voice: VoiceOpts,
        limits: Limits,
        client: reqwest::Client,
        tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        App {
            messages: Vec::new(),
            current_stream: String::new(),
            is_streaming: false,
            scroll_offset: 0,
            is_at_bottom: true,
            total_lines: 0,
            chat_height: 0,
            input: Input::default(),
            input_mode: InputMode::Normal,
            menu: None,
            input_history: Vec::new(),
            history_pos: None,
            slash_sel: 0,
            provider,
            voice,
            limits,
            spinner_frame: 0,
            spinner_msg: String::new(),
            voice_text_mode: false,
            vad_speech: false,
            tool_iters: 0,
            pending_tools: VecDeque::new(),
            batch_confirmed: false,
            llm_task: None,
            interrupted: false,
            undo_stack: Vec::new(),
            allowed_cmds: HashSet::new(),
            allow_writes: false,
            should_quit: false,
            quit_pending: false,
            tx,
            llm_messages: vec![crate::llm::Message::new("system", SYSTEM_PROMPT)],
            tools_schema: crate::tools::tools_schema(),
            voice_handle: None,
            client,
            trusted: true,
        }
    }

    // ── Scroll ──────────────────────────────────────────────────────────────

    /// Batas scroll paling bawah = baris konten dikurangi tinggi viewport, supaya
    /// scroll-down berhenti tepat saat pesan terakhir terlihat (tak masuk area
    /// kosong di bawahnya).
    fn max_scroll(&self) -> u16 {
        self.total_lines.saturating_sub(self.chat_height)
    }

    pub fn scroll_up(&mut self, n: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
        self.is_at_bottom = false;
    }

    pub fn scroll_down(&mut self, n: u16) {
        let max = self.max_scroll();
        self.scroll_offset = (self.scroll_offset + n).min(max);
        self.is_at_bottom = self.scroll_offset >= max;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
        self.is_at_bottom = true;
    }

    // ── Chat bubbles ─────────────────────────────────────────────────────────

    pub fn push_system(&mut self, msg: &str) {
        self.messages.push(ChatBubble { role: BubbleRole::System, content: msg.to_string() });
    }

    pub fn push_user(&mut self, msg: &str) {
        self.messages.push(ChatBubble { role: BubbleRole::User, content: msg.to_string() });
    }

    pub fn push_tool(&mut self, msg: &str) {
        self.messages.push(ChatBubble { role: BubbleRole::Tool, content: msg.to_string() });
    }

    // ── Streaming ────────────────────────────────────────────────────────────

    pub fn start_streaming(&mut self, msg: &str) {
        self.current_stream.clear();
        self.is_streaming = true;
        self.input_mode = InputMode::Processing;
        self.spinner_msg = msg.to_string();
        self.spinner_frame = 0;
    }

    pub fn append_chunk(&mut self, chunk: &str) {
        self.current_stream.push_str(chunk);
    }

    pub fn finish_streaming(&mut self) {
        if !self.current_stream.is_empty() {
            self.messages.push(ChatBubble {
                role: BubbleRole::Assistant,
                content: self.current_stream.clone(),
            });
        }
        self.current_stream.clear();
        self.is_streaming = false;
        self.input_mode = InputMode::Normal;
    }

    // ── Banner ───────────────────────────────────────────────────────────────

    pub fn push_banner(&mut self) {
        let cwd = std::env::current_dir()
            .map(|p| home_short(&p.to_string_lossy()))
            .unwrap_or_default();
        let mode = match (self.voice.listen, self.voice.speak) {
            (true, _)      => "voice (hands-free)",
            (false, true)  => "text + voice",
            _              => "text",
        };
        let msg = format!(
            "╭──────────────────────────────────────────────────\n\
             │ V O C A  ·  AI Coding Assistant\n\
             ├──────────────────────────────────────────────────\n\
             │ model  : {}\n\
             │ lang   : {}\n\
             │ folder : {}\n\
             │ mode   : {}\n\
             ╰──────────────────────────────────────────────────",
            self.provider.model,
            self.voice.lang.to_uppercase(),
            cwd, mode,
        );
        self.push_system(&msg);
    }
}

fn home_short(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let h = home.to_string_lossy();
        if let Some(rest) = path.strip_prefix(h.as_ref()) {
            return format!("~{rest}");
        }
    }
    path.to_string()
}
