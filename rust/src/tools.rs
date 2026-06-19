//! tools.rs — "tangan" agent (port dari voca/tools.py).
//!
//! Semua aksi dibatasi di dalam folder kerja (WORKSPACE = cwd). Aksi yang
//! mengubah sistem (write_file, edit_file, run_command) MINTA KONFIRMASI dulu.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::ui;

const IGNORE_DIRS: &[&str] = &[
    ".git", "__pycache__", "node_modules", ".venv", "venv", ".voca",
    ".mypy_cache", ".pytest_cache", "dist", "build", ".next", ".idea", "target",
];

fn env_usize(k: &str, d: usize) -> usize {
    env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

/// Folder kerja = direktori tempat program dijalankan.
fn workspace() -> PathBuf {
    env::current_dir().unwrap_or_else(|_| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
}

/// Normalisasi path secara leksikal (tanpa butuh file-nya ada), lalu pastikan
/// tetap di dalam WORKSPACE. Cegah keluar lewat '..' atau path absolut.
fn resolve_safe(path: &str) -> Result<PathBuf, String> {
    let ws = workspace();
    let mut result = ws.clone();
    for comp in Path::new(path).components() {
        match comp {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            Component::Normal(c) => result.push(c),
            Component::RootDir | Component::Prefix(_) => {
                // Path absolut diberikan → mulai ulang dari situ.
                result = PathBuf::from(comp.as_os_str());
            }
        }
    }
    if result == ws || result.starts_with(&ws) {
        Ok(result)
    } else {
        Err(format!("Akses ditolak: '{path}' di luar folder kerja."))
    }
}

/// Konfirmasi y/N lewat stdin (default: tidak).
fn confirm(prompt: &str) -> bool {
    print!("  {prompt} [y/N] ");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    if std::io::stdin().read_line(&mut s).is_err() {
        return false;
    }
    matches!(s.trim().to_lowercase().as_str(), "y" | "yes" | "ya")
}

// ---------------------------------------------------------------------------
// Implementasi tool
// ---------------------------------------------------------------------------
fn list_files(path: &str) -> String {
    let base = match resolve_safe(path) {
        Ok(b) => b,
        Err(e) => return e,
    };
    if !base.exists() {
        return format!("Path '{path}' tidak ditemukan.");
    }
    let max_depth = env_usize("LIST_MAX_DEPTH", 4);
    let max_entries = env_usize("LIST_MAX_ENTRIES", 400);
    let mut lines: Vec<String> = Vec::new();
    let mut truncated = false;
    walk(&base, 0, max_depth, max_entries, &mut lines, &mut truncated);
    if lines.is_empty() {
        return "(folder kosong)".to_string();
    }
    let mut hasil = lines.join("\n");
    if truncated {
        hasil.push_str(&format!(
            "\n... (dipotong di {max_entries} baris — pakai path lebih spesifik atau search_files)"
        ));
    }
    hasil
}

fn walk(dir: &Path, depth: usize, max_depth: usize, max_entries: usize,
        lines: &mut Vec<String>, truncated: &mut bool) {
    if *truncated || depth > max_depth {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.path().is_dir() {
            if !IGNORE_DIRS.contains(&name.as_str()) {
                dirs.push(entry.path());
            }
        } else {
            files.push(name);
        }
    }
    dirs.sort();
    files.sort();
    let indent = "  ".repeat(depth);
    for f in files {
        if lines.len() >= max_entries {
            *truncated = true;
            return;
        }
        lines.push(format!("{indent}{f}"));
    }
    for d in dirs {
        if lines.len() >= max_entries {
            *truncated = true;
            return;
        }
        let name = d.file_name().unwrap().to_string_lossy();
        lines.push(format!("{indent}{name}/"));
        walk(&d, depth + 1, max_depth, max_entries, lines, truncated);
    }
}

fn read_file(path: &str, start_line: Option<u64>, end_line: Option<u64>) -> String {
    let target = match resolve_safe(path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if !target.exists() {
        return format!("File '{path}' tidak ditemukan.");
    }
    let teks = match fs::read_to_string(&target) {
        Ok(t) => t,
        Err(e) => return format!("Gagal membaca '{path}': {e}"),
    };

    if start_line.is_some() || end_line.is_some() {
        let semua: Vec<&str> = teks.lines().collect();
        let a = start_line.unwrap_or(1).saturating_sub(1) as usize;
        let b = end_line.map(|x| x as usize).unwrap_or(semua.len()).min(semua.len());
        if a >= b {
            return "(rentang baris kosong)".to_string();
        }
        return semua[a..b]
            .iter()
            .enumerate()
            .map(|(i, ln)| format!("{}: {ln}", a + i + 1))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let max_read = env_usize("MAX_READ_CHARS", 20000);
    if teks.chars().count() > max_read {
        let potong: String = teks.chars().take(max_read).collect();
        return format!(
            "{potong}\n\n... (file dipotong di {max_read} karakter — pakai start_line & end_line)"
        );
    }
    teks
}

fn write_file(path: &str, content: &str) -> String {
    let target = match resolve_safe(path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let ada = target.exists();
    let aksi = if ada { "menimpa" } else { "membuat" };
    if !confirm(&format!("Agent ingin {aksi} file '{path}'. Lanjut?")) {
        return format!("Dibatalkan oleh user. File '{path}' tidak diubah.");
    }
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(&target, content) {
        Ok(_) => format!("Berhasil {aksi} file '{path}'."),
        Err(e) => format!("Gagal menulis '{path}': {e}"),
    }
}

fn edit_file(path: &str, old_string: &str, new_string: &str) -> String {
    let target = match resolve_safe(path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if !target.exists() {
        return format!("File '{path}' tidak ada. Pakai write_file untuk membuat file baru.");
    }
    let lama = match fs::read_to_string(&target) {
        Ok(t) => t,
        Err(e) => return format!("Gagal membaca '{path}': {e}"),
    };
    let jumlah = lama.matches(old_string).count();
    if jumlah == 0 {
        return format!(
            "Teks tak ditemukan di '{path}'. Pastikan old_string sama persis (spasi & indentasi)."
        );
    }
    if jumlah > 1 {
        return format!(
            "Teks muncul {jumlah}x di '{path}' — tidak unik. Perluas old_string dengan konteks."
        );
    }
    if old_string == new_string {
        return "Tidak ada perubahan (old_string sama dengan new_string).".to_string();
    }
    let baru = lama.replacen(old_string, new_string, 1);
    if !confirm(&format!("Agent ingin mengedit '{path}'. Lanjut?")) {
        return format!("Dibatalkan oleh user. File '{path}' tidak diubah.");
    }
    match fs::write(&target, baru) {
        Ok(_) => format!("Berhasil mengedit '{path}'."),
        Err(e) => format!("Gagal menulis '{path}': {e}"),
    }
}

fn run_command(command: &str) -> String {
    if !confirm(&format!("Agent ingin menjalankan: `{command}`. Lanjut?")) {
        return "Dibatalkan oleh user. Command tidak dijalankan.".to_string();
    }
    ui::info(&format!("$ {command}"));
    let ws = workspace();
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", command]).current_dir(&ws).output()
    } else {
        Command::new("sh").args(["-c", command]).current_dir(&ws).output()
    };
    let out = match output {
        Ok(o) => o,
        Err(e) => return format!("Gagal menjalankan command: {e}"),
    };
    let mut teks = String::new();
    teks.push_str(&String::from_utf8_lossy(&out.stdout));
    teks.push_str(&String::from_utf8_lossy(&out.stderr));
    let teks = teks.trim();
    let teks = if teks.is_empty() { "(tanpa output)" } else { teks };
    let max_out = env_usize("MAX_OUTPUT_CHARS", 8000);
    let teks: String = if teks.chars().count() > max_out {
        let potong: String = teks.chars().take(max_out).collect();
        format!("{potong}\n... (output dipotong di {max_out} karakter)")
    } else {
        teks.to_string()
    };
    let code = out.status.code().unwrap_or(-1);
    format!("(exit code {code})\n{teks}")
}

fn search_files(query: &str, path: &str) -> String {
    let base = match resolve_safe(path) {
        Ok(b) => b,
        Err(e) => return e,
    };
    if !base.exists() {
        return format!("Path '{path}' tidak ditemukan.");
    }
    let max_results = env_usize("SEARCH_MAX_RESULTS", 50);

    // Coba ripgrep (cepat); fallback ke pencarian sederhana kalau rg tak ada.
    let mut cmd = Command::new("rg");
    cmd.args(["--no-heading", "--line-number", "--color", "never", "-S", "--max-filesize", "1M"]);
    for d in IGNORE_DIRS {
        cmd.args(["--glob", &format!("!**/{d}/**")]);
    }
    cmd.args(["--", query]).arg(&base).current_dir(workspace());
    if let Ok(out) = cmd.output() {
        if out.status.code().map(|c| c == 0 || c == 1).unwrap_or(false) {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let baris: Vec<&str> = stdout.lines().collect();
            if baris.is_empty() {
                return format!("Tidak ada hasil untuk '{query}'.");
            }
            let mut hasil: Vec<String> =
                baris.iter().take(max_results).map(|l| l.chars().take(260).collect()).collect();
            if baris.len() > max_results {
                hasil.push(format!("... (dipotong di {max_results} hasil)"));
            }
            return hasil.join("\n");
        }
    }
    search_fallback(query, &base, max_results)
}

fn search_fallback(query: &str, base: &Path, max_results: usize) -> String {
    let q = query.to_lowercase();
    let ws = workspace();
    let mut hasil: Vec<String> = Vec::new();
    let mut stack = vec![base.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !IGNORE_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
                continue;
            }
            if p.metadata().map(|m| m.len() > 1_000_000).unwrap_or(true) {
                continue;
            }
            let Ok(isi) = fs::read_to_string(&p) else { continue };
            for (i, ln) in isi.lines().enumerate() {
                if ln.to_lowercase().contains(&q) {
                    let rel = p.strip_prefix(&ws).unwrap_or(&p).display();
                    let snippet: String = ln.trim().chars().take(200).collect();
                    hasil.push(format!("{rel}:{}: {snippet}", i + 1));
                    if hasil.len() >= max_results {
                        hasil.push(format!("... (dipotong di {max_results} hasil)"));
                        return hasil.join("\n");
                    }
                }
            }
        }
    }
    if hasil.is_empty() {
        format!("Tidak ada hasil untuk '{query}'.")
    } else {
        hasil.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Skema tool (format function-calling OpenAI) + dispatch
// ---------------------------------------------------------------------------
pub fn tools_schema() -> Value {
    json!([
        {"type":"function","function":{
            "name":"list_files",
            "description":"Lihat struktur folder & file di folder kerja. Pakai ini dulu untuk memahami lingkungan.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Path folder (default '.')"}}}}},
        {"type":"function","function":{
            "name":"search_files",
            "description":"Cari teks/kode di folder kerja (seperti grep). Pakai untuk menemukan lokasi sesuatu SEBELUM membaca file besar.",
            "parameters":{"type":"object","properties":{
                "query":{"type":"string","description":"Teks/kata kunci yang dicari"},
                "path":{"type":"string","description":"Folder pencarian (default '.')"}},
                "required":["query"]}}},
        {"type":"function","function":{
            "name":"read_file",
            "description":"Baca isi file teks. Untuk file besar, baca sebagian via start_line & end_line.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Path file"},
                "start_line":{"type":"integer","description":"Baris awal (opsional, mulai 1)"},
                "end_line":{"type":"integer","description":"Baris akhir (opsional)"}},
                "required":["path"]}}},
        {"type":"function","function":{
            "name":"edit_file",
            "description":"Edit file yang SUDAH ADA dengan mengganti satu potongan teks (find/replace). old_string harus persis & unik.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Path file"},
                "old_string":{"type":"string","description":"Teks lama (persis & unik)"},
                "new_string":{"type":"string","description":"Teks pengganti"}},
                "required":["path","old_string","new_string"]}}},
        {"type":"function","function":{
            "name":"write_file",
            "description":"Buat file BARU atau timpa seluruh isi file. Untuk ubah sebagian file yang ada, pakai edit_file. Minta konfirmasi user.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Path file tujuan"},
                "content":{"type":"string","description":"Isi file"}},
                "required":["path","content"]}}},
        {"type":"function","function":{
            "name":"run_command",
            "description":"Jalankan perintah terminal di folder kerja. Minta konfirmasi user.",
            "parameters":{"type":"object","properties":{
                "command":{"type":"string","description":"Perintah shell"}},
                "required":["command"]}}}
    ])
}

/// Jalankan tool berdasarkan nama + argumen JSON. Selalu kembalikan String hasil.
pub fn dispatch(name: &str, args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or_else(|_| json!({}));
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match name {
        "list_files" => list_files(if s("path").is_empty() { "." } else { s("path") }),
        "search_files" => {
            let path = if s("path").is_empty() { "." } else { s("path") };
            search_files(s("query"), path)
        }
        "read_file" => read_file(
            s("path"),
            args.get("start_line").and_then(|v| v.as_u64()),
            args.get("end_line").and_then(|v| v.as_u64()),
        ),
        "edit_file" => edit_file(s("path"), s("old_string"), s("new_string")),
        "write_file" => write_file(s("path"), s("content")),
        "run_command" => run_command(s("command")),
        other => format!("Tool tidak dikenal: {other}"),
    }
}

/// Ringkasan argumen untuk ditampilkan di baris tool (mis. path / command).
pub fn summarize_args(args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or_else(|_| json!({}));
    for k in ["path", "command", "query"] {
        if let Some(v) = args.get(k).and_then(|v| v.as_str()) {
            return v.chars().take(80).collect();
        }
    }
    String::new()
}
