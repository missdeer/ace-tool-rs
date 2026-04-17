# Search Context 过滤功能修复实现计划

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复空结果语义不一致问题和补充关键回归测试，确保过滤功能完全符合设计要求。

**Architecture:**
- Task 1 修复 `search_context` 空结果分支，区分"索引为空"和"过滤后为空"两种场景
- Task 2-4 补充三个关键回归测试，覆盖 `from_args()` 默认注入、非法 glob 处理、无参数旧行为

**Tech Stack:** Rust, anyhow (错误处理), globset (glob 编译)

---

## 文件结构

| 文件 | 职责 | 修改类型 |
|------|------|----------|
| `src/index/manager.rs:1291-1295` | 空结果分支逻辑 | **修改** |
| `src/search_filter.rs` | 过滤选项模型 | 无需修改 |
| `tests/index_test.rs` | 索引层测试 | **新增测试** |
| `tests/tools_test.rs` | 工具层测试 | **新增测试** |

---

## Chunk 1: 修复空结果语义

### Task 1: 区分"索引为空"与"过滤后为空"

**Files:**
- Modify: `src/index/manager.rs:1291-1295`

**背景：** 当前实现无论是否启用过滤，空结果都返回 "No blobs found after filtering"。设计要求：
- 未启用过滤时应保持旧语义 "No blobs found after indexing"
- 启用过滤且全被排空时，应返回明确的"过滤后无候选"

- [ ] **Step 1: 查看当前代码**

当前代码位置 `src/index/manager.rs:1280-1295`：

```rust
// Apply filters and get blob hashes
let blob_names = if filters.is_active() {
    let mut filter = filters.clone();
    filter
        .compile_globs()
        .map_err(|e| anyhow!("Failed to compile glob patterns: {}", e))?;
    index_data.get_filtered_blob_hashes(&filter)
} else {
    index_data.get_all_blob_hashes()
};

if blob_names.is_empty() {
    return Err(anyhow!(
        "No blobs found after filtering. Try adjusting your filter criteria."
    ));
}
```

- [ ] **Step 2: 修改空结果分支逻辑**

将 `src/index/manager.rs:1291-1295` 替换为：

```rust
if blob_names.is_empty() {
    if filters.is_active() {
        return Err(anyhow!(
            "No blobs found after filtering. Try adjusting your filter criteria."
        ));
    } else {
        return Err(anyhow!("No blobs found after indexing"));
    }
}
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo check 2>&1 | head -20`
Expected: 无编译错误

- [ ] **Step 4: 提交**

```bash
git add src/index/manager.rs
git commit -m "fix: distinguish empty index vs empty filter result in search_context"
```

---

## Chunk 2: 补充回归测试

### Task 2: 测试 from_args() 默认文档扩展名注入

**Files:**
- Modify: `tests/tools_test.rs`

**目标：** 验证 `exclude_document_files=true` 时，默认文档扩展名（.md, .txt 等）被正确注入到 `exclude_extensions` 集合。

- [ ] **Step 1: 写失败测试**

在 `tests/tools_test.rs` 末尾添加：

```rust
#[test]
fn test_from_args_injects_default_document_extensions() {
    use ace_tool_rs::search_filter::SearchFilterOptions;
    use ace_tool_rs::tools::search_context::SearchContextArgs;

    let args = SearchContextArgs {
        project_root_path: Some("/path".to_string()),
        query: Some("test".to_string()),
        exclude_document_files: Some(true),
        exclude_extensions: None,
        exclude_globs: None,
    };

    let filter = SearchFilterOptions::from_args(&args);

    // 验证默认文档扩展名被注入
    assert!(filter.exclude_extensions.contains(".md"));
    assert!(filter.exclude_extensions.contains(".mdx"));
    assert!(filter.exclude_extensions.contains(".txt"));
    assert!(filter.exclude_extensions.contains(".csv"));
    assert!(filter.exclude_extensions.contains(".tsv"));
    assert!(filter.exclude_extensions.contains(".rst"));
    assert!(filter.exclude_extensions.contains(".adoc"));
    assert!(filter.exclude_extensions.contains(".tex"));
    assert!(filter.exclude_extensions.contains(".org"));

    // 验证 is_active() 返回 true
    assert!(filter.is_active());
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test test_from_args_injects_default_document_extensions --test tools_test 2>&1 | tail -20`
Expected: `test test_from_args_injects_default_document_extensions ... ok`

- [ ] **Step 3: 提交**

```bash
git add tests/tools_test.rs
git commit -m "test: verify default document extensions injection in from_args"
```

---

### Task 3: 测试非法 glob 在工具层的错误处理

**Files:**
- Modify: `tests/tools_test.rs`

**目标：** 验证非法 glob 模式（如 `[`）在工具层被正确捕获并返回友好错误消息。

- [ ] **Step 1: 写失败测试**

在 `tests/tools_test.rs` 末尾添加：

```rust
#[test]
fn test_invalid_glob_pattern_error_handling() {
    use ace_tool_rs::search_filter::SearchFilterOptions;
    use ace_tool_rs::tools::search_context::SearchContextArgs;

    let args = SearchContextArgs {
        project_root_path: Some("/path".to_string()),
        query: Some("test".to_string()),
        exclude_document_files: None,
        exclude_extensions: None,
        exclude_globs: Some(vec!["[".to_string()]), // 无效 glob 模式
    };

    let mut filter = SearchFilterOptions::from_args(&args);
    let result = filter.compile_globs();

    // 验证编译失败
    assert!(result.is_err());

    // 验证错误消息包含模式信息
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("["));
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test test_invalid_glob_pattern_error_handling --test tools_test 2>&1 | tail -20`
Expected: `test test_invalid_glob_pattern_error_handling ... ok`

- [ ] **Step 3: 提交**

```bash
git add tests/tools_test.rs
git commit -m "test: verify invalid glob pattern error handling"
```

---

### Task 4: 测试无过滤参数时保持旧行为

**Files:**
- Modify: `tests/index_test.rs`

**目标：** 验证不传任何过滤参数时，`is_active()` 返回 false，且 `get_all_blob_hashes()` 被调用（非 `get_filtered_blob_hashes()`）。

- [ ] **Step 1: 写失败测试**

在 `tests/index_test.rs` 末尾添加：

```rust
#[test]
fn test_no_filter_params_returns_all_blobs() {
    use ace_tool_rs::search_filter::SearchFilterOptions;

    // 创建测试索引数据
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
        "docs/guide.txt".to_string(),
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

    // 无过滤参数
    let filter = SearchFilterOptions::default();

    // 验证 is_active() 返回 false
    assert!(!filter.is_active());

    // 验证 get_all_blob_hashes() 返回全部 blob
    let all_blobs = index.get_all_blob_hashes();
    assert_eq!(all_blobs.len(), 3);

    // 验证 get_filtered_blob_hashes() 对默认 filter 也返回全部
    let filtered_blobs = index.get_filtered_blob_hashes(&filter);
    assert_eq!(filtered_blobs.len(), 3);

    // 两者结果一致（无过滤时行为相同）
    let mut all_sorted = all_blobs;
    all_sorted.sort();
    let mut filtered_sorted = filtered_blobs;
    filtered_sorted.sort();
    assert_eq!(all_sorted, filtered_sorted);
}
```

- [ ] **Step 2: 运行测试验证通过**

Run: `cargo test test_no_filter_params_returns_all_blobs --test index_test 2>&1 | tail -20`
Expected: `test test_no_filter_params_returns_all_blobs ... ok`

- [ ] **Step 3: 提交**

```bash
git add tests/index_test.rs
git commit -m "test: verify no filter params returns all blobs (backward compatible)"
```

---

## Chunk 3: 最终验证

### Task 5: 运行全部测试确保无回归

- [ ] **Step 1: 运行全部测试**

Run: `cargo test --test index_test --test tools_test 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 2: 最终验证**

确认以下行为正确：
1. 索引为空且无过滤 → "No blobs found after indexing"
2. 过滤后为空 → "No blobs found after filtering..."
3. `from_args()` 正确注入默认文档扩展名
4. 非法 glob 返回友好错误
5. 无参数时返回全部 blob

---

## 验收标准

- [ ] 所有测试通过
- [ ] 空结果错误消息区分两种场景
- [ ] 三个回归测试覆盖关键入口行为
- [ ] 无破坏性变更，向后兼容