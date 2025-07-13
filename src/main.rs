use clap::{Arg, Command};
use comrude_core::Config;
use comrude_providers::{ProviderManager, OpenAIProvider, AnthropicProvider, OllamaProvider};
use std::io::{self, Write};
use std::process::Command;
use crossterm::{
    execute, 
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen}, 
    cursor::MoveTo, 
    style::ResetColor
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Clear screen immediately when application starts
    eprintln!("DEBUG: About to call clear_screen()");
    if let Err(e) = clear_screen() {
        eprintln!("Warning: Failed to clear screen: {}", e);
    }
    eprintln!("DEBUG: clear_screen() call completed");

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
        start_simple_interactive_mode(provider_manager).await?;
    } else {
        // Handle direct commands here in the future
        println!("Direct command mode not implemented yet. Use --interactive or -i for interactive mode.");
    }

    Ok(())
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
    // Register OpenAI provider if API key is available
    if std::env::var("OPENAI_API_KEY").is_ok() {
        let config = comrude_core::OpenAIConfig::default();
        if let Ok(provider) = OpenAIProvider::new(config) {
            let _ = manager.register_provider(Box::new(provider)).await;
            println!("✓ OpenAI provider registered");
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
        }
    } else {
        println!("ℹ Anthropic provider not available (ANTHROPIC_API_KEY not set)");
    }

    // Register Ollama provider (always available for local use)
    let config = comrude_core::OllamaConfig::default();
    if let Ok(provider) = OllamaProvider::new(config) {
        let _ = manager.register_provider(Box::new(provider)).await;
        println!("✓ Ollama provider registered");
    }

    Ok(())
}

fn clear_screen() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("DEBUG: Attempting to clear screen...");
    
    // Try multiple approaches
    let reset_result = Command::new("reset").status();
    eprintln!("DEBUG: reset command result: {:?}", reset_result);
    
    if reset_result.is_err() {
        let clear_result = Command::new("clear").status();
        eprintln!("DEBUG: clear command result: {:?}", clear_result);
    }
    
    // Also try direct escape sequences
    print!("\x1b[2J\x1b[H\x1b[3J");
    io::stdout().flush()?;
    eprintln!("DEBUG: Direct escape sequences sent");
    
    Ok(())
}

async fn start_simple_interactive_mode(provider_manager: ProviderManager) -> Result<(), Box<dyn std::error::Error>> {
    println!("Comrude - Universal AI Development Assistant");
    println!("Available commands: ask <question>, help, providers, quit");
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
        "ask" => {
            if parts.len() < 2 {
                println!("Usage: ask <question>");
                return Ok(());
            }
            let question = parts[1..].join(" ");
            handle_ask_command(provider_manager, question).await?;
        }
        "help" => {
            show_help();
        }
        "providers" => {
            list_providers(provider_manager).await;
        }
        _ => {
            println!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
        }
    }

    Ok(())
}

async fn handle_ask_command(
    provider_manager: &std::sync::Arc<ProviderManager>,
    question: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use comrude_core::GenerationRequest;

    println!("User: {}", question);

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
            println!("Assistant: {}", response.content);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }

    Ok(())
}

fn show_help() {
    let help_text = r#"
Comrude - Universal AI Development Assistant

Commands:
  ask <question>  - Ask a question to the AI
  help            - Show this help message
  providers       - List available providers
  quit, exit, q   - Exit the application

Examples:
  ask What is Rust?
  ask How do I create a vector in Rust?
"#;
    println!("{}", help_text.trim());
}

async fn list_providers(provider_manager: &std::sync::Arc<ProviderManager>) {
    let providers = provider_manager.list_providers().await;
    if providers.is_empty() {
        println!("No providers available");
    } else {
        println!("Available providers: {}", providers.join(", "));
    }
}