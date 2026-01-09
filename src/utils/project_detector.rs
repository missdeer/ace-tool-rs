//! Project root detection utilities

use std::fs;
use std::path::{Path, PathBuf};

/// Get the .ace-tool directory path for a project
/// Creates the directory if it doesn't exist
pub fn get_ace_dir(project_root: &Path) -> PathBuf {
    let ace_dir = project_root.join(".ace-tool");

    if !ace_dir.exists() {
        if let Err(e) = fs::create_dir_all(&ace_dir) {
            tracing::warn!("Failed to create .ace-tool directory: {}", e);
        } else {
            // Try to add .ace-tool to .gitignore
            add_to_gitignore(project_root);
        }
    }

    ace_dir
}

/// Add .ace-tool to .gitignore
fn add_to_gitignore(project_root: &Path) {
    let gitignore_path = project_root.join(".gitignore");

    let content = if gitignore_path.exists() {
        match fs::read_to_string(&gitignore_path) {
            Ok(c) => c,
            Err(_) => return,
        }
    } else {
        String::new()
    };

    // Check if already included
    if gitignore_has_ace_tool(&content) {
        return;
    }

    // Add .ace-tool to .gitignore
    let new_content = if content.ends_with('\n') || content.is_empty() {
        format!("{}.ace-tool/\n", content)
    } else {
        format!("{}\n.ace-tool/\n", content)
    };

    if let Err(e) = fs::write(&gitignore_path, new_content) {
        tracing::warn!("Failed to update .gitignore: {}", e);
    }
}

fn gitignore_has_ace_tool(content: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        let entry = line.split('#').next().unwrap_or(line).trim();
        entry == ".ace-tool" || entry == ".ace-tool/"
    })
}

/// Get index file path
pub fn get_index_file_path(project_root: &Path) -> PathBuf {
    let ace_dir = get_ace_dir(project_root);
    ace_dir.join("index.bin")
}
