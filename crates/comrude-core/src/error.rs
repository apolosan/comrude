use thiserror::Error;

#[derive(Error, Debug)]
pub enum ComrudeError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Command error: {0}")]
    Command(String),

    #[error("Context error: {0}")]
    Context(String),

    #[error("File operation error: {0}")]
    FileOp(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing configuration file")]
    MissingFile,

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid value for field {field}: {value}")]
    InvalidValue { field: String, value: String },

    #[error("Environment variable not found: {0}")]
    EnvVarNotFound(String),
}

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Provider not found: {0}")]
    NotFound(String),

    #[error("Provider not configured: {0}")]
    NotConfigured(String),

    #[error("Authentication failed for provider {0}")]
    AuthFailed(String),

    #[error("Rate limit exceeded for provider {0}")]
    RateLimited(String),

    #[error("API error from {provider}: {message}")]
    ApiError { provider: String, message: String },

    #[error("Model not available: {model} on provider {provider}")]
    ModelNotAvailable { provider: String, model: String },

    #[error("Network timeout for provider {0}")]
    Timeout(String),

    #[error("Invalid response from provider {0}")]
    InvalidResponse(String),
}

pub type Result<T> = std::result::Result<T, ComrudeError>;
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;
pub type ProviderResult<T> = std::result::Result<T, ProviderError>;