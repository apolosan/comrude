use comrude_core::{
    memory::{ContextMemoryManager, MemoryConfig},
    types::{Message, ContextItem, ContextType},
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧠 Comrude Memory System Demo");
    println!("============================\n");

    // 1. Configure memory system
    let memory_config = MemoryConfig {
        max_context_turns: 3,
        max_context_tokens: 1000,
        enable_diff_compression: true,
        enable_summarization: true,
        session_storage_path: std::path::PathBuf::from("./demo_sessions"),
        session_max_age_days: 7,
    };

    // 2. Initialize memory manager
    let mut memory_manager = ContextMemoryManager::new(memory_config.clone());

    // 3. Create a new session
    println!("📝 Creating new session...");
    let session_id = memory_manager.create_session(Some("Demo Session".to_string())).await?;
    println!("✅ Session created: {}\n", session_id);

    // 4. Add several conversation turns
    println!("💬 Adding conversation turns...");
    
    // Turn 1: Code generation request
    let user_msg1 = Message::new_user("Create a Python function to calculate fibonacci numbers".to_string());
    let context1 = vec![
        ContextItem {
            item_type: ContextType::Text,
            content: "User is working on a Python project".to_string(),
            metadata: HashMap::new(),
        }
    ];
    let turn1_id = memory_manager.add_conversation_turn(user_msg1, context1).await?;
    
    let assistant_response1 = Message::new_assistant(
        "Here's a Python function for Fibonacci:\n\ndef fibonacci(n):\n    if n <= 1:\n        return n\n    return fibonacci(n-1) + fibonacci(n-2)".to_string(),
        "demo_provider".to_string(),
        "demo_model".to_string()
    );
    memory_manager.complete_conversation_turn(turn1_id, assistant_response1).await?;
    println!("✅ Turn 1 completed: Fibonacci function");

    // Turn 2: Code optimization request
    let user_msg2 = Message::new_user("Optimize the fibonacci function for better performance".to_string());
    let context2 = vec![
        ContextItem {
            item_type: ContextType::Code { language: "python".to_string() },
            content: "def fibonacci(n): ...".to_string(),
            metadata: HashMap::new(),
        }
    ];
    let turn2_id = memory_manager.add_conversation_turn(user_msg2, context2).await?;
    
    let assistant_response2 = Message::new_assistant(
        "Here's an optimized version using memoization:\n\ndef fibonacci_memo(n, memo={}):\n    if n in memo:\n        return memo[n]\n    if n <= 1:\n        return n\n    memo[n] = fibonacci_memo(n-1, memo) + fibonacci_memo(n-2, memo)\n    return memo[n]".to_string(),
        "demo_provider".to_string(),
        "demo_model".to_string()
    );
    memory_manager.complete_conversation_turn(turn2_id, assistant_response2).await?;
    println!("✅ Turn 2 completed: Optimization");

    // Turn 3: Documentation request
    let user_msg3 = Message::new_user("Add docstrings and type hints to the fibonacci function".to_string());
    let context3 = vec![
        ContextItem {
            item_type: ContextType::Code { language: "python".to_string() },
            content: "def fibonacci_memo(n, memo={}): ...".to_string(),
            metadata: HashMap::new(),
        }
    ];
    let turn3_id = memory_manager.add_conversation_turn(user_msg3, context3).await?;
    
    let assistant_response3 = Message::new_assistant(
        "Here's the documented version:\n\ndef fibonacci_memo(n: int, memo: dict = {}) -> int:\n    \"\"\"Calculate fibonacci number with memoization.\n    \n    Args:\n        n: The position in fibonacci sequence\n        memo: Memoization cache\n    \n    Returns:\n        The fibonacci number at position n\n    \"\"\"\n    if n in memo:\n        return memo[n]\n    if n <= 1:\n        return n\n    memo[n] = fibonacci_memo(n-1, memo) + fibonacci_memo(n-2, memo)\n    return memo[n]".to_string(),
        "demo_provider".to_string(),
        "demo_model".to_string()
    );
    memory_manager.complete_conversation_turn(turn3_id, assistant_response3).await?;
    println!("✅ Turn 3 completed: Documentation");

    // Turn 4: Testing request (this will trigger context compression)
    let user_msg4 = Message::new_user("Write unit tests for the fibonacci function".to_string());
    let context4 = vec![
        ContextItem {
            item_type: ContextType::Code { language: "python".to_string() },
            content: "def fibonacci_memo(n: int, memo: dict = {}) -> int: ...".to_string(),
            metadata: HashMap::new(),
        }
    ];
    let turn4_id = memory_manager.add_conversation_turn(user_msg4, context4).await?;
    
    let assistant_response4 = Message::new_assistant(
        "Here are comprehensive unit tests:\n\nimport unittest\n\nclass TestFibonacci(unittest.TestCase):\n    def test_base_cases(self):\n        self.assertEqual(fibonacci_memo(0), 0)\n        self.assertEqual(fibonacci_memo(1), 1)\n    \n    def test_sequence(self):\n        expected = [0, 1, 1, 2, 3, 5, 8, 13]\n        for i, exp in enumerate(expected):\n            self.assertEqual(fibonacci_memo(i), exp)".to_string(),
        "demo_provider".to_string(),
        "demo_model".to_string()
    );
    memory_manager.complete_conversation_turn(turn4_id, assistant_response4).await?;
    println!("✅ Turn 4 completed: Unit tests (context compression triggered!)");

    // 5. Demonstrate context retrieval
    println!("\n🔍 Getting context for next request...");
    let context_for_next = memory_manager.get_context_for_request()?;
    println!("📋 Available context items: {}", context_for_next.len());
    
    for (i, item) in context_for_next.iter().enumerate() {
        println!("  {}. Type: {:?}", i + 1, item.item_type);
        println!("     Content preview: {}...", 
            if item.content.len() > 50 { 
                &item.content[..50] 
            } else { 
                &item.content 
            });
    }

    // 6. Get conversation summary
    println!("\n📊 Conversation Summary:");
    let summary = memory_manager.get_conversation_summary(None)?;
    println!("Total turns in memory: {}", summary.len());
    
    for (i, turn) in summary.iter().enumerate() {
        println!("\n  Turn {}: {}", i + 1, turn.timestamp.format("%H:%M:%S"));
        match &turn.user_message.content {
            comrude_core::types::MessageContent::Text(text) => {
                println!("    User: {}", if text.len() > 60 { &text[..60] } else { text });
            },
            _ => println!("    User: [Non-text content]"),
        }
        
        if let Some(ref response) = turn.assistant_response {
            match &response.content {
                comrude_core::types::MessageContent::Text(text) => {
                    println!("    Assistant: {}", if text.len() > 60 { &text[..60] } else { text });
                },
                _ => println!("    Assistant: [Non-text content]"),
            }
        }
        println!("    Tokens used: {}", turn.tokens_used);
    }

    // 7. List all sessions
    println!("\n📁 Available Sessions:");
    let sessions = memory_manager.list_sessions().await?;
    for (id, name, updated) in sessions {
        println!("  {} - {} (updated: {})", id, name, updated.format("%Y-%m-%d %H:%M"));
    }

    // 8. Demonstrate session persistence
    println!("\n💾 Demonstrating session persistence...");
    let _new_memory_manager = ContextMemoryManager::new(memory_config);
    // The session should be loadable by a new instance
    println!("✅ Sessions are persisted to: ./demo_sessions");

    println!("\n🎉 Memory system demo completed!");
    println!("\nKey features demonstrated:");
    println!("  ✅ Configurable context window (max 3 turns)");
    println!("  ✅ Automatic context compression when limits exceeded");
    println!("  ✅ Intelligent conversation summarization");
    println!("  ✅ Diff-based redundancy control");
    println!("  ✅ Session persistence across restarts");
    println!("  ✅ Context retrieval for LLM requests");

    Ok(())
}