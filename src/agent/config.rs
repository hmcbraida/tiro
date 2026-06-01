//! Agent configuration: provider selection, model, prompt, secrets.
//!
//! Loaded as part of the top-level config (see `setup::load_config`).
//! Resolution rules:
//! - `*_path` wins over the inline form (api_key_path > api_key, etc.).
//! - Paths in the toml are resolved relative to the directory containing
//!   `config.toml`, not the CWD.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    None,
    OpenAi,
    Anthropic,
    Ollama,
}

impl Provider {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Provider::None),
            "openai" => Some(Provider::OpenAi),
            "anthropic" => Some(Provider::Anthropic),
            "ollama" => Some(Provider::Ollama),
            _ => None,
        }
    }

    pub fn requires_api_key(self) -> bool {
        matches!(self, Provider::OpenAi | Provider::Anthropic)
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::None => "none",
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Ollama => "ollama",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-haiku-4-5",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Ollama => "llama3.2",
            Provider::None => "",
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Resolved agent config -- paths read off disk, defaults filled in.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub provider: Provider,
    pub model: String,
    pub system_prompt: String,
    pub api_key: Option<String>,
    pub ollama_url: String,
}

impl AgentConfig {
    pub fn disabled() -> Self {
        Self {
            provider: Provider::None,
            model: String::new(),
            system_prompt: crate::agent::prompt::DEFAULT.to_string(),
            api_key: None,
            ollama_url: default_ollama_url(),
        }
    }
}

/// Raw `[agent]` block as deserialised from toml. Crate-private; only the
/// resolved [`AgentConfig`] crosses module boundaries.
#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct RawAgent {
    #[serde(default)]
    pub(crate) provider: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
    #[serde(default)]
    pub(crate) system_prompt: Option<String>,
    #[serde(default)]
    pub(crate) system_prompt_path: Option<String>,
    #[serde(default)]
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) api_key_path: Option<String>,
    #[serde(default)]
    pub(crate) ollama_url: Option<String>,
}

impl RawAgent {
    /// Resolve the raw config relative to the config file's directory.
    pub(crate) fn resolve(
        self,
        config_dir: &Path,
    ) -> Result<AgentConfig, AgentConfigError> {
        let provider = match self.provider.as_deref() {
            None => Provider::None,
            Some(s) => Provider::parse(s)
                .ok_or_else(|| AgentConfigError::UnknownProvider(s.into()))?,
        };

        let model = self
            .model
            .unwrap_or_else(|| provider.default_model().to_string());

        let system_prompt = match (self.system_prompt_path, self.system_prompt)
        {
            (Some(path), _) => read_text(config_dir.join(path))?,
            (None, Some(inline)) => inline,
            (None, None) => crate::agent::prompt::DEFAULT.to_string(),
        };

        let api_key = match (self.api_key_path, self.api_key) {
            (Some(path), _) => Some(read_text(config_dir.join(path))?),
            (None, Some(inline)) => Some(inline),
            (None, None) => None,
        };

        if provider.requires_api_key() && api_key.is_none() {
            return Err(AgentConfigError::MissingApiKey {
                provider: provider.label(),
            });
        }

        Ok(AgentConfig {
            provider,
            model,
            system_prompt,
            api_key,
            ollama_url: self.ollama_url.unwrap_or_else(default_ollama_url),
        })
    }
}

fn read_text(path: PathBuf) -> Result<String, AgentConfigError> {
    std::fs::read_to_string(&path)
        .map(|s| s.trim_end().to_string())
        .map_err(|e| AgentConfigError::Io(path, e))
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

#[derive(Debug)]
pub enum AgentConfigError {
    Io(PathBuf, std::io::Error),
    MissingApiKey { provider: &'static str },
    UnknownProvider(String),
}

impl fmt::Display for AgentConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentConfigError::Io(p, e) => {
                write!(f, "reading {}: {e}", p.display())
            }
            AgentConfigError::MissingApiKey { provider } => {
                write!(
                    f,
                    "[agent].api_key (or api_key_path) is required for provider '{provider}'"
                )
            }
            AgentConfigError::UnknownProvider(p) => {
                write!(
                    f,
                    "[agent].provider '{p}' is not one of: none, openai, anthropic, ollama"
                )
            }
        }
    }
}

impl std::error::Error for AgentConfigError {}
