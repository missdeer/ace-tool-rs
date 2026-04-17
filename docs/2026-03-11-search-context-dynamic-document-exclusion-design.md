# SearchContext 动态排除文档类内容设计说明

> 状态：评审通过，待实现
>
> 日期：2026-03-11
>
> 目标：为 `search_context` MCP 工具增加“搜索时动态排除文档类内容”的能力，而不是修改全局默认索引规则。

---

## 1. 背景

当前 `ace-tool-rs` 的 `search_context` 能力已经可用，但在真实检索中存在一个明显问题：

- 源码类结果通常能命中
- `README.md`、`README-zh-CN.md`、`*.txt` 这类文档也会进入候选
- 对“我要找实现代码，不想看文档”的场景，噪声偏高

在前一轮实际检索评测中，这个问题已经重复出现：

- 工具入口类问题：源码能命中，但会混入 README
- transport 协议类问题：源码命中完整，但 README 会抢前排
- 参数约束类问题：README 权重甚至高于源码，影响可直接消费性

因此，需要一种更灵活的机制，让调用方可以在“这一次搜索”里决定：

- 是否排除 `md/txt` 等文档类内容
- 是否排除某些路径模式，比如 `docs/**`
- 是否保留当前默认行为，避免影响已有调用方

---

## 2. 当前实现现状

### 2.1 `search_context` 当前参数能力

`SearchContextArgs` 目前只有两个字段：

- `project_root_path`
- `query`

代码位置：

- `src/tools/search_context.rs:50-79`
- `src/tools/search_context.rs:82-87`

这意味着当前 **不支持查询时传递任何过滤参数**。

### 2.2 当前哪些文件会进入索引

默认可索引文本扩展名中，明确包含：

- `.md`
- `.mdx`
- `.txt`

代码位置：

- `src/config.rs:157-215`
- 关键点：`src/config.rs:209-211`

默认特殊文件名中，也包含：

- `README`
- `CHANGELOG`
- `TODO`
- `ROADMAP`

代码位置：

- `src/config.rs:414-478`
- 关键点：`src/config.rs:464-474`

这说明 Markdown / TXT / README 类内容在当前实现里本来就是“可索引文本”。

### 2.3 当前哪些文件已经默认排除

默认排除模式中已经包含：

- `*.pdf`
- `*.doc`
- `*.docx`
- `*.xls`
- `*.xlsx`

代码位置：

- `src/config.rs:286-410`
- 关键点：`src/config.rs:390-394`

所以：

- Word / PDF 这类二进制文档，当前已经默认排除
- Markdown / TXT 这类纯文本文档，当前不会被默认排除

### 2.4 当前文件过滤发生在哪一层

索引阶段会通过以下链路过滤文件：

```text
collect_file_paths_standalone()
  -> should_exclude_standalone()
  -> is_indexable_file_standalone()
```

代码位置：

- `src/index/manager.rs:1633-1657`
- `src/index/manager.rs:1661-1718`
- `src/index/manager.rs:1720-1738`

这套过滤依赖：

- 默认 `exclude_patterns`
- `.gitignore`
- `.aceignore`

代码位置：

- `src/index/manager.rs:1602-1629`
- `README-zh-CN.md:294-306`
- `README-zh-CN.md:449`

### 2.5 当前搜索请求如何发送

`IndexManager::search_context()` 当前流程是：

```text
1. index_project()
2. load_index()
3. get_all_blob_hashes()
4. 把全部 blob_hashes 发给 /agents/codebase-retrieval
```

代码位置：

- `src/index/manager.rs:1247-1284`
- `IndexData::get_all_blob_hashes()` 在 `src/index/manager.rs:72-79`

当前行为的关键点是：

- 本地索引里保存了 `relative path -> blob_hashes` 的映射
- 但搜索请求发出前，没有按路径、扩展名、模式做二次过滤

---

## 3. 需求描述

目标能力：

1. 调用方可以在 **单次搜索请求** 中动态决定是否排除文档类内容
2. 不能破坏现有默认行为
3. 不能要求调用方必须修改 `.aceignore`
4. 不能把“一次性过滤条件”污染为全局索引规则

希望支持的使用方式示例：

```json
{
  "project_root_path": "/path/to/repo",
  "query": "查找 MCP transport 自动探测实现",
  "exclude_document_files": true
}
```

```json
{
  "project_root_path": "/path/to/repo",
  "query": "查找启动流程",
  "exclude_extensions": [".md", ".txt"],
  "exclude_globs": ["docs/**", "**/README*"]
}
```

---

## 4. 方案对比

### 方案 A：修改全局默认索引规则

做法：

- 直接把 `.md`、`.txt` 从 `default_text_extensions()` 删除
- 或者加到 `default_exclude_patterns()` 里

优点：

- 实现最简单
- 能直接减少索引中的文档噪声

缺点：

- 影响所有用户、所有查询
- 会破坏当前“README/配置文档也可搜索”的既有行为
- 无法按请求动态开关

结论：

- **不推荐**

### 方案 B：查询时把动态过滤条件写进索引流程

做法：

- 在 `search_context` 请求里传过滤参数
- 把参数一路传到 `index_project()` / 文件扫描阶段
- 用不同过滤规则生成不同索引内容

优点：

- 逻辑直观
- 索引阶段就排除了无关文件

缺点：

- 当前索引缓存 `config_hash` 只和 `max_lines_per_blob` 有关
- 代码位置：`src/index/manager.rs:176-182`
- 如果动态过滤影响索引结果，但不进入 `config_hash`，会污染缓存语义
- 如果把动态过滤也纳入 `config_hash`，会导致一次搜索一个索引版本，复杂度明显上升

结论：

- **不推荐作为第一版**

### 方案 C：查询时按索引条目二次过滤 blob

做法：

- 索引保持现状
- `load_index()` 后不直接使用 `get_all_blob_hashes()`
- 改为：
  - 遍历 `IndexData.entries`
  - 根据 rel_path / extension / glob 过滤条目
  - 收集过滤后的 `blob_hashes`
  - 再发送给 `/agents/codebase-retrieval`

优点：

- 真正满足“单次请求动态开关”
- 不污染索引缓存
- 不改变默认索引规则
- 改动边界清晰，兼容性好

缺点：

- 文档文件仍然会被索引，只是在搜索请求阶段不参与召回
- 不能减少本地索引大小，也不能减少上传成本

结论：

- **推荐作为第一版实现**

---

## 5. 推荐方案

推荐采用 **方案 C：查询时按索引条目二次过滤 blob**。

### 5.1 设计原则

1. 默认行为完全保持兼容
2. 过滤参数只作用于本次查询
3. 不改动 `.gitignore` / `.aceignore` 语义
4. 不改变当前索引缓存模型

### 5.2 新增参数设计

建议在 `SearchContextArgs` 中增加以下字段：

```rust
pub struct SearchContextArgs {
    pub project_root_path: Option<String>,
    pub query: Option<String>,
    pub exclude_document_files: Option<bool>,
    pub exclude_extensions: Option<Vec<String>>,
    pub exclude_globs: Option<Vec<String>>,
}
```

推荐语义：

- `exclude_document_files`
  - 快捷开关
  - 为 `true` 时，使用内置文档类型过滤集合
- `exclude_extensions`
  - 精确按扩展名排除，如 `.md`、`.txt`
  - 调用方传参时要求带前导 `.`，实现层会统一做 `trim + to_lowercase()`
- `exclude_globs`
  - 按路径模式排除，如 `docs/**`、`**/README*`

三者关系：

- **取并集（Union）**
- 只要命中任一规则，该条目即不参与本次搜索
- 不存在“后者覆盖前者”的优先级语义

### 5.3 `exclude_document_files` 的默认集合建议

第一版建议只覆盖“常见文档噪声”，不要过度扩大范围：

- `.md`
- `.mdx`
- `.txt`
- `.csv`
- `.tsv`
- `.rst`
- `.adoc`
- `.tex`
- `.org`

说明：

- `*.docx`、`*.pdf` 已经是默认排除项，不必重复处理
- 第一版不建议把 `.json`、`.yaml` 视为“文档”
  - 这些往往是配置或协议文件，代码定位场景下价值很高
- `README.md`、`README-zh-CN.md` 这类文件应被视为文档噪声
  - 当 `exclude_document_files = true` 时，默认应被排除

---

## 6. 具体修改点

### 6.1 MCP 工具参数定义

文件：

- `src/tools/search_context.rs`

改动：

- 扩充 `SearchContextToolDef::get_input_schema()`
- 扩充 `SearchContextArgs`
- 对新增参数做基础归一化

### 6.2 搜索选项模型

建议新增一个内部结构，例如：

```rust
pub struct SearchFilterOptions {
    pub exclude_document_files: bool,
    pub exclude_extensions: HashSet<String>,
    pub exclude_globs: Vec<String>,
    pub compiled_exclude_globs: Option<GlobSet>,
}
```

用途：

- 避免把 MCP 入参直接塞进索引层
- 让 `IndexManager` 的接口更明确
- 在构造阶段一次性完成扩展名归一化与 glob 编译，避免在条目遍历时重复做字符串处理

### 6.3 `IndexManager::search_context()` 扩展

当前签名：

```rust
pub async fn search_context(&self, query: &str) -> Result<String>
```

建议改为：

```rust
pub async fn search_context(
    &self,
    query: &str,
    filters: &SearchFilterOptions,
) -> Result<String>
```

### 6.4 blob 收集逻辑改造

当前逻辑：

```text
index_data.get_all_blob_hashes()
```

建议改为：

```text
for (rel_path, entry) in index_data.entries {
  if should_include_in_search(rel_path, filters) {
    collect entry.blob_hashes
  }
}
```

这里需要新增例如：

- `should_exclude_from_search(rel_path, filters)`
- `normalize_extension()`
- `compile_glob_patterns()`，推荐直接使用 `globset` crate 预编译

性能要求：

- 不在遍历 `IndexData.entries` 的循环内部动态编译 glob
- 对扩展名匹配统一走小写归一化后的 `HashSet`
- 对路径模式匹配统一走预编译后的 matcher

### 6.5 向后兼容

如果调用方不传新参数：

- 行为必须与现在完全一致
- 即：仍然把全部已索引 blob 发给检索服务

---

## 7. 为什么不建议第一版就改索引层

当前索引缓存校验依赖 `config_hash`，而 `config_hash` 目前只包含：

- `max_lines_per_blob`

代码位置：

- `src/index/manager.rs:176-182`

如果把“本次查询排除 `.md`”这种动态条件也带进索引层，会出现两个问题：

1. 缓存内容和查询条件耦合
2. 同一仓库可能需要维护多套索引结果

这会显著提高实现复杂度，不适合作为第一版。

因此，第一版应该只在“搜索请求组装阶段”做过滤。

---

## 8. 测试方案

建议至少补以下测试：

### 8.1 参数与 schema 测试

文件：

- `tests/tools_test.rs`

测试点：

- schema 中新增字段存在
- `SearchContextArgs` 序列化/反序列化正常
- 默认不传参数时行为兼容

### 8.2 过滤逻辑单测

建议新增针对路径过滤的纯函数测试，覆盖：

- `README.md`
- `docs/guide.md`
- `src/main.rs`
- `notes.txt`
- `config/app.yaml`

测试点：

- `exclude_document_files = true` 时：
  - `README.md` 被排除
  - `notes.txt` 被排除
  - `src/main.rs` 不排除
  - `config/app.yaml` 不排除

- `exclude_extensions = [".md"]` 时：
  - 只排 `.md`

- `exclude_globs = ["docs/**"]` 时：
  - 只排 `docs` 下路径

- 组合场景：
  - `exclude_document_files = true`
  - `exclude_extensions = [".md"]`
  - `exclude_globs = ["docs/**"]`
  - 验证三者按 **Union** 生效，而不是覆盖关系

- 大小写场景：
  - `README.MD`
  - `notes.TxT`
  - 验证扩展名归一化后仍能正确过滤

- 过度过滤场景：
  - 过滤规则把全部条目都排空
  - 需要优雅返回空候选或空结果
  - 不能 panic，不能出现 `unwrap()` 崩溃

### 8.3 搜索请求构造测试

建议在 `IndexManager` 层补一个可测试辅助函数，避免单测必须依赖真实 HTTP。

例如：

- 从 `IndexData.entries` + filters 构造 `blob_names`

这样可以直接验证：

- 过滤前 blob 数量
- 过滤后 blob 数量
- 具体哪些 blob 被保留

---

## 9. 风险与边界

### 9.1 风险

- 如果调用方误用 `exclude_globs`，可能把有效内容也排掉
- 如果“文档类”定义过宽，可能误伤一些本应保留的说明性源码文件

### 9.2 边界

- 第一版不处理“搜索结果出来后再按内容类型二次排序”
- 第一版不改变索引体积
- 第一版不改 `.aceignore` / `.gitignore`
- 第一版不支持 include-only 模式
- 第一版不引入 `include_globs` / `include_extensions`
- 如果未来确有“只搜文档”诉求，优先考虑 `search_mode = CodeOnly | DocOnly | Mixed`

### 9.3 后续扩展方向

如果第一版效果良好，可以继续做：

1. 路径级 boost / demote
2. 面向“代码优先”检索的快捷模式，例如：
   - `search_mode = "code_only"`
   - `search_mode = "doc_only"`
   - `search_mode = "mixed"`
3. 评估是否需要更复杂的 include 语义

---

## 10. 建议的落地顺序

### Phase 1

- 新增 MCP 参数
- 搜索请求阶段做 blob 过滤
- 补单测

### Phase 2

- 根据实际效果决定是否加入 `include_*`
- 根据检索质量评估是否增加路径权重控制

### Phase 3

- 评估是否值得进一步做“索引层差异化缓存”

---

## 11. 需要团队评审确认的问题

1. `exclude_document_files` 的默认文档类型集合是否接受？
2. 第一版是否只做查询时过滤，不改索引规则？
3. 是否需要第一版同时支持 `exclude_globs`？
4. `README.md` 是否应该被视为默认文档噪声？
5. 后续是否需要单独引入 `search_mode`，而不是直接引入复杂的 `include_*` 语义？

---

## 12. 最终建议

建议团队通过以下结论：

- **做**
- **第一版按“查询时动态过滤 blob”实现**
- **不改默认索引规则**
- **新增 `exclude_document_files` + `exclude_extensions` + `exclude_globs`**
- **实现时对 glob 使用预编译 matcher，对扩展名使用小写归一化**
- **过滤规则按 Union 叠加**
- **补“全量过滤为空”的安全测试**

这条路线实现成本中等、边界清晰、兼容性好，适合作为当前问题的第一版解法。

---

## 13. 本轮评审结论摘录

本轮团队反馈已确认以下方向：

- 强烈赞同采用“查询时按索引条目二次过滤 blob”
- 第一版只做查询时过滤，不改索引规则
- 第一版建议同时支持 `exclude_globs`
- `README.md` 应视为默认文档噪声
- `include_*` 暂缓，优先考虑未来引入 `search_mode`

这些结论已合并进本文档，后续实现以本版本为准。
