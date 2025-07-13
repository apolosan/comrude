use async_trait::async_trait;
use comrude_core::{
    GenerationRequest, GenerationResponse, StreamChunk, ProviderCapabilities,
    ModelInfo, HealthStatus, Result, ProviderError, OllamaConfig,
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
pub struct OllamaProvider {
    client: Client,
    config: OllamaConfig,
}

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: Option<String>,
    template: Option<String>,
    context: Option<Vec<i32>>,
    stream: bool,
    raw: Option<bool>,
    format: Option<String>,
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: Option<f32>,
    top_p: Option<f32>,
    top_k: Option<i32>,
    num_predict: Option<i32>,
}

#[derive(Deserialize)]
struct OllamaResponse {
    model: String,
    created_at: String,
    response: String,
    done: bool,
    context: Option<Vec<i32>>,
    total_duration: Option<u64>,
    load_duration: Option<u64>,
    prompt_eval_count: Option<u32>,
    prompt_eval_duration: Option<u64>,
    eval_count: Option<u32>,
    eval_duration: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    modified_at: String,
    size: u64,
    digest: String,
    details: Option<OllamaModelDetails>,
}

#[derive(Deserialize)]
struct OllamaModelDetails {
    format: String,
    family: String,
    families: Option<Vec<String>>,
    parameter_size: String,
    quantization_level: String,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        Ok(Self {
            client,
            config,
        })
    }

    fn build_prompt_from_messages(&self, messages: &[Message], main_prompt: &str) -> String {
        let mut prompt_parts = Vec::new();

        for msg in messages {
            let content = match &msg.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Code { language: _, content } => content.clone(),
                MessageContent::File { path: _, preview } => {
                    preview.clone().unwrap_or_else(|| "File content".to_string())
                },
                MessageContent::Error { error_type: _, message } => message.clone(),
                MessageContent::Progress { stage, percentage: _ } => stage.clone(),
            };

            let prefix = match &msg.sender {
                MessageSender::User => "Human: ",
                MessageSender::Assistant { .. } => "Assistant: ",
                MessageSender::System => "System: ",
            };

            prompt_parts.push(format!("{}{}", prefix, content));
        }

        prompt_parts.push(format!("Human: {}", main_prompt));
        prompt_parts.push("Assistant: ".to_string());

        prompt_parts.join("\n\n")
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn version(&self) -> &str {
        "1.0.0"
    }

    fn description(&self) -> &str {
        "Ollama local models provider"
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            max_context_length: 32768, // Varies by model, this is a conservative estimate
            supports_streaming: true,
            supports_tools: false, // Ollama doesn't support structured tools yet
            supports_vision: false, // Most Ollama models don't support vision
            supports_embeddings: true,
            supports_fine_tuning: false,
            rate_limits: comrude_core::RateLimits {
                requests_per_minute: 1000, // No rate limits for local Ollama
                tokens_per_minute: 100000,
            },
        }
    }

    fn supported_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "llama2:7b".to_string(),
                name: "Llama 2 7B".to_string(),
                description: "Meta's Llama 2 7B parameter model".to_string(),
                context_length: 4096,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0, // Local models are free
                    output: 0.0,
                },
                capabilities: vec!["text".to_string()],
            },
            ModelInfo {
                id: "llama2:13b".to_string(),
                name: "Llama 2 13B".to_string(),
                description: "Meta's Llama 2 13B parameter model".to_string(),
                context_length: 4096,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0,
                    output: 0.0,
                },
                capabilities: vec!["text".to_string()],
            },
            ModelInfo {
                id: "codellama:7b".to_string(),
                name: "Code Llama 7B".to_string(),
                description: "Meta's Code Llama 7B parameter model for code generation".to_string(),
                context_length: 16384,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0,
                    output: 0.0,
                },
                capabilities: vec!["text".to_string(), "code".to_string()],
            },
            ModelInfo {
                id: "codellama:13b".to_string(),
                name: "Code Llama 13B".to_string(),
                description: "Meta's Code Llama 13B parameter model for code generation".to_string(),
                context_length: 16384,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0,
                    output: 0.0,
                },
                capabilities: vec!["text".to_string(), "code".to_string()],
            },
            ModelInfo {
                id: "mistral:7b".to_string(),
                name: "Mistral 7B".to_string(),
                description: "Mistral AI's 7B parameter model".to_string(),
                context_length: 8192,
                cost_per_1k_tokens: CostPer1k {
                    input: 0.0,
                    output: 0.0,
                },
                capabilities: vec!["text".to_string()],
            },
        ]
    }

    async fn health_check(&self) -> Result<HealthStatus> {
        let url = format!("{}/api/tags", self.config.endpoint);
        
        let start = std::time::Instant::now();
        let response = self.client
            .get(&url)
            .send()
            .await;

        let latency_ms = start.elapsed().as_millis() as u64;

        match response {
            Ok(resp) if resp.status().is_success() => {
                if latency_ms > 5000 {
                    Ok(HealthStatus::Degraded { latency_ms })
                } else {
                    Ok(HealthStatus::Healthy)
                }
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
            HealthStatus::Healthy | HealthStatus::Degraded { .. } => Ok(()),
            _ => Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "ollama".to_string(),
                    message: "Ollama server is not healthy".to_string(),
                }
            ))
        }
    }

    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse> {
        let url = format!("{}/api/generate", self.config.endpoint);
        
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

        let prompt = self.build_prompt_from_messages(&all_messages, &request.prompt);

        let options = OllamaOptions {
            temperature: request.temperature,
            top_p: None,
            top_k: None,
            num_predict: request.max_tokens.map(|t| t as i32),
        };

        let ollama_request = OllamaRequest {
            model,
            prompt,
            system: request.system_prompt,
            template: None,
            context: None,
            stream: false,
            raw: None,
            format: None,
            options: Some(options),
        };

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(comrude_core::ComrudeError::Provider(
                ProviderError::ApiError {
                    provider: "ollama".to_string(),
                    message: format!("HTTP {}: {}", status, error_text),
                }
            ));
        }

        let ollama_response: OllamaResponse = response.json().await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        // Calculate token usage from eval counts if available
        let tokens_used = if let (Some(prompt_tokens), Some(completion_tokens)) = 
            (ollama_response.prompt_eval_count, ollama_response.eval_count) {
            TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            }
        } else {
            TokenUsage::default()
        };

        let finish_reason = if ollama_response.done {
            FinishReason::Stop
        } else {
            FinishReason::Length
        };

        Ok(GenerationResponse {
            content: ollama_response.response,
            model_used: ollama_response.model,
            tokens_used,
            cost: 0.0, // Local models are free
            finish_reason,
            tool_calls: Vec::new(), // Ollama doesn't support tools yet
            metadata: {
                let mut meta = std::collections::HashMap::new();
                if let Some(duration) = ollama_response.total_duration {
                    meta.insert("total_duration_ns".to_string(), duration.into());
                }
                if let Some(duration) = ollama_response.eval_duration {
                    meta.insert("eval_duration_ns".to_string(), duration.into());
                }
                meta
            },
        })
    }

    async fn generate_stream(&self, _request: GenerationRequest) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>> {
        // For now, return an error as streaming implementation is complex
        Err(comrude_core::ComrudeError::Provider(
            ProviderError::ApiError {
                provider: "ollama".to_string(),
                message: "Streaming not implemented yet".to_string(),
            }
        ))
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embeddings", self.config.endpoint);
        
        let request_body = serde_json::json!({
            "model": self.config.default_model,
            "prompt": text
        });

        let response = self.client
            .post(&url)
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
                    provider: "ollama".to_string(),
                    message: format!("HTTP {}: {}", status, error_text),
                }
            ));
        }

        let response_json: serde_json::Value = response.json().await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        let embedding = response_json["embedding"]
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
        let url = format!("{}/api/tags", self.config.endpoint);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| comrude_core::ComrudeError::Network(e))?;

        if !response.status().is_success() {
            return Ok(self.supported_models()); // Fallback to static list
        }

        let tags_response: OllamaTagsResponse = response.json().await
            .map_err(|_| comrude_core::ComrudeError::Provider(
                ProviderError::InvalidResponse("Failed to parse models response".to_string())
            ))?;

        let models = tags_response.models.into_iter()
            .map(|model| {
                // Try to determine context length from model name/family
                let context_length = if model.name.contains("codellama") {
                    16384
                } else if model.name.contains("llama2") {
                    4096
                } else if model.name.contains("mistral") {
                    8192
                } else {
                    4096 // Default
                };

                ModelInfo {
                    id: model.name.clone(),
                    name: model.name.clone(),
                    description: format!("Ollama model: {}", model.name),
                    context_length,
                    cost_per_1k_tokens: CostPer1k {
                        input: 0.0,
                        output: 0.0,
                    },
                    capabilities: vec!["text".to_string()],
                }
            })
            .collect();

        Ok(models)
    }
}