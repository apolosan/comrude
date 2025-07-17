use clap::{Arg, Command};
use comrude_core::{Config, ComrudeEngine};
use comrude_core::types::Message;
use comrude_providers::{ProviderManager, OpenAIProvider, AnthropicProvider, OllamaProvider};
use std::io::{self, Write};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode}
};
use libc::{setpgid, killpg, SIGTERM, SIGINT, signal};
use std::os::unix::process::CommandExt;

// Command stack entry
#[derive(Debug, Clone)]
struct CommandStackEntry {
    command: String,
    pid: u32,
    pgid: i32,
}

// Global state for auto-confirmation mode
static AUTO_CONFIRM: Mutex<bool> = Mutex::new(false);

// Command stack for proper signal isolation
static COMMAND_STACK: Mutex<VecDeque<CommandStackEntry>> = Mutex::new(VecDeque::new());

// Atomic flag for SIGINT handling
static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

// Signal handler for SIGINT (CTRL+C)
extern "C" fn sigint_handler(_: i32) {
    SIGINT_RECEIVED.store(true, Ordering::Relaxed);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Install SIGINT handler
    unsafe {
        signal(SIGINT, sigint_handler as usize);
    }
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
    let mut provider_manager = ProviderManager::new(config.clone());

    // Register providers based on configuration
    register_providers(&mut provider_manager).await?;

    // Set default provider if specified
    if let Some(provider_name) = matches.get_one::<String>("provider") {
        if let Err(e) = provider_manager.set_current_provider(provider_name).await {
            eprintln!("Warning: Failed to set provider '{}': {}", provider_name, e);
        }
    } else {
        // Try to use default provider from config first
        if let Some(default_provider) = &config.app.default_provider {
            if let Err(_) = provider_manager.set_current_provider(default_provider).await {
                // If default provider fails, auto-select best available provider
                if let Ok(provider) = provider_manager.auto_select_provider().await {
                    let _ = provider_manager.set_current_provider(&provider).await;
                }
            }
        } else {
            // Auto-select best available provider
            if let Ok(provider) = provider_manager.auto_select_provider().await {
                let _ = provider_manager.set_current_provider(&provider).await;
            }
        }
    }

    // Start interactive mode if requested or no specific command
    if matches.get_flag("interactive") || std::env::args().len() == 1 {
        // Clear screen before starting interactive mode
        clear_screen();
        start_memory_interactive_mode(provider_manager, config).await?;
    } else {
        // Handle direct commands here in the future
        println!("Direct command mode not implemented yet. Use --interactive or -i for interactive mode.");
    }

    Ok(())
}

fn clear_screen() {
    // Use ANSI escape codes instead of reset command for cross-platform compatibility
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush().unwrap_or(());
}

fn cleanup_child_processes() {
    // Terminate any running child process groups from command stack
    let stack = COMMAND_STACK.lock().unwrap();
    for entry in stack.iter() {
        println!("🧹 Cleaning up child process group {}", entry.pgid);
        unsafe {
            // First try SIGTERM for graceful shutdown
            killpg(entry.pgid, SIGTERM);
            
            // Give processes time to cleanup
            std::thread::sleep(std::time::Duration::from_millis(100));
            
            // Force kill if still running
            killpg(entry.pgid, SIGINT);
        }
    }
}

async fn get_interactive_input(buffer: &mut String) -> Result<Option<String>, Box<dyn std::error::Error>> {
    buffer.clear();
    
    // Enable raw mode to capture CTRL+C and other key events
    enable_raw_mode()?;
    
    let result = loop {
        // Check for input events with a short timeout
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key_event) => {
                    match key_event.code {
                        KeyCode::Char(c) => {
                            if key_event.modifiers.contains(KeyModifiers::CONTROL) && c == 'c' {
                                // Check for SIGINT flag from native handler
                                if SIGINT_RECEIVED.load(Ordering::Relaxed) {
                                    SIGINT_RECEIVED.store(false, Ordering::Relaxed); // Reset flag
                                    
                                    // Check if any command is running on the stack
                                    let stack = COMMAND_STACK.lock().unwrap();
                                    
                                    if stack.is_empty() {
                                        // No command running, quit the application
                                        println!("\n^C");
                                        break Ok(None);
                                    } else {
                                        // Commands are running, but CTRL+C is handled by their execution loops
                                        // Just continue here
                                        continue;
                                    }
                                }
                            } else {
                                // Regular character input
                                buffer.push(c);
                                print!("{}", c);
                                io::stdout().flush()?;
                            }
                        }
                        KeyCode::Enter => {
                            println!();
                            break Ok(Some(buffer.clone()));
                        }
                        KeyCode::Backspace => {
                            if !buffer.is_empty() {
                                buffer.pop();
                                print!("\x08 \x08"); // Backspace, space, backspace
                                io::stdout().flush()?;
                            }
                        }
                        KeyCode::Esc => {
                            // Escape key - clear current input
                            for _ in 0..buffer.len() {
                                print!("\x08 \x08");
                            }
                            buffer.clear();
                            io::stdout().flush()?;
                        }
                        _ => {
                            // Ignore other keys
                        }
                    }
                }
                _ => {
                    // Ignore other events
                }
            }
        }
    };
    
    // Always disable raw mode before returning
    disable_raw_mode()?;
    result
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

async fn start_memory_interactive_mode(provider_manager: ProviderManager, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("Comrude - Universal AI Development Assistant");
    println!("Available commands: <question>, /reset, /select, /help, /providers, /list, /model, /memory, /clear, /quit");
    println!("Type '/help' for more information.\n");

    let provider_manager = Arc::new(provider_manager);
    
    // Initialize ComrudeEngine with memory
    let memory_config = config.memory.clone().into();
    let mut engine = ComrudeEngine::new_with_config(memory_config);
    let _session_id = engine.create_session(Some("Main Session".to_string())).await?;
    
    let mut input_buffer = String::new();
    
    loop {
        print!("comrude> ");
        io::stdout().flush()?;
        
        // Get input using signal-aware event handling
        let command = match get_interactive_input(&mut input_buffer).await? {
            Some(cmd) => cmd,
            None => break, // EOF or quit signal
        };
        
        if command.is_empty() {
            continue;
        }
        
        if command == "quit" || command == "exit" || command == "q" || 
           command == "/quit" || command == "/exit" || command == "/q" {
            break;
        }
        
        if let Err(e) = process_memory_command(&provider_manager, &mut engine, &command).await {
            eprintln!("Error processing command: {}", e);
        }
    }
    
    // Clear screen on exit
    clear_screen();
    
    // Clean up any running child processes
    cleanup_child_processes();
    
    Ok(())
}

async fn process_memory_command(
    provider_manager: &Arc<ProviderManager>,
    engine: &mut ComrudeEngine,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    match parts[0] {
        "/help" => {
            show_help();
        }
        "/providers" => {
            list_providers(provider_manager).await;
        }
        "/list" => {
            list_models(provider_manager).await;
        }
        "/reset" => {
            // Clear the console
            print!("\x1B[2J\x1B[1;1H");
            println!("Comrude - Universal AI Development Assistant");
            println!("Available commands: <question>, /reset, /select, /help, /providers, /list, /model, /memory, /clear, /quit");
            println!("Type '/help' for more information.\n");
        }
        "/quit" | "/exit" | "/q" => {
            // Exit the application gracefully
            std::process::exit(0);
        }
        _ if parts[0] == "/memory" => {
            if parts.len() > 1 {
                // /memory <content> - add persistent instruction
                let persistent_content = parts[1..].join(" ");
                handle_memory_add_instruction(engine, persistent_content).await?;
            } else {
                // /memory - display formatted memory context
                handle_memory_display(engine).await?;
            }
        }
        _ if parts[0] == "/clear" => {
            // Clear both screen and memory context
            handle_clear_command(engine).await?;
        }
        _ if parts[0] == "/select" => {
            if parts.len() > 1 {
                let provider_name = parts[1];
                handle_select_with_name(provider_manager, provider_name).await?;
            } else {
                handle_select_command(provider_manager).await?;
            }
        }
        _ if parts[0] == "/model" => {
            if parts.len() > 1 {
                let model_name = parts[1];
                handle_model_command(provider_manager, model_name).await?;
            } else {
                show_current_model(provider_manager).await;
            }
        }
        _ => {
            // Always treat user input as a question for the AI with memory
            // The LLM will interpret and generate appropriate commands
            handle_memory_ask_command(provider_manager, engine, command.to_string()).await?;
        }
    }

    Ok(())
}

fn validate_and_clean_cli_response(response: &str) -> String {
    let response = response.trim();
    
    // Check if response already follows CLI format (contains code blocks or commands)
    if response.contains("```") || response.starts_with("sudo ") || response.starts_with("cat >") || response.starts_with("mkdir ") || response.starts_with("touch ") {
        return response.to_string();
    }
    
    // Check for prohibited explanatory text patterns
    let prohibited_patterns = [
        "let me", "i'll", "here's", "this will", "to do this", "you can", "first,", "next,", "then,", "finally,",
        "explanation:", "note:", "tip:", "important:", "remember:", "here is", "this is"
    ];
    
    let response_lower = response.to_lowercase();
    let has_explanations = prohibited_patterns.iter().any(|&pattern| response_lower.contains(pattern));
    
    if has_explanations {
        // Attempt to extract only code/command portions
        let lines: Vec<&str> = response.lines().collect();
        let mut clean_lines = Vec::new();
        
        for line in lines {
            let _line_lower = line.trim().to_lowercase();
            // Keep lines that look like commands or code
            if line.trim().is_empty() || 
               line.starts_with("sudo ") || line.starts_with("mkdir ") || line.starts_with("cat >") ||
               line.starts_with("chmod ") || line.starts_with("./") || line.starts_with("python") ||
               line.starts_with("gcc ") || line.starts_with("cargo ") || line.starts_with("npm ") ||
               line.starts_with("#include") || line.starts_with("#!/") || line.contains("EOF") {
                clean_lines.push(line);
            }
        }
        
        if !clean_lines.is_empty() {
            clean_lines.join("\n")
        } else {
            // Fallback: return original but with warning prefix
            format!("# WARNING: Response may contain explanations - CLI format preferred\n{}", response)
        }
    } else {
        response.to_string()
    }
}

fn supports_system_prompt(provider_name: &Option<String>) -> bool {
    match provider_name {
        Some(name) => {
            let name_lower = name.to_lowercase();
            name_lower.contains("openai") || name_lower.contains("anthropic") || name_lower.contains("claude") || name_lower.contains("gpt")
        }
        None => false
    }
}

fn load_cli_system_prompt() -> Result<String, Box<dyn std::error::Error>> {
    let cli_prompt_path = "cli_agent_system_prompt.md";
    match std::fs::read_to_string(cli_prompt_path) {
        Ok(content) => Ok(content),
        Err(_) => {
            // Fallback CLI instructions if file is not found
            Ok(r#"You are a CLI agent. Respond ONLY with executable code or terminal commands. No explanations.

MANDATORY FORMAT:
- Code: ```language\n[code]\n```\n---\n```bash\n[save/execute commands]\n```
- Commands: Direct terminal commands only

PROHIBITED: Explanations, comments, natural language responses."#.to_string())
        }
    }
}

async fn handle_memory_ask_command(
    provider_manager: &Arc<ProviderManager>,
    engine: &mut ComrudeEngine,
    question: String,
) -> Result<(), Box<dyn std::error::Error>> {
    use comrude_core::GenerationRequest;
    use std::collections::HashMap;

    // Check if any providers are available
    let providers = provider_manager.list_providers().await;
    if providers.is_empty() {
        eprintln!("Error: No providers available. Please configure at least one:");
        eprintln!("  - Set ANTHROPIC_API_KEY environment variable for Claude");
        eprintln!("  - Set OPENAI_API_KEY environment variable for GPT");
        eprintln!("  - Install and run Ollama for local models");
        return Ok(());
    }

    // Create user message
    let user_message = Message::new_user(question.clone());
    
    // Start conversation turn with memory context
    let _turn_id = engine.start_conversation_turn(user_message, vec![]).await?;

    // Get context from memory for the request
    let context = engine.get_context_for_request().await?;
    
    // Load CLI system prompt
    let cli_system_prompt = load_cli_system_prompt()?;
    
    // Get current provider for fallback detection
    let current_provider = provider_manager.get_current_provider_name().await;
    
    // Build request with CLI enforcement
    let request = if supports_system_prompt(&current_provider) {
        // Use system prompt for supported providers
        GenerationRequest {
            prompt: question,
            model: None,
            system_prompt: Some(cli_system_prompt),
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: false,
            tools: Vec::new(),
            context,
            metadata: HashMap::new(),
        }
    } else {
        // Fallback: wrap prompt with CLI instructions for unsupported providers
        let enforced_prompt = format!("{}\n\nUser Request: {}", cli_system_prompt, question);
        GenerationRequest {
            prompt: enforced_prompt,
            model: None,
            system_prompt: None,
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: false,
            tools: Vec::new(),
            context,
            metadata: HashMap::new(),
        }
    };

    match provider_manager.generate(request).await {
        Ok(response) => {
            // Validate and potentially clean CLI response
            let cli_response = validate_and_clean_cli_response(&response.content);
            
            // Print CLI-validated response
            println!("\n{}\n", cli_response);
            
            // Parse and execute commands from LLM response
            execute_commands_from_response(&cli_response).await?;
            
            // Create assistant message and complete the conversation turn
            let assistant_message = Message::new_assistant(
                cli_response.clone(), 
                response.model_used.clone(), 
                response.model_used.clone()
            );
            engine.complete_conversation_turn(assistant_message).await?;
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

async fn execute_commands_from_response(response: &str) -> Result<(), Box<dyn std::error::Error>> {
    let commands = parse_commands_from_response(response);
    
    if commands.is_empty() {
        return Ok(());
    }
    
    println!("󱁍 Commands detected in response:");
    for (i, cmd) in commands.iter().enumerate() {
        println!("  {}: {}", i + 1, cmd);
    }
    
    let auto_confirm = {
        let lock = AUTO_CONFIRM.lock().unwrap();
        *lock
    };
    
    if auto_confirm {
        println!("🚀 Auto-confirmation enabled. Executing commands...");
        for cmd in &commands {
            execute_single_command(cmd).await?;
        }
    } else {
        println!("\n󰊠 Execute these commands? [y/N/a(ll)/s(kip)]");
        println!("  y/Y = Execute next command");
        println!("  a/A = Execute all commands");
        println!("  s/S = Skip all commands");
        println!("  SHIFT+TAB = Toggle auto-confirmation");
        
        let mut i = 0;
        while i < commands.len() {
            let cmd = &commands[i];
            println!("\nCommand {}/{}: {}", i + 1, commands.len(), cmd);
            
            match get_user_confirmation().await? {
                UserChoice::Yes => {
                    execute_single_command(cmd).await?;
                    i += 1;
                }
                UserChoice::All => {
                    for remaining_cmd in &commands[i..] {
                        execute_single_command(remaining_cmd).await?;
                    }
                    break;
                }
                UserChoice::Skip => {
                    println!("Skipping remaining commands.");
                    break;
                }
                UserChoice::ToggleAutoConfirm => {
                    toggle_auto_confirm();
                    println!("Auto-confirmation toggled.");
                }
            }
        }
    }
    
    Ok(())
}

#[derive(Debug)]
enum UserChoice {
    Yes,
    All,
    Skip,
    ToggleAutoConfirm,
}

async fn get_user_confirmation() -> Result<UserChoice, Box<dyn std::error::Error>> {
    print!("Execute? [y/N/a/s] (SHIFT+TAB for auto-mode): ");
    io::stdout().flush()?;
    
    enable_raw_mode()?;
    
    loop {
        if let Ok(Event::Key(key_event)) = event::read() {
            disable_raw_mode()?;
            
            match key_event.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    println!("y");
                    return Ok(UserChoice::Yes);
                }
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    println!("a");
                    return Ok(UserChoice::All);
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    println!("s");
                    return Ok(UserChoice::Skip);
                }
                KeyCode::BackTab => {
                    println!("[AUTO-CONFIRM TOGGLED]");
                    return Ok(UserChoice::ToggleAutoConfirm);
                }
                KeyCode::Enter | KeyCode::Char('n') | KeyCode::Char('N') => {
                    println!("n");
                    return Ok(UserChoice::Skip);
                }
                _ => {
                    enable_raw_mode()?;
                    continue;
                }
            }
        }
    }
}

fn toggle_auto_confirm() {
    let mut lock = AUTO_CONFIRM.lock().unwrap();
    *lock = !*lock;
    let status = if *lock { "ENABLED" } else { "DISABLED" };
    println!("Auto-confirmation: {}", status);
}

async fn execute_single_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 Executing: {}", command);
    
    // Safety check for dangerous commands
    if is_dangerous_command(command) {
        println!("⚠️  DANGEROUS COMMAND DETECTED!");
        println!("Command: {}", command);
        print!("Are you SURE you want to execute this? [y/N]: ");
        io::stdout().flush()?;
        
        let mut confirmation = String::new();
        io::stdin().read_line(&mut confirmation)?;
        
        if !confirmation.trim().to_lowercase().starts_with('y') {
            println!("Command execution cancelled for safety.");
            return Ok(());
        }
    }
    
    // Choose execution mode based on command type
    if is_interactive_command(command) {
        execute_interactive_command(command).await
    } else {
        execute_batch_command(command).await
    }
}

fn is_interactive_command(command: &str) -> bool {
    let interactive_commands = [
        "ping", "tail", "watch", "top", "htop", "less", "more",
        "docker logs", "kubectl logs", "npm run", "cargo run", 
        "python -u", "node", "ssh", "telnet", "nc", "netcat",
        "gdb", "lldb", "mysql", "psql", "redis-cli", "mongo"
    ];
    
    interactive_commands.iter().any(|&cmd| command.starts_with(cmd))
}

async fn execute_interactive_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 Running interactive command (CTRL+C to interrupt)...");
    
    
    // Create process with new process group for signal isolation
    let mut child = if command.contains("&&") || command.contains("||") || command.contains(";") {
        let mut cmd = ProcessCommand::new("bash");
        cmd.arg("-c")
           .arg(command)
           .stdout(Stdio::inherit())
           .stderr(Stdio::inherit())
           .stdin(Stdio::inherit());
        
        // Use pre_exec to set new process group before exec
        unsafe {
            cmd.pre_exec(|| {
                // Create new process group with child as leader
                setpgid(0, 0);
                Ok(())
            });
        }
        
        cmd.spawn()?
    } else {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }
        
        let mut cmd = ProcessCommand::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        
        cmd.stdout(Stdio::inherit())
           .stderr(Stdio::inherit())
           .stdin(Stdio::inherit());
        
        // Use pre_exec to set new process group before exec
        unsafe {
            cmd.pre_exec(|| {
                // Create new process group with child as leader
                setpgid(0, 0);
                Ok(())
            });
        }
        
        cmd.spawn()?
    };
    
    let child_pid = child.id();
    let child_pgid = child_pid as i32; // Child is its own process group leader
    println!("🔄 Child process {} running in new process group {}", child_pid, child_pgid);
    
    // Push command to stack
    push_command_to_stack(command.to_string(), child_pid, child_pgid);
    
    // Execute with isolated signal handling
    let exit_status = execute_with_signal_isolation(&mut child).await?;
    
    // Pop command from stack
    pop_command_from_stack();
    
    if let Some(code) = exit_status.code() {
        if code == 0 {
            println!("✅ Command completed successfully");
        } else {
            println!("❌ Command failed with exit code: {}", code);
        }
    } else {
        println!("🚫 Command terminated by signal");
    }
    
    Ok(())
}

// Push command to the command stack
fn push_command_to_stack(command: String, pid: u32, pgid: i32) {
    let entry = CommandStackEntry { command, pid, pgid };
    let mut stack = COMMAND_STACK.lock().unwrap();
    stack.push_back(entry);
    println!("📚 Command stack depth: {}", stack.len());
}

// Pop command from the command stack
fn pop_command_from_stack() {
    let mut stack = COMMAND_STACK.lock().unwrap();
    stack.pop_back();
    println!("📚 Command stack depth: {}", stack.len());
}

// Get the current active command from stack
fn get_current_command() -> Option<CommandStackEntry> {
    let stack = COMMAND_STACK.lock().unwrap();
    stack.back().cloned()
}

// Execute with clean terminal output and native signal handling
async fn execute_with_signal_isolation(child: &mut std::process::Child) -> Result<std::process::ExitStatus, Box<dyn std::error::Error>> {
    // Main execution loop - child process has completely normal terminal access
    let exit_status = loop {
        // Check if child has finished
        match child.try_wait()? {
            Some(status) => {
                // Child finished normally
                break status;
            }
            None => {
                // Check for SIGINT flag from native signal handler
                if SIGINT_RECEIVED.load(Ordering::Relaxed) {
                    // CTRL+C was detected - handle it
                    SIGINT_RECEIVED.store(false, Ordering::Relaxed); // Reset flag
                    
                    // Check if we have a command in the stack
                    let stack = COMMAND_STACK.lock().unwrap();
                    if let Some(cmd_entry) = stack.back() {
                        // Send SIGINT to the command's process group
                        unsafe {
                            killpg(cmd_entry.pgid, SIGINT);
                        }
                        drop(stack); // Release lock before waiting
                        
                        // Wait for child to actually exit after signal
                        if let Ok(status) = child.wait() {
                            break status;
                        }
                    } else {
                        // No command running, should not happen in this context
                        // but if it does, just continue
                        drop(stack);
                    }
                }
                // Small sleep to avoid busy waiting
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    };
    
    Ok(exit_status)
}

async fn execute_batch_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = if command.contains("&&") || command.contains("||") || command.contains(";") {
        // Execute complex command through shell
        ProcessCommand::new("bash")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?
    } else {
        // Parse and execute simple command
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }
        
        let mut cmd = ProcessCommand::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?
    };
    
    if output.status.success() {
        if !output.stdout.is_empty() {
            println!("✅ Output:");
            println!("{}", String::from_utf8_lossy(&output.stdout));
        } else {
            println!("✅ Command executed successfully (no output)");
        }
    } else {
        println!("❌ Command failed with exit code: {:?}", output.status.code());
        if !output.stderr.is_empty() {
            println!("Error output:");
            println!("{}", String::from_utf8_lossy(&output.stderr));
        }
    }
    
    Ok(())
}

fn parse_commands_from_response(response: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let lines: Vec<&str> = response.lines().collect();
    let mut in_bash_block = false;
    let mut in_code_block = false;
    
    for line in lines {
        let trimmed = line.trim();
        
        // Handle code blocks
        if trimmed.starts_with("```bash") || trimmed.starts_with("```sh") {
            in_bash_block = true;
            continue;
        } else if trimmed.starts_with("```") && in_bash_block {
            in_bash_block = false;
            continue;
        } else if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        
        // Skip content in non-bash code blocks
        if in_code_block && !in_bash_block {
            continue;
        }
        
        // Extract commands
        if in_bash_block || is_direct_command(trimmed) {
            if !trimmed.is_empty() && !trimmed.starts_with("#") && trimmed != "---" {
                commands.push(trimmed.to_string());
            }
        }
    }
    
    commands
}

fn is_direct_command(line: &str) -> bool {
    let trimmed = line.trim();
    
    // Empty lines are not commands
    if trimmed.is_empty() {
        return false;
    }
    
    // Skip comments
    if trimmed.starts_with('#') {
        return false;
    }
    
    // Check for obvious question patterns first
    let question_indicators = [
        "how ", "what ", "why ", "when ", "where ", "which ", "who ",
        "can you ", "could you ", "would you ", "please ", "help me ",
        "explain ", "show me ", "tell me ", "i need ", "i want ",
        "how to ", "what is ", "como ", "o que ", "por que ", "quando ",
        "onde ", "qual ", "quem ", "pode ", "poderia ", "ajude-me ",
        "explique ", "mostre-me ", "me diga ", "preciso ", "quero ",
        "?", "como fazer", "what's", "how's", "pingue ", "execute ",
        "faça ", "rode ", "mostre ", "liste ", "crie ", "delete "
    ];
    
    let line_lower = trimmed.to_lowercase();
    if question_indicators.iter().any(|&indicator| line_lower.contains(indicator)) {
        return false;
    }
    
    // Check if it starts with common command patterns
    let command_patterns = [
        // Direct executable calls
        "./", "/", "~/" , 
        // Common shell commands that are very likely to be intentional
        "sudo ", "ssh ", "scp ", "rsync ", "git ", "docker ", "kubectl ",
        "systemctl ", "service ", "apt ", "yum ", "pip ", "npm ", "cargo ",
        "make ", "cmake ", "gcc ", "g++ ", "rustc ", "javac ", "python ",
        "node ", "go ", "ruby ", "php ", "perl ", "bash ", "sh ", "zsh ",
        // File operations with clear syntax
        "ls ", "ll ", "cat ", "less ", "more ", "head ", "tail ", "grep ",
        "find ", "locate ", "which ", "whereis ", "file ", "stat ",
        "cp ", "mv ", "rm ", "mkdir ", "rmdir ", "touch ", "chmod ", "chown ",
        // Network tools
        "ping ", "traceroute ", "nslookup ", "dig ", "curl ", "wget ",
        "nc ", "netcat ", "telnet ", "ftp ", "sftp ",
        // Process management
        "ps ", "top ", "htop ", "kill ", "killall ", "jobs ", "nohup ",
        // Text processing
        "sed ", "awk ", "sort ", "uniq ", "wc ", "tr ", "cut ",
        // Archive tools
        "tar ", "gzip ", "gunzip ", "zip ", "unzip "
    ];
    
    // Check for exact command pattern matches
    if command_patterns.iter().any(|&pattern| line_lower.starts_with(pattern)) {
        return true;
    }
    
    // For single word commands, be more restrictive
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() == 1 {
        // Single words are likely questions unless they're very common commands
        let common_single_commands = [
            "pwd", "date", "whoami", "uptime", "free", "df", "du", "history",
            "clear", "exit", "logout", "reboot", "shutdown", "sync"
        ];
        return common_single_commands.contains(&parts[0]);
    }
    
    // For multi-word, check if first word looks like a real command
    if parts.len() > 1 {
        let first_word = parts[0];
        
        // Must be reasonable length and format
        if first_word.len() > 0 && first_word.len() < 20 &&
           first_word.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') &&
           !first_word.chars().all(|c| c.is_numeric()) &&
           !first_word.chars().next().unwrap().is_uppercase() { // Commands usually lowercase
            return true;
        }
    }
    
    false
}

fn is_dangerous_command(command: &str) -> bool {
    let dangerous_patterns = [
        "rm -rf /", "rm -rf /*", ":(){ :|:& };:", "dd if=", "mkfs.",
        "format ", "fdisk ", "parted ", "> /dev/", "chmod 777 /",
        "chown root", "sudo su", "sudo -i", "passwd root",
        "userdel ", "deluser ", "shutdown ", "reboot ", "halt ",
        "init 0", "init 6", "systemctl poweroff", "systemctl reboot"
    ];
    
    dangerous_patterns.iter().any(|&pattern| command.contains(pattern))
}

async fn handle_memory_display(engine: &ComrudeEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Memory Context Status");
    println!("========================\n");
    
    // Get conversation summary
    match engine.get_conversation_summary(None).await {
        Ok(turns) => {
            if turns.is_empty() {
                println!("📝 No conversation history found.\n");
            } else {
                println!("📝 Conversation History ({} turns):", turns.len());
                println!("------------------------------------");
                
                for (i, turn) in turns.iter().enumerate() {
                    let timestamp = turn.timestamp.format("%Y-%m-%d %H:%M:%S");
                    println!("\n🔸 Turn {} ({})", i + 1, timestamp);
                    
                    // Display user message
                    match &turn.user_message.content {
                        comrude_core::MessageContent::Text(text) => {
                            let preview = if text.len() > 100 {
                                format!("{}...", &text[..100])
                            } else {
                                text.clone()
                            };
                            println!("  👤 User: {}", preview);
                        },
                        comrude_core::MessageContent::Code { language, content } => {
                            let preview = if content.len() > 50 {
                                format!("{}...", &content[..50])
                            } else {
                                content.clone()
                            };
                            println!("  👤 User: [Code in {}] {}", language, preview);
                        },
                        _ => {
                            println!("  👤 User: [Non-text content]");
                        }
                    }
                    
                    // Display assistant response if available
                    if let Some(ref response) = turn.assistant_response {
                        match &response.content {
                            comrude_core::MessageContent::Text(text) => {
                                let preview = if text.len() > 100 {
                                    format!("{}...", &text[..100])
                                } else {
                                    text.clone()
                                };
                                println!("  🤖 Assistant: {}", preview);
                            },
                            comrude_core::MessageContent::Code { language, content } => {
                                let preview = if content.len() > 50 {
                                    format!("{}...", &content[..50])
                                } else {
                                    content.clone()
                                };
                                println!("  🤖 Assistant: [Code in {}] {}", language, preview);
                            },
                            _ => {
                                println!("  🤖 Assistant: [Non-text content]");
                            }
                        }
                    }
                    
                    println!("  📊 Tokens used: {}", turn.tokens_used);
                }
                println!();
            }
        },
        Err(e) => {
            println!("❌ Error retrieving conversation history: {}\n", e);
        }
    }
    
    // Get current context for requests
    match engine.get_context_for_request().await {
        Ok(context) => {
            if context.is_empty() {
                println!("🔄 Current Context: Empty\n");
            } else {
                println!("🔄 Current Context ({} items):", context.len());
                println!("-----------------------------");
                
                for (i, item) in context.iter().enumerate() {
                    let preview = if item.content.len() > 80 {
                        format!("{}...", &item.content[..80])
                    } else {
                        item.content.clone()
                    };
                    
                    let role = item.metadata.get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    
                    let icon = match role {
                        "user" => "👤",
                        "assistant" => "🤖",
                        "system" => "⚙️",
                        _ => "📄"
                    };
                    
                    println!("  {} Item {}: {}", icon, i + 1, preview);
                }
                println!();
            }
        },
        Err(e) => {
            println!("❌ Error retrieving current context: {}\n", e);
        }
    }
    
    // Show memory statistics
    println!("📊 Memory Statistics:");
    println!("-------------------");
    println!("  💾 Max context turns: 3");
    println!("  🎯 Max context tokens: 8000");
    println!("  🗜️  Compression enabled: Yes");
    println!("  📈 Summarization enabled: Yes");
    println!();
    
    Ok(())
}

async fn handle_memory_add_instruction(engine: &mut ComrudeEngine, content: String) -> Result<(), Box<dyn std::error::Error>> {
    println!("💾 Adding persistent instruction to memory...");
    
    // Create a system message with the persistent instruction
    let system_message = comrude_core::Message::new_system(format!("[PERSISTENT INSTRUCTION] {}", content));
    let context = vec![];
    
    // Add the instruction as a conversation turn
    match engine.start_conversation_turn(system_message.clone(), context).await {
        Ok(_turn_id) => {
            // Complete the turn with the same content as confirmation
            let confirmation_message = comrude_core::Message::new_system(
                "Persistent instruction added to memory context.".to_string()
            );
            
            match engine.complete_conversation_turn(confirmation_message).await {
                Ok(_) => {
                    println!("✅ Persistent instruction added successfully:");
                    println!("   📝 \"{}\"", content);
                    println!("   ℹ️  This instruction will be included in all future requests.\n");
                },
                Err(e) => {
                    println!("❌ Error completing instruction addition: {}\n", e);
                }
            }
        },
        Err(e) => {
            println!("❌ Error adding persistent instruction: {}\n", e);
        }
    }
    
    Ok(())
}

async fn handle_clear_command(engine: &mut ComrudeEngine) -> Result<(), Box<dyn std::error::Error>> {
    println!("🗑️ Clearing screen and memory context...");
    
    // Clear the screen first
    clear_screen();
    
    // Create a new session to effectively clear memory
    match engine.create_session(Some("Fresh Session".to_string())).await {
        Ok(_session_id) => {
            println!("✅ Memory context cleared successfully!");
            println!("🔄 Started fresh session with clean memory.\n");
            
            // Show the standard welcome message
            println!("Comrude - Universal AI Development Assistant");
            println!("Available commands: <question>, /reset, /select, /help, /providers, /list, /model, /memory, /clear, /quit");
            println!("Type '/help' for more information.\n");
        },
        Err(e) => {
            println!("❌ Error clearing memory context: {}", e);
            println!("🔄 Screen cleared, but memory context may still be active.\n");
        }
    }
    
    Ok(())
}

fn show_help() {
    let help_text = r#"
Comrude - Universal AI Development Assistant

Commands:
  <question>          - Ask a question to the AI (no prefix needed)
  /reset              - Clear the console
  /select             - Select which AI provider to use (interactive)
  /select <provider>  - Select provider directly by name
  /help               - Show this help message
  /providers          - List available providers
  /list               - List available models for current provider
  /model              - Show current model
  /model <model_id>   - Select model for current provider
  /memory             - Display formatted memory context and conversation history
  /memory <content>   - Add persistent instruction to memory context
  /clear              - Clear both screen and memory context (fresh session)
  /quit, /exit, /q    - Exit the application

Command Execution Features:
  • Automatic detection of CLI commands in LLM responses
  • User confirmation before execution (y/N/a/s)
  • SHIFT+TAB toggles auto-confirmation mode
  • Safety checks for dangerous commands
  • Supports both simple and complex shell commands

Execution Controls:
  y/Y = Execute next command
  a/A = Execute all remaining commands
  s/S = Skip all commands
  n/N = Skip current command
  SHIFT+TAB = Toggle auto-confirmation mode

Examples:
  What is Rust?
  How do I create a vector in Rust?
  Create a new Rust project
  Install a package with cargo
  /select
  /select anthropic
  /list
  /model codellama:7b
  /reset
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