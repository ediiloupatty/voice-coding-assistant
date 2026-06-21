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
            model: env_or("QWEN_MODEL", "qwen-max"),
        },
        Provider {
            code: "openai",
            name: "OpenAI",
            api_key: env::var("OPENAI_API_KEY").ok(),
            base_url: env_or("OPENAI_BASE_URL", "https://api.openai.com/v1"),
            model: env_or("OPENAI_MODEL", "gpt-5.1"),
        },
        Provider {
            code: "openrouter",
            name: "OpenRouter",
            api_key: env::var("OPENROUTER_API_KEY").ok(),
            base_url: env_or("OPENROUTER_BASE_URL", "https://openrouter.ai/api/v1"),
            model: env_or("OPENROUTER_MODEL", "openrouter/auto"),
        },
        Provider {
            code: "deepseek",
            name: "DeepSeek",
            api_key: env::var("DEEPSEEK_API_KEY").ok(),
            base_url: env_or("DEEPSEEK_BASE_URL", "https://api.deepseek.com"),
            model: env_or("DEEPSEEK_MODEL", "deepseek-chat"),
        },
        Provider {
            code: "gemini",
            name: "Gemini",
            api_key: env::var("GEMINI_API_KEY").ok(),
            base_url: env_or(
                "GEMINI_BASE_URL",
                "https://generativelanguage.googleapis.com/v1beta/openai/",
            ),
            model: env_or("GEMINI_MODEL", "gemini-2.5-flash"),
        },
        Provider {
            code: "claude",
            name: "Claude",
            api_key: env::var("ANTHROPIC_API_KEY").ok(),
            base_url: env_or("ANTHROPIC_BASE_URL", "https://api.anthropic.com/v1/"),
            model: env_or("ANTHROPIC_MODEL", "claude-sonnet-4-6"),
        },
        Provider {
            code: "grok",
            name: "xAI Grok",
            api_key: env::var("XAI_API_KEY").ok(),
            base_url: env_or("XAI_BASE_URL", "https://api.x.ai/v1"),
            model: env_or("XAI_MODEL", "grok-4.3"),
        },
        Provider {
            code: "groq",
            name: "Groq",
            api_key: env::var("GROQ_API_KEY").ok(),
            base_url: env_or("GROQ_BASE_URL", "https://api.groq.com/openai/v1"),
            model: env_or("GROQ_MODEL", "llama-3.3-70b-versatile"),
        },
        Provider {
            code: "mistral",
            name: "Mistral",
            api_key: env::var("MISTRAL_API_KEY").ok(),
            base_url: env_or("MISTRAL_BASE_URL", "https://api.mistral.ai/v1"),
            model: env_or("MISTRAL_MODEL", "mistral-large-latest"),
        },
        Provider {
            code: "together",
            name: "Together AI",
            api_key: env::var("TOGETHER_API_KEY").ok(),
            base_url: env_or("TOGETHER_BASE_URL", "https://api.together.xyz/v1"),
            model: env_or(
                "TOGETHER_MODEL",
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
            ),
        },
        Provider {
            code: "perplexity",
            name: "Perplexity",
            api_key: env::var("PERPLEXITY_API_KEY").ok(),
            base_url: env_or("PERPLEXITY_BASE_URL", "https://api.perplexity.ai"),
            model: env_or("PERPLEXITY_MODEL", "sonar"),
        },
        Provider {
            code: "cerebras",
            name: "Cerebras",
            api_key: env::var("CEREBRAS_API_KEY").ok(),
            base_url: env_or("CEREBRAS_BASE_URL", "https://api.cerebras.ai/v1"),
            model: env_or("CEREBRAS_MODEL", "llama-3.3-70b"),
        },
        Provider {
            code: "fireworks",
            name: "Fireworks",
            api_key: env::var("FIREWORKS_API_KEY").ok(),
            base_url: env_or(
                "FIREWORKS_BASE_URL",
                "https://api.fireworks.ai/inference/v1",
            ),
            model: env_or(
                "FIREWORKS_MODEL",
                "accounts/fireworks/models/llama-v3p3-70b-instruct",
            ),
        },
        Provider {
            code: "minimax",
            name: "MiniMax",
            api_key: env::var("MINIMAX_API_KEY").ok(),
            base_url: env_or("MINIMAX_BASE_URL", "https://api.minimax.io/v1"),
            model: env_or("MINIMAX_MODEL", "MiniMax-M2.7"),
        },
        Provider {
            code: "kimi",
            name: "Moonshot Kimi",
            api_key: env::var("MOONSHOT_API_KEY").ok(),
            base_url: env_or("MOONSHOT_BASE_URL", "https://api.moonshot.ai/v1"),
            model: env_or("MOONSHOT_MODEL", "kimi-k2.7-code"),
        },
        Provider {
            code: "glm",
            name: "Zhipu GLM",
            api_key: env::var("ZAI_API_KEY").ok(),
            base_url: env_or("ZAI_BASE_URL", "https://api.z.ai/api/paas/v4"),
            model: env_or("ZAI_MODEL", "glm-4.7"),
        },
        Provider {
            code: "sambanova",
            name: "SambaNova",
            api_key: env::var("SAMBANOVA_API_KEY").ok(),
            base_url: env_or("SAMBANOVA_BASE_URL", "https://api.sambanova.ai/v1"),
            model: env_or("SAMBANOVA_MODEL", "Meta-Llama-3.3-70B-Instruct"),
        },
        Provider {
            code: "nvidia",
            name: "NVIDIA NIM",
            api_key: env::var("NVIDIA_API_KEY").ok(),
            base_url: env_or("NVIDIA_BASE_URL", "https://integrate.api.nvidia.com/v1"),
            model: env_or("NVIDIA_MODEL", "meta/llama-3.3-70b-instruct"),
        },
        Provider {
            code: "github",
            name: "GitHub Models",
            api_key: env::var("GITHUB_MODELS_TOKEN").ok(),
            base_url: env_or(
                "GITHUB_MODELS_BASE_URL",
                "https://models.github.ai/inference",
            ),
            model: env_or("GITHUB_MODELS_MODEL", "openai/gpt-4o"),
        },
        // ── Lokal (gratis, tanpa internet) — api_key tak wajib ──────────────
        Provider {
            code: "ollama",
            name: "Ollama (lokal)",
            api_key: Some(env_or("OLLAMA_API_KEY", "ollama")),
            base_url: env_or("OLLAMA_BASE_URL", "http://localhost:11434/v1"),
            model: env_or("OLLAMA_MODEL", "llama3.3"),
        },
        Provider {
            code: "lmstudio",
            name: "LM Studio (lokal)",
            api_key: Some(env_or("LMSTUDIO_API_KEY", "lm-studio")),
            base_url: env_or("LMSTUDIO_BASE_URL", "http://localhost:1234/v1"),
            model: env_or("LMSTUDIO_MODEL", "local-model"),
        },
    ]
}

/// Provider yang berjalan lokal — tak butuh API key (tak perlu prompt).
pub fn is_local(code: &str) -> bool {
    matches!(code, "ollama" | "lmstudio")
}

/// Daftar pendek model pilihan untuk sebuah provider: (id_model, label_manusia).
/// Item pertama = rekomendasi default. Kosong → model bebas/berbasis-path
/// (mis. OpenRouter/Together/Ollama) → pakai default, tak usah picker model.
pub fn models_for(code: &str) -> &'static [(&'static str, &'static str)] {
    match code {
        "openrouter" => &[
            ("openrouter/auto", "auto-pilih terbaik · rekomendasi"),
            ("anthropic/claude-sonnet-4.5", "coding terbaik"),
            ("openai/gpt-5.1", "coding & agentic"),
            ("google/gemini-3.1-pro-preview", "paling pintar"),
            ("deepseek/deepseek-chat", "paling murah"),
            ("openai/gpt-oss-120b:free", "gratis"),
        ],
        "qwen" => &[
            ("qwen-max", "seimbang · rekomendasi"),
            ("qwen-plus", "murah & cepat"),
        ],
        "openai" => &[
            ("gpt-5.1", "coding & agentic · rekomendasi"),
            ("gpt-5.5", "frontier, paling pintar"),
            ("gpt-5.2-chat-latest", "instant, cepat"),
        ],
        "deepseek" => &[
            ("deepseek-chat", "seimbang · rekomendasi"),
            ("deepseek-reasoner", "penalaran terbaik"),
        ],
        "gemini" => &[
            ("gemini-2.5-flash", "cepat & murah · rekomendasi"),
            ("gemini-3.1-pro-preview", "paling pintar"),
        ],
        "claude" => &[
            ("claude-sonnet-4-6", "juara coding · rekomendasi"),
            ("claude-opus-4-8", "paling pintar (mahal)"),
            ("claude-haiku-4-5", "cepat & murah"),
        ],
        "grok" => &[
            ("grok-4.3", "flagship efisien · rekomendasi"),
            ("grok-4.1", "alternatif"),
        ],
        "groq" => &[
            ("llama-3.3-70b-versatile", "kilat · rekomendasi"),
            ("llama-3.1-8b-instant", "paling cepat"),
        ],
        "mistral" => &[
            ("mistral-large-latest", "rekomendasi"),
            ("mistral-small-latest", "murah & cepat"),
        ],
        "perplexity" => &[
            ("sonar", "rekomendasi"),
            ("sonar-pro", "lebih pintar"),
        ],
        "cerebras" => &[
            ("llama-3.3-70b", "kilat · rekomendasi"),
            ("llama3.1-8b", "paling cepat"),
        ],
        "minimax" => &[
            ("MiniMax-M2.7", "flagship coding · rekomendasi"),
            ("MiniMax-M3", "frontier, 1M context (paling pintar)"),
            ("MiniMax-M2.7-highspeed", "lebih cepat"),
        ],
        "kimi" => &[
            ("kimi-k2.7-code", "agentic coding terbaru · rekomendasi"),
            ("kimi-k2-thinking", "penalaran lebih dalam"),
            ("kimi-k2.6", "versi sebelumnya"),
        ],
        "glm" => &[
            ("glm-4.7", "coding kuat · rekomendasi"),
            ("glm-5.2", "paling pintar (rival Opus)"),
            ("glm-4.7-flash", "gratis & cepat"),
        ],
        // openrouter / together / fireworks / ollama / lmstudio: ID bebas → kosong.
        _ => &[],
    }
}

/// Kode provider default dari env `VOCA_PROVIDER`, fallback ke "qwen".
#[allow(dead_code)]
pub fn default_code() -> String {
    let want = env_or("VOCA_PROVIDER", "qwen");
    if all().iter().any(|p| p.code == want) {
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
        "gemini" => "GEMINI_API_KEY",
        "claude" => "ANTHROPIC_API_KEY",
        "grok" => "XAI_API_KEY",
        "groq" => "GROQ_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "together" => "TOGETHER_API_KEY",
        "perplexity" => "PERPLEXITY_API_KEY",
        "cerebras" => "CEREBRAS_API_KEY",
        "fireworks" => "FIREWORKS_API_KEY",
        "minimax" => "MINIMAX_API_KEY",
        "kimi" => "MOONSHOT_API_KEY",
        "glm" => "ZAI_API_KEY",
        "sambanova" => "SAMBANOVA_API_KEY",
        "nvidia" => "NVIDIA_API_KEY",
        "github" => "GITHUB_MODELS_TOKEN",
        "ollama" => "OLLAMA_API_KEY",
        "lmstudio" => "LMSTUDIO_API_KEY",
        _ => "DASHSCOPE_API_KEY", // qwen
    }
}

/// Ambil provider berdasarkan kode.
pub fn by_code(code: &str) -> Option<Provider> {
    all().into_iter().find(|p| p.code == code)
}
