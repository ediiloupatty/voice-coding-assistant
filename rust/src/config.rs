//! config.rs — muat konfigurasi dari .env + config user, dan alur input API key.
//!
//! Prioritas sumber API key:
//!   1. Env var / .env (kompatibel mundur dengan versi Python)
//!   2. File config user (~/.config/voca/config.json) — tersimpan otomatis
//!   3. Prompt interaktif saat pertama jalan → disimpan ke #2

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

/// Batas perilaku agent (port dari config.py).
pub struct Limits {
    pub max_history: usize,
    pub max_tool_iters: usize,
    pub llm_max_retries: u32,
    pub llm_retry_base_delay: f64,
    pub temperature: f32,
}

impl Limits {
    pub fn from_env() -> Self {
        let get_usize = |k: &str, d: usize| {
            env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
        };
        let get_u32 = |k: &str, d: u32| {
            env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
        };
        let get_f64 = |k: &str, d: f64| {
            env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
        };
        Limits {
            max_history: get_usize("MAX_HISTORY", 30),
            max_tool_iters: get_usize("MAX_TOOL_ITERS", 15),
            llm_max_retries: get_u32("LLM_MAX_RETRIES", 4),
            llm_retry_base_delay: get_f64("LLM_RETRY_BASE_DELAY", 2.0),
            temperature: get_f64("QWEN_TEMPERATURE", 0.3) as f32,
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct UserConfig {
    /// Pasangan ENV_VAR -> nilai (mis. DASHSCOPE_API_KEY -> "sk-...").
    #[serde(default)]
    vars: BTreeMap<String, String>,
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("voca").join("config.json"))
}

fn load_user_config() -> UserConfig {
    config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Simpan satu pasangan key=value ke config user (merge, bukan timpa).
fn save_user_var(key: &str, value: &str) -> Result<PathBuf> {
    let path = config_path().context("tak menemukan direktori config user")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut cfg = load_user_config();
    cfg.vars.insert(key.to_string(), value.to_string());
    let json = serde_json::to_string_pretty(&cfg)?;
    fs::write(&path, json)?;
    // Batasi permission (best-effort, Unix).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// Muat semua konfigurasi ke environment proses:
///   .env (cwd ke atas)  →  config user (hanya isi yang belum ada di env)
pub fn load() {
    // 1. .env dari direktori kerja, naik ke atas (best-effort).
    let _ = dotenvy::dotenv();
    // 2. config user: set hanya kalau belum ada di environment.
    for (k, v) in load_user_config().vars {
        if env::var(&k).is_err() {
            env::set_var(&k, v);
        }
    }
}

/// Pastikan provider aktif punya API key. Kalau belum, minta interaktif lalu
/// simpan ke config user. Return nilai key.
pub fn ensure_api_key(provider_code: &str, provider_name: &str) -> Result<String> {
    let env_name = crate::provider::api_key_env(provider_code);
    if let Ok(k) = env::var(env_name) {
        if !k.trim().is_empty() {
            return Ok(k);
        }
    }

    // Prompt interaktif.
    use std::io::Write;
    println!();
    println!("  Belum ada API key untuk {provider_name}.");
    print!("  Tempel API key Anda: ");
    std::io::stdout().flush().ok();
    let mut key = String::new();
    std::io::stdin().read_line(&mut key)?;
    let key = key.trim().to_string();
    if key.is_empty() {
        anyhow::bail!("API key kosong — tidak bisa lanjut.");
    }

    env::set_var(env_name, &key);
    match save_user_var(env_name, &key) {
        Ok(path) => println!("  ✓ Tersimpan di {}", path.display()),
        Err(e) => eprintln!("  (gagal menyimpan config: {e} — key tetap dipakai sesi ini)"),
    }
    Ok(key)
}
