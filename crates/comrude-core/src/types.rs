use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub stream: bool,
    pub tools: Vec<ToolDefinition>,
    pub context: Vec<ContextItem>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Default for GenerationRequest {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            system_prompt: None,
            model: None,
            max_tokens: Some(4096),
            temperature: Some(0.7),
            stream: false,
            tools: Vec::new(),
            context: Vec::new(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationResponse {
    pub content: String,
    pub model_used: String,
    pub tokens_used: TokenUsage,
    pub cost: f64,
    pub finish_reason: FinishReason,
    pub tool_calls: Vec<ToolCall>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamChunk {
    Content(String),
    ToolCall(ToolCall),
    TokenUsage(TokenUsage),
    Done,
    Error(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub item_type: ContextType,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextType {
    File { path: String },
    Code { language: String },
    Text,
    GitDiff,
    Command { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub context_length: u32,
    pub cost_per_1k_tokens: CostPer1k,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostPer1k {
    pub input: f64,
    pub output: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub max_context_length: u32,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_embeddings: bool,
    pub supports_fine_tuning: bool,
    pub rate_limits: RateLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub requests_per_minute: u32,
    pub tokens_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded { latency_ms: u64 },
    Unhealthy,
    RateLimited { reset_time: DateTime<Utc> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub sender: MessageSender,
    pub content: MessageContent,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageSender {
    User,
    Assistant { provider: String, model: String },
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Code { language: String, content: String },
    File { path: String, preview: Option<String> },
    Error { error_type: String, message: String },
    Progress { stage: String, percentage: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageStatus {
    Pending,
    Processing,
    Complete,
    Error,
}

#[derive(Debug, Clone)]
pub enum CommandType {
    Ask,
    Code,
    Explain,
    Help,
    Context,
    Provider,
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub command_type: CommandType,
    pub args: Vec<String>,
    pub flags: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub tokens_used: TokenUsage,
    pub cost: f64,
    pub request_type: RequestType,
}

#[derive(Debug, Clone)]
pub enum RequestType {
    Generation,
    Embedding,
    FineTuning,
}

impl Message {
    pub fn new_user(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::User,
            content: MessageContent::Text(content),
            status: MessageStatus::Complete,
        }
    }

    pub fn new_assistant(content: String, provider: String, model: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::Assistant { provider, model },
            content: MessageContent::Text(content),
            status: MessageStatus::Complete,
        }
    }

    pub fn new_system(content: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::System,
            content: MessageContent::Text(content),
            status: MessageStatus::Complete,
        }
    }
}