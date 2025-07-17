# Comrude - Your Universal AI Development Comrade

<p align="center">
  <strong>An open-source, Rust-powered, interactive terminal interface for universal access to local and cloud-based Large Language Models (LLMs).</strong>
</p>

<p align="center">
  <a href="https://crates.io/crates/comrude"><img src="https://img.shields.io/crates/v/comrude.svg" alt="Crates.io"></a>
  <a href="https://github.com/apolosan/comrude/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-GPLv3-blue.svg" alt="License"></a>
</p>

---

**Comrude** is a command-line tool designed to be a developer's best friend. It combines the raw power and performance of Rust with an intelligent command-line interface, providing a seamless experience for interacting with a wide array of AI providers. Whether you're using OpenAI's GPT-4, Anthropic's Claude 3, or running local models via Ollama, Comrude offers a single, unified, and powerful interface with advanced memory capabilities and intelligent command execution.

## 🌟 Key Features

- **Intelligent CLI Interface**: Direct command-line interaction with context-aware AI responses and real-time command execution.
- **Universal LLM Access**: Native support for major providers like **OpenAI**, **Anthropic**, and local models through **Ollama**.
- **Advanced Memory System**: Persistent conversation history with intelligent compression and context management across sessions.
- **Smart Command Execution**: Automatic CLI command interpretation and execution from AI responses with proper signal handling.
- **Extensible & Modular**: Built with a clean, multi-crate architecture in Rust, allowing for easy extension and maintenance.
- **Context-Aware**: Automatically detects your project's context, including files and Git status, to provide more relevant AI assistance.
- **Performance-First**: Blazing-fast startup times and low memory footprint, thanks to its Rust core.
- **Signal-Aware Operation**: Proper CTRL+C handling for command interruption without terminating the main application.
- **Offline Capability**: Full support for local models via Ollama, ensuring privacy and functionality without an internet connection.

## 🏗️ Architecture Overview

Comrude is built on a modular workspace architecture, ensuring separation of concerns and high maintainability.

```
comrude/
├── crates/
│   ├── comrude          # Main binary crate with CLI interface and signal handling
│   ├── comrude-core     # Core logic, memory system, and execution engine
│   ├── comrude-providers# Abstraction layer for all LLM providers (OpenAI, Anthropic, Ollama)
│   ├── comrude-shell    # TUI components and event handling
│   └── comrude-tools    # Utility functions (file system, shell commands)
├── .comrude/
│   └── sessions/        # Persistent memory storage
└── tests/
    ├── integration      # Cross-crate integration tests
    └── e2e              # Full end-to-end user workflow tests
```

This structure allows for robust, independent testing of each component and facilitates community contributions to specific areas like adding new providers or tools.

## 🚀 Getting Started

### Prerequisites

- **Rust**: Install the Rust toolchain via [rustup](https://rustup.rs/).
- **Git**: Required for context-aware features.

### Installation

1.  **Clone the repository:**

    ```bash
    git clone https://github.com/apolosan/comrude.git
    cd comrude
    ```

2.  **Build the project:**

    ```bash
    cargo build --release
    ```

3.  **Run Comrude:**
    The executable will be located at `target/release/comrude`. You can run it directly or add it to your system's PATH.
    ```bash
    ./target/release/comrude
    ```

### Configuration

Comrude automatically detects configuration from `~/.config/comrude/config.toml` or creates defaults. Configure providers using environment variables:

```bash
# Set API keys as environment variables
export ANTHROPIC_API_KEY="your_anthropic_key_here"
export OPENAI_API_KEY="your_openai_key_here"

# Run Comrude
./target/release/comrude
```

**Configuration Example (`~/.config/comrude/config.toml`):**

```toml
# App configuration
[app]
default_provider = "anthropic"

# Memory system configuration
[memory]
max_context_turns = 3
max_context_tokens = 8000
enable_diff_compression = true
enable_summarization = true
session_storage_path = ".comrude/sessions"
session_max_age_days = 30

# Provider configurations
[providers.openai]
enabled = true
api_key_env = "OPENAI_API_KEY"

[providers.anthropic]
enabled = true
api_key_env = "ANTHROPIC_API_KEY"

[providers.ollama]
enabled = true # No API key needed for local models
base_url = "http://localhost:11434"
```

## 🎮 Usage

Launch Comrude by simply running the executable. You will be greeted by the interactive shell with persistent memory.

### Interactive Commands

Comrude features an intelligent CLI system where the AI interprets your natural language and generates appropriate commands:

```bash
# Direct questions - AI remembers context across sessions
comrude> How do I list all files in the current directory?
# AI responds with CLI command and executes it

comrude> Create a Rust function that reads a file
# AI generates code and provides save commands

comrude> ping google.com
# AI interprets and executes: ping google.com
# Use CTRL+C to interrupt commands (not Comrude itself)
```

### System Commands

- **`/help`**: Show available commands and usage information
- **`/providers`**: List available AI providers
- **`/select [provider]`**: Switch between AI providers
- **`/model [model_name]`**: Change the current model
- **`/memory [instruction]`**: Add persistent instructions or view memory context
- **`/clear`**: Clear both screen and memory context
- **`/reset`**: Reset the interface
- **`/quit`** or **`/exit`**: Exit Comrude

### Memory System

Comrude includes an advanced memory system that:

- Remembers your name, preferences, and conversation history
- Persists context across application restarts
- Automatically compresses old conversations to maintain performance
- Provides intelligent context retrieval for relevant responses

```bash
comrude> My name is João and I prefer TypeScript
comrude> /quit
# Restart Comrude
comrude> What's my name and preferred language?
# AI remembers: "Your name is João and you prefer TypeScript"
```

## 🧪 Testing

The project maintains a comprehensive test suite to ensure quality and stability.

- **Run all tests:**
  ```bash
  cargo test
  ```
- **Run tests for a specific package:**
  ```bash
  cargo test -p comrude-core
  ```

## 🤝 Contributing

Contributions are welcome! Whether it's adding a new provider, improving the UI, or fixing a bug, please feel free to open an issue or submit a pull request.

## 📜 License

This project is licensed under the **GNU General Public License v3.0**. See the [LICENSE](LICENSE) file for details.
