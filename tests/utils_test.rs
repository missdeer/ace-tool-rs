//! Tests for utils module

use std::fs;
use tempfile::TempDir;

use ace_tool::utils::project_detector::{get_ace_dir, get_index_file_path};

#[test]
fn test_get_ace_dir_creates_directory() {
    let temp_dir = TempDir::new().unwrap();
    let ace_dir = get_ace_dir(temp_dir.path());

    assert!(ace_dir.exists());
    assert!(ace_dir.is_dir());
    assert_eq!(ace_dir, temp_dir.path().join(".ace-tool"));
}

#[test]
fn test_get_ace_dir_idempotent() {
    let temp_dir = TempDir::new().unwrap();

    // Call twice
    let ace_dir1 = get_ace_dir(temp_dir.path());
    let ace_dir2 = get_ace_dir(temp_dir.path());

    assert_eq!(ace_dir1, ace_dir2);
    assert!(ace_dir1.exists());
}

#[test]
fn test_get_ace_dir_adds_to_gitignore_new_file() {
    let temp_dir = TempDir::new().unwrap();
    let gitignore_path = temp_dir.path().join(".gitignore");

    // No .gitignore initially
    assert!(!gitignore_path.exists());

    // Create .ace-tool dir
    get_ace_dir(temp_dir.path());

    // .gitignore should now exist with .ace-tool
    assert!(gitignore_path.exists());
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains(".ace-tool/"));
}

#[test]
fn test_get_ace_dir_adds_to_existing_gitignore() {
    let temp_dir = TempDir::new().unwrap();
    let gitignore_path = temp_dir.path().join(".gitignore");

    // Create existing .gitignore
    fs::write(&gitignore_path, "node_modules/\n").unwrap();

    // Create .ace-tool dir
    get_ace_dir(temp_dir.path());

    // .gitignore should contain both
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("node_modules/"));
    assert!(content.contains(".ace-tool/"));
}

#[test]
fn test_get_ace_dir_does_not_duplicate_in_gitignore() {
    let temp_dir = TempDir::new().unwrap();
    let gitignore_path = temp_dir.path().join(".gitignore");

    // Create .gitignore that already has .ace-tool
    fs::write(&gitignore_path, "node_modules/\n.ace-tool/\n").unwrap();

    // Create .ace-tool dir
    get_ace_dir(temp_dir.path());

    // Should not have duplicate entries
    let content = fs::read_to_string(&gitignore_path).unwrap();
    let count = content.matches(".ace-tool").count();
    assert_eq!(count, 1);
}

#[test]
fn test_get_ace_dir_ignores_similar_gitignore_entries() {
    let temp_dir = TempDir::new().unwrap();
    let gitignore_path = temp_dir.path().join(".gitignore");

    // .ace-tooling should not block adding .ace-tool/
    fs::write(&gitignore_path, ".ace-tooling/\n").unwrap();

    get_ace_dir(temp_dir.path());

    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains(".ace-tool/"));
    let count = content
        .lines()
        .filter(|line| {
            let line = line.trim();
            line == ".ace-tool" || line == ".ace-tool/"
        })
        .count();
    assert_eq!(count, 1);
}

#[test]
fn test_get_ace_dir_handles_gitignore_without_trailing_newline() {
    let temp_dir = TempDir::new().unwrap();
    let gitignore_path = temp_dir.path().join(".gitignore");

    // Create .gitignore without trailing newline
    fs::write(&gitignore_path, "node_modules/").unwrap();

    // Create .ace-tool dir
    get_ace_dir(temp_dir.path());

    // Should add newline before .ace-tool
    let content = fs::read_to_string(&gitignore_path).unwrap();
    assert!(content.contains("node_modules/\n.ace-tool/"));
}

#[test]
fn test_get_index_file_path() {
    let temp_dir = TempDir::new().unwrap();
    let index_path = get_index_file_path(temp_dir.path());

    assert_eq!(
        index_path,
        temp_dir.path().join(".ace-tool").join("index.json")
    );
    // The .ace-tool directory should have been created
    assert!(temp_dir.path().join(".ace-tool").exists());
}

#[test]
fn test_get_index_file_path_consistent() {
    let temp_dir = TempDir::new().unwrap();

    let path1 = get_index_file_path(temp_dir.path());
    let path2 = get_index_file_path(temp_dir.path());

    assert_eq!(path1, path2);
}
