use async_trait::async_trait;
use comrude_core::{
    GenerationRequest, GenerationResponse, StreamChunk, ProviderCapabilities,
    ModelInfo, HealthStatus, Result, ProviderError, OpenAIConfig,
    Message, MessageSender, MessageContent, TokenUsage, FinishReason, CostPer1k
};
use crate::traits::LLMProvider;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use futures::Stream;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug)]
pub struct OpenAIProvider {
    client: Client,
    config: OpenAIConfig,
    api_key: String,
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    stream: Option<bool>,
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OpenAITool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAIFunction,
}

#[derive(Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    id: String,
    object: String,
    created: u64,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    index: u32,
    message: Option<OpenAIMessage>,
    delta: Option<OpenAIDelta>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
    object: String,
    created: u64,
    owned_by: String,
}

impl OpenAIProvider {
    pub fn new(config: OpenAIConfig) -> Result<Self> {
        let api_key = std::env::var(&config.api_key_env)
            .map_err(|_| comrude_core::ComrudeError::Provider(
                ProviderError::AuthFailed("OpenAI API key not found".to_string())
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

    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAIMessage> {
        messages.iter().map(|msg| {
            let role = match &msg.sender {
                MessageSender::User => "user",
                MessageSender::Assistant { .. } => "assistant",
                MessageSender::System => "system",
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

            OpenAIMessage {
                role: role.to_string(),
                content,
            }
        }).collect()
    }

    fn convert_tools(&self, tools: &[comrude_core::ToolDefinition]) -> Vec<OpenAITool> {
        tools.iter().map(|tool| {
            OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    parameters: tool.parameters.clone(),
                },
            }
        }).collect()
    }
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "OpenAI GPT models provider"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: 128000,
            supports_streaming: true,
            supports_tools: true,
            supports_vision: true,
            supports_embeddings: true,
            supports_fine_tuning: false,
            rate_limits: comrude_core::RateLimits {
                requests_per_minute: 3500,
                tokens_per_minute: 90000,
            },
        }
    }

    fn supported_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                description: "OpenAI's most capable model".to_string(),
                context_length: 8192,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.03,
                    output: 0.06,
                },
                capabilities: vec!["text".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-4o".to_string(),
                name: "GPT-4o".to_string(),
                description: "OpenAI's newest model with vision capabilities".to_string(),
                context_length: 128000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.005,
                    output: 0.015,
                },
                capabilities: vec!["text".to_string(), "tools".to_string(), "vision".to_string()],
            },
            ModelInfo {
                id: "gpt-4o-mini".to_string(),
                name: "GPT-4o Mini".to_string(),
                description: "Smaller, faster version of GPT-4o".to_string(),
                context_length: 128000,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.00015,
                    output: 0.0006,
                },
                capabilities: vec!["text".to_string(), "tools".to_string()],
            },
            ModelInfo {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                description: "Fast and efficient model for most tasks".to_string(),
                context_length: 16384,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0015,
                    output: 0.002,
                },
                capabilities: vec!["text".to_string(), "tools".to_string()],
            },
        ]
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        let url = format!("{}/models", self.config.base_url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
                    provider: "openai".to_string(),
                    message: "OpenAI API is not healthy".to_string(),
                }
            ))
        }
    }

    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let url = format!("{}/chat/completions", self.config.base_url);
        
        let model = request.model.unwrap_or_else(|| self.config.default_model.clone());
        
        // Build context messages
        let mut messages = Vec::new();
        
        // Add system prompt if provided
        if let Some(system_prompt) = &request.system_prompt {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: system_prompt.clone(),
            });
        }

        // Add context messages from request
        for context_item in &request.context {
            messages.push(OpenAIMessage {
                role: "user".to_string(),
                content: format!("Context: {}", context_item.content),
            });
        }

        // Add main prompt
        messages.push(OpenAIMessage {
            role: "user".to_string(),
            content: request.prompt.clone(),
        });

        let openai_request = OpenAIRequest {
            model,
            messages,
            max_tokens: request.max_tokens,
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
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "openai".to_string(),
                    message: format!("HTTP {}: {}", status, error_text),
                }
            ));
        }

        let openai_response: OpenAIResponse = response.json().await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        let choice = openai_response.choices.into_iter().next()
            .ok_or_else(|| comrude_core::ComrudeError::Provider(
                ProviderError::InvalidResponse("No choices in response".to_string())
            ))?;

        let content = choice.message
            .map(|msg| msg.content)
            .unwrap_or_else(|| "No content in response".to_string());

        let tokens_used = openai_response.usage.map(|usage| TokenUsage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
        }).unwrap_or_default();

        let finish_reason = match choice.finish_reason.as_deref() {
            Some("stop") => FinishReason::Stop,
            Some("length") => FinishReason::Length,
            Some("tool_calls") => FinishReason::ToolCalls,
            Some("content_filter") => FinishReason::ContentFilter,
            Some(other) => FinishReason::Error(other.to_string()),
            None => FinishReason::Stop,
        };

        Ok(GenerationResponse {
            content,
            model_used: openai_response.model,
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
                provider: "openai".to_string(),
                message: "Streaming not implemented yet".to_string(),
            }
        ))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.config.base_url);
        
        let request_body = serde_json::json!({
            "input": text,
            "model": "text-embedding-ada-002"
        });

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "openai".to_string(),
                    message: format!("HTTP {}: {}", status, error_text),
                }
            ));
        }

        let response_json: serde_json::Value = response.json().await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        let embedding = response_json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| comrude_core::ComrudeError::Provider(
                ProviderError::InvalidResponse("No embedding in response".to_string())
            ))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();

        Ok(embedding)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/models", self.config.base_url);
        
        let response = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            return Ok(self.supported_models()); // Fallback to static list
        }

        let models_response: OpenAIModelsResponse = response.json().await
            .map_err(|_| comrude_core::ComrudeError::Provider(
                ProviderError::InvalidResponse("Failed to parse models response".to_string())
            ))?;

        let models = models_response.data.into_iter()
            .filter(|model| model.id.starts_with("gpt-"))
            .map(|model| ModelInfo {
                id: model.id.clone(),
                name: model.id.clone(),
                description: format!("OpenAI model: {}", model.id),
                context_length: 4096, // Default, could be improved with model-specific data
                cost_per_1k_tokens: CostPer1k {
                    input: 0.001,
                    output: 0.002,
                },
                capabilities: vec!["text".to_string()],
            })
            .collect();

        Ok(models)
    }
}