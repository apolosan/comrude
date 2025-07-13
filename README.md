# Comrude - Your Universal AI Development Comrade

<p align="center">
  <img src="https://raw.githubusercontent.com/apolosan/comrude/main/assets/comrude_logo.png" alt="Comrude Logo" width="200"/>
</p>

<p align="center">
  <strong>An open-source, Rust-powered, interactive terminal interface for universal access to local and cloud-based Large Language Models (LLMs).</strong>
</p>

<p align="center">
  <a href="https://github.com/apolosan/comrude/actions/workflows/tests.yml"><img src="https://github.com/apolosan/comrude/actions/workflows/tests.yml/badge.svg" alt="Tests"></a>
  <a href="https://crates.io/crates/comrude"><img src="https://img.shields.io/crates/v/comrude.svg" alt="Crates.io"></a>
  <a href="https://github.com/apolosan/comrude/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-GPLv3-blue.svg" alt="License"></a>
</p>

---

**Comrude** is a command-line tool designed to be a developer's best friend. It combines the raw power and performance of Rust with a rich, interactive Terminal User Interface (TUI), providing a seamless experience for interacting with a wide array of AI providers. Whether you're using OpenAI's GPT-4, Anthropic's Claude 3, or running local models via Ollama, Comrude offers a single, unified, and powerful interface.

## 🌟 Key Features

- **Rich Interactive TUI**: A modern, multi-panel terminal interface with syntax highlighting, real-time previews, and customizable themes.
- **Universal LLM Access**: Native support for major providers like **OpenAI**, **Anthropic**, **Google AI**, and local models through **Ollama**.
- **Extensible & Modular**: Built with a clean, multi-crate architecture in Rust, allowing for easy extension and maintenance.
- **Context-Aware**: Automatically detects your project's context, including files and Git status, to provide more relevant AI assistance.
- **Intelligent Command System**: Features like smart auto-completion, command history, and AI-generated suggestions to boost your productivity.
- **Performance-First**: Blazing-fast startup times and low memory footprint, thanks to its Rust core.
- **Offline Capability**: Full support for local models via Ollama, ensuring privacy and functionality without an internet connection.

## 🏗️ Architecture Overview

Comrude is built on a modular workspace architecture, ensuring separation of concerns and high maintainability.

```
comrude/
├── crates/
│   ├── comrude          # Main binary crate and entry point
│   ├── comrude-core     # Core logic, execution engine, and type definitions
│   ├── comrude-providers# Abstraction layer for all LLM providers (OpenAI, Ollama, etc.)
│   ├── comrude-shell    # The interactive TUI, event handling, and UI components
│   └── comrude-tools    # Utility functions (file system, shell commands)
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

On the first run, Comrude will create a default configuration file at `~/.config/comrude/config.toml`. You'll need to add your API keys to this file to enable cloud-based providers.

```toml
# ~/.config/comrude/config.toml

[providers.openai]
enabled = true
api_key_env = "OPENAI_API_KEY" # Set this environment variable

[providers.anthropic]
enabled = true
api_key_env = "ANTHROPIC_API_KEY" # Set this environment variable

[providers.ollama]
enabled = true # No API key needed for local models
```

## 🎮 Usage

Launch Comrude by simply running the executable. You will be greeted by the interactive shell.

### Basic Commands

-   **`ask "your question"`**: The primary command to interact with the AI.
    ```bash
    > ask "How do I parse command-line arguments in Rust?"
    ```
-   **`code "your request"`**: Specifically for generating code snippets.
    ```bash
    > code "create a Rust function that reads a file and returns its content"
    ```
-   **`review <file_or_directory>`**: Ask the AI to review code for bugs, style, or security.
    ```bash
    > review src/main.rs
    ```
-   **`context <subcommand>`**: Manage the files and information the AI has in its context.
    ```bash
    > context add src/
    > context show
    ```

Use `Tab` for smart auto-completion and `Up`/`Down` arrows to navigate your command history.

## 🧪 Testing

The project maintains a comprehensive test suite to ensure quality and stability.

-   **Run all tests:**
    ```bash
    cargo test
    ```
-   **Run tests for a specific package:**
    ```bash
    cargo test -p comrude-core
    ```

## 🤝 Contributing

Contributions are welcome! Whether it's adding a new provider, improving the UI, or fixing a bug, please feel free to open an issue or submit a pull request.

## 📜 License

This project is licensed under the **GNU General Public License v3.0**. See the [LICENSE](LICENSE) file for details.
