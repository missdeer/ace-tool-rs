# Search Context 过滤功能语义完善计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完善过滤功能语义，包括无扩展名文档文件排除、空结果语义优化、契约层测试补充。

**Architecture:**
- Task 1-2 解决无扩展名文档文件（如 README、CHANGELOG）的排除问题
- Task 3-4 优化过滤后空结果的语义，从 Error 改为返回空结果
- Task 5-6 补充契约层端到端测试

**Tech Stack:** Rust, anyhow (错误处理), globset (glob 编译)

---

## 文件结构

| 文件 | 职责 | 修改类型 |
|------|------|----------|
| `src/search_filter.rs` | 过滤选项模型，添加无扩展名文件名检查 | **修改** |
| `tests/tools_test.rs` | 工具层契约测试 | **新增测试** |
| `tests/index_test.rs` | 过滤逻辑测试 | **新增测试** |

---

## Chunk 1: 无扩展名文档文件排除

### Task 1: 添加无扩展名文档文件名排除逻辑

**Files:**
- Modify: `src/search_filter.rs`

**背景：** 当前 `exclude_document_files=true` 只排除有扩展名的文档文件（.md, .txt 等），但 `README`、`CHANGELOG`、`TODO` 等无扩展名特殊文件名无法被排除。

**目标：** 扩展过滤逻辑，使其能够排除无扩展名的文档文件名。

- [ ] **Step 1: 在 src/search_filter.rs 中添加默认无扩展名文档文件名常量**

在 `DEFAULT_DOCUMENT_EXTENSIONS` 常量后添加：

```rust
/// Default document filenames (without extension) to exclude when `exclude_document_files` is true
pub const DEFAULT_DOCUMENT_FILENAMES: &[&str] = &[
    "README", "CHANGELOG", "TODO", "ROADMAP",
    "LICENSE", "LICENCE", "AUTHORS", "CONTRIBUTORS",
    "HISTORY", "COPYING", "NEWS", "CHANGES",
];
```

- [ ] **Step 2: 修改 SearchFilterOptions 结构体**

在 `SearchFilterOptions` 结构体中添加新字段：

```rust
/// Search filter options for excluding entries from search results
#[derive(Debug, Clone, Default)]
pub struct SearchFilterOptions {
    /// Whether to exclude document files (md, txt, etc.)
    pub exclude_document_files: bool,
    /// Extensions to exclude (normalized to lowercase with leading dot)
    pub exclude_extensions: HashSet<String>,
    /// Filenames without extension to exclude (e.g., README, CHANGELOG)
    pub exclude_filenames: HashSet<String>,
    /// Glob patterns to exclude
    pub exclude_globs: Vec<String>,
    /// Compiled glob matcher (lazy initialization)
    compiled_globset: Option<globset::GlobSet>,
}
```

- [ ] **Step 3: 修改 from_args() 方法**

修改 `from_args()` 方法，在 `exclude_document_files=true` 时注入默认文件名：

```rust
impl SearchFilterOptions {
    /// Create filter options from MCP tool arguments
    pub fn from_args(args: &crate::tools::search_context::SearchContextArgs) -> Self {
        let exclude_document_files = args.exclude_document_files.unwrap_or(false);

        let mut exclude_extensions = HashSet::new();
        let mut exclude_filenames = HashSet::new();

        // Handle exclude_extensions - normalize to lowercase with leading dot
        if let Some(ref exts) = args.exclude_extensions {
            for ext in exts {
                let normalized = normalize_extension(ext);
                if !normalized.is_empty() {
                    exclude_extensions.insert(normalized);
                }
            }
        }

        // Add default document extensions and filenames if exclude_document_files is true
        if exclude_document_files {
            for ext in DEFAULT_DOCUMENT_EXTENSIONS {
                exclude_extensions.insert(ext.to_string());
            }
            for name in DEFAULT_DOCUMENT_FILENAMES {
                exclude_filenames.insert(name.to_lowercase());
            }
        }

        Self {
            exclude_document_files,
            exclude_extensions,
            exclude_filenames,
            exclude_globs: args.exclude_globs.clone().unwrap_or_default(),
            compiled_globset: None,
        }
    }
```

- [ ] **Step 4: 添加 get_filename 辅助函数**

在 `get_extension` 函数后添加：

```rust
/// Extract filename (without extension) from path
fn get_filename(path: &str) -> Option<String> {
    let path_lower = path.to_lowercase();
    // Get the last component of the path
    let filename = path_lower.rsplit('/').next()?;
    // If it has an extension, remove it
    if let Some(dot_idx) = filename.rfind('.') {
        // Make sure it's not a hidden file like ".gitignore"
        if dot_idx > 0 {
            return Some(filename[..dot_idx].to_string());
        }
    }
    Some(filename.to_string())
}
```

- [ ] **Step 5: 修改 should_exclude() 方法**

修改 `should_exclude()` 方法，添加文件名检查：

```rust
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

    // Check filename exclusion (for files without extension like README)
    if !self.exclude_filenames.is_empty() {
        if let Some(filename) = get_filename(rel_path) {
            if self.exclude_filenames.contains(&filename) {
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
```

- [ ] **Step 6: 验证编译通过**

Run: `cargo check 2>&1 | head -30`
Expected: 无编译错误

- [ ] **Step 7: 提交**

```bash
git add src/search_filter.rs
git commit -m "feat: add filename-based exclusion for files like README, CHANGELOG"
```

---

### Task 2: 添加无扩展名文档文件排除测试

**Files:**
- Modify: `src/search_filter.rs` (内联测试模块)

- [ ] **Step 1: 在 src/search_filter.rs 测试模块末尾添加测试**

```rust
#[test]
fn test_get_filename() {
    assert_eq!(get_filename("README"), Some("readme".to_string()));
    assert_eq!(get_filename("src/README"), Some("readme".to_string()));
    assert_eq!(get_filename("docs/CHANGELOG"), Some("changelog".to_string()));
    assert_eq!(get_filename("README.md"), Some("readme".to_string()));
    assert_eq!(get_filename("src/main.rs"), Some("main".to_string()));
    assert_eq!(get_filename(".gitignore"), Some(".gitignore".to_string()));
}

#[test]
fn test_exclude_document_filenames() {
    let mut filter = SearchFilterOptions {
        exclude_document_files: true,
        ..Default::default()
    };
    // Manually populate for this test
    for name in DEFAULT_DOCUMENT_FILENAMES {
        filter.exclude_filenames.insert(name.to_lowercase());
    }

    // 无扩展名文档文件应该被排除
    assert!(filter.should_exclude("README"));
    assert!(filter.should_exclude("docs/README"));
    assert!(filter.should_exclude("CHANGELOG"));
    assert!(filter.should_exclude("TODO"));
    assert!(filter.should_exclude("ROADMAP"));

    // 有扩展名的文档文件也应该被排除（通过扩展名）
    for ext in DEFAULT_DOCUMENT_EXTENSIONS {
        filter.exclude_extensions.insert(ext.to_string());
    }
    assert!(filter.should_exclude("README.md"));
    assert!(filter.should_exclude("docs/guide.txt"));

    // 普通源码文件不应被排除
    assert!(!filter.should_exclude("src/main.rs"));
    assert!(!filter.should_exclude("lib/controller.py"));
}

#[test]
fn test_from_args_populates_filenames() {
    use crate::tools::search_context::SearchContextArgs;

    let args = SearchContextArgs {
        project_root_path: Some("/path".to_string()),
        query: Some("test".to_string()),
        exclude_document_files: Some(true),
        exclude_extensions: None,
        exclude_globs: None,
    };

    let filter = SearchFilterOptions::from_args(&args);

    // 验证默认文件名被注入
    assert!(filter.exclude_filenames.contains("readme"));
    assert!(filter.exclude_filenames.contains("changelog"));
    assert!(filter.exclude_filenames.contains("todo"));
    assert!(filter.exclude_filenames.contains("roadmap"));
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test --lib 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add src/search_filter.rs
git commit -m "test: add tests for filename-based document exclusion"
```

---

## Chunk 2: 空结果语义优化

### Task 3: 修改过滤后空结果语义

**Files:**
- Modify: `src/index/manager.rs:1291-1299`

**背景：** 设计文档要求"过滤规则把全部条目都排空时，优雅返回空候选或空结果，不能 panic"。当前实现返回 Error，调用方可能误判为"工具执行失败"。

**目标：** 将"过滤后全空"从 Error 改为返回空结果（让检索服务处理空 blob 列表）。

- [ ] **Step 1: 修改空结果分支逻辑**

将 `src/index/manager.rs:1291-1299` 替换为：

```rust
if blob_names.is_empty() {
    if filters.is_active() {
        // 过滤后为空：返回空结果，而非错误
        info!("All blobs excluded by filter criteria");
        return Ok("No matching files found after applying filter criteria.".to_string());
    } else {
        return Err(anyhow!("No blobs found after indexing"));
    }
}
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo check 2>&1 | head -20`
Expected: 无编译错误

- [ ] **Step 3: 提交**

```bash
git add src/index/manager.rs
git commit -m "fix: return empty result instead of error when filter excludes all blobs"
```

---

### Task 4: 添加空结果语义测试

**Files:**
- Modify: `tests/index_test.rs`

- [ ] **Step 1: 添加空结果测试**

在 `tests/index_test.rs` 末尾添加：

```rust
#[test]
fn test_filter_all_excluded_returns_empty_result() {
    use ace_tool::search_filter::SearchFilterOptions;

    // 创建只有文档文件的索引
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
    entries.insert(
        "CHANGELOG".to_string(),
        FileEntry {
            mtime_secs: 2000,
            mtime_nanos: 0,
            size: 200,
            blob_hashes: vec!["hash_changelog".to_string()],
        },
    );

    let index = IndexData {
        version: 2,
        config_hash: "hash".to_string(),
        entries,
    };

    // 创建排除所有文件的过滤器
    let mut filter = SearchFilterOptions {
        exclude_document_files: true,
        ..Default::default()
    };
    for ext in ace_tool::search_filter::DEFAULT_DOCUMENT_EXTENSIONS {
        filter.exclude_extensions.insert(ext.to_string());
    }
    for name in ace_tool::search_filter::DEFAULT_DOCUMENT_FILENAMES {
        filter.exclude_filenames.insert(name.to_lowercase());
    }

    // 验证过滤后返回空列表（不 panic）
    let filtered = index.get_filtered_blob_hashes(&filter);
    assert!(filtered.is_empty());

    // 验证 is_active() 为 true
    assert!(filter.is_active());
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test test_filter_all_excluded_returns_empty_result --test index_test 2>&1 | tail -20`
Expected: `test test_filter_all_excluded_returns_empty_result ... ok`

- [ ] **Step 3: 提交**

```bash
git add tests/index_test.rs
git commit -m "test: verify filter-all-excluded returns empty result gracefully"
```

---

## Chunk 3: 契约层测试

### Task 5: 添加 execute() 非法 glob 返回测试

**Files:**
- Modify: `tests/tools_test.rs`

- [ ] **Step 1: 添加契约层测试**

在 `tests/tools_test.rs` 末尾添加：

```rust
#[test]
fn test_execute_invalid_glob_returns_error_text() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().to_str().unwrap().to_string();

    let config = Arc::new(Config::default());
    let tool = SearchContextTool::new(config);

    let args = SearchContextArgs {
        project_root_path: Some(project_path),
        query: Some("test query".to_string()),
        exclude_document_files: None,
        exclude_extensions: None,
        exclude_globs: Some(vec!["[".to_string()]), // 无效 glob
    };

    let result = await_test(tool.execute(args));

    // 验证返回的错误文本格式
    assert!(result.text.starts_with("Error:"));
    assert!(result.text.contains("Invalid glob pattern"));
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test test_execute_invalid_glob_returns_error_text --test tools_test 2>&1 | tail -20`
Expected: `test test_execute_invalid_glob_returns_error_text ... ok`

- [ ] **Step 3: 提交**

```bash
git add tests/tools_test.rs
git commit -m "test: verify execute() returns proper error for invalid glob"
```

---

### Task 6: 最终验证

- [ ] **Step 1: 运行全部测试**

Run: `cargo test --test index_test --test tools_test 2>&1 | tail -40`
Expected: 所有测试通过

- [ ] **Step 2: 运行库测试**

Run: `cargo test --lib 2>&1 | tail -30`
Expected: 所有测试通过

---

## 验收标准

- [ ] 无扩展名文档文件（README、CHANGELOG 等）可被 `exclude_document_files=true` 排除
- [ ] 过滤后全空返回空结果而非 Error
- [ ] 契约层测试覆盖 execute() 的错误返回格式
- [ ] 所有测试通过
- [ ] 无破坏性变更，向后兼容