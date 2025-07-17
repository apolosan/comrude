use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tokio::fs;
use crate::types::{Message, ContextItem};
use crate::error::ComrudeResult;

/// Configuration for the memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum number of conversation turns to maintain in context
    pub max_context_turns: usize,
    /// Maximum tokens allowed in context before compression
    pub max_context_tokens: usize,
    /// Enable diff-based compression to reduce redundancy
    pub enable_diff_compression: bool,
    /// Enable automatic context summarization
    pub enable_summarization: bool,
    /// Path to store persistent sessions
    pub session_storage_path: PathBuf,
    /// Maximum age of sessions before archival (in days)
    pub session_max_age_days: u32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_context_turns: 3,
            max_context_tokens: 8000,
            enable_diff_compression: true,
            enable_summarization: true,
            session_storage_path: PathBuf::from(".comrude/sessions"),
            session_max_age_days: 30,
        }
    }
}

/// A conversation turn containing user instruction and assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub user_message: Message,
    pub assistant_response: Option<Message>,
    pub context_snapshot: Vec<ContextItem>,
    pub tokens_used: u32,
}

/// Differential representation of content changes between contexts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDiff {
    pub base_context_id: Uuid,
    pub added_items: Vec<ContextItem>,
    pub removed_item_ids: Vec<String>,
    pub modified_items: Vec<ModifiedContextItem>,
    pub compression_ratio: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifiedContextItem {
    pub item_id: String,
    pub previous_content_hash: String,
    pub content_diff: String, // Text-based diff representation
}

/// Session containing conversation history and context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSession {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub conversation_turns: VecDeque<ConversationTurn>,
    pub cumulative_context: Vec<ContextItem>,
    pub session_metadata: HashMap<String, serde_json::Value>,
    pub config: MemoryConfig,
}

/// Core memory management system
#[derive(Debug)]
pub struct ContextMemoryManager {
    current_session: Option<ConversationSession>,
    config: MemoryConfig,
    session_cache: HashMap<Uuid, ConversationSession>,
    diff_engine: DiffEngine,
}

/// Engine for computing and applying diffs between contexts
#[derive(Debug)]
pub struct DiffEngine {
    content_hasher: ContentHasher,
}

#[derive(Debug)]
pub struct ContentHasher;

impl ContextMemoryManager {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            current_session: None,
            config,
            session_cache: HashMap::new(),
            diff_engine: DiffEngine::new(),
        }
    }

    /// Create a new conversation session
    pub async fn create_session(&mut self, name: Option<String>) -> ComrudeResult<Uuid> {
        let session_id = Uuid::new_v4();
        let session_name = name.unwrap_or_else(|| format!("Session {}", 
            chrono::Utc::now().format("%Y-%m-%d %H:%M")));

        let session = ConversationSession {
            id: session_id,
            name: session_name,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            conversation_turns: VecDeque::new(),
            cumulative_context: Vec::new(),
            session_metadata: HashMap::new(),
            config: self.config.clone(),
        };

        self.current_session = Some(session.clone());
        self.session_cache.insert(session_id, session);
        
        // Persist session
        self.save_session(session_id).await?;
        
        Ok(session_id)
    }

    /// Add a new conversation turn to the current session
    pub async fn add_conversation_turn(
        &mut self,
        user_message: Message,
        context: Vec<ContextItem>,
    ) -> ComrudeResult<Uuid> {
        let turn_id = Uuid::new_v4();
        let tokens_estimate = Self::estimate_tokens(&user_message, &context);

        let conversation_turn = ConversationTurn {
            id: turn_id,
            timestamp: Utc::now(),
            user_message,
            assistant_response: None,
            context_snapshot: context.clone(),
            tokens_used: tokens_estimate,
        };

        // Extract session_id for later use
        let session_id = {
            let session = self.current_session.as_ref()
                .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;
            session.id
        };

        // Apply diff compression if enabled
        if self.config.enable_diff_compression {
            self.apply_context_compression_for_current_session(&context).await?;
        }

        // Add turn to current session
        {
            let session = self.current_session.as_mut()
                .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;
            
            session.conversation_turns.push_back(conversation_turn);
            session.updated_at = Utc::now();
        }

        // Maintain context window size
        self.maintain_context_window_for_current_session().await?;

        // Update cache and persist
        {
            let session = self.current_session.as_ref().unwrap();
            self.session_cache.insert(session_id, session.clone());
        }
        self.save_session(session_id).await?;

        Ok(turn_id)
    }

    /// Complete a conversation turn with assistant response
    pub async fn complete_conversation_turn(
        &mut self,
        turn_id: Uuid,
        assistant_response: Message,
    ) -> ComrudeResult<()> {
        let session_id = {
            let session = self.current_session.as_ref()
                .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;
            session.id
        };

        // Find and update the conversation turn
        {
            let session = self.current_session.as_mut()
                .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;

            if let Some(turn) = session.conversation_turns.iter_mut()
                .find(|turn| turn.id == turn_id) {
                let response_tokens = Self::estimate_response_tokens(&Some(assistant_response.clone()));
                turn.assistant_response = Some(assistant_response);
                turn.tokens_used += response_tokens;
            }

            session.updated_at = Utc::now();
        }

        // Update cache and persist
        {
            let session = self.current_session.as_ref().unwrap();
            self.session_cache.insert(session_id, session.clone());
        }
        self.save_session(session_id).await?;

        Ok(())
    }

    /// Get contextual information for the next LLM request
    pub fn get_context_for_request(&self) -> ComrudeResult<Vec<ContextItem>> {
        let session = self.current_session.as_ref()
            .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;

        let mut context_items = Vec::new();

        // Add conversation history as context
        let recent_turns = session.conversation_turns.iter()
            .rev()
            .take(self.config.max_context_turns);

        for turn in recent_turns {
            // Add user message as context
            context_items.push(self.message_to_context_item(&turn.user_message, "user"));

            // Add assistant response if available
            if let Some(ref response) = turn.assistant_response {
                context_items.push(self.message_to_context_item(response, "assistant"));
            }
        }

        // Apply diff compression to reduce redundancy
        if self.config.enable_diff_compression {
            context_items = self.diff_engine.compress_context_items(context_items)?;
        }

        Ok(context_items)
    }

    /// Get conversation history formatted for display
    pub fn get_conversation_summary(&self, limit: Option<usize>) -> ComrudeResult<Vec<ConversationTurn>> {
        let session = self.current_session.as_ref()
            .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;

        let turns = if let Some(limit) = limit {
            session.conversation_turns.iter().rev().take(limit).cloned().collect()
        } else {
            session.conversation_turns.iter().cloned().collect()
        };

        Ok(turns)
    }

    /// Load an existing session
    pub async fn load_session(&mut self, session_id: Uuid) -> ComrudeResult<()> {
        // Check cache first
        if let Some(session) = self.session_cache.get(&session_id) {
            self.current_session = Some(session.clone());
            return Ok(());
        }

        // Load from storage
        let session_path = self.get_session_path(session_id);
        if !session_path.exists() {
            return Err(crate::error::ComrudeError::NotFound(
                format!("Session {} not found", session_id)
            ));
        }

        let session_data = fs::read_to_string(&session_path).await
            .map_err(|e| crate::error::ComrudeError::IoError(e))?;

        let session: ConversationSession = serde_json::from_str(&session_data)
            .map_err(|e| crate::error::ComrudeError::SerializationError(e.to_string()))?;

        self.current_session = Some(session.clone());
        self.session_cache.insert(session_id, session);

        Ok(())
    }

    /// Save current session to storage
    pub async fn save_session(&self, session_id: Uuid) -> ComrudeResult<()> {
        let session = self.session_cache.get(&session_id)
            .ok_or_else(|| crate::error::ComrudeError::NotFound(
                format!("Session {} not in cache", session_id)
            ))?;

        // Ensure storage directory exists
        fs::create_dir_all(&self.config.session_storage_path).await
            .map_err(|e| crate::error::ComrudeError::IoError(e))?;

        let session_path = self.get_session_path(session_id);
        let session_data = serde_json::to_string_pretty(session)
            .map_err(|e| crate::error::ComrudeError::SerializationError(e.to_string()))?;

        fs::write(&session_path, session_data).await
            .map_err(|e| crate::error::ComrudeError::IoError(e))?;

        Ok(())
    }

    /// List all available sessions
    pub async fn list_sessions(&self) -> ComrudeResult<Vec<(Uuid, String, DateTime<Utc>)>> {
        let mut sessions = Vec::new();

        if !self.config.session_storage_path.exists() {
            return Ok(sessions);
        }

        let mut entries = fs::read_dir(&self.config.session_storage_path).await
            .map_err(|e| crate::error::ComrudeError::IoError(e))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| crate::error::ComrudeError::IoError(e))? {
            
            if let Some(filename) = entry.file_name().to_str() {
                if filename.ends_with(".json") {
                    if let Ok(session_id) = Uuid::parse_str(&filename[..filename.len()-5]) {
                        // Quick metadata read without full session load
                        if let Ok(metadata) = self.read_session_metadata(session_id).await {
                            sessions.push((session_id, metadata.0, metadata.1));
                        }
                    }
                }
            }
        }

        // Sort by last updated (most recent first)
        sessions.sort_by(|a, b| b.2.cmp(&a.2));

        Ok(sessions)
    }

    // Private helper methods
    
    async fn apply_context_compression_for_current_session(
        &mut self,
        new_context: &[ContextItem],
    ) -> ComrudeResult<()> {
        let session = self.current_session.as_mut()
            .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;
        
        // Create diff from previous context
        if !session.cumulative_context.is_empty() {
            let diff = self.diff_engine.create_context_diff(
                &session.cumulative_context,
                new_context,
            )?;

            // Update cumulative context with diff
            session.cumulative_context = self.diff_engine.apply_diff(
                &session.cumulative_context,
                &diff,
            )?;
        } else {
            session.cumulative_context = new_context.to_vec();
        }

        Ok(())
    }

    async fn apply_context_compression(
        &self,
        session: &mut ConversationSession,
        new_context: &[ContextItem],
    ) -> ComrudeResult<()> {
        // Create diff from previous context
        if !session.cumulative_context.is_empty() {
            let diff = self.diff_engine.create_context_diff(
                &session.cumulative_context,
                new_context,
            )?;

            // Update cumulative context with diff
            session.cumulative_context = self.diff_engine.apply_diff(
                &session.cumulative_context,
                &diff,
            )?;
        } else {
            session.cumulative_context = new_context.to_vec();
        }

        Ok(())
    }

    async fn maintain_context_window_for_current_session(&mut self) -> ComrudeResult<()> {
        let session = self.current_session.as_mut()
            .ok_or_else(|| crate::error::ComrudeError::InvalidState("No active session".to_string()))?;
        
        // Remove old turns if exceeding max context
        while session.conversation_turns.len() > self.config.max_context_turns {
            session.conversation_turns.pop_front();
        }

        // Check token limit and compress if needed
        let total_tokens: u32 = session.conversation_turns.iter()
            .map(|turn| turn.tokens_used)
            .sum();

        if total_tokens > self.config.max_context_tokens as u32 {
            if self.config.enable_summarization {
                // Intelligent summarization inline to avoid borrow conflicts
                let turns_to_keep = self.config.max_context_turns / 2;
                let turns_count = session.conversation_turns.len();
                
                if turns_count > turns_to_keep {
                    let turns_to_summarize = turns_count - turns_to_keep;
                    let mut summarized_turns = Vec::new();
                    
                    // Extract oldest turns for summarization
                    for _ in 0..turns_to_summarize {
                        if let Some(turn) = session.conversation_turns.pop_front() {
                            summarized_turns.push(turn);
                        }
                    }
                    
                    // Create a condensed summary of the old conversations
                    let summary = Self::create_conversation_summary(&summarized_turns)?;
                    
                    // Create a summary turn to represent the condensed conversation
                    let summary_turn = ConversationTurn {
                        id: Uuid::new_v4(),
                        timestamp: Utc::now(),
                        user_message: Message::new_system(format!("[SUMMARY] Previous conversation containing {} turns", summarized_turns.len())),
                        assistant_response: Some(Message::new_system(summary)),
                        context_snapshot: Vec::new(),
                        tokens_used: Self::estimate_tokens(
                            &Message::new_system("[SUMMARY]".to_string()), 
                            &[]
                        ),
                    };
                    
                    // Insert summary at the beginning
                    session.conversation_turns.push_front(summary_turn);
                    
                    // Update metadata to track summarization
                    session.session_metadata.insert(
                        "last_summarization".to_string(),
                        serde_json::Value::String(Utc::now().to_rfc3339())
                    );
                    session.session_metadata.insert(
                        "turns_summarized".to_string(),
                        serde_json::Value::Number(serde_json::Number::from(turns_to_summarize))
                    );
                }
            } else {
                // Fallback: just remove oldest turns
                while session.conversation_turns.len() > self.config.max_context_turns / 2 {
                    session.conversation_turns.pop_front();
                }
            }
        }

        Ok(())
    }

    async fn maintain_context_window(&self, session: &mut ConversationSession) -> ComrudeResult<()> {
        // Remove old turns if exceeding max context
        while session.conversation_turns.len() > self.config.max_context_turns {
            session.conversation_turns.pop_front();
        }

        // Check token limit and compress if needed
        let total_tokens: u32 = session.conversation_turns.iter()
            .map(|turn| turn.tokens_used)
            .sum();

        if total_tokens > self.config.max_context_tokens as u32 {
            if self.config.enable_summarization {
                // TODO: Implement intelligent summarization
                // For now, just remove oldest turns
                while session.conversation_turns.len() > self.config.max_context_turns / 2 {
                    session.conversation_turns.pop_front();
                }
            }
        }

        Ok(())
    }

    /// Intelligent summarization strategy for context compression
    async fn intelligent_summarization(&self, session: &mut ConversationSession) -> ComrudeResult<()> {
        // Strategy: Summarize older conversation turns while preserving recent ones
        let turns_to_keep = self.config.max_context_turns / 2; // Keep half the limit as recent
        let turns_count = session.conversation_turns.len();
        
        if turns_count <= turns_to_keep {
            return Ok(()); // Nothing to summarize
        }
        
        let turns_to_summarize = turns_count - turns_to_keep;
        let mut summarized_turns = Vec::new();
        
        // Extract oldest turns for summarization
        for _ in 0..turns_to_summarize {
            if let Some(turn) = session.conversation_turns.pop_front() {
                summarized_turns.push(turn);
            }
        }
        
        // Create a condensed summary of the old conversations
        let summary = ContextMemoryManager::create_conversation_summary(&summarized_turns)?;
        
        // Create a summary turn to represent the condensed conversation
        let summary_turn = ConversationTurn {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_message: Message::new_system(format!("[SUMMARY] Previous conversation containing {} turns", summarized_turns.len())),
            assistant_response: Some(Message::new_system(summary)),
            context_snapshot: Vec::new(),
            tokens_used: Self::estimate_tokens(
                &Message::new_system("[SUMMARY]".to_string()), 
                &[]
            ),
        };
        
        // Insert summary at the beginning
        session.conversation_turns.push_front(summary_turn);
        
        // Update metadata to track summarization
        session.session_metadata.insert(
            "last_summarization".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339())
        );
        session.session_metadata.insert(
            "turns_summarized".to_string(),
            serde_json::Value::Number(serde_json::Number::from(turns_to_summarize))
        );
        
        Ok(())
    }
    
    /// Create a condensed summary from conversation turns
    fn create_conversation_summary(turns: &[ConversationTurn]) -> ComrudeResult<String> {
        if turns.is_empty() {
            return Ok("No conversation to summarize.".to_string());
        }
        
        let mut summary_parts = Vec::new();
        
        // Group turns by topic/theme for better summarization
        let mut current_topic = String::new();
        let mut topic_turns = Vec::new();
        
        for turn in turns {
            let user_content = match &turn.user_message.content {
                crate::types::MessageContent::Text(text) => text.clone(),
                crate::types::MessageContent::Code { content, language } => {
                    format!("Code request in {}: {}", language, 
                        if content.len() > 100 { 
                            format!("{}...", &content[..100]) 
                        } else { 
                            content.clone() 
                        })
                },
                _ => "Non-text message".to_string(),
            };
            
            // Simple topic detection based on keywords
            let detected_topic = Self::detect_conversation_topic(&user_content);
            
            if detected_topic != current_topic && !topic_turns.is_empty() {
                // Summarize current topic group
                let topic_summary = Self::summarize_topic_group(&current_topic, &topic_turns);
                summary_parts.push(topic_summary);
                topic_turns.clear();
            }
            
            current_topic = detected_topic;
            topic_turns.push(turn);
        }
        
        // Summarize the last topic group
        if !topic_turns.is_empty() {
            let topic_summary = Self::summarize_topic_group(&current_topic, &topic_turns);
            summary_parts.push(topic_summary);
        }
        
        // Combine all topic summaries
        let full_summary = if summary_parts.len() == 1 {
            summary_parts.into_iter().next().unwrap()
        } else {
            format!(
                "Conversation covered {} topics:\n{}",
                summary_parts.len(),
                summary_parts.join("\n\n")
            )
        };
        
        Ok(full_summary)
    }
    
    /// Detect the main topic of a conversation message
    fn detect_conversation_topic(content: &str) -> String {
        let content_lower = content.to_lowercase();
        
        // Programming/Code topics
        if content_lower.contains("function") || content_lower.contains("class") || 
           content_lower.contains("code") || content_lower.contains("bug") ||
           content_lower.contains("implement") || content_lower.contains("debug") {
            return "Programming".to_string();
        }
        
        // File operations
        if content_lower.contains("file") || content_lower.contains("directory") ||
           content_lower.contains("folder") || content_lower.contains("save") ||
           content_lower.contains("read") || content_lower.contains("write") {
            return "File Operations".to_string();
        }
        
        // Configuration/Setup
        if content_lower.contains("config") || content_lower.contains("setup") ||
           content_lower.contains("install") || content_lower.contains("configure") {
            return "Configuration".to_string();
        }
        
        // Explanations/Help
        if content_lower.contains("explain") || content_lower.contains("help") ||
           content_lower.contains("how") || content_lower.contains("what") ||
           content_lower.contains("why") {
            return "Explanation/Help".to_string();
        }
        
        // Default topic
        "General Discussion".to_string()
    }
    
    /// Summarize a group of conversation turns with the same topic
    fn summarize_topic_group(topic: &str, turns: &[&ConversationTurn]) -> String {
        if turns.is_empty() {
            return format!("{}: No activity", topic);
        }
        
        let mut key_points = Vec::new();
        let mut code_snippets = 0;
        let mut questions_asked = 0;
        
        for turn in turns {
            // Analyze user message
            match &turn.user_message.content {
                crate::types::MessageContent::Text(text) => {
                    if text.contains('?') {
                        questions_asked += 1;
                    }
                    
                    // Extract key action words
                    let actions = Self::extract_action_words(text);
                    if !actions.is_empty() {
                        key_points.push(format!("User: {}", actions.join(", ")));
                    }
                },
                crate::types::MessageContent::Code { language, .. } => {
                    code_snippets += 1;
                    key_points.push(format!("Code in {}", language));
                },
                _ => {},
            };
            
            // Analyze assistant response if available
            if let Some(ref response) = turn.assistant_response {
                match &response.content {
                    crate::types::MessageContent::Text(text) => {
                        let actions = Self::extract_action_words(text);
                        if !actions.is_empty() {
                            key_points.push(format!("Assistant: {}", actions.join(", ")));
                        }
                    },
                    crate::types::MessageContent::Code { language, .. } => {
                        key_points.push(format!("Generated {} code", language));
                    },
                    _ => {},
                }
            }
        }
        
        // Build summary
        let mut summary = format!("**{}** ({} turns)", topic, turns.len());
        
        if questions_asked > 0 {
            summary.push_str(&format!(" - {} questions asked", questions_asked));
        }
        
        if code_snippets > 0 {
            summary.push_str(&format!(" - {} code snippets", code_snippets));
        }
        
        if !key_points.is_empty() {
            // Limit to most important points
            let max_points = 3;
            let points_to_show = if key_points.len() > max_points {
                key_points.into_iter().take(max_points).collect::<Vec<_>>()
            } else {
                key_points
            };
            
            summary.push_str(&format!("\nKey activities: {}", points_to_show.join("; ")));
        }
        
        summary
    }
    
    /// Extract action words from text content
    fn extract_action_words(text: &str) -> Vec<String> {
        let action_patterns = [
            "create", "build", "implement", "develop", "write", "generate",
            "fix", "debug", "solve", "resolve", "update", "modify", "change",
            "explain", "describe", "analyze", "review", "check", "test",
            "install", "configure", "setup", "deploy", "run", "execute",
            "read", "parse", "load", "save", "export", "import",
            "optimize", "improve", "refactor", "clean", "organize",
        ];
        
        let text_lower = text.to_lowercase();
        let mut found_actions = Vec::new();
        
        for action in &action_patterns {
            if text_lower.contains(action) {
                found_actions.push(action.to_string());
            }
        }
        
        // Remove duplicates and limit
        found_actions.sort();
        found_actions.dedup();
        found_actions.into_iter().take(3).collect()
    }

    fn estimate_tokens(message: &Message, context: &[ContextItem]) -> u32 {
        // Simple token estimation (roughly 4 characters per token)
        let message_tokens = match &message.content {
            crate::types::MessageContent::Text(text) => text.len() / 4,
            crate::types::MessageContent::Code { content, .. } => content.len() / 4,
            _ => 50, // Default estimation for other types
        };

        let context_tokens: usize = context.iter()
            .map(|item| item.content.len() / 4)
            .sum();

        (message_tokens + context_tokens) as u32
    }

    fn estimate_response_tokens(response: &Option<Message>) -> u32 {
        response.as_ref()
            .map(|msg| match &msg.content {
                crate::types::MessageContent::Text(text) => text.len() / 4,
                crate::types::MessageContent::Code { content, .. } => content.len() / 4,
                _ => 50,
            })
            .unwrap_or(0) as u32
    }

    fn message_to_context_item(&self, message: &Message, role: &str) -> ContextItem {
        let content = match &message.content {
            crate::types::MessageContent::Text(text) => text.clone(),
            crate::types::MessageContent::Code { content, language } => {
                format!("```{}\n{}\n```", language, content)
            },
            _ => format!("{:?}", message.content),
        };

        let mut metadata = HashMap::new();
        metadata.insert("role".to_string(), serde_json::Value::String(role.to_string()));
        metadata.insert("timestamp".to_string(), 
            serde_json::Value::String(message.timestamp.to_rfc3339()));

        ContextItem {
            item_type: crate::types::ContextType::Text,
            content,
            metadata,
        }
    }

    fn get_session_path(&self, session_id: Uuid) -> PathBuf {
        self.config.session_storage_path.join(format!("{}.json", session_id))
    }

    async fn read_session_metadata(&self, session_id: Uuid) -> ComrudeResult<(String, DateTime<Utc>)> {
        let session_path = self.get_session_path(session_id);
        let session_data = fs::read_to_string(&session_path).await
            .map_err(|e| crate::error::ComrudeError::IoError(e))?;

        // Parse only the metadata we need
        let session_value: serde_json::Value = serde_json::from_str(&session_data)
            .map_err(|e| crate::error::ComrudeError::SerializationError(e.to_string()))?;

        let name = session_value["name"].as_str()
            .unwrap_or("Unnamed Session").to_string();

        let updated_at = session_value["updated_at"].as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        Ok((name, updated_at))
    }
}

impl DiffEngine {
    fn new() -> Self {
        Self {
            content_hasher: ContentHasher,
        }
    }

    fn create_context_diff(
        &self,
        old_context: &[ContextItem],
        new_context: &[ContextItem],
    ) -> ComrudeResult<ContextDiff> {
        let base_context_id = Uuid::new_v4();
        let mut added_items = Vec::new();
        let mut removed_item_ids = Vec::new();
        let mut modified_items = Vec::new();

        // Create maps for efficient lookup
        let old_map: HashMap<String, &ContextItem> = old_context.iter()
            .enumerate()
            .map(|(i, item)| (i.to_string(), item))
            .collect();

        let new_map: HashMap<String, &ContextItem> = new_context.iter()
            .enumerate()
            .map(|(i, item)| (i.to_string(), item))
            .collect();

        // Find added items
        for (key, item) in &new_map {
            if !old_map.contains_key(key) {
                added_items.push((*item).clone());
            }
        }

        // Find removed items
        for key in old_map.keys() {
            if !new_map.contains_key(key) {
                removed_item_ids.push(key.clone());
            }
        }

        // Find modified items
        for (key, new_item) in &new_map {
            if let Some(old_item) = old_map.get(key) {
                let old_hash = self.content_hasher.hash_content(&old_item.content);
                let new_hash = self.content_hasher.hash_content(&new_item.content);
                
                if old_hash != new_hash {
                    let content_diff = self.compute_text_diff(&old_item.content, &new_item.content);
                    modified_items.push(ModifiedContextItem {
                        item_id: key.clone(),
                        previous_content_hash: old_hash,
                        content_diff,
                    });
                }
            }
        }

        // Calculate compression ratio
        let original_size = old_context.iter().map(|item| item.content.len()).sum::<usize>();
        let compressed_size = added_items.iter().map(|item| item.content.len()).sum::<usize>()
            + modified_items.iter().map(|item| item.content_diff.len()).sum::<usize>();

        let compression_ratio = if original_size > 0 {
            compressed_size as f32 / original_size as f32
        } else {
            1.0
        };

        Ok(ContextDiff {
            base_context_id,
            added_items,
            removed_item_ids,
            modified_items,
            compression_ratio,
        })
    }

    fn apply_diff(
        &self,
        base_context: &[ContextItem],
        diff: &ContextDiff,
    ) -> ComrudeResult<Vec<ContextItem>> {
        let mut result = base_context.to_vec();

        // Remove items
        result.retain(|item| {
            let item_index = base_context.iter().position(|x| std::ptr::eq(x, item))
                .map(|i| i.to_string())
                .unwrap_or_default();
            !diff.removed_item_ids.contains(&item_index)
        });

        // Apply modifications
        for modification in &diff.modified_items {
            if let Ok(index) = modification.item_id.parse::<usize>() {
                if index < result.len() {
                    // Apply text diff (simplified - in production would use proper diff algorithm)
                    result[index].content = modification.content_diff.clone();
                }
            }
        }

        // Add new items
        result.extend(diff.added_items.clone());

        Ok(result)
    }

    fn compress_context_items(&self, items: Vec<ContextItem>) -> ComrudeResult<Vec<ContextItem>> {
        // Simple deduplication based on content hashes
        let mut seen_hashes = std::collections::HashSet::new();
        let mut compressed = Vec::new();

        for item in items {
            let content_hash = self.content_hasher.hash_content(&item.content);
            if !seen_hashes.contains(&content_hash) {
                seen_hashes.insert(content_hash);
                compressed.push(item);
            }
        }

        Ok(compressed)
    }

    fn compute_text_diff(&self, old_text: &str, new_text: &str) -> String {
        // Simplified diff - in production would use proper diff algorithm like Myers
        if old_text == new_text {
            new_text.to_string()
        } else {
            format!("DIFF: {} -> {}", old_text.len(), new_text.len())
        }
    }
}

impl ContentHasher {
    fn hash_content(&self, content: &str) -> String {
        // Simple hash implementation - in production would use SHA-256 or similar
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_session_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = MemoryConfig {
            session_storage_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let mut manager = ContextMemoryManager::new(config);
        let session_id = manager.create_session(Some("Test Session".to_string())).await.unwrap();

        assert!(manager.current_session.is_some());
        assert_eq!(manager.current_session.as_ref().unwrap().name, "Test Session");
        assert_eq!(manager.current_session.as_ref().unwrap().id, session_id);
    }

    #[tokio::test]
    async fn test_conversation_turns() {
        let temp_dir = TempDir::new().unwrap();
        let config = MemoryConfig {
            session_storage_path: temp_dir.path().to_path_buf(),
            max_context_turns: 2,
            ..Default::default()
        };

        let mut manager = ContextMemoryManager::new(config);
        let _session_id = manager.create_session(None).await.unwrap();

        let user_msg = Message::new_user("Hello, world!".to_string());
        let context = vec![];
        
        let turn_id = manager.add_conversation_turn(user_msg, context).await.unwrap();
        
        let assistant_msg = Message::new_assistant(
            "Hello! How can I help you?".to_string(),
            "test".to_string(),
            "test-model".to_string()
        );
        
        manager.complete_conversation_turn(turn_id, assistant_msg).await.unwrap();

        let summary = manager.get_conversation_summary(None).unwrap();
        assert_eq!(summary.len(), 1);
        assert!(summary[0].assistant_response.is_some());
    }

    #[tokio::test]
    async fn test_context_window_maintenance() {
        let temp_dir = TempDir::new().unwrap();
        let config = MemoryConfig {
            session_storage_path: temp_dir.path().to_path_buf(),
            max_context_turns: 2,
            ..Default::default()
        };

        let mut manager = ContextMemoryManager::new(config);
        let _session_id = manager.create_session(None).await.unwrap();

        // Add 3 conversation turns (exceeding the limit of 2)
        for i in 0..3 {
            let user_msg = Message::new_user(format!("Message {}", i));
            let turn_id = manager.add_conversation_turn(user_msg, vec![]).await.unwrap();
            
            let assistant_msg = Message::new_assistant(
                format!("Response {}", i),
                "test".to_string(),
                "test-model".to_string()
            );
            manager.complete_conversation_turn(turn_id, assistant_msg).await.unwrap();
        }

        let summary = manager.get_conversation_summary(None).unwrap();
        assert_eq!(summary.len(), 2); // Should maintain only 2 turns
    }

    #[test]
    fn test_diff_engine() {
        let engine = DiffEngine::new();
        
        let old_context = vec![
            ContextItem {
                item_type: crate::types::ContextType::Text,
                content: "Original content".to_string(),
                metadata: HashMap::new(),
            }
        ];

        let new_context = vec![
            ContextItem {
                item_type: crate::types::ContextType::Text,
                content: "Modified content".to_string(),
                metadata: HashMap::new(),
            }
        ];

        let diff = engine.create_context_diff(&old_context, &new_context).unwrap();
        assert!(diff.compression_ratio > 0.0);
        
        let applied = engine.apply_diff(&old_context, &diff).unwrap();
        assert_eq!(applied.len(), 1);
    }
}