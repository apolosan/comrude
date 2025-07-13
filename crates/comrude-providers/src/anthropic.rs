use async_trait::async_trait;
use comrude_core::{
    GenerationRequest, GenerationResponse, StreamChunk, ProviderCapabilities,
    ModelInfo, HealthStatus, Result, ProviderError, AnthropicConfig,
    Message, MessageSender, MessageContent, TokenUsage, FinishReason, CostPer1k
};
use chrono::Utc;
use uuid::Uuid;
use crate::traits::LLMProvider;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use futures::Stream;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug)]
pub struct AnthropicProvider {
    client: Client,
    config: AnthropicConfig,
    api_key: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    system: Option<String>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Vec<AnthropicTool>>,
}

#[derive(Serialize, Deserialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    id: String,
    #[serde(rename = "type")]
    response_type: String,
    role: String,
    content: Vec<AnthropicContent>,
    model: String,
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(config: AnthropicConfig) -> Result<Self> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| comrude_core::ComrudeError::Provider(
                ProviderError::AuthFailed("Anthropic API key not found".to_string())
            ))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        Ok(Self {
            client,
            config,
            api_key,
        })
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<AnthropicMessage> {
        messages.iter().filter_map(|msg| {
            // Anthropic doesn't support system messages in the messages array
            if matches!(msg.sender, MessageSender::System) {
                return None;
            }

            let role = match &msg.sender {
                MessageSender::User => "user",
                MessageSender::Assistant { .. } => "assistant",
                MessageSender::System => return None, // Handled separately
            };

            let content = match &msg.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Code { language: _, content } => content.clone(),
                MessageContent::File { path: _, preview } => {
                    preview.clone().unwrap_or_else(|| "File content".to_string())
                },
                MessageContent::Error { error_type: _, message } => message.clone(),
                MessageContent::Progress { stage, percentage: _ } => stage.clone(),
            };

            Some(AnthropicMessage {
                role: role.to_string(),
                content,
            })
        }).collect()
    }

    fn extract_system_prompt(&self, messages: &[Message]) -> Option<String> {
        messages.iter()
            .find(|msg| matches!(msg.sender, MessageSender::System))
            .and_then(|msg| match &msg.content {
                MessageContent::Text(text) => Some(text.clone()),
                _ => None,
            })
    }

    fn convert_tools(&self, tools: &[comrude_core::ToolDefinition]) -> Vec<AnthropicTool> {
        tools.iter().map(|tool| {
            AnthropicTool {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.parameters.clone(),
            }
        }).collect()
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Anthropic Claude models provider"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: 200000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_embeddings: false,
            supports_fine_tuning: false,
            rate_limits: comrude_core::RateLimits {
                requests_per_minute: 1000,
                tokens_per_minute: 100000,
            },
        }
    }

    fn supported_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "claude-3-haiku-20240307".to_string(),
                name: "Claude 3 Haiku".to_string(),
                description: "Fast and efficient model for light tasks".to_string(),
                context_length: 200000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.00025,
                    output: 0.00125,
                },
                capabilities: vec!["text".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "claude-3-sonnet-20240229".to_string(),
                name: "Claude 3 Sonnet".to_string(),
                description: "Balanced performance for a wide range of tasks".to_string(),
                context_length: 200000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.003,
                    output: 0.015,
                },
                capabilities: vec!["text".to_string(), "tools".to_string(), "vision".to_string()],
            },
            ModelInfo {
                id: "claude-3-opus-20240229".to_string(),
                name: "Claude 3 Opus".to_string(),
                description: "Most capable model for complex tasks".to_string(),
                context_length: 200000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.015,
                    output: 0.075,
                },
                capabilities: vec!["text".to_string(), "tools".to_string(), "vision".to_string()],
            },
            ModelInfo {
                id: "claude-3-5-sonnet-20241022".to_string(),
                name: "Claude 3.5 Sonnet".to_string(),
                description: "Enhanced sonnet model with improved capabilities".to_string(),
                context_length: 200000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.003,
                    output: 0.015,
                },
                capabilities: vec!["text".to_string(), "tools".to_string(), "vision".to_string()],
            },
        ]
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        // Anthropic doesn't have a simple health check endpoint
        // We'll try a minimal request to verify the API key works
        let test_request = AnthropicRequest {
            model: self.config.default_model.clone(),
            max_tokens: 1,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: "Hi".to_string(),
            }],
            system: None,
            temperature: Some(0.0),
            stream: Some(false),
            tools: None,
        };

        let url = format!("{}/v1/messages", self.config.base_url);
        
        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&test_request)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                Ok(HealthStatus::Healthy)
            }
            Ok(_resp) => {
                Ok(HealthStatus::Unhealthy)
            }
            Err(_) => {
                Ok(HealthStatus::Unhealthy)
            }
        }
    }

    async fn test_connection(&self) -> Result<()> {
        let health = self.health_check().await?;
        match health {
            HealthStatus::Healthy => Ok(()),
            _ => Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "anthropic".to_string(),
                    message: "Anthropic API is not healthy".to_string(),
                }
            ))
        }
    }

    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let url = format!("{}/v1/messages", self.config.base_url);
        
        let model = request.model.unwrap_or_else(|| self.config.default_model.clone());
        
        // Build context messages
        let mut all_messages = Vec::new();
        
        // Add context messages from request
        for context_item in &request.context {
            all_messages.push(Message {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                sender: MessageSender::User,
                content: MessageContent::Text(format!(
                    "Context: {}", 
                    context_item.content
                )),
                status: comrude_core::MessageStatus::Complete,
            });
        }

        // Add main prompt
        all_messages.push(Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::User,
            content: MessageContent::Text(request.prompt.clone()),
            status: comrude_core::MessageStatus::Complete,
        });

        let messages = self.convert_messages(&all_messages);
        let system_prompt = request.system_prompt;

        let anthropic_request = AnthropicRequest {
            model,
            max_tokens: request.max_tokens.unwrap_or(self.config.max_tokens),
            messages,
            system: system_prompt,
            temperature: request.temperature,
            stream: Some(false),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(self.convert_tools(&request.tools))
            },
        };

        let response = self.client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_request)
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "anthropic".to_string(),
                    message: format!("HTTP {}: {}", status, error_text),
                }
            ));
        }

        let anthropic_response: AnthropicResponse = response.json().await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        let content = anthropic_response.content
            .into_iter()
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join("\n");

        let tokens_used = TokenUsage {
            prompt_tokens: anthropic_response.usage.input_tokens,
            completion_tokens: anthropic_response.usage.output_tokens,
            total_tokens: anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens,
        };

        let finish_reason = match anthropic_response.stop_reason.as_deref() {
            Some("end_turn") => FinishReason::Stop,
            Some("max_tokens") => FinishReason::Length,
            Some("tool_use") => FinishReason::ToolCalls,
            Some(other) => FinishReason::Error(other.to_string()),
            None => FinishReason::Stop,
        };

        Ok(GenerationResponse {
            content,
            model_used: anthropic_response.model,
            tokens_used,
            cost: 0.0, // TODO: Calculate actual cost
            finish_reason,
            tool_calls: Vec::new(), // TODO: Extract tool calls
            metadata: std::collections::HashMap::new(),
        })
    }

    async fn generate_stream(&self, _request: GenerationRequest) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // For now, return an error as streaming implementation is complex
        Err(comrude_core::ComrudeError::Provider(
            ProviderError::ApiError {
                provider: "anthropic".to_string(),
                message: "Streaming not implemented yet".to_string(),
            }
        ))
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        // Anthropic doesn't provide embeddings
        Err(comrude_core::ComrudeError::Provider(
            ProviderError::ApiError {
                provider: "anthropic".to_string(),
                message: "Embeddings not supported by Anthropic".to_string(),
            }
        ))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        // Anthropic doesn't have a public models endpoint
        // Return the static list
        Ok(self.supported_models())
    }
}