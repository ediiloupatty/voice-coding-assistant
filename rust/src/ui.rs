//! ui.rs — tampilan CLI minimalis, aksen teal (port ringan dari voca/ui.py).

const TEAL: &str = "\x1b[38;5;37m";
const DIM: &str = "\x1b[2m";
const BOLD: &str = "\x1b[1m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

/// Banner pembuka.
pub fn banner(provider_name: &str, model: &str) {
    println!();
    println!("  {BOLD}{TEAL}Voca{RESET} {DIM}· asisten coding (Rust){RESET}");
    println!("  {DIM}provider: {provider_name} · model: {model}{RESET}");
    println!("  {DIM}ketik pesan, atau /exit untuk keluar.{RESET}");
    println!();
}

/// Prompt input ala kotak (dipakai sebagai prompt rustyline).
pub fn input_prompt() -> String {
    format!("{TEAL}›{RESET} ")
}

/// Label "voca ›" tepat sebelum potongan teks pertama dari asisten.
pub fn assistant_prefix() {
    print!("{DIM}voca ›{RESET} ");
    use std::io::Write;
    std::io::stdout().flush().ok();
}

/// Baris penanda saat sebuah tool dipanggil (mis. "⚙ read_file  src/main.rs").
pub fn tool_line(name: &str, arg_summary: &str) {
    if arg_summary.is_empty() {
        println!("  {TEAL}⚙{RESET} {DIM}{name}{RESET}");
    } else {
        println!("  {TEAL}⚙{RESET} {DIM}{name}{RESET}  {arg_summary}");
    }
}

pub fn info(msg: &str) {
    println!("  {DIM}{msg}{RESET}");
}

pub fn warn(msg: &str) {
    println!("  {YELLOW}! {msg}{RESET}");
}

pub fn error(msg: &str) {
    eprintln!("  {RED}✗ {msg}{RESET}");
}
