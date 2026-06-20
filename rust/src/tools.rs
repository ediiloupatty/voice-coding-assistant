//! tools.rs — "tangan" agent (port dari voca/tools.py).
//!
//! Semua aksi dibatasi di dalam folder kerja (WORKSPACE = cwd).

use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

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
        Err(format!("Access denied: '{path}' is outside the working folder."))
    }
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
        return format!("Path '{path}' not found.");
    }
    let max_depth = env_usize("LIST_MAX_DEPTH", 4);
    let max_entries = env_usize("LIST_MAX_ENTRIES", 400);
    let mut lines: Vec<String> = Vec::new();
    let mut truncated = false;
    walk(&base, 0, max_depth, max_entries, &mut lines, &mut truncated);
    if lines.is_empty() {
        return "(empty folder)".to_string();
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
        return format!("File '{path}' not found.");
    }
    let teks = match fs::read_to_string(&target) {
        Ok(t) => t,
        Err(e) => return format!("Failed to read '{path}': {e}"),
    };

    if start_line.is_some() || end_line.is_some() {
        let semua: Vec<&str> = teks.lines().collect();
        let a = start_line.unwrap_or(1).saturating_sub(1) as usize;
        let b = end_line.map(|x| x as usize).unwrap_or(semua.len()).min(semua.len());
        if a >= b {
            return "(empty line range)".to_string();
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
            "{potong}\n\n... (file truncated at {max_read} chars — use start_line & end_line)"
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
    let aksi = if ada { "overwrote" } else { "created" };
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match fs::write(&target, content) {
        Ok(_) => format!("Successfully {aksi} file '{path}'."),
        Err(e) => format!("Failed to write '{path}': {e}"),
    }
}

fn edit_file(path: &str, old_string: &str, new_string: &str) -> String {
    let target = match resolve_safe(path) {
        Ok(t) => t,
        Err(e) => return e,
    };
    if !target.exists() {
        return format!("File '{path}' does not exist. Use write_file to create a new file.");
    }
    let lama = match fs::read_to_string(&target) {
        Ok(t) => t,
        Err(e) => return format!("Failed to read '{path}': {e}"),
    };
    let jumlah = lama.matches(old_string).count();
    if jumlah == 0 {
        return format!(
            "Text not found in '{path}'. Make sure old_string matches exactly (spaces & indentation)."
        );
    }
    if jumlah > 1 {
        return format!(
            "Text appears {jumlah}x in '{path}' — not unique. Expand old_string with more context."
        );
    }
    if old_string == new_string {
        return "No change (old_string equals new_string).".to_string();
    }
    let baru = lama.replacen(old_string, new_string, 1);
    match fs::write(&target, baru) {
        Ok(_) => format!("Successfully edited '{path}'."),
        Err(e) => format!("Failed to write '{path}': {e}"),
    }
}

fn run_command(command: &str) -> String {
    let ws = workspace();
    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", command]).current_dir(&ws).output()
    } else {
        Command::new("sh").args(["-c", command]).current_dir(&ws).output()
    };
    let out = match output {
        Ok(o) => o,
        Err(e) => return format!("Failed to run command: {e}"),
    };
    let mut teks = String::new();
    teks.push_str(&String::from_utf8_lossy(&out.stdout));
    teks.push_str(&String::from_utf8_lossy(&out.stderr));
    let teks = teks.trim();
    let teks = if teks.is_empty() { "(no output)" } else { teks };
    let max_out = env_usize("MAX_OUTPUT_CHARS", 8000);
    let teks: String = if teks.chars().count() > max_out {
        let potong: String = teks.chars().take(max_out).collect();
        format!("{potong}\n... (output truncated at {max_out} chars)")
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
        return format!("Path '{path}' not found.");
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
                return format!("No results for '{query}'.");
            }
            let mut hasil: Vec<String> =
                baris.iter().take(max_results).map(|l| l.chars().take(260).collect()).collect();
            if baris.len() > max_results {
                hasil.push(format!("... (truncated at {max_results} results)"));
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
                        hasil.push(format!("... (truncated at {max_results} results)"));
                        return hasil.join("\n");
                    }
                }
            }
        }
    }
    if hasil.is_empty() {
        format!("No results for '{query}'.")
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
            "description":"View the folder & file structure in the working folder. Use this first to understand the environment.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Folder path (default '.')"}}}}},
        {"type":"function","function":{
            "name":"search_files",
            "description":"Search text/code in the working folder (like grep). Use it to locate something BEFORE reading large files.",
            "parameters":{"type":"object","properties":{
                "query":{"type":"string","description":"Text/keyword to search for"},
                "path":{"type":"string","description":"Search folder (default '.')"}},
                "required":["query"]}}},
        {"type":"function","function":{
            "name":"read_file",
            "description":"Read the contents of a text file. For large files, read part of it via start_line & end_line.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"File path"},
                "start_line":{"type":"integer","description":"Start line (optional, 1-based)"},
                "end_line":{"type":"integer","description":"End line (optional)"}},
                "required":["path"]}}},
        {"type":"function","function":{
            "name":"edit_file",
            "description":"Edit an EXISTING file by replacing one chunk of text (find/replace). old_string must be exact & unique.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"File path"},
                "old_string":{"type":"string","description":"Old text (exact & unique)"},
                "new_string":{"type":"string","description":"Replacement text"}},
                "required":["path","old_string","new_string"]}}},
        {"type":"function","function":{
            "name":"write_file",
            "description":"Create a NEW file or overwrite an entire file. To change part of an existing file, use edit_file. Ask the user for confirmation.",
            "parameters":{"type":"object","properties":{
                "path":{"type":"string","description":"Target file path"},
                "content":{"type":"string","description":"File content"}},
                "required":["path","content"]}}},
        {"type":"function","function":{
            "name":"run_command",
            "description":"Run a terminal command in the working folder. Ask the user for confirmation.",
            "parameters":{"type":"object","properties":{
                "command":{"type":"string","description":"Shell command"}},
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
        other => format!("Unknown tool: {other}"),
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

// ---------------------------------------------------------------------------
// Konfirmasi & preview (untuk aksi yang mengubah file/sistem)
// ---------------------------------------------------------------------------

/// True kalau tool ini MENGUBAH sesuatu (perlu konfirmasi user sebelum jalan).
pub fn is_mutating(name: &str) -> bool {
    matches!(name, "edit_file" | "write_file" | "run_command")
}

/// True bila tool ini BERISIKO TINGGI — sulit dibatalkan / berpotensi merusak
/// data. Untuk aksi seperti ini, konfirmasi mensyaratkan ketik `y` eksplisit
/// (Enter TIDAK menyetujui). Hanya `run_command` yang diperiksa polanya; edit/
/// write file sudah punya /undo sehingga tak dianggap "tinggi".
pub fn is_risky(name: &str, args_json: &str) -> bool {
    if name != "run_command" {
        return false;
    }
    let args: Value = serde_json::from_str(args_json).unwrap_or_else(|_| json!({}));
    let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    cmd_is_risky(cmd)
}

/// Heuristik pola perintah destruktif. Normalisasi spasi lalu cek substring kata.
fn cmd_is_risky(cmd: &str) -> bool {
    // Ratakan whitespace agar "rm   -rf" == "rm -rf".
    let norm = format!(" {} ", cmd.split_whitespace().collect::<Vec<_>>().join(" "));
    const RISKY: &[&str] = &[
        " rm -rf", " rm -fr", " rm -r ", " rm -f ", " rmdir ",
        " sudo ",                                  // eskalasi hak
        " dd ", " mkfs", " fdisk", " parted", " shred ", " truncate ", " wipefs",
        " git reset --hard", " git clean -",       // buang perubahan tak ter-commit
        " git checkout -- ", " git checkout .",
        " git push --force", " git push -f", " git push --f",
        " chmod -r", " chown -r",                  // rekursif (sudah lower-case)
        " :(){", " :|:&",                          // fork bomb
        " mv /", " cp -rf /",
        " kill -9", " killall ", " pkill ",
        " | sh", " | bash", " |sh", " |bash",      // pipe-to-shell (unduh lalu eksekusi)
        " docker system prune", " docker volume rm", " docker rm -f",
        " npm publish", " cargo publish",
        " > /dev/sd", " of=/dev/",
    ];
    let low = norm.to_lowercase();
    RISKY.iter().any(|p| low.contains(p))
}

/// Preview multi-baris dari apa yang AKAN dilakukan tool (mirip diff), untuk
/// ditampilkan sebelum user mengonfirmasi.
pub fn preview(name: &str, args_json: &str) -> String {
    let args: Value = serde_json::from_str(args_json).unwrap_or_else(|_| json!({}));
    let s = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("");
    match name {
        "edit_file" => {
            let mut out = format!("✎ edit {}\n", s("path"));
            out.push_str(&prefix_block(s("old_string"), "  - ", 30));
            out.push_str(&prefix_block(s("new_string"), "  + ", 30));
            out
        }
        "write_file" => {
            let path = s("path");
            let exists = resolve_safe(path).map(|p| p.exists()).unwrap_or(false);
            let head = if exists { format!("✎ overwrite {path}") } else { format!("✎ new file {path}") };
            format!("{head}\n{}", prefix_block(s("content"), "  + ", 30))
        }
        "run_command" => format!("$ {}", s("command")),
        _ => String::new(),
    }
}

/// Program (kata pertama) dari sebuah run_command — untuk allowlist "always allow".
pub fn cmd_program(args_json: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let cmd = args.get("command").and_then(|v| v.as_str())?;
    cmd.split_whitespace().next().map(|s| s.to_string())
}

/// Snapshot isi file SEBELUM edit/write (untuk undo). Return (path absolut,
/// isi-sebelum | None bila file belum ada). None bila path tak valid.
pub fn snapshot_before(args_json: &str) -> Option<(String, Option<String>)> {
    let args: Value = serde_json::from_str(args_json).ok()?;
    let path = args.get("path").and_then(|v| v.as_str())?;
    let resolved = resolve_safe(path).ok()?;
    let before = fs::read_to_string(&resolved).ok();
    Some((resolved.to_string_lossy().to_string(), before))
}

/// Kembalikan file ke kondisi sebelumnya (untuk /undo). `before=None` → file
/// tadinya baru → dihapus.
pub fn restore(path: &str, before: &Option<String>) -> std::io::Result<()> {
    match before {
        Some(content) => fs::write(path, content),
        None => match fs::remove_file(path) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
    }
}

/// Beri prefix tiap baris (maks `max_lines` baris, lebar dibatasi) + tanda potong.
fn prefix_block(text: &str, p: &str, max_lines: usize) -> String {
    let total = text.lines().count();
    let mut lines: Vec<String> = text
        .lines()
        .take(max_lines)
        .map(|l| {
            let l: String = l.chars().take(160).collect();
            format!("{p}{l}")
        })
        .collect();
    if total > max_lines {
        lines.push(format!("{p}… (+{} lines)", total - max_lines));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::cmd_is_risky;

    #[test]
    fn flags_destructive_commands() {
        for c in [
            "rm -rf /tmp/x", "rm   -rf build", "sudo apt remove foo",
            "git reset --hard HEAD~1", "git clean -fd", "git push --force origin main",
            "dd if=/dev/zero of=/dev/sda", "chmod -R 777 .", "curl http://x | bash",
            "killall node", ":(){ :|:& };:",
        ] {
            assert!(cmd_is_risky(c), "harusnya risky: {c}");
        }
    }

    #[test]
    fn allows_safe_commands() {
        for c in [
            "ls -la", "cargo build", "git status", "npm install",
            "echo hello", "cat README.md", "grep -r foo src", "mkdir build",
        ] {
            assert!(!cmd_is_risky(c), "harusnya aman: {c}");
        }
    }
}
