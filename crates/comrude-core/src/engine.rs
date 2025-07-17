use crate::{
    error::Result,
    memory::{ContextMemoryManager, MemoryConfig},
    types::{GenerationRequest, Message, ParsedCommand, ContextItem},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Debug)]
pub struct ComrudeEngine {
    // Legacy fields for backward compatibility
    conversation_history: Arc<RwLock<Vec<Message>>>,
    current_context: Arc<RwLock<Vec<String>>>,
    
    // New memory management system
    memory_manager: Arc<RwLock<ContextMemoryManager>>,
    current_turn_id: Arc<RwLock<Option<Uuid>>>,
}

impl ComrudeEngine {
    pub fn new() -> Self {
        let memory_config = MemoryConfig::default();
        Self {
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            current_context: Arc::new(RwLock::new(Vec::new())),
            memory_manager: Arc::new(RwLock::new(ContextMemoryManager::new(memory_config))),
            current_turn_id: Arc::new(RwLock::new(None)),
        }
    }

    pub fn new_with_config(memory_config: MemoryConfig) -> Self {
        Self {
            conversation_history: Arc::new(RwLock::new(Vec::new())),
            current_context: Arc::new(RwLock::new(Vec::new())),
            memory_manager: Arc::new(RwLock::new(ContextMemoryManager::new(memory_config))),
            current_turn_id: Arc::new(RwLock::new(None)),
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

    // New memory-aware methods

    /// Initialize a new session with memory management
    pub async fn create_session(&self, name: Option<String>) -> Result<Uuid> {
        let mut manager = self.memory_manager.write().await;
        let session_id = manager.create_session(name).await
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))?;
        Ok(session_id)
    }

    /// Load an existing session
    pub async fn load_session(&self, session_id: Uuid) -> Result<()> {
        let mut manager = self.memory_manager.write().await;
        manager.load_session(session_id).await
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))?;
        Ok(())
    }

    /// Start a new conversation turn with context-aware processing
    pub async fn start_conversation_turn(&self, user_message: Message, context: Vec<ContextItem>) -> Result<Uuid> {
        let mut manager = self.memory_manager.write().await;
        let turn_id = manager.add_conversation_turn(user_message.clone(), context).await
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))?;
        
        // Store current turn ID
        let mut current_turn = self.current_turn_id.write().await;
        *current_turn = Some(turn_id);

        // Also update legacy conversation history for backward compatibility
        self.add_message(user_message).await;

        Ok(turn_id)
    }

    /// Complete the current conversation turn with assistant response
    pub async fn complete_conversation_turn(&self, assistant_response: Message) -> Result<()> {
        let current_turn = self.current_turn_id.read().await;
        if let Some(turn_id) = *current_turn {
            let mut manager = self.memory_manager.write().await;
            manager.complete_conversation_turn(turn_id, assistant_response.clone()).await
                .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))?;

            // Also update legacy conversation history
            self.add_message(assistant_response).await;
        } else {
            return Err(crate::error::ComrudeError::Memory("No active conversation turn".to_string()));
        }
        Ok(())
    }

    /// Get contextual information for the next LLM request
    pub async fn get_context_for_request(&self) -> Result<Vec<ContextItem>> {
        let manager = self.memory_manager.read().await;
        manager.get_context_for_request()
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))
    }

    /// Get conversation summary with optional limit
    pub async fn get_conversation_summary(&self, limit: Option<usize>) -> Result<Vec<crate::memory::ConversationTurn>> {
        let manager = self.memory_manager.read().await;
        manager.get_conversation_summary(limit)
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))
    }

    /// List all available sessions
    pub async fn list_sessions(&self) -> Result<Vec<(Uuid, String, chrono::DateTime<chrono::Utc>)>> {
        let manager = self.memory_manager.read().await;
        manager.list_sessions().await
            .map_err(|e| crate::error::ComrudeError::Memory(e.to_string()))
    }

    /// Build request with memory context integration
    pub async fn build_request_with_memory(&self, command: &ParsedCommand) -> Result<GenerationRequest> {
        let mut request = self.build_request_from_command(command)?;
        
        // Add conversation context from memory
        let context_items = self.get_context_for_request().await?;
        request.context.extend(context_items);
        
        Ok(request)
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