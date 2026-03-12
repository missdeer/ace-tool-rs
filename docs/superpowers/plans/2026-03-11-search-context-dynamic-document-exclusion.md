# SearchContext 动态排除文档类内容实现计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `search_context` MCP 工具增加"搜索时动态排除文档类内容"的能力，支持 `exclude_document_files`、`exclude_extensions`、`exclude_globs` 三个过滤参数。

**Architecture:** 采用"查询时按索引条目二次过滤 blob"方案（方案 C），不修改索引层逻辑。在 `search_context` 方法中，`load_index()` 后遍历 `IndexData.entries`，根据过滤参数筛选条目，收集过滤后的 `blob_hashes` 发送给检索服务。

**Tech Stack:** Rust + globset crate（用于 glob 模式预编译匹配）

---

## 文件变更清单

### 新增文件
| 文件路径 | 职责 |
|---------|------|
| `src/search_filter.rs` | 搜索过滤选项模型 + 过滤逻辑实现 |

### 修改文件
| 文件路径 | 变更内容 |
|---------|---------|
| `src/tools/search_context.rs:51-79` | 扩展 `get_input_schema()` 添加新参数定义 |
| `src/tools/search_context.rs:82-87` | 扩展 `SearchContextArgs` 添加新字段 |
| `src/tools/search_context.rs:106-165` | 修改 `execute()` 方法传递过滤参数 |
| `src/index/manager.rs:1247-1284` | 修改 `search_context()` 方法签名和 blob 收集逻辑 |
| `src/lib.rs` | 导出 `search_filter` 模块 |
| `Cargo.toml` | 添加 `globset` 依赖 |
| `tests/tools_test.rs` | 新增参数和 schema 测试 |
| `tests/index_test.rs` | 新增过滤逻辑测试 |

---

## Chunk 1: 依赖与基础模型

### Task 1: 添加 globset 依赖

**Files:**
- Modify: `Cargo.toml:58`

- [ ] **Step 1: 添加 globset crate 依赖**

在 `[dependencies]` 部分添加 globset：

```toml
# Glob pattern matching for search filtering
globset = "0.4"
```

- [ ] **Step 2: 验证依赖可编译**

Run: `cargo check`
Expected: 编译成功，无错误

---

### Task 2: 创建 SearchFilterOptions 模型

**Files:**
- Create: `src/search_filter.rs`

- [ ] **Step 3: 编写 SearchFilterOptions 结构体定义测试**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filter_options_default() {
        let filter = SearchFilterOptions::default();
        assert!(!filter.exclude_document_files);
        assert!(filter.exclude_extensions.is_empty());
        assert!(filter.exclude_globs.is_empty());
    }

    #[test]
    fn test_search_filter_options_from_args() {
        let args = SearchContextArgs {
            project_root_path: Some("/path".to_string()),
            query: Some("query".to_string()),
            exclude_document_files: Some(true),
            exclude_extensions: Some(vec![".md".to_string(), ".txt".to_string()]),
            exclude_globs: Some(vec!["docs/**".to_string()]),
        };

        let filter = SearchFilterOptions::from_args(&args);
        assert!(filter.exclude_document_files);
        assert!(filter.exclude_extensions.contains(".md"));
        assert!(filter.exclude_extensions.contains(".txt"));
        assert_eq!(filter.exclude_globs.len(), 1);
    }
}
```

- [ ] **Step 4: 运行测试验证失败**

Run: `cargo test search_filter --no-run`
Expected: 编译错误，类型未定义

- [ ] **Step 5: 实现 SearchFilterOptions 结构体**

```rust
//! Search filtering options for dynamic document exclusion

use std::collections::HashSet;

use globset::{Glob, GlobSetBuilder};

/// Default document file extensions to exclude when `exclude_document_files` is true
const DEFAULT_DOCUMENT_EXTENSIONS: &[&str] = &[
    ".md", ".mdx", ".txt", ".csv", ".tsv", ".rst", ".adoc", ".tex", ".org",
];

/// Search filter options for excluding entries from search results
#[derive(Debug, Clone, Default)]
pub struct SearchFilterOptions {
    /// Whether to exclude document files (md, txt, etc.)
    pub exclude_document_files: bool,
    /// Extensions to exclude (normalized to lowercase with leading dot)
    pub exclude_extensions: HashSet<String>,
    /// Glob patterns to exclude
    pub exclude_globs: Vec<String>,
    /// Compiled glob matcher (lazy initialization)
    compiled_globset: Option<globset::GlobSet>,
}

impl SearchFilterOptions {
    /// Create filter options from MCP tool arguments
    pub fn from_args(args: &crate::tools::search_context::SearchContextArgs) -> Self {
        let mut filter = Self::default();

        // Handle exclude_document_files
        filter.exclude_document_files = args.exclude_document_files.unwrap_or(false);

        // Handle exclude_extensions - normalize to lowercase with leading dot
        if let Some(ref exts) = args.exclude_extensions {
            for ext in exts {
                let normalized = normalize_extension(ext);
                if !normalized.is_empty() {
                    filter.exclude_extensions.insert(normalized);
                }
            }
        }

        // Handle exclude_globs
        if let Some(ref globs) = args.exclude_globs {
            filter.exclude_globs = globs.clone();
        }

        // Add default document extensions if exclude_document_files is true
        if filter.exclude_document_files {
            for ext in DEFAULT_DOCUMENT_EXTENSIONS {
                filter.exclude_extensions.insert(ext.to_string());
            }
        }

        filter
    }

    /// Compile glob patterns into a matcher (call once before filtering)
    pub fn compile_globs(&mut self) -> Result<(), globset::Error> {
        if self.exclude_globs.is_empty() {
            self.compiled_globset = None;
            return Ok(());
        }

        let mut builder = GlobSetBuilder::new();
        for pattern in &self.exclude_globs {
            builder.add(Glob::new(pattern)?);
        }

        self.compiled_globset = Some(builder.build()?);
        Ok(())
    }

    /// Check if a relative path should be excluded from search
    pub fn should_exclude(&self, rel_path: &str) -> bool {
        // Check extension exclusion
        if !self.exclude_extensions.is_empty() {
            if let Some(ext) = get_extension(rel_path) {
                if self.exclude_extensions.contains(&ext) {
                    return true;
                }
            }
        }

        // Check glob pattern exclusion
        if let Some(ref globset) = self.compiled_globset {
            if globset.is_match(rel_path) {
                return true;
            }
        }

        false
    }

    /// Check if any filtering is active
    pub fn is_active(&self) -> bool {
        self.exclude_document_files || !self.exclude_extensions.is_empty() || !self.exclude_globs.is_empty()
    }
}

/// Normalize extension to lowercase with leading dot
fn normalize_extension(ext: &str) -> String {
    let trimmed = ext.trim().to_lowercase();
    if trimmed.starts_with('.') {
        trimmed
    } else if !trimmed.is_empty() {
        format!(".{}", trimmed)
    } else {
        trimmed
    }
}

/// Extract extension from path (lowercase, with leading dot)
fn get_extension(path: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    let idx = path_lower.rfind('.')?;
    // Ensure the dot is not part of a directory name (no slash after the dot)
    if path_lower[idx..].contains('/') || path_lower[idx..].contains('\\') {
        return None;
    }
    Some(path_lower[idx..].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filter_options_default() {
        let filter = SearchFilterOptions::default();
        assert!(!filter.exclude_document_files);
        assert!(filter.exclude_extensions.is_empty());
        assert!(filter.exclude_globs.is_empty());
        assert!(!filter.is_active());
    }

    #[test]
    fn test_normalize_extension() {
        assert_eq!(normalize_extension("md"), ".md");
        assert_eq!(normalize_extension(".md"), ".md");
        assert_eq!(normalize_extension(" .TXT "), ".txt");
        assert_eq!(normalize_extension(""), "");
        assert_eq!(normalize_extension("  "), "");
    }

    #[test]
    fn test_get_extension() {
        assert_eq!(get_extension("README.md"), Some(".md".to_string()));
        assert_eq!(get_extension("src/main.rs"), Some(".rs".to_string()));
        assert_eq!(get_extension("docs/guide.MD"), Some(".md".to_string()));
        assert_eq!(get_extension("notes.TxT"), Some(".txt".to_string()));
        assert_eq!(get_extension("noextension"), None);
        assert_eq!(get_extension(".hidden"), None);
    }

    #[test]
    fn test_should_exclude_by_extension() {
        let mut filter = SearchFilterOptions::default();
        filter.exclude_extensions.insert(".md".to_string());

        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("docs/guide.MD")); // Case insensitive
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config.yaml"));
    }

    #[test]
    fn test_should_exclude_by_glob() {
        let mut filter = SearchFilterOptions {
            exclude_globs: vec!["docs/**".to_string(), "**/README*".to_string()],
            ..Default::default()
        };
        filter.compile_globs().unwrap();

        assert!(filter.should_exclude("docs/guide.md"));
        assert!(filter.should_exclude("docs/api/reference.rs"));
        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("subdir/README-zh-CN.md"));
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config/app.yaml"));
    }

    #[test]
    fn test_exclude_document_files() {
        let mut filter = SearchFilterOptions {
            exclude_document_files: true,
            ..Default::default()
        };
        // from_args would populate exclude_extensions with default doc extensions

        // Manually populate for this test
        for ext in DEFAULT_DOCUMENT_EXTENSIONS {
            filter.exclude_extensions.insert(ext.to_string());
        }

        assert!(filter.should_exclude("README.md"));
        assert!(filter.should_exclude("notes.txt"));
        assert!(filter.should_exclude("data.csv"));
        assert!(!filter.should_exclude("src/main.rs"));
        assert!(!filter.should_exclude("config.yaml"));
    }

    #[test]
    fn test_combined_filters_union() {
        let mut filter = SearchFilterOptions {
            exclude_document_files: true,
            exclude_globs: vec!["docs/**".to_string()],
            ..Default::default()
        };
        for ext in DEFAULT_DOCUMENT_EXTENSIONS {
            filter.exclude_extensions.insert(ext.to_string());
        }
        filter.exclude_extensions.insert(".rs".to_string());
        filter.compile_globs().unwrap();

        // Excluded by extension (.md from document files)
        assert!(filter.should_exclude("README.md"));
        // Excluded by extension (.rs from exclude_extensions)
        assert!(filter.should_exclude("src/main.rs"));
        // Excluded by glob (docs/**)
        assert!(filter.should_exclude("docs/config.yaml"));
        // Not excluded
        assert!(!filter.should_exclude("config/app.yaml"));
    }

    #[test]
    fn test_filter_is_active() {
        let filter1 = SearchFilterOptions {
            exclude_document_files: true,
            ..Default::default()
        };
        assert!(filter1.is_active());

        let mut filter2 = SearchFilterOptions::default();
        filter2.exclude_extensions.insert(".md".to_string());
        assert!(filter2.is_active());

        let filter3 = SearchFilterOptions {
            exclude_globs: vec!["docs/**".to_string()],
            ..Default::default()
        };
        assert!(filter3.is_active());

        let filter4 = SearchFilterOptions::default();
        assert!(!filter4.is_active());
    }
}
```

- [ ] **Step 6: 运行测试验证通过**

Run: `cargo test search_filter --no-fail-fast`
Expected: 所有测试通过

- [ ] **Step 7: 在 lib.rs 中导出模块**

在 `src/lib.rs` 中添加模块导出：

```rust
pub mod search_filter;
```

- [ ] **Step 8: 验证模块可导入**

Run: `cargo check`
Expected: 编译成功

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml src/search_filter.rs src/lib.rs
git commit -m "feat: add SearchFilterOptions model for dynamic document exclusion"
```

---

## Chunk 2: MCP 工具参数扩展

### Task 3: 扩展 SearchContextArgs 参数

**Files:**
- Modify: `src/tools/search_context.rs:51-87`

- [ ] **Step 10: 编写参数序列化测试**

在 `tests/tools_test.rs` 中添加：

```rust
#[test]
fn test_search_context_args_with_filters() {
    let args = SearchContextArgs {
        project_root_path: Some("/path/to/project".to_string()),
        query: Some("find code".to_string()),
        exclude_document_files: Some(true),
        exclude_extensions: Some(vec![".md".to_string(), ".txt".to_string()]),
        exclude_globs: Some(vec!["docs/**".to_string()]),
    };

    let json = serde_json::to_string(&args).unwrap();
    assert!(json.contains("exclude_document_files"));
    assert!(json.contains("exclude_extensions"));
    assert!(json.contains("exclude_globs"));

    let deserialized: SearchContextArgs = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.exclude_document_files, Some(true));
    assert_eq!(deserialized.exclude_extensions, Some(vec![".md".to_string(), ".txt".to_string()]));
    assert_eq!(deserialized.exclude_globs, Some(vec!["docs/**".to_string()]));
}

#[test]
fn test_search_context_args_backward_compatible() {
    // Old format without new fields should still work
    let json = r#"{"project_root_path":"/path","query":"test"}"#;
    let args: SearchContextArgs = serde_json::from_str(json).unwrap();
    assert_eq!(args.project_root_path, Some("/path".to_string()));
    assert_eq!(args.query, Some("test".to_string()));
    assert!(args.exclude_document_files.is_none());
    assert!(args.exclude_extensions.is_none());
    assert!(args.exclude_globs.is_none());
}
```

- [ ] **Step 11: 运行测试验证失败**

Run: `cargo test test_search_context_args_with_filters --no-run`
Expected: 编译错误，字段未定义

- [ ] **Step 12: 扩展 SearchContextArgs 结构体**

修改 `src/tools/search_context.rs` 中的 `SearchContextArgs`：

```rust
/// Tool arguments
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchContextArgs {
    pub project_root_path: Option<String>,
    pub query: Option<String>,
    /// Whether to exclude document files (md, txt, csv, etc.) from search
    pub exclude_document_files: Option<bool>,
    /// File extensions to exclude (e.g., [".md", ".txt"])
    pub exclude_extensions: Option<Vec<String>>,
    /// Glob patterns to exclude (e.g., ["docs/**", "**/README*"])
    pub exclude_globs: Option<Vec<String>>,
}
```

- [ ] **Step 13: 运行测试验证通过**

Run: `cargo test test_search_context_args`
Expected: 所有测试通过

- [ ] **Step 14: Commit**

```bash
git add src/tools/search_context.rs tests/tools_test.rs
git commit -m "feat: add filter parameters to SearchContextArgs"
```

---

### Task 4: 扩展 MCP Input Schema

**Files:**
- Modify: `src/tools/search_context.rs:51-79`

- [ ] **Step 15: 编写 schema 测试**

在 `tests/tools_test.rs` 中添加：

```rust
#[test]
fn test_get_input_schema_includes_filters() {
    let schema = SearchContextToolDef::get_input_schema();

    // Check new fields exist in schema
    assert!(schema["properties"]["exclude_document_files"].is_object());
    assert_eq!(schema["properties"]["exclude_document_files"]["type"], "boolean");

    assert!(schema["properties"]["exclude_extensions"].is_object());
    assert_eq!(schema["properties"]["exclude_extensions"]["type"], "array");

    assert!(schema["properties"]["exclude_globs"].is_object());
    assert_eq!(schema["properties"]["exclude_globs"]["type"], "array");

    // New fields should NOT be required (backward compatible)
    let required = schema["required"].as_array().unwrap();
    assert!(!required.iter().any(|r| r == "exclude_document_files"));
    assert!(!required.iter().any(|r| r == "exclude_extensions"));
    assert!(!required.iter().any(|r| r == "exclude_globs"));
}
```

- [ ] **Step 16: 运行测试验证失败**

Run: `cargo test test_get_input_schema_includes_filters --no-run`
Expected: 测试失败，字段不存在

- [ ] **Step 17: 扩展 get_input_schema 方法**

修改 `src/tools/search_context.rs` 中的 `get_input_schema`：

```rust
impl SearchContextToolDef {
    pub fn get_input_schema() -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "project_root_path": {
                    "type": "string",
                    "description": "Absolute path to the project root directory. Use forward slashes (/) as separators. Example: /Users/username/projects/myproject or C:/Users/username/projects/myproject"
                },
                "query": {
                    "type": "string",
                    "description": r#"Natural language description of the code you are looking for.

Provide a clear description of the code behavior, workflow, or issue you want to locate. You may also add optional keywords to improve semantic matching.

Recommended format: Natural language description + optional keywords

Examples:
- "I want to find where the server handles chunk merging in the file upload process. Keywords: upload chunk merge, file service"
- "Locate where the system refreshes cached data after user permissions are updated. Keywords: permission update, cache refresh"
- "Find the initialization flow of message queue consumers during startup. Keywords: mq consumer init, subscribe"
- "Show me how configuration hot-reload is triggered and applied in the code. Keywords: config reload, hot update"
- "Where is the function that handles user authentication?"
- "What tests are there for the login functionality?"
- "How is the database connected to the application?""#
                },
                "exclude_document_files": {
                    "type": "boolean",
                    "description": "If true, exclude document files (.md, .mdx, .txt, .csv, .tsv, .rst, .adoc, .tex, .org) from search. Useful when you want to focus on source code only."
                },
                "exclude_extensions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "File extensions to exclude from search. Include the leading dot, e.g., [\".md\", \".txt\"]. Case-insensitive matching."
                },
                "exclude_globs": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Glob patterns to exclude from search. Examples: [\"docs/**\", \"**/README*\", \"test/**\"]. Uses standard glob syntax."
                }
            },
            "required": ["project_root_path", "query"]
        })
    }
}
```

- [ ] **Step 18: 运行测试验证通过**

Run: `cargo test test_get_input_schema`
Expected: 所有测试通过

- [ ] **Step 19: Commit**

```bash
git add src/tools/search_context.rs tests/tools_test.rs
git commit -m "feat: extend MCP input schema with filter parameters"
```

---

## Chunk 3: 索引层过滤集成

### Task 5: 修改 IndexManager::search_context 方法

**Files:**
- Modify: `src/index/manager.rs:1247-1394`

- [ ] **Step 20: 编写 blob 过滤测试**

在 `tests/index_test.rs` 中添加：

```rust
use ace_tool::search_filter::SearchFilterOptions;

#[test]
fn test_filter_blob_hashes_by_extension() {
    let mut entries = HashMap::new();
    entries.insert(
        "src/main.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash_rs".to_string()],
        },
    );
    entries.insert(
        "README.md".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash_md".to_string()],
        },
    );
    entries.insert(
        "notes.txt".to_string(),
        FileEntry {
            mtime_secs: 3000,
            mtime_nanos: 0,
            size: 300,
            blob_hashes: vec!["hash_txt".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude .md and .txt
    let mut filter = SearchFilterOptions::default();
    filter.exclude_extensions.insert(".md".to_string());
    filter.exclude_extensions.insert(".txt".to_string());

    let filtered = index.get_filtered_blob_hashes(&filter);
    assert_eq!(filtered.len(), 1);
    assert!(filtered.contains(&"hash_rs".to_string()));
    assert!(!filtered.contains(&"hash_md".to_string()));
    assert!(!filtered.contains(&"hash_txt".to_string()));
}

#[test]
fn test_filter_blob_hashes_by_glob() {
    let mut entries = HashMap::new();
    entries.insert(
        "src/main.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash_src".to_string()],
        },
    );
    entries.insert(
        "docs/guide.md".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash_docs".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude docs/**
    let mut filter = SearchFilterOptions {
        exclude_globs: vec!["docs/**".to_string()],
        ..Default::default()
    };
    filter.compile_globs().unwrap();

    let filtered = index.get_filtered_blob_hashes(&filter);
    assert_eq!(filtered.len(), 1);
    assert!(filtered.contains(&"hash_src".to_string()));
}

#[test]
fn test_filter_blob_hashes_empty_result() {
    let mut entries = HashMap::new();
    entries.insert(
        "README.md".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash_md".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude .md
    let mut filter = SearchFilterOptions::default();
    filter.exclude_extensions.insert(".md".to_string());

    let filtered = index.get_filtered_blob_hashes(&filter);
    assert!(filtered.is_empty());
}

#[test]
fn test_filter_blob_hashes_no_filter() {
    let mut entries = HashMap::new();
    entries.insert(
        "src/main.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    entries.insert(
        "README.md".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // No filter
    let filter = SearchFilterOptions::default();
    let all_hashes = index.get_filtered_blob_hashes(&filter);
    assert_eq!(all_hashes.len(), 2);
}

#[test]
fn test_filter_blob_hashes_chunked_files() {
    let mut entries = HashMap::new();
    entries.insert(
        "large_file.md".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 10000,
            blob_hashes: vec![
                "chunk1_hash".to_string(),
                "chunk2_hash".to_string(),
                "chunk3_hash".to_string(),
            ],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude .md
    let mut filter = SearchFilterOptions::default();
    filter.exclude_extensions.insert(".md".to_string());

    let filtered = index.get_filtered_blob_hashes(&filter);
    // All chunks should be excluded
    assert!(filtered.is_empty());
}
```

- [ ] **Step 21: 运行测试验证失败**

Run: `cargo test test_filter_blob --no-run`
Expected: 编译错误，方法未定义

- [ ] **Step 22: 为 IndexData 添加 get_filtered_blob_hashes 方法**

在 `src/index/manager.rs` 的 `IndexData` impl 块中添加：

```rust
use crate::search_filter::SearchFilterOptions;

impl IndexData {
    /// Get all blob hashes from all entries
    pub fn get_all_blob_hashes(&self) -> Vec<String> {
        self.entries
            .values()
            .flat_map(|e| e.blob_hashes.iter().cloned())
            .collect()
    }

    /// Get blob hashes from entries that pass the filter
    pub fn get_filtered_blob_hashes(&self, filter: &SearchFilterOptions) -> Vec<String> {
        self.entries
            .iter()
            .filter(|(rel_path, _)| !filter.should_exclude(rel_path))
            .flat_map(|(_, entry)| entry.blob_hashes.iter().cloned())
            .collect()
    }
}
```

- [ ] **Step 23: 运行测试验证通过**

Run: `cargo test test_filter_blob`
Expected: 所有测试通过

- [ ] **Step 24: Commit**

```bash
git add src/index/manager.rs tests/index_test.rs
git commit -m "feat: add get_filtered_blob_hashes method to IndexData"
```

---

### Task 6: 修改 search_context 方法签名和实现

**Files:**
- Modify: `src/index/manager.rs:1247-1394`
- Modify: `src/tools/search_context.rs:106-165`

- [ ] **Step 25: 修改 search_context 方法签名**

在 `src/index/manager.rs` 中修改 `search_context` 方法签名：

```rust
/// Execute a search with optional filtering
pub async fn search_context(
    &self,
    query: &str,
    filters: &SearchFilterOptions,
) -> Result<String> {
    info!("Starting search: {}", query);

    // Auto-index first
    let index_result = self.index_project().await;
    if index_result.status == "error" {
        return Err(anyhow!("Failed to index project: {}", index_result.message));
    }
    if index_result.status == "partial" {
        warn!(
            "Indexing completed with some failures: {}",
            index_result.message
        );
    }

    // Load index
    let index_data = self.load_index();

    // Apply filters and get blob hashes
    let blob_names = if filters.is_active() {
        let mut filter = filters.clone();
        filter.compile_globs()
            .map_err(|e| anyhow!("Failed to compile glob patterns: {}", e))?;
        index_data.get_filtered_blob_hashes(&filter)
    } else {
        index_data.get_all_blob_hashes()
    };

    if blob_names.is_empty() {
        return Err(anyhow!("No blobs found after filtering. Try adjusting your filter criteria."));
    }

    // Execute search
    info!("Searching {} chunks...", blob_names.len());

    // ... rest of the method unchanged
```

- [ ] **Step 26: 修改 SearchContextTool::execute 方法**

在 `src/tools/search_context.rs` 中修改 `execute` 方法：

```rust
use crate::search_filter::SearchFilterOptions;

/// Execute the tool
pub async fn execute(&self, args: SearchContextArgs) -> ToolResult {
    let query = match &args.query {
        Some(q) if !q.is_empty() => q.clone(),
        _ => {
            return ToolResult {
                text: "Error: query is required".to_string(),
            };
        }
    };

    let project_root_path = match &args.project_root_path {
        Some(p) if !p.is_empty() => p.clone(),
        _ => {
            return ToolResult {
                text: "Error: project_root_path is required".to_string(),
            };
        }
    };

    // Normalize path (use forward slashes)
    let project_root = project_root_path.replace('\\', "/");
    let project_path = PathBuf::from(&project_root);

    // Validate path exists
    if !project_path.exists() {
        return ToolResult {
            text: format!("Error: Project path does not exist: {}", project_root),
        };
    }

    // Validate is directory
    if !project_path.is_dir() {
        return ToolResult {
            text: format!("Error: Project path is not a directory: {}", project_root),
        };
    }

    info!("Executing search_context for: {}", project_root);

    // Build filter options from args
    let mut filters = SearchFilterOptions::from_args(&args);
    if let Err(e) = filters.compile_globs() {
        return ToolResult {
            text: format!("Error: Invalid glob pattern: {}", e),
        };
    }

    // Create index manager and execute search
    let manager = match IndexManager::new(self.config.clone(), project_path) {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create IndexManager: {}", e);
            return ToolResult {
                text: format!("Error: {}", e),
            };
        }
    };

    match manager.search_context(&query, &filters).await {
        Ok(result) => ToolResult { text: result },
        Err(e) => {
            error!("Search failed: {}", e);
            ToolResult {
                text: format!("Error: {}", e),
            }
        }
    }
}
```

- [ ] **Step 27: 验证编译通过**

Run: `cargo check`
Expected: 编译成功

- [ ] **Step 28: Commit**

```bash
git add src/index/manager.rs src/tools/search_context.rs
git commit -m "feat: integrate SearchFilterOptions into search_context flow"
```

---

## Chunk 4: 完整测试与验证

### Task 7: 添加边界条件测试

**Files:**
- Modify: `tests/index_test.rs`
- Modify: `tests/tools_test.rs`

- [ ] **Step 29: 添加大小写兼容性测试**

在 `tests/index_test.rs` 中添加：

```rust
#[test]
fn test_filter_case_insensitive_extension() {
    let mut entries = HashMap::new();
    entries.insert(
        "README.MD".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash_upper".to_string()],
        },
    );
    entries.insert(
        "notes.TxT".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash_mixed".to_string()],
        },
    );
    entries.insert(
        "guide.md".to_string(),
        FileEntry {
            mtime_secs: 3000,
            mtime_nanos: 0,
            size: 300,
            blob_hashes: vec!["hash_lower".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude .md (lowercase input)
    let mut filter = SearchFilterOptions::default();
    filter.exclude_extensions.insert(".md".to_string());
    filter.exclude_extensions.insert(".txt".to_string());

    let filtered = index.get_filtered_blob_hashes(&filter);
    // All three should be excluded due to case-insensitive matching
    assert!(filtered.is_empty());
}
```

- [ ] **Step 30: 添加组合过滤 Union 测试**

在 `tests/index_test.rs` 中添加：

```rust
#[test]
fn test_filter_union_semantics() {
    let mut entries = HashMap::new();
    entries.insert(
        "src/main.rs".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash_rs".to_string()],
        },
    );
    entries.insert(
        "README.md".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash_md".to_string()],
        },
    );
    entries.insert(
        "docs/guide.yaml".to_string(),
        FileEntry {
            mtime_secs: 3000,
            mtime_nanos: 0,
            size: 300,
            blob_hashes: vec!["hash_docs_yaml".to_string()],
        },
    );
    entries.insert(
        "config/app.yaml".to_string(),
        FileEntry {
            mtime_secs: 4000,
            mtime_nanos: 0,
            size: 400,
            blob_hashes: vec!["hash_config_yaml".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude .md + docs/**
    let mut filter = SearchFilterOptions {
        exclude_globs: vec!["docs/**".to_string()],
        ..Default::default()
    };
    filter.exclude_extensions.insert(".md".to_string());
    filter.compile_globs().unwrap();

    let filtered = index.get_filtered_blob_hashes(&filter);
    // Should exclude:
    // - README.md (by extension)
    // - docs/guide.yaml (by glob)
    // Should include:
    // - src/main.rs
    // - config/app.yaml
    assert_eq!(filtered.len(), 2);
    assert!(filtered.contains(&"hash_rs".to_string()));
    assert!(filtered.contains(&"hash_config_yaml".to_string()));
}
```

- [ ] **Step 31: 添加过度过滤安全测试**

在 `tests/index_test.rs` 中添加：

```rust
#[test]
fn test_filter_all_excluded_returns_empty_gracefully() {
    let mut entries = HashMap::new();
    entries.insert(
        "README.md".to_string(),
        FileEntry {
            mtime_secs: 1000,
            mtime_nanos: 0,
            size: 100,
            blob_hashes: vec!["hash1".to_string()],
        },
    );
    entries.insert(
        "docs/guide.md".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash2".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // Filter: exclude all
    let mut filter = SearchFilterOptions {
        exclude_document_files: true,
        exclude_globs: vec!["docs/**".to_string()],
        ..Default::default()
    };
    for ext in ace_tool::search_filter::DEFAULT_DOCUMENT_EXTENSIONS {
        filter.exclude_extensions.insert(ext.to_string());
    }
    filter.compile_globs().unwrap();

    let filtered = index.get_filtered_blob_hashes(&filter);
    // Should return empty gracefully, not panic
    assert!(filtered.is_empty());
}
```

- [ ] **Step 32: 运行所有测试**

Run: `cargo test --no-fail-fast`
Expected: 所有测试通过

- [ ] **Step 33: Commit**

```bash
git add tests/index_test.rs tests/tools_test.rs
git commit -m "test: add edge case tests for search filtering"
```

---

### Task 8: 最终验证

- [ ] **Step 34: 运行完整测试套件**

Run: `cargo test --all`
Expected: 所有测试通过

- [ ] **Step 35: 运行 clippy 检查**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: 无警告

- [ ] **Step 36: 运行格式化检查**

Run: `cargo fmt --check`
Expected: 无差异

- [ ] **Step 37: 最终 Commit**

```bash
git add -A
git commit -m "feat: complete search_context dynamic document exclusion implementation

- Add SearchFilterOptions model for filtering blob entries
- Support exclude_document_files, exclude_extensions, exclude_globs
- Implement case-insensitive extension matching
- Use globset for efficient glob pattern matching
- Add comprehensive test coverage for all filter scenarios"
```

---

## 验收标准

- [ ] `exclude_document_files = true` 时，`.md`、`.txt` 等文档文件不参与搜索
- [ ] `exclude_extensions = [".md"]` 时，只排除 `.md` 文件
- [ ] `exclude_globs = ["docs/**"]` 时，`docs/` 目录下的所有文件被排除
- [ ] 三者组合时按 Union 语义生效
- [ ] 扩展名匹配大小写不敏感
- [ ] 不传过滤参数时行为与修改前完全一致
- [ ] 过滤后无候选 blob 时返回友好错误信息，不 panic
- [ ] 所有测试通过
- [ ] 无 clippy 警告
