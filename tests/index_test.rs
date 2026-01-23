//! Tests for index module

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

use ace_tool::config::{Config, ConfigOptions};
use ace_tool::index::{Blob, FileEntry, IndexData, IndexManager, IndexResult, IndexStats};

fn create_test_config() -> Arc<Config> {
    Config::new(
        "https://api.example.com".to_string(),
        "test-token".to_string(),
        ConfigOptions::default(),
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
    assert!(index.entries.is_empty());

    // Save some blob names using new IndexData format
    let mut index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries: std::collections::HashMap::new(),
    };
    index_data.entries.insert(
        "file1.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    index_data.entries.insert(
        "file2.rs".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string(), "hash3".to_string()],
        },
    );
    manager.save_index(&index_data).unwrap();

    // Load and verify
    let loaded = manager.load_index();
    assert_eq!(loaded.entries.len(), 2);
    let all_hashes = loaded.get_all_blob_hashes();
    assert_eq!(all_hashes.len(), 3);
    assert!(all_hashes.contains(&"hash1".to_string()));
    assert!(all_hashes.contains(&"hash2".to_string()));
    assert!(all_hashes.contains(&"hash3".to_string()));
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

// ============================================================================
// IndexData and FileEntry structure tests
// ============================================================================

#[test]
fn test_index_data_default() {
    let index = IndexData::default();
    assert_eq!(index.version, 0);
    assert!(index.config_hash.is_empty());
    assert!(index.entries.is_empty());
}

#[test]
fn test_index_data_serialization() {
    let mut entries = HashMap::new();
    entries.insert(
        "src/main.rs".to_string(),
        FileEntry {
            mtime_secs: 1704067200,
            mtime_nanos: 123456789,
            size: 1024,
            blob_hashes: vec!["abc123".to_string(), "def456".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "test_hash_123".to_string(),
        entries,
    };

    let json = serde_json::to_string(&index).unwrap();
    let deserialized: IndexData = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.version, 2);
    assert_eq!(deserialized.config_hash, "test_hash_123");
    assert_eq!(deserialized.entries.len(), 1);

    let entry = deserialized.entries.get("src/main.rs").unwrap();
    assert_eq!(entry.mtime_secs, 1704067200);
    assert_eq!(entry.mtime_nanos, 123456789);
    assert_eq!(entry.size, 1024);
    assert_eq!(entry.blob_hashes.len(), 2);
}

#[test]
fn test_index_data_get_all_blob_hashes() {
    let mut entries = HashMap::new();
    entries.insert(
        "file1.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string(), "hash2".to_string()],
        },
    );
    entries.insert(
        "file2.rs".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash3".to_string()],
        },
    );
    entries.insert(
        "file3.rs".to_string(),
        FileEntry {
            mtime_secs: 3000,
            mtime_nanos: 0,
            size: 300,
            blob_hashes: vec![], // Empty blob_hashes
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    let all_hashes = index.get_all_blob_hashes();
    assert_eq!(all_hashes.len(), 3);
    assert!(all_hashes.contains(&"hash1".to_string()));
    assert!(all_hashes.contains(&"hash2".to_string()));
    assert!(all_hashes.contains(&"hash3".to_string()));
}

#[test]
fn test_file_entry_serialization() {
    let entry = FileEntry {
        mtime_secs: 1704067200,
        mtime_nanos: 500000000,
        size: 2048,
        blob_hashes: vec!["abc".to_string(), "def".to_string(), "ghi".to_string()],
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: FileEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.mtime_secs, entry.mtime_secs);
    assert_eq!(deserialized.mtime_nanos, entry.mtime_nanos);
    assert_eq!(deserialized.size, entry.size);
    assert_eq!(deserialized.blob_hashes, entry.blob_hashes);
}

#[test]
fn test_file_entry_clone() {
    let entry = FileEntry {
        mtime_secs: 1000,
        mtime_nanos: 500,
        size: 100,
        blob_hashes: vec!["hash1".to_string()],
    };

    let cloned = entry.clone();
    assert_eq!(cloned.mtime_secs, entry.mtime_secs);
    assert_eq!(cloned.mtime_nanos, entry.mtime_nanos);
    assert_eq!(cloned.size, entry.size);
    assert_eq!(cloned.blob_hashes, entry.blob_hashes);
}

// ============================================================================
// Config hash tests
// ============================================================================

#[test]
fn test_config_hash_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let config = create_test_config();

    let manager1 = IndexManager::new(config.clone(), temp_dir.path().to_path_buf()).unwrap();
    let manager2 = IndexManager::new(config, temp_dir.path().to_path_buf()).unwrap();

    // Same config should produce same hash
    assert_eq!(manager1.config_hash(), manager2.config_hash());
}

#[test]
fn test_config_hash_changes_with_max_lines() {
    let temp_dir = TempDir::new().unwrap();

    let mut config1 = (*create_test_config()).clone();
    config1.max_lines_per_blob = 100;

    let mut config2 = (*create_test_config()).clone();
    config2.max_lines_per_blob = 200;

    let manager1 = IndexManager::new(Arc::new(config1), temp_dir.path().to_path_buf()).unwrap();
    let manager2 = IndexManager::new(Arc::new(config2), temp_dir.path().to_path_buf()).unwrap();

    // Different max_lines_per_blob should produce different hash
    assert_ne!(manager1.config_hash(), manager2.config_hash());
}

#[test]
fn test_config_hash_not_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    assert!(!manager.config_hash().is_empty());
    // Config hash should be a hex string (16 chars = 8 bytes)
    assert_eq!(manager.config_hash().len(), 16);
}

// ============================================================================
// Index version and migration tests
// ============================================================================

#[test]
fn test_load_index_with_wrong_config_hash_returns_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Save index with different config_hash
    let mut index_data = IndexData {
        version: 2,
        config_hash: "different_hash".to_string(),
        entries: HashMap::new(),
    };
    index_data.entries.insert(
        "file.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    manager.save_index(&index_data).unwrap();

    // Load should return empty index due to config_hash mismatch
    let loaded = manager.load_index();
    assert!(loaded.entries.is_empty());
}

#[test]
fn test_load_index_with_wrong_version_returns_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Save index with old version
    let index_data = IndexData {
        version: 1, // Old version
        config_hash: manager.config_hash().to_string(),
        entries: HashMap::new(),
    };
    manager.save_index(&index_data).unwrap();

    // Load should return empty index due to version mismatch
    let loaded = manager.load_index();
    assert!(loaded.entries.is_empty());
}

#[test]
fn test_load_index_corrupted_data_returns_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Write corrupted bincode data
    let ace_dir = temp_dir.path().join(".ace-tool");
    fs::create_dir_all(&ace_dir).unwrap();
    let index_path = ace_dir.join("index.bin");
    fs::write(&index_path, b"invalid bincode data").unwrap();

    // Load should return empty index
    let loaded = manager.load_index();
    assert!(loaded.entries.is_empty());
}

#[test]
fn test_load_index_nonexistent_returns_empty() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // No index file exists
    let loaded = manager.load_index();
    assert!(loaded.entries.is_empty());
}

// ============================================================================
// Atomic save tests
// ============================================================================

#[test]
fn test_save_index_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries: HashMap::new(),
    };

    manager.save_index(&index_data).unwrap();

    let index_path = temp_dir.path().join(".ace-tool").join("index.bin");
    assert!(index_path.exists());
}

#[test]
fn test_save_index_overwrites_existing() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Save first index
    let mut index1 = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries: HashMap::new(),
    };
    index1.entries.insert(
        "file1.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    manager.save_index(&index1).unwrap();

    // Save second index with different content
    let mut index2 = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries: HashMap::new(),
    };
    index2.entries.insert(
        "file2.rs".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string()],
        },
    );
    manager.save_index(&index2).unwrap();

    // Load should return second index
    let loaded = manager.load_index();
    assert_eq!(loaded.entries.len(), 1);
    assert!(loaded.entries.contains_key("file2.rs"));
    assert!(!loaded.entries.contains_key("file1.rs"));
}

#[test]
fn test_save_index_no_temp_file_left() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries: HashMap::new(),
    };
    manager.save_index(&index_data).unwrap();

    // Check no .tmp file exists
    let ace_dir = temp_dir.path().join(".ace-tool");
    let tmp_path = ace_dir.join("index.bin.tmp");
    assert!(!tmp_path.exists());
}

// ============================================================================
// FileEntry mtime handling tests
// ============================================================================

#[test]
fn test_file_entry_with_nanoseconds() {
    let entry = FileEntry {
        mtime_secs: 1704067200,
        mtime_nanos: 999999999, // Max nanoseconds
        size: 1024,
        blob_hashes: vec!["hash".to_string()],
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: FileEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.mtime_nanos, 999999999);
}

#[test]
fn test_file_entry_with_zero_nanoseconds() {
    // mtime_nanos=0 indicates low-precision filesystem
    let entry = FileEntry {
        mtime_secs: 1704067200,
        mtime_nanos: 0,
        size: 1024,
        blob_hashes: vec!["hash".to_string()],
    };

    assert_eq!(entry.mtime_nanos, 0);
}

#[test]
fn test_file_entry_large_file_size() {
    let entry = FileEntry {
        mtime_secs: 1704067200,
        mtime_nanos: 0,
        size: u64::MAX, // Maximum file size
        blob_hashes: vec!["hash".to_string()],
    };

    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: FileEntry = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.size, u64::MAX);
}

// ============================================================================
// Index entries management tests
// ============================================================================

#[test]
fn test_index_with_multiple_files() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let mut entries = HashMap::new();
    for i in 0..100 {
        entries.insert(
            format!("file{}.rs", i),
            FileEntry {
                mtime_secs: 1000 + i as u64,
                mtime_nanos: i as u32,
                size: 100 + i as u64,
                blob_hashes: vec![format!("hash{}", i)],
            },
        );
    }

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries,
    };
    manager.save_index(&index_data).unwrap();

    let loaded = manager.load_index();
    assert_eq!(loaded.entries.len(), 100);

    // Verify specific entries
    let entry50 = loaded.entries.get("file50.rs").unwrap();
    assert_eq!(entry50.mtime_secs, 1050);
    assert_eq!(entry50.mtime_nanos, 50);
}

#[test]
fn test_index_with_chunked_file() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    // Simulate a large file split into chunks
    let mut entries = HashMap::new();
    entries.insert(
        "large_file.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100000,
            blob_hashes: vec![
                "chunk1_hash".to_string(),
                "chunk2_hash".to_string(),
                "chunk3_hash".to_string(),
                "chunk4_hash".to_string(),
                "chunk5_hash".to_string(),
            ],
        },
    );

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries,
    };
    manager.save_index(&index_data).unwrap();

    let loaded = manager.load_index();
    let entry = loaded.entries.get("large_file.rs").unwrap();
    assert_eq!(entry.blob_hashes.len(), 5);
}

// ============================================================================
// Edge cases and error handling tests
// ============================================================================

#[test]
fn test_index_with_unicode_filenames() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let mut entries = HashMap::new();
    entries.insert(
        "文件.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    entries.insert(
        "ファイル.rs".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string()],
        },
    );
    entries.insert(
        "файл.rs".to_string(),
        FileEntry {
            mtime_secs: 3000,
            mtime_nanos: 0,
            size: 300,
            blob_hashes: vec!["hash3".to_string()],
        },
    );

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries,
    };
    manager.save_index(&index_data).unwrap();

    let loaded = manager.load_index();
    assert_eq!(loaded.entries.len(), 3);
    assert!(loaded.entries.contains_key("文件.rs"));
    assert!(loaded.entries.contains_key("ファイル.rs"));
    assert!(loaded.entries.contains_key("файл.rs"));
}

#[test]
fn test_index_with_special_path_characters() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let mut entries = HashMap::new();
    entries.insert(
        "path/with spaces/file.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    entries.insert(
        "path-with-dashes/file.rs".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string()],
        },
    );

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries,
    };
    manager.save_index(&index_data).unwrap();

    let loaded = manager.load_index();
    assert_eq!(loaded.entries.len(), 2);
}

#[test]
fn test_index_empty_blob_hashes() {
    let temp_dir = TempDir::new().unwrap();
    let manager = create_test_manager(temp_dir.path().to_path_buf());

    let mut entries = HashMap::new();
    entries.insert(
        "file.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec![], // Empty
        },
    );

    let index_data = IndexData {
        version: 2,
        config_hash: manager.config_hash().to_string(),
        entries,
    };
    manager.save_index(&index_data).unwrap();

    let loaded = manager.load_index();
    let entry = loaded.entries.get("file.rs").unwrap();
    assert!(entry.blob_hashes.is_empty());
}

// ============================================================================
// IndexStats tests
// ============================================================================

#[test]
fn test_index_stats_with_failed_batches() {
    let stats = IndexStats {
        total_blobs: 100,
        existing_blobs: 50,
        new_blobs: 45,
        failed_batches: Some(5),
    };

    assert_eq!(stats.total_blobs, 100);
    assert_eq!(stats.existing_blobs, 50);
    assert_eq!(stats.new_blobs, 45);
    assert_eq!(stats.failed_batches, Some(5));
}

#[test]
fn test_index_stats_no_failed_batches() {
    let stats = IndexStats {
        total_blobs: 100,
        existing_blobs: 50,
        new_blobs: 50,
        failed_batches: None,
    };

    assert_eq!(stats.failed_batches, None);
}

#[test]
fn test_index_result_success() {
    let result = IndexResult {
        status: "success".to_string(),
        message: "Indexed 100 blobs".to_string(),
        stats: Some(IndexStats {
            total_blobs: 100,
            existing_blobs: 0,
            new_blobs: 100,
            failed_batches: None,
        }),
    };

    assert_eq!(result.status, "success");
    assert!(result.message.contains("100"));
}

#[test]
fn test_index_result_partial() {
    let result = IndexResult {
        status: "partial".to_string(),
        message: "Indexed with some failures".to_string(),
        stats: Some(IndexStats {
            total_blobs: 100,
            existing_blobs: 50,
            new_blobs: 45,
            failed_batches: Some(5),
        }),
    };

    assert_eq!(result.status, "partial");
}

#[test]
fn test_index_result_error() {
    let result = IndexResult {
        status: "error".to_string(),
        message: "No files found".to_string(),
        stats: None,
    };

    assert_eq!(result.status, "error");
    assert!(result.stats.is_none());
}
