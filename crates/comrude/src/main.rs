use clap::{Arg, Command};
use comrude_core::Config;
use comrude_providers::{ProviderManager, OpenAIProvider, AnthropicProvider, OllamaProvider};
use std::io::{self, Write};
use std::process::Command as ProcessCommand;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("comrude")
        .version("0.1.0")
        .author("Comrude Team")
        .about("Universal AI Development Assistant")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Sets a custom config file")
        )
        .arg(
            Arg::new("provider")
                .short('p')
                .long("provider")
                .value_name("PROVIDER")
                .help("Sets the default provider (openai, anthropic, ollama)")
        )
        .arg(
            Arg::new("model")
                .short('m')
                .long("model")
                .value_name("MODEL")
                .help("Sets the default model")
        )
        .arg(
            Arg::new("interactive")
                .short('i')
                .long("interactive")
                .action(clap::ArgAction::SetTrue)
                .help("Start in interactive mode")
        )
        .get_matches();

    // Load configuration
    let config_path = matches.get_one::<String>("config");
    let config = load_config(config_path).await?;

    // Initialize provider manager
    let mut provider_manager = ProviderManager::new(config);

    // Register providers based on configuration
    register_providers(&mut provider_manager).await?;

    // Set default provider if specified
    if let Some(provider_name) = matches.get_one::<String>("provider") {
        if let Err(e) = provider_manager.set_current_provider(provider_name).await {
            eprintln!("Warning: Failed to set provider '{}': {}", provider_name, e);
        }
    } else {
        // Auto-select best available provider
        if let Ok(provider) = provider_manager.auto_select_provider().await {
            let _ = provider_manager.set_current_provider(&provider).await;
        }
    }

    // Start interactive mode if requested or no specific command
    if matches.get_flag("interactive") || std::env::args().len() == 1 {
        // Clear screen before starting interactive mode
        clear_screen();
        start_simple_interactive_mode(provider_manager).await?;
    } else {
        // Handle direct commands here in the future
        println!("Direct command mode not implemented yet. Use --interactive or -i for interactive mode.");
    }

    Ok(())
}

fn clear_screen() {
    // Execute the system 'reset' command directly
    let _ = ProcessCommand::new("reset").status();
}

async fn load_config(config_path: Option<&String>) -> Result<Config, Box<dyn std::error::Error>> {
    let config = match config_path {
        Some(path) => {
            // Load from specified file
            println!("Loading config from: {}", path);
            if std::path::Path::new(path).exists() {
                load_config_from_file(path)?
            } else {
                eprintln!("Config file not found: {}", path);
                Config::default()
            }
        }
        None => {
            // Try to load from default locations
            let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let default_paths = [
                format!("{}/.config/comrude/config.toml", home_dir),
                "comrude.toml".to_string(),
                "./config/config.toml".to_string(),
            ];

            let mut found_config = None;
            for path in &default_paths {
                if std::path::Path::new(path).exists() {
                    println!("Found config at: {}", path);
                    found_config = Some(load_config_from_file(path)?);
                    break;
                }
            }

            match found_config {
                Some(config) => config,
                None => {
                    println!("No config file found, using defaults");
                    Config::default()
                }
            }
        }
    };

    // Validate configuration
    if let Err(e) = config.validate() {
        eprintln!("Warning: Configuration validation failed: {}", e);
    }

    Ok(config)
}

fn load_config_from_file(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

async fn register_providers(manager: &mut ProviderManager) -> Result<(), Box<dyn std::error::Error>> {
    let mut registered_count = 0;

    // Register OpenAI provider if API key is available
    if std::env::var("OPENAI_API_KEY").is_ok() {
        let config = comrude_core::OpenAIConfig::default();
        if let Ok(provider) = OpenAIProvider::new(config) {
            let _ = manager.register_provider(Box::new(provider)).await;
            println!("✓ OpenAI provider registered");
            registered_count += 1;
        }
    } else {
        println!("ℹ OpenAI provider not available (OPENAI_API_KEY not set)");
    }

    // Register Anthropic provider if API key is available
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        let config = comrude_core::AnthropicConfig::default();
        if let Ok(provider) = AnthropicProvider::new(config) {
            let _ = manager.register_provider(Box::new(provider)).await;
            println!("✓ Anthropic provider registered");
            registered_count += 1;
        }
    } else {
        println!("ℹ Anthropic provider not available (ANTHROPIC_API_KEY not set)");
    }

    // Register Ollama provider (always available for local use)
    let config = comrude_core::OllamaConfig::default();
    if let Ok(provider) = OllamaProvider::new(config) {
        let _ = manager.register_provider(Box::new(provider)).await;
        println!("✓ Ollama provider registered");
        registered_count += 1;
    }

    if registered_count == 0 {
        eprintln!("⚠ Warning: No providers registered. Please set at least one API key:");
        eprintln!("  - ANTHROPIC_API_KEY for Claude models");
        eprintln!("  - OPENAI_API_KEY for GPT models");
        eprintln!("  - Or install Ollama for local models");
    }

    Ok(())
}

async fn start_simple_interactive_mode(provider_manager: ProviderManager) -> Result<(), Box<dyn std::error::Error>> {
    println!("Comrude - Universal AI Development Assistant");
    println!("Available commands: <question>, reset, select, help, providers, list, model, quit");
    println!("Type 'help' for more information.\n");

    let provider_manager = std::sync::Arc::new(provider_manager);
    let stdin = io::stdin();
    
    loop {
        print!("comrude> ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        match stdin.read_line(&mut input) {
            Ok(0) => {
                // EOF reached - exit gracefully
                break;
            }
            Ok(_) => {
                let command = input.trim();
                if command.is_empty() {
                    continue;
                }
                
                if command == "quit" || command == "exit" || command == "q" {
                    break;
                }
                
                if let Err(e) = process_simple_command(&provider_manager, command).await {
                    eprintln!("Error processing command: {}", e);
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    // Handle EOF gracefully
                    break;
                } else {
                    eprintln!("Error reading input: {}", e);
                    return Err(e.into());
                }
            }
        }
    }
    
    // Clear screen on exit
    clear_screen();
    
    Ok(())
}

async fn process_simple_command(
    provider_manager: &std::sync::Arc<ProviderManager>,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    match parts[0] {
        "help" => {
            show_help();
        }
        "providers" => {
            list_providers(provider_manager).await;
        }
        "list" => {
            list_models(provider_manager).await;
        }
        "reset" => {
            // Clear the console
            print!("\x1B[2J\x1B[1;1H");
            println!("Comrude - Universal AI Development Assistant");
            println!("Available commands: <question>, reset, select, help, providers, list, model, quit");
            println!("Type 'help' for more information.\n");
        }
        _ if parts[0] == "select" => {
            if parts.len() > 1 {
                let provider_name = parts[1];
                handle_select_with_name(provider_manager, provider_name).await?;
            } else {
                handle_select_command(provider_manager).await?;
            }
        }
        _ if parts[0] == "model" => {
            if parts.len() > 1 {
                let model_name = parts[1];
                handle_model_command(provider_manager, model_name).await?;
            } else {
                show_current_model(provider_manager).await;
            }
        }
        _ => {
            // Treat any other input as a question for the AI
            handle_ask_command(provider_manager, command.to_string()).await?;
        }
    }

    Ok(())
}

async fn handle_ask_command(
    provider_manager: &std::sync::Arc<ProviderManager>,
    question: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use comrude_core::GenerationRequest;

    // Don't echo the user's question

    // Check if any providers are available
    let providers = provider_manager.list_providers().await;
    if providers.is_empty() {
        eprintln!("Error: No providers available. Please configure at least one:");
        eprintln!("  - Set ANTHROPIC_API_KEY environment variable for Claude");
        eprintln!("  - Set OPENAI_API_KEY environment variable for GPT");
        eprintln!("  - Install and run Ollama for local models");
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

    match provider_manager.generate(request).await {
        Ok(response) => {
            println!("\nAssistant: {}\n", response.content);
        }
        Err(e) => {
            eprintln!("\nError: {}", e);
            eprintln!("\nTip: If you're getting authentication errors:");
            eprintln!("  - For Anthropic: export ANTHROPIC_API_KEY=your_key_here");
            eprintln!("  - For OpenAI: export OPENAI_API_KEY=your_key_here");
        }
    }

    Ok(())
}

fn show_help() {
    let help_text = r#"
Comrude - Universal AI Development Assistant

Commands:
  <question>          - Ask a question to the AI (no 'ask' prefix needed)
  reset               - Clear the console
  select              - Select which AI provider to use (interactive)
  select <provider>   - Select provider directly by name
  help                - Show this help message
  providers           - List available providers
  list                - List available models for current provider
  model               - Show current model
  model <model_id>    - Select model for current provider
  quit, exit, q       - Exit the application

Examples:
  What is Rust?
  How do I create a vector in Rust?
  select
  select anthropic
  list
  model codellama:7b
  reset
"#;
    println!("{}", help_text.trim());
}

async fn list_providers(provider_manager: &std::sync::Arc<ProviderManager>) {
    let providers = provider_manager.list_providers().await;
    let current_provider = provider_manager.get_current_provider_name().await;
    
    if providers.is_empty() {
        println!("\nNo providers available\n");
    } else {
        println!("\nAvailable providers:");
        for provider in &providers {
            if current_provider.as_ref() == Some(provider) {
                println!("  {} (current)", provider);
            } else {
                println!("  {}", provider);
            }
        }
        
        if let Some(current) = current_provider {
            println!("\nCurrent provider: {}\n", current);
        } else {
            println!("\nNo provider currently selected. Use 'select' to choose one.\n");
        }
    }
}

async fn handle_select_command(provider_manager: &std::sync::Arc<ProviderManager>) -> Result<(), Box<dyn std::error::Error>> {
    let providers = provider_manager.list_providers().await;
    
    if providers.is_empty() {
        println!("No providers available. Please configure at least one:");
        println!("  - Set ANTHROPIC_API_KEY environment variable for Claude");
        println!("  - Set OPENAI_API_KEY environment variable for GPT");
        println!("  - Install and run Ollama for local models");
        return Ok(());
    }

    println!("Available providers:");
    for (i, provider) in providers.iter().enumerate() {
        println!("  {}: {}", i + 1, provider);
    }
    
    print!("Select provider (1-{}) or press Enter to cancel: ", providers.len());
    io::stdout().flush()?;
    
    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let input = input.trim();
            
            if input.is_empty() {
                println!("Selection cancelled.");
                return Ok(());
            }
            
            if let Ok(choice) = input.parse::<usize>() {
                if choice > 0 && choice <= providers.len() {
                    let selected_provider = &providers[choice - 1];
                    
                    match provider_manager.set_current_provider(selected_provider).await {
                        Ok(_) => {
                            println!("\n✓ Selected provider: {}\n", selected_provider);
                        }
                        Err(e) => {
                            eprintln!("\nError setting provider: {}\n", e);
                        }
                    }
                } else {
                    println!("Invalid selection. Please choose a number between 1 and {}.", providers.len());
                }
            } else {
                println!("Invalid input. Please enter a number.");
            }
        }
        Err(e) => {
            eprintln!("Error reading input: {}", e);
        }
    }
    
    Ok(())
}

async fn handle_select_with_name(provider_manager: &std::sync::Arc<ProviderManager>, provider_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let providers = provider_manager.list_providers().await;
    
    if providers.is_empty() {
        println!("No providers available. Please configure at least one:");
        println!("  - Set ANTHROPIC_API_KEY environment variable for Claude");
        println!("  - Set OPENAI_API_KEY environment variable for GPT");
        println!("  - Install and run Ollama for local models");
        return Ok(());
    }

    // Check if the provider name exists
    if providers.contains(&provider_name.to_string()) {
        match provider_manager.set_current_provider(provider_name).await {
            Ok(_) => {
                println!("\n✓ Selected provider: {}\n", provider_name);
            }
            Err(e) => {
                eprintln!("\nError setting provider: {}\n", e);
            }
        }
    } else {
        println!("Provider '{}' not found.", provider_name);
        println!("Available providers: {}", providers.join(", "));
        println!("Use 'select' without arguments to choose interactively.");
    }
    
    Ok(())
}

async fn list_models(provider_manager: &std::sync::Arc<ProviderManager>) {
    match provider_manager.list_models_for_current_provider().await {
        Ok(models) => {
            let current_provider = provider_manager.get_current_provider_name().await;
            let current_model = provider_manager.get_current_model().await;
            
            if let Some(provider) = current_provider {
                println!("\nAvailable models for {}:\n", provider);
                
                for model in &models {
                    let current_marker = if current_model.as_ref() == Some(&model.id) {
                        " (current)"
                    } else {
                        ""
                    };
                    
                    println!("  {} - {}{}", model.id, model.name, current_marker);
                    
                    if !model.description.is_empty() {
                        println!("    {}", model.description);
                    }
                    
                    println!("    Context: {} tokens, Cost: ${:.4}/${:.4} per 1k tokens\n",
                        model.context_length,
                        model.cost_per_1k_tokens.input,
                        model.cost_per_1k_tokens.output
                    );
                }
                
                if let Some(current) = current_model {
                    println!("Current model: {}", current);
                }
                println!("Use 'model <model_id>' to select a different model.\n");
            } else {
                println!("No provider selected. Use 'select' to choose a provider first.");
            }
        }
        Err(e) => {
            eprintln!("Error listing models: {}", e);
        }
    }
}

async fn show_current_model(provider_manager: &std::sync::Arc<ProviderManager>) {
    let current_provider = provider_manager.get_current_provider_name().await;
    let current_model = provider_manager.get_current_model().await;
    
    match (current_provider, current_model) {
        (Some(provider), Some(model)) => {
            println!("\nCurrent provider: {}", provider);
            println!("Current model: {}\n", model);
        }
        (Some(provider), None) => {
            println!("\nCurrent provider: {}", provider);
            println!("No model selected\n");
        }
        (None, _) => {
            println!("\nNo provider selected. Use 'select' to choose a provider first.\n");
        }
    }
}

async fn handle_model_command(provider_manager: &std::sync::Arc<ProviderManager>, model_name: &str) -> Result<(), Box<dyn std::error::Error>> {
    // First check if we have a current provider
    let current_provider = provider_manager.get_current_provider_name().await;
    if current_provider.is_none() {
        println!("No provider selected. Use 'select' to choose a provider first.");
        return Ok(());
    }

    // Try to list models to validate the model exists
    match provider_manager.list_models_for_current_provider().await {
        Ok(models) => {
            let model_exists = models.iter().any(|m| m.id == model_name);
            
            if model_exists {
                match provider_manager.set_model_for_current_provider(model_name).await {
                    Ok(_) => {
                        println!("\n✓ Model set to: {}\n", model_name);
                    }
                    Err(e) => {
                        eprintln!("\nError setting model: {}\n", e);
                    }
                }
            } else {
                let available_models: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
                println!("Model '{}' not found.", model_name);
                println!("Available models: {}", available_models.join(", "));
                println!("Use 'list' to see all models with descriptions.");
            }
        }
        Err(e) => {
            eprintln!("Error listing models: {}", e);
        }
    }
    
    Ok(())
}