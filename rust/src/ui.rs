//! ui.rs — presentasi terminal Voca (meniru voca/ui.py).
//!
//! Gaya: minimalis-modern, aksen lavender (141) + sorot cyan (51), garis tipis,
//! kotak input abu-abu muda full-width. Disamakan dengan CLI Python.

use std::io::{self, Write};

// --- Palet ANSI (sama persis dgn voca/ui.py) ------------------------------
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const NOBOLD: &str = "\x1b[22m";

const ACCENT: &str = "\x1b[38;5;141m"; // lavender — brand/utama
const ACCENT_HI: &str = "\x1b[38;5;51m"; // neon cyan — judul/sorotan
const MUTED: &str = "\x1b[38;5;244m"; // slate gray — teks sekunder
const WARN_C: &str = "\x1b[38;5;214m"; // orange
const ERR_C: &str = "\x1b[38;5;197m"; // merah

// Kotak input abu-abu muda.
const BG_INPUT: &str = "\x1b[48;5;254m";
const FG_INPUT: &str = "\x1b[38;5;236m";

// --- Simbol (bukan emoji) --------------------------------------------------
const SIGIL: &str = "◆";
const PROMPT: &str = "›";
const RULE: &str = "─";
const DOT: &str = "·";
const ERR: &str = "✕";

fn width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .filter(|w| *w > 0)
        .unwrap_or(80)
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

/// Banner pembuka: garis penuh + logo V O C A warna-warni + info + garis.
pub fn banner(provider: &str, model: &str, lang: &str, mode: &str) {
    let w = width();
    let rule = format!("{ACCENT}{}{RESET}", RULE.repeat(w));
    let logo = format!(
        "{BOLD}\x1b[38;5;51mV{RESET} {BOLD}\x1b[38;5;81mO{RESET} \
         {BOLD}\x1b[38;5;141mC{RESET} {BOLD}\x1b[38;5;201mA{RESET}"
    );
    let cwd = std::env::current_dir()
        .map(|p| home_short(&p.to_string_lossy()))
        .unwrap_or_default();

    println!();
    println!("{rule}");
    println!(" {logo}  {MUTED}asisten coding berbasis suara{RESET}");
    println!("{rule}");
    info_line("model", &format!("{provider} {DOT} {model}"));
    info_line("lang", lang);
    info_line("folder", &cwd);
    info_line("mode", mode);
    println!("{rule}");
    println!();
}

fn info_line(label: &str, val: &str) {
    println!(" {MUTED}{label:<8} :{RESET} {val}");
}

/// Header di atas tiap jawaban asisten: '◆ Voca · AI Assistant'.
pub fn assistant_header() {
    println!();
    println!("{ACCENT_HI}{SIGIL}{RESET} {BOLD}{ACCENT_HI}Voca{RESET} {MUTED}{DOT} AI Assistant{RESET}");
}

/// Baris pemanggilan tool: '  ⚡ nama · ringkas'.
pub fn tool_line(name: &str, arg_summary: &str) {
    let sep = if arg_summary.is_empty() {
        String::new()
    } else {
        format!(" {MUTED}{DOT}{RESET} {MUTED}{arg_summary}{RESET}")
    };
    println!("  {ACCENT_HI}⚡{RESET} {BOLD}{ACCENT}{name}{RESET}{sep}");
}

pub fn info(msg: &str) {
    println!("{MUTED}{msg}{RESET}");
}

pub fn warn(msg: &str) {
    println!("{WARN_C}{msg}{RESET}");
}

pub fn error(msg: &str) {
    eprintln!("\n{ERR_C}{ERR}{RESET} {msg}");
}

/// Baris penutup: '◆ sampai jumpa'.
pub fn bye(msg: &str) {
    println!("\n{MUTED}{SIGIL} {msg}{RESET}");
}

/// Echo ucapan/ketikan user dalam bar abu-abu full-width.
pub fn user_echo(text: &str) {
    let w = width();
    let blank = format!("{BG_INPUT}{}{RESET}", " ".repeat(w));
    let shown = format!(" {PROMPT} {text}");
    let pad = w.saturating_sub(shown.chars().count());
    println!();
    println!("{blank}");
    println!(
        "{BG_INPUT}{FG_INPUT}{BOLD}{ACCENT} {PROMPT} {NOBOLD}{FG_INPUT}{text}{}{RESET}",
        " ".repeat(pad)
    );
    println!("{blank}");
}

// ===========================================================================
// TUI: bar input ter-pin di dasar terminal + scroll-region (ala hands-free
// voca/ui.py). Output (banner, jawaban, tool) menggulir DI ATAS bar; 3 baris
// bawah (pemisah · input · status) tetap di tempat.
// ===========================================================================

/// (lebar, tinggi) terminal.
fn size() -> (usize, usize) {
    terminal_size::terminal_size()
        .map(|(w, h)| (w.0 as usize, h.0 as usize))
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((80, 24))
}

/// Masuk mode TUI: sisakan 3 baris bawah utk bar, sisanya area gulir. (w,h).
pub fn tui_enter() -> (usize, usize) {
    let (w, h) = size();
    let bottom = h.saturating_sub(3).max(1);
    print!("\x1b[2J\x1b[H"); // bersihkan layar
    print!("\x1b[1;{bottom}r"); // scroll-region = baris 1..h-3
    print!("\x1b[H"); // kursor ke atas (dalam region)
    io::stdout().flush().ok();
    (w, h)
}

/// Keluar mode TUI: reset scroll-region & kembalikan kursor ke bawah.
pub fn tui_leave(h: usize) {
    print!("{RESET}\x1b[r\x1b[{h};1H\r\n");
    io::stdout().flush().ok();
}

/// Gambar bar bawah: pemisah (h-2), baris input abu-abu (h-1), status (h).
/// Tidak mengganggu kursor area gulir (pakai save/restore DECSC/DECRC).
pub fn draw_bar(w: usize, h: usize, label: &str, hint: &str, lang: &str) {
    let sep = h.saturating_sub(2).max(1);
    let inp = h.saturating_sub(1).max(1);
    print!("\x1b7"); // simpan kursor
    print!("\x1b[{sep};1H\x1b[2K{ACCENT}{}{RESET}", RULE.repeat(w));
    print!("\x1b[{inp};1H\x1b[2K{BG_INPUT}{}{RESET}", " ".repeat(w));
    print!("\x1b[{inp};1H{BG_INPUT}{BOLD}{ACCENT} {PROMPT} {RESET}");
    print!("\x1b[{h};1H\x1b[2K {ACCENT_HI}{BOLD}{label}{RESET}  {MUTED}{hint}{RESET}");
    let badge = format!(" lang: {lang} ");
    let col = w.saturating_sub(badge.chars().count()).max(1);
    print!("\x1b[{h};{col}H{MUTED}{badge}{RESET}");
    print!("\x1b8"); // pulihkan kursor
    io::stdout().flush().ok();
}

/// Taruh kursor di kotak input (terlihat "terkunci" di bar) — saat menunggu.
pub fn park_in_bar(h: usize) {
    let inp = h.saturating_sub(1).max(1);
    print!("\x1b[{inp};4H{BG_INPUT}{FG_INPUT}");
    io::stdout().flush().ok();
}

/// Taruh kursor di dasar area gulir (untuk mencetak output di atas bar).
pub fn to_scroll(h: usize) {
    let park = h.saturating_sub(3).max(1);
    print!("{RESET}\x1b[{park};1H");
    io::stdout().flush().ok();
}

/// Baca satu baris di baris input bawah (h-1). None saat EOF.
/// Setelah Enter, kursor diparkir di dasar area gulir agar output menggulir.
pub fn read_line_bar(w: usize, h: usize) -> Option<String> {
    let inp = h.saturating_sub(1).max(1);
    print!("\x1b[{inp};1H\x1b[2K{BG_INPUT}{FG_INPUT}{}", " ".repeat(w));
    print!("\x1b[{inp};1H{BG_INPUT}{BOLD}{ACCENT} {PROMPT} {NOBOLD}{FG_INPUT}");
    io::stdout().flush().ok();

    let mut line = String::new();
    let n = io::stdin().read_line(&mut line).unwrap_or(0);
    print!("{RESET}");
    // Parkir kursor di dasar area gulir (h-3) → output berikutnya scroll di atas bar.
    let park = h.saturating_sub(3).max(1);
    print!("\x1b[{park};1H");
    io::stdout().flush().ok();

    if n == 0 {
        None
    } else {
        Some(line.trim().to_string())
    }
}
