//! Tests for index module

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

use ace_tool::config::Config;
use ace_tool::index::{Blob, IndexManager, IndexResult, IndexStats};

fn create_test_config() -> Arc<Config> {
    Config::new(
        "https://api.example.com".to_string(),
        "test-token".to_string(),
    )
    .unwrap()
}

fn create_test_manager(project_root: PathBuf) -> IndexManager {
    let config = create_test_config();
    IndexManager::new(config, project_root).unwrap()
}

#[test]
fn test_calculate_blob_name() {
    let hash1 = IndexManager::calculate_blob_name("test.rs", "fn main() {}");
    let hash2 = IndexManager::calculate_blob_name("test.rs", "fn main() {}");
    let hash3 = IndexManager::calculate_blob_name("test.rs", "fn main() { }");
    let hash4 = IndexManager::calculate_blob_name("other.rs", "fn main() {}");

    // Same path and content should produce same hash
    assert_eq!(hash1, hash2);
    // Different content should produce different hash
    assert_ne!(hash1, hash3);
    // Different path should produce different hash
    assert_ne!(hash1, hash4);
    // Hash should be 64 characters (SHA-256 hex)
    assert_eq!(hash1.len(), 64);
}

#[test]
fn test_sanitize_content() {
    // Should keep normal text
    let normal = "Hello, World!\nThis is a test.";
    assert_eq!(IndexManager::sanitize_content(normal), normal);

    // Should remove NULL characters
    let with_null = "Hello\x00World";
    assert_eq!(IndexManager::sanitize_content(with_null), "HelloWorld");

    // Should remove control characters but keep newlines and tabs
    let with_controls = "Hello\x01\x02\x03World\n\tTest";
    assert_eq!(
        IndexManager::sanitize_content(with_controls),
        "HelloWorld\n\tTest"
    );

    // Should keep carriage returns
    let with_cr = "Line1\r\nLine2";
    assert_eq!(IndexManager::sanitize_content(with_cr), "Line1\r\nLine2");
}

#[test]
fn test_is_binary_content() {
    // Normal text should not be binary
    let text = "This is normal text with some punctuation! @#$%";
    assert!(!IndexManager::is_binary_content(text));

    // Content with many null bytes should be binary
    let binary = "\x00\x01\x02\x03\x04\x05\x06\x07\x08normal";
    assert!(IndexManager::is_binary_content(binary));

    // Less than 10% non-printable should not be binary
    let mostly_text = "Normal text with one \x00 null byte in a longer string";
    assert!(!IndexManager::is_binary_content(mostly_text));
}

#[test]
fn test_split_file_content_small_file() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let content = "line1\nline2\nline3";
    let blobs = manager.split_file_content("test.txt", content);

    assert_eq!(blobs.len(), 1);
    assert_eq!(blobs[0].path, "test.txt");
    assert_eq!(blobs[0].content, content);
}

#[test]
fn test_split_file_content_large_file() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = (*create_test_config()).clone();
    config.max_lines_per_blob = 10;
    let config = Arc::new(config);

    let manager = IndexManager::new(config, temp_dir.path().to_path_buf()).unwrap();

    // Create content with 25 lines
    let lines: Vec<String> = (1..=25).map(|i| format!("line{}", i)).collect();
    let content = lines.join("\n");

    let blobs = manager.split_file_content("test.txt", &content);

    // Should be split into 3 chunks (10, 10, 5)
    assert_eq!(blobs.len(), 3);
    assert_eq!(blobs[0].path, "test.txt#chunk1of3");
    assert_eq!(blobs[1].path, "test.txt#chunk2of3");
    assert_eq!(blobs[2].path, "test.txt#chunk3of3");
}

#[test]
fn test_match_pattern_simple() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Exact match
    assert!(manager.match_pattern("node_modules", "node_modules"));
    assert!(!manager.match_pattern("node_module", "node_modules"));

    // Wildcard match
    assert!(manager.match_pattern("test.pyc", "*.pyc"));
    assert!(manager.match_pattern("module.pyc", "*.pyc"));
    assert!(!manager.match_pattern("test.py", "*.pyc"));

    // Single character wildcard
    assert!(manager.match_pattern("test1", "test?"));
    assert!(manager.match_pattern("testA", "test?"));
    assert!(!manager.match_pattern("test12", "test?"));
}

#[test]
fn test_blob_serialization() {
    let blob = Blob {
        path: "src/main.rs".to_string(),
        content: "fn main() {}".to_string(),
    };

    let json = serde_json::to_string(&blob).unwrap();
    assert!(json.contains("src/main.rs"));
    assert!(json.contains("fn main() {}"));

    let deserialized: Blob = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.path, blob.path);
    assert_eq!(deserialized.content, blob.content);
}

#[test]
fn test_index_manager_new() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();

    let manager = IndexManager::new(config.clone(), temp_dir.path().to_path_buf());
    assert!(manager.is_ok());

    let manager = manager.unwrap();
    assert_eq!(manager.base_url(), "https://api.example.com");
    assert_eq!(manager.token(), "test-token");
}

#[test]
fn test_load_save_index() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Initially empty
    let index = manager.load_index();
    assert!(index.is_empty());

    // Save some blob names
    let blob_names = vec![
        "hash1".to_string(),
        "hash2".to_string(),
        "hash3".to_string(),
    ];
    manager.save_index(&blob_names).unwrap();

    // Load and verify
    let loaded = manager.load_index();
    assert_eq!(loaded.len(), 3);
    assert!(loaded.contains(&"hash1".to_string()));
    assert!(loaded.contains(&"hash2".to_string()));
    assert!(loaded.contains(&"hash3".to_string()));
}

#[test]
fn test_collect_files_with_text_files() {
    let temp_dir = TempDir::new().unwrap();

    // Create some test files
    let rs_file = temp_dir.path().join("main.rs");
    let mut f = fs::File::create(&rs_file).unwrap();
    writeln!(f, "fn main() {{ println!(\"Hello\"); }}").unwrap();

    let txt_file = temp_dir.path().join("readme.txt");
    let mut f = fs::File::create(&txt_file).unwrap();
    writeln!(f, "This is a readme").unwrap();

    let manager = create_test_manager(temp_dir.path().to_path_buf());
    let blobs = manager.collect_files().unwrap();

    // Check that the expected files are included (may include .gitignore from get_ace_dir)
    let paths: Vec<&str> = blobs.iter().map(|b| b.path.as_str()).collect();
    assert!(paths.contains(&"main.rs"));
    assert!(paths.contains(&"readme.txt"));
    assert!(blobs.len() >= 2);
}

#[test]
fn test_collect_files_excludes_binary_extensions() {
    let temp_dir = TempDir::new().unwrap();

    // Create a text file
    let rs_file = temp_dir.path().join("main.rs");
    fs::write(&rs_file, "fn main() {}").unwrap();

    // Create a "binary" file (by extension)
    let png_file = temp_dir.path().join("image.png");
    fs::write(&png_file, "fake png content").unwrap();

    let manager = create_test_manager(temp_dir.path().to_path_buf());
    let blobs = manager.collect_files().unwrap();

    // main.rs should be included, image.png should not
    let paths: Vec<&str> = blobs.iter().map(|b| b.path.as_str()).collect();
    assert!(paths.contains(&"main.rs"));
    assert!(!paths.contains(&"image.png"));
}

#[test]
fn test_collect_files_excludes_directories() {
    let temp_dir = TempDir::new().unwrap();

    // Create a file
    let rs_file = temp_dir.path().join("main.rs");
    fs::write(&rs_file, "fn main() {}").unwrap();

    // Create node_modules directory with a file
    let node_modules = temp_dir.path().join("node_modules");
    fs::create_dir(&node_modules).unwrap();
    let js_file = node_modules.join("package.js");
    fs::write(&js_file, "module.exports = {}").unwrap();

    let manager = create_test_manager(temp_dir.path().to_path_buf());
    let blobs = manager.collect_files().unwrap();

    // main.rs should be included, file in node_modules should not
    let paths: Vec<&str> = blobs.iter().map(|b| b.path.as_str()).collect();
    assert!(paths.contains(&"main.rs"));
    assert!(!paths.iter().any(|p| p.contains("node_modules")));
}

#[test]
fn test_index_result_fields() {
    let result = IndexResult {
        status: "success".to_string(),
        message: "Indexed 10 blobs".to_string(),
        stats: Some(IndexStats {
            total_blobs: 10,
            existing_blobs: 5,
            new_blobs: 5,
            failed_batches: None,
        }),
    };

    assert_eq!(result.status, "success");
    assert!(result.stats.is_some());
    let stats = result.stats.unwrap();
    assert_eq!(stats.total_blobs, 10);
    assert_eq!(stats.existing_blobs, 5);
    assert_eq!(stats.new_blobs, 5);
}
