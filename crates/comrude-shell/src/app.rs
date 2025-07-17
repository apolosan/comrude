use comrude_core::{GenerationRequest, GenerationResponse, Message, MessageSender, MessageContent, MessageStatus};
use uuid::Uuid;
use chrono::Utc;
use comrude_providers::ProviderManager;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ConversationEntry {
    pub message: Message,
    pub response: Option<GenerationResponse>,
}

#[derive(Debug)]
pub struct AppState {
    pub conversation: Arc<RwLock<VecDeque<ConversationEntry>>>,
    pub current_input: String,
    pub input_mode: InputMode,
    pub current_command: Option<String>,
    pub status_message: Option<String>,
    pub provider_manager: Arc<ProviderManager>,
    pub should_quit: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Normal,
    Insert,
    Command,
}

impl AppState {
    pub fn new(provider_manager: ProviderManager) -> Self {
        Self {
            conversation: Arc::new(RwLock::new(VecDeque::new())),
            current_input: String::new(),
            input_mode: InputMode::Normal,
            current_command: None,
            status_message: None,
            provider_manager: Arc::new(provider_manager),
            should_quit: false,
        }
    }

    pub async fn add_user_message(&self, content: String) {
        let message = Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::User,
            content: MessageContent::Text(content),
            status: MessageStatus::Complete,
        };

        let entry = ConversationEntry {
            message,
            response: None,
        };

        let mut conversation = self.conversation.write().await;
        conversation.push_back(entry);
    }

    pub async fn add_assistant_response(&self, response: GenerationResponse) {
        let mut conversation = self.conversation.write().await;
        if let Some(last_entry) = conversation.back_mut() {
            last_entry.response = Some(response);
        }
    }

    pub async fn process_command(&mut self, command: &str) -> Result<(), Box<dyn std::error::Error>> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        match parts[0] {
            "/quit" | "/exit" | "/q" => {
                self.should_quit = true;
            }
            "/help" => {
                self.show_help().await;
            }
            "/clear" => {
                let mut conversation = self.conversation.write().await;
                conversation.clear();
            }
            "/reset" => {
                let mut conversation = self.conversation.write().await;
                conversation.clear();
                self.status_message = Some("Console cleared".to_string());
            }
            _ if parts[0] == "/select" => {
                if parts.len() > 1 {
                    let provider_name = parts[1];
                    self.handle_select_with_name(provider_name).await;
                } else {
                    self.handle_select_command().await;
                }
            }
            "/providers" => {
                self.list_providers().await;
            }
            "/list" => {
                self.list_models().await;
            }
            _ if parts[0] == "/model" => {
                if parts.len() > 1 {
                    let model_name = parts[1];
                    self.handle_model_command(model_name).await;
                } else {
                    self.show_current_model().await;
                }
            }
            _ => {
                // Treat any other input as a question for the AI
                self.handle_ask_command(command.to_string()).await?;
            }
        }

        Ok(())
    }

    async fn handle_ask_command(&mut self, question: String) -> Result<(), Box<dyn std::error::Error>> {
        self.add_user_message(question.clone()).await;

        // Check if any providers are available
        let providers = self.provider_manager.list_providers().await;
        if providers.is_empty() {
            self.status_message = Some("No providers available. Please configure API keys.".to_string());
            return Ok(());
        }

        let request = GenerationRequest {
            prompt: question,
            model: None,
            system_prompt: Some("You are a helpful AI assistant.".to_string()),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: false,
            tools: Vec::new(),
            context: Vec::new(),
            metadata: std::collections::HashMap::new(),
        };

        match self.provider_manager.generate(request).await {
            Ok(response) => {
                self.add_assistant_response(response).await;
                self.status_message = Some("Response generated successfully".to_string());
            }
            Err(e) => {
                self.status_message = Some(format!("Error: {}. Check API keys configuration.", e));
            }
        }

        Ok(())
    }

    async fn show_help(&mut self) {
        let help_text = r#"
Comrude - Universal AI Development Assistant

Commands:
  <question>          - Ask a question to the AI (no prefix needed)
  /help               - Show this help message
  /clear              - Clear conversation history
  /reset              - Clear console and conversation history
  /select             - Select which AI provider to use (interactive)
  /select <provider>  - Select provider directly by name
  /providers          - List available providers
  /list               - List available models for current provider
  /model              - Show current model
  /model <model_id>   - Select model for current provider
  /quit, /exit, /q    - Exit the application

Navigation:
  Tab             - Switch between input modes
  Enter           - Execute command or send message
  Esc             - Return to normal mode
"#.trim();

        let message = Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::System,
            content: MessageContent::Text(help_text.to_string()),
            status: MessageStatus::Complete,
        };

        let entry = ConversationEntry {
            message,
            response: None,
        };

        let mut conversation = self.conversation.write().await;
        conversation.push_back(entry);
    }

    async fn list_providers(&mut self) {
        let providers = self.provider_manager.list_providers().await;
        let current_provider = self.provider_manager.get_current_provider_name().await;
        
        let provider_list = if providers.is_empty() {
            "No providers available".to_string()
        } else {
            let mut list = String::from("Available providers:\n");
            for provider in &providers {
                if current_provider.as_ref() == Some(provider) {
                    list.push_str(&format!("  {} (current)\n", provider));
                } else {
                    list.push_str(&format!("  {}\n", provider));
                }
            }
            
            if let Some(current) = current_provider {
                list.push_str(&format!("\nCurrent provider: {}", current));
            } else {
                list.push_str("\nNo provider currently selected. Use 'select' to choose one.");
            }
            
            list
        };

        let message = Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::System,
            content: MessageContent::Text(provider_list),
            status: MessageStatus::Complete,
        };

        let entry = ConversationEntry {
            message,
            response: None,
        };

        let mut conversation = self.conversation.write().await;
        conversation.push_back(entry);
    }

    async fn handle_select_command(&mut self) {
        let providers = self.provider_manager.list_providers().await;
        
        if providers.is_empty() {
            self.status_message = Some("No providers available. Configure API keys first.".to_string());
            return;
        }

        let mut selection_text = String::new();
        selection_text.push_str("Available providers:\n");
        for (i, provider) in providers.iter().enumerate() {
            selection_text.push_str(&format!("  {}: {}\n", i + 1, provider));
        }
        selection_text.push_str(&format!("\nType the number (1-{}) to select provider:", providers.len()));

        let message = Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::System,
            content: MessageContent::Text(selection_text),
            status: MessageStatus::Complete,
        };

        let entry = ConversationEntry {
            message,
            response: None,
        };

        let mut conversation = self.conversation.write().await;
        conversation.push_back(entry);

        self.status_message = Some("Enter provider number to select".to_string());
    }

    pub async fn handle_provider_selection(&mut self, input: &str) -> bool {
        let providers = self.provider_manager.list_providers().await;
        
        if providers.is_empty() {
            return false;
        }

        if let Ok(choice) = input.parse::<usize>() {
            if choice > 0 && choice <= providers.len() {
                let selected_provider = &providers[choice - 1];
                
                match self.provider_manager.set_current_provider(selected_provider).await {
                    Ok(_) => {
                        self.status_message = Some(format!("✓ Selected provider: {}", selected_provider));
                        
                        let confirmation_message = Message {
                            id: Uuid::new_v4(),
                            timestamp: Utc::now(),
                            sender: MessageSender::System,
                            content: MessageContent::Text(format!("Provider set to: {}", selected_provider)),
                            status: MessageStatus::Complete,
                        };

                        let entry = ConversationEntry {
                            message: confirmation_message,
                            response: None,
                        };

                        let mut conversation = self.conversation.write().await;
                        conversation.push_back(entry);
                        
                        return true;
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Error setting provider: {}", e));
                    }
                }
            } else {
                self.status_message = Some(format!("Invalid selection. Choose 1-{}", providers.len()));
            }
        } else {
            self.status_message = Some("Invalid input. Please enter a number.".to_string());
        }
        
        false
    }

    async fn handle_select_with_name(&mut self, provider_name: &str) {
        let providers = self.provider_manager.list_providers().await;
        
        if providers.is_empty() {
            self.status_message = Some("No providers available. Configure API keys first.".to_string());
            return;
        }

        // Check if the provider name exists
        if providers.contains(&provider_name.to_string()) {
            match self.provider_manager.set_current_provider(provider_name).await {
                Ok(_) => {
                    self.status_message = Some(format!("✓ Selected provider: {}", provider_name));
                    
                    let confirmation_message = Message {
                        id: Uuid::new_v4(),
                        timestamp: Utc::now(),
                        sender: MessageSender::System,
                        content: MessageContent::Text(format!("Provider set to: {}", provider_name)),
                        status: MessageStatus::Complete,
                    };

                    let entry = ConversationEntry {
                        message: confirmation_message,
                        response: None,
                    };

                    let mut conversation = self.conversation.write().await;
                    conversation.push_back(entry);
                }
                Err(e) => {
                    self.status_message = Some(format!("Error setting provider: {}", e));
                }
            }
        } else {
            let error_message = format!(
                "Provider '{}' not found.\nAvailable providers: {}\nUse 'select' without arguments to choose interactively.",
                provider_name,
                providers.join(", ")
            );
            
            self.status_message = Some(format!("Provider '{}' not found", provider_name));
            
            let message = Message {
                id: Uuid::new_v4(),
                timestamp: Utc::now(),
                sender: MessageSender::System,
                content: MessageContent::Text(error_message),
                status: MessageStatus::Complete,
            };

            let entry = ConversationEntry {
                message,
                response: None,
            };

            let mut conversation = self.conversation.write().await;
            conversation.push_back(entry);
        }
    }

    pub fn clear_input(&mut self) {
        self.current_input.clear();
    }

    pub fn add_char(&mut self, c: char) {
        self.current_input.push(c);
    }

    pub fn remove_char(&mut self) {
        self.current_input.pop();
    }

    pub fn set_input_mode(&mut self, mode: InputMode) {
        self.input_mode = mode;
    }

    pub fn get_input(&self) -> &str {
        &self.current_input
    }

    pub fn take_input(&mut self) -> String {
        let input = self.current_input.clone();
        self.current_input.clear();
        input
    }

    async fn list_models(&mut self) {
        match self.provider_manager.list_models_for_current_provider().await {
            Ok(models) => {
                let current_provider = self.provider_manager.get_current_provider_name().await;
                let current_model = self.provider_manager.get_current_model().await;
                
                let mut model_list = String::new();
                if let Some(provider) = current_provider {
                    model_list.push_str(&format!("Available models for {}:\n\n", provider));
                    
                    for model in &models {
                        let current_marker = if current_model.as_ref() == Some(&model.id) {
                            " (current)"
                        } else {
                            ""
                        };
                        
                        model_list.push_str(&format!(
                            "  {} - {}{}\n",
                            model.id, 
                            model.name,
                            current_marker
                        ));
                        
                        if !model.description.is_empty() {
                            model_list.push_str(&format!("    {}\n", model.description));
                        }
                        
                        model_list.push_str(&format!(
                            "    Context: {} tokens, Cost: ${:.4}/${:.4} per 1k tokens\n\n",
                            model.context_length,
                            model.cost_per_1k_tokens.input,
                            model.cost_per_1k_tokens.output
                        ));
                    }
                    
                    if let Some(current) = current_model {
                        model_list.push_str(&format!("Current model: {}\n", current));
                    }
                    model_list.push_str("Use 'model <model_id>' to select a different model.");
                } else {
                    model_list = "No provider selected. Use 'select' to choose a provider first.".to_string();
                }

                let message = Message {
                    id: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    sender: MessageSender::System,
                    content: MessageContent::Text(model_list),
                    status: MessageStatus::Complete,
                };

                let entry = ConversationEntry {
                    message,
                    response: None,
                };

                let mut conversation = self.conversation.write().await;
                conversation.push_back(entry);
            }
            Err(e) => {
                self.status_message = Some(format!("Error listing models: {}", e));
            }
        }
    }

    async fn show_current_model(&mut self) {
        let current_provider = self.provider_manager.get_current_provider_name().await;
        let current_model = self.provider_manager.get_current_model().await;
        
        let model_info = match (current_provider, current_model) {
            (Some(provider), Some(model)) => {
                format!("Current provider: {}\nCurrent model: {}", provider, model)
            }
            (Some(provider), None) => {
                format!("Current provider: {}\nNo model selected", provider)
            }
            (None, _) => {
                "No provider selected. Use 'select' to choose a provider first.".to_string()
            }
        };

        let message = Message {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            sender: MessageSender::System,
            content: MessageContent::Text(model_info),
            status: MessageStatus::Complete,
        };

        let entry = ConversationEntry {
            message,
            response: None,
        };

        let mut conversation = self.conversation.write().await;
        conversation.push_back(entry);
    }

    async fn handle_model_command(&mut self, model_name: &str) {
        // First check if we have a current provider
        let current_provider = self.provider_manager.get_current_provider_name().await;
        if current_provider.is_none() {
            self.status_message = Some("No provider selected. Use 'select' to choose a provider first.".to_string());
            return;
        }

        // Try to list models to validate the model exists
        match self.provider_manager.list_models_for_current_provider().await {
            Ok(models) => {
                let model_exists = models.iter().any(|m| m.id == model_name);
                
                if model_exists {
                    match self.provider_manager.set_model_for_current_provider(model_name).await {
                        Ok(_) => {
                            self.status_message = Some(format!("✓ Model set to: {}", model_name));
                            
                            let confirmation_message = Message {
                                id: Uuid::new_v4(),
                                timestamp: Utc::now(),
                                sender: MessageSender::System,
                                content: MessageContent::Text(format!("Model changed to: {}", model_name)),
                                status: MessageStatus::Complete,
                            };

                            let entry = ConversationEntry {
                                message: confirmation_message,
                                response: None,
                            };

                            let mut conversation = self.conversation.write().await;
                            conversation.push_back(entry);
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Error setting model: {}", e));
                        }
                    }
                } else {
                    let available_models: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
                    let error_message = format!(
                        "Model '{}' not found.\nAvailable models: {}\nUse 'list' to see all models with descriptions.",
                        model_name,
                        available_models.join(", ")
                    );
                    
                    self.status_message = Some(format!("Model '{}' not found", model_name));
                    
                    let message = Message {
                        id: Uuid::new_v4(),
                        timestamp: Utc::now(),
                        sender: MessageSender::System,
                        content: MessageContent::Text(error_message),
                        status: MessageStatus::Complete,
                    };

                    let entry = ConversationEntry {
                        message,
                        response: None,
                    };

                    let mut conversation = self.conversation.write().await;
                    conversation.push_back(entry);
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Error listing models: {}", e));
            }
        }
    }
}