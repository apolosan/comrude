//! File operation tools
//! 
//! Tools for reading, writing, and manipulating files that can be
//! exposed to LLM providers.

use std::path::Path;
use std::fs;
use anyhow::Result;

/// Read the contents of a file
pub async fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let content = fs::read_to_string(path)?;
    Ok(content)
}

/// Write content to a file
pub async fn write_file<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    fs::write(path, content)?;
    Ok(())
}

/// List files in a directory
pub async fn list_directory<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let entries = fs::read_dir(path)?;
    let mut files = Vec::new();
    
    for entry in entries {
        let entry = entry?;
        files.push(entry.file_name().to_string_lossy().to_string());
    }
    
    Ok(files)
}