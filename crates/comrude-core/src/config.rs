use crate::error::{ConfigError, ConfigResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub app: AppConfig,
    pub ui: UIConfig,
    pub providers: ProvidersConfig,
    pub files: FilesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIConfig {
    pub theme: String,
    pub auto_save_history: bool,
    pub max_history_items: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub openai: Option<OpenAIConfig>,
    pub anthropic: Option<AnthropicConfig>,
    pub ollama: Option<OllamaConfig>,
    pub google: Option<GoogleConfig>,
    pub huggingface: Option<HuggingFaceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    pub enabled: bool,
    pub api_key_env: String,
    pub default_model: String,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub enabled: bool,
    pub api_key_env: String,
    pub default_model: String,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub default_model: String,
    pub timeout_seconds: u64,
    pub auto_pull_models: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleConfig {
    pub enabled: bool,
    pub api_key_env: String,
    pub default_model: String,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HuggingFaceConfig {
    pub enabled: bool,
    pub api_key_env: String,
    pub default_model: String,
    pub max_tokens: u32,
    pub timeout_seconds: u64,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilesConfig {
    pub max_file_size_mb: u64,
    pub allowed_extensions: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            app: AppConfig {
                name: "Comrude".to_string(),
                version: "0.1.0".to_string(),
            },
            ui: UIConfig {
                theme: "dark".to_string(),
                auto_save_history: true,
                max_history_items: 1000,
            },
            providers: ProvidersConfig {
                openai: Some(OpenAIConfig {
                    enabled: true,
                    api_key_env: "OPENAI_API_KEY".to_string(),
                    default_model: "gpt-4".to_string(),
                    max_tokens: 4096,
                    timeout_seconds: 30,
                    base_url: "https://api.openai.com/v1".to_string(),
                }),
                anthropic: Some(AnthropicConfig {
                    enabled: true,
                    api_key_env: "ANTHROPIC_API_KEY".to_string(),
                    default_model: "claude-3-5-sonnet-20241022".to_string(),
                    max_tokens: 4096,
                    timeout_seconds: 30,
                    base_url: "https://api.anthropic.com".to_string(),
                }),
                ollama: Some(OllamaConfig {
                    enabled: true,
                    endpoint: "http://localhost:11434".to_string(),
                    default_model: "codellama:7b".to_string(),
                    timeout_seconds: 60,
                    auto_pull_models: false,
                }),
                google: None,
                huggingface: None,
            },
            files: FilesConfig {
                max_file_size_mb: 10,
                allowed_extensions: vec![
                    "rs", "py", "js", "ts", "go", "java", "cpp", "c", 
                    "md", "txt", "json", "yaml", "toml"
                ].into_iter().map(String::from).collect(),
            },
        }
    }
}

impl Config {
    pub fn load() -> ConfigResult<Self> {
        let mut settings = config::Config::builder();

        // 1. Load default configuration
        settings = settings.add_source(
            config::File::from_str(
                include_str!("../../../config/default.toml"),
                config::FileFormat::Toml
            )
        );

        // 2. Load user configuration if it exists
        if let Some(config_dir) = dirs::config_dir() {
            let user_config = config_dir.join("comrude").join("config.toml");
            if user_config.exists() {
                settings = settings.add_source(
                    config::File::from(user_config).required(false)
                );
            }
        }

        // 3. Override with environment variables
        settings = settings.add_source(
            config::Environment::with_prefix("COMRUDE").separator("_")
        );

        settings
            .build()
            .map_err(|e| ConfigError::Invalid(e.to_string()))?
            .try_deserialize()
            .map_err(|e| ConfigError::Invalid(e.to_string()))
    }

    pub fn validate(&self) -> ConfigResult<()> {
        // Validate that at least one provider is enabled
        let has_enabled_provider = [
            self.providers.openai.as_ref().map(|p| p.enabled).unwrap_or(false),
            self.providers.anthropic.as_ref().map(|p| p.enabled).unwrap_or(false),
            self.providers.ollama.as_ref().map(|p| p.enabled).unwrap_or(false),
            self.providers.google.as_ref().map(|p| p.enabled).unwrap_or(false),
            self.providers.huggingface.as_ref().map(|p| p.enabled).unwrap_or(false),
        ].iter().any(|&enabled| enabled);

        if !has_enabled_provider {
            return Err(ConfigError::Invalid(
                "At least one provider must be enabled".to_string()
            ));
        }

        // Validate OpenAI config if enabled
        if let Some(openai) = &self.providers.openai {
            if openai.enabled {
                self.validate_api_key_env(&openai.api_key_env)?;
                if openai.max_tokens == 0 {
                    return Err(ConfigError::InvalidValue {
                        field: "providers.openai.max_tokens".to_string(),
                        value: "0".to_string(),
                    });
                }
            }
        }

        // Validate Anthropic config if enabled
        if let Some(anthropic) = &self.providers.anthropic {
            if anthropic.enabled {
                self.validate_api_key_env(&anthropic.api_key_env)?;
                if anthropic.max_tokens == 0 {
                    return Err(ConfigError::InvalidValue {
                        field: "providers.anthropic.max_tokens".to_string(),
                        value: "0".to_string(),
                    });
                }
            }
        }

        // Validate file size limit
        if self.files.max_file_size_mb == 0 {
            return Err(ConfigError::InvalidValue {
                field: "files.max_file_size_mb".to_string(),
                value: "0".to_string(),
            });
        }

        Ok(())
    }

    fn validate_api_key_env(&self, env_var: &str) -> ConfigResult<()> {
        if std::env::var(env_var).is_err() {
            return Err(ConfigError::EnvVarNotFound(env_var.to_string()));
        }
        Ok(())
    }

    pub fn get_enabled_providers(&self) -> Vec<String> {
        let mut providers = Vec::new();
        
        if self.providers.openai.as_ref().map(|p| p.enabled).unwrap_or(false) {
            providers.push("openai".to_string());
        }
        if self.providers.anthropic.as_ref().map(|p| p.enabled).unwrap_or(false) {
            providers.push("anthropic".to_string());
        }
        if self.providers.ollama.as_ref().map(|p| p.enabled).unwrap_or(false) {
            providers.push("ollama".to_string());
        }
        if self.providers.google.as_ref().map(|p| p.enabled).unwrap_or(false) {
            providers.push("google".to_string());
        }
        if self.providers.huggingface.as_ref().map(|p| p.enabled).unwrap_or(false) {
            providers.push("huggingface".to_string());
        }
        
        providers
    }
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key_env: "OPENAI_API_KEY".to_string(),
            default_model: "gpt-4".to_string(),
            max_tokens: 4096,
            timeout_seconds: 30,
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}

impl Default for AnthropicConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key_env: "ANTHROPIC_API_KEY".to_string(),
            default_model: "claude-3-5-sonnet-20241022".to_string(),
            max_tokens: 4096,
            timeout_seconds: 30,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "http://localhost:11434".to_string(),
            default_model: "codellama:7b".to_string(),
            timeout_seconds: 60,
            auto_pull_models: false,
        }
    }
}