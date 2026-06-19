//! provider.rs — definisi provider LLM aktif (port dari voca/provider.py).
//!
//! Semua provider dipakai lewat endpoint chat-completions OpenAI-compatible,
//! jadi pindah provider = ganti (api_key, base_url, model). Qwen tetap default.

use std::env;

/// Satu provider LLM (Qwen / OpenAI / OpenRouter / DeepSeek).
#[derive(Clone, Debug)]
pub struct Provider {
    pub code: &'static str,
    pub name: &'static str,
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Bangun daftar provider dari environment (sesudah .env + config user dimuat).
pub fn all() -> Vec<Provider> {
    vec![
        Provider {
            code: "qwen",
            name: "Qwen",
            api_key: env::var("DASHSCOPE_API_KEY").ok(),
            base_url: env_or(
                "QWEN_BASE_URL",
                "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
            ),
            model: env_or("QWEN_MODEL", "qwen-plus"),
        },
        Provider {
            code: "openai",
            name: "OpenAI",
            api_key: env::var("OPENAI_API_KEY").ok(),
            base_url: env_or("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            model: env_or("OPENAI_MODEL", "gpt-4o"),
        },
        Provider {
            code: "openrouter",
            name: "OpenRouter",
            api_key: env::var("OPENROUTER_API_KEY").ok(),
            base_url: env_or("OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1"),
            model: env_or("OPENROUTER_MODEL", "openai/gpt-oss-120b:free"),
        },
        Provider {
            code: "deepseek",
            name: "DeepSeek",
            api_key: env::var("DEEPSEEK_API_KEY").ok(),
            base_url: env_or("DEEPSEEK_BASE_URL", "https://api.deepseek.com"),
            model: env_or("DEEPSEEK_MODEL", "deepseek-v4-flash"),
        },
    ]
}

/// Kode provider default (VOCA_PROVIDER), jatuh ke "qwen" kalau tak dikenal.
pub fn default_code() -> String {
    let want = env_or("VOCA_PROVIDER", "qwen");
    let known = ["qwen", "openai", "openrouter", "deepseek"];
    if known.contains(&want.as_str()) {
        want
    } else {
        "qwen".to_string()
    }
}

/// Nama env var API key untuk sebuah kode provider (dipakai saat menyimpan).
pub fn api_key_env(code: &str) -> &'static str {
    match code {
        "openai" => "OPENAI_API_KEY",
        "openrouter" => "OPENROUTER_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        _ => "DASHSCOPE_API_KEY", // qwen
    }
}

/// Ambil provider berdasarkan kode.
pub fn by_code(code: &str) -> Option<Provider> {
    all().into_iter().find(|p| p.code == code)
}
