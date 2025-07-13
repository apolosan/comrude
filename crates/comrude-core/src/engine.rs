use crate::{
    error::Result,
    types::{GenerationRequest, Message, ParsedCommand},
};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ComrudeEngine {
    conversation_history: Arc<RwLock<Vec<Message>>>,
    current_context: Arc<RwLock<Vec<String>>>,
}

impl ComrudeEngine {
    pub fn new() -> Self {
        Self {
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            current_context: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_message(&self, message: Message) {
        let mut history = self.conversation_history.write().await;
        history.push(message);
    }

    pub async fn get_conversation_history(&self) -> Vec<Message> {
        let history = self.conversation_history.read().await;
        history.clone()
    }

    pub async fn clear_conversation(&self) {
        let mut history = self.conversation_history.write().await;
        history.clear();
    }

    pub async fn add_context(&self, context: String) {
        let mut ctx = self.current_context.write().await;
        if !ctx.contains(&context) {
            ctx.push(context);
        }
    }

    pub async fn get_context(&self) -> Vec<String> {
        let ctx = self.current_context.read().await;
        ctx.clone()
    }

    pub async fn clear_context(&self) {
        let mut ctx = self.current_context.write().await;
        ctx.clear();
    }

    pub fn build_request_from_command(&self, command: &ParsedCommand) -> Result<GenerationRequest> {
        let mut request = GenerationRequest::default();

        match command.command_type {
            crate::types::CommandType::Ask => {
                if let Some(prompt) = command.args.first() {
                    request.prompt = prompt.clone();
                } else {
                    return Err(crate::error::ComrudeError::Command(
                        "Ask command requires a prompt".to_string()
                    ));
                }
            }
            crate::types::CommandType::Code => {
                if let Some(code_request) = command.args.first() {
                    request.prompt = format!(
                        "Generate code for: {}\n\nRequirements:\n- Include comments\n- Follow best practices\n- Provide complete, runnable code",
                        code_request
                    );
                } else {
                    return Err(crate::error::ComrudeError::Command(
                        "Code command requires a description".to_string()
                    ));
                }
            }
            crate::types::CommandType::Explain => {
                if let Some(target) = command.args.first() {
                    if std::path::Path::new(target).exists() {
                        let content = std::fs::read_to_string(target)
                            .map_err(|e| crate::error::ComrudeError::FileOp(e.to_string()))?;
                        request.prompt = format!(
                            "Explain this code in detail:\n\n```\n{}\n```\n\nProvide:\n- What it does\n- How it works\n- Key concepts used",
                            content
                        );
                    } else {
                        request.prompt = format!(
                            "Explain this code or concept:\n\n{}\n\nProvide a detailed explanation.",
                            target
                        );
                    }
                } else {
                    return Err(crate::error::ComrudeError::Command(
                        "Explain command requires a target".to_string()
                    ));
                }
            }
            _ => {
                return Err(crate::error::ComrudeError::Command(
                    "Command type not supported yet".to_string()
                ));
            }
        }

        // Apply flags
        if let Some(model) = command.flags.get("model") {
            request.model = Some(model.clone());
        }
        if let Some(provider) = command.flags.get("provider") {
            request.metadata.insert("preferred_provider".to_string(), provider.clone().into());
        }
        if command.flags.contains_key("stream") {
            request.stream = true;
        }
        if let Some(temp) = command.flags.get("temperature") {
            if let Ok(temperature) = temp.parse::<f32>() {
                request.temperature = Some(temperature);
            }
        }

        Ok(request)
    }
}

impl Default for ComrudeEngine {
    fn default() -> Self {
        Self::new()
    }
}