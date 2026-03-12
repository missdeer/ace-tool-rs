# Search Context 检索工具使用指南

## 工具概述

`search_context` 是一个语义代码检索工具，用于在代码库中搜索与自然语言查询相关的代码片段。它会自动索引项目，然后通过语义相似度匹配返回最相关的代码。

---

## 参数说明

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `project_root_path` | string | ✅ | 项目根目录绝对路径，使用正斜杠 `/` |
| `query` | string | ✅ | 自然语言搜索查询 |
| `exclude_document_files` | boolean | ❌ | 是否排除文档文件（.md, .txt, README 等） |
| `exclude_extensions` | array | ❌ | 要排除的扩展名列表，如 `[".md", ".txt"]` |
| `exclude_globs` | array | ❌ | 要排除的 glob 模式，如 `["docs/**", "**/test*"]` |

---

## 使用场景与最佳实践

### 场景 1：查找代码实现

**目标**：找到某个功能的具体实现代码

**推荐查询格式**：
```
[功能描述] + [可选关键词]

示例：
- "用户认证流程是如何实现的？关键词：auth, login, token"
- "MCP transport 协议的自动探测逻辑在哪里？关键词：transport, detect"
- "文件上传时的分块合并是在哪个函数处理的？关键词：upload, chunk, merge"
```

**示例调用**：
```json
{
  "project_root_path": "/Users/alistar/projects/myapp",
  "query": "数据库连接池是如何初始化和管理的？关键词：pool, connection, init"
}
```

### 场景 2：只搜索源码，排除文档

**目标**：避免 README、CHANGELOG 等文档噪声，只获取代码

**推荐方式**：使用 `exclude_document_files: true`

**示例调用**：
```json
{
  "project_root_path": "/Users/alistar/projects/myapp",
  "query": "错误处理机制是如何设计的？",
  "exclude_document_files": true
}
```

**排除的文件类型**：
- 扩展名：`.md`, `.mdx`, `.txt`, `.csv`, `.tsv`, `.rst`, `.adoc`, `.tex`, `.org`
- 无扩展名：`README`, `CHANGELOG`, `TODO`, `ROADMAP`, `LICENSE` 等

### 场景 3：排除特定目录或文件

**目标**：排除测试代码、配置文件等

**推荐方式**：使用 `exclude_globs`

**示例调用**：
```json
{
  "project_root_path": "/Users/alistar/projects/myapp",
  "query": "核心业务逻辑的实现",
  "exclude_globs": ["tests/**", "**/*_test.rs", "**/mock/**", "docs/**"]
}
```

### 场景 4：排除特定扩展名

**目标**：只搜索特定语言，或排除配置文件

**推荐方式**：使用 `exclude_extensions`

**示例调用**：
```json
{
  "project_root_path": "/Users/alistar/projects/myapp",
  "query": "API 路由定义在哪里？",
  "exclude_extensions": [".json", ".yaml", ".yml", ".toml"]
}
```

### 场景 5：组合过滤

**目标**：精确控制搜索范围

**示例调用**：
```json
{
  "project_root_path": "/Users/alistar/projects/myapp",
  "query": "核心算法实现",
  "exclude_document_files": true,
  "exclude_extensions": [".rs"],
  "exclude_globs": ["src/generated/**"]
}
```

**过滤规则说明**：
- 三个过滤条件是 **Union（并集）** 关系
- 满足任一条件的文件都会被排除
- 不存在覆盖/优先级关系

---

## 查询编写技巧

### ✅ 好的查询

```json
// 描述清晰 + 可选关键词
"查找处理用户登录请求的入口函数在哪里？关键词：login, handler, route"

// 问题导向
"HTTP 请求超时后是如何重试的？关键词：timeout, retry, backoff"

// 功能定位
"配置文件的热更新是如何触发的？关键词：config, reload, hot"
```

### ❌ 不好的查询

```json
// 太泛泛
"代码"

// 只有类名/函数名（应该用 grep）
"UserManager"

// 太具体要求位置
"第 100 行附近的代码"
```

---

## 注意事项

### 1. 首次调用会自动索引

第一次调用 `search_context` 时，会对项目进行索引。大型项目可能需要几秒到几十秒。后续调用会使用缓存，速度更快。

### 2. 索引缓存机制

- 索引结果缓存在 `.ace/index` 目录
- 配置变更（如 `.aceignore` 修改）会触发重新索引
- 如需强制重建索引，可删除 `.ace` 目录

### 3. 过滤后无结果

如果过滤条件导致没有匹配的文件，工具会返回：
```
No matching files found after applying filter criteria.
```
这不是错误，而是正常的空结果。请尝试放宽过滤条件。

### 4. 不适合的场景

以下场景建议使用其他工具：

| 场景 | 推荐工具 |
|------|----------|
| 精确查找某个符号的定义 | `grep` / `ripgrep` |
| 查看特定文件内容 | `Read` 工具 |
| 查找所有引用 | `grep` / IDE |
| 查看 git 历史 | `git log` |

---

## 完整示例

### 示例 1：排查 Bug

```json
{
  "project_root_path": "/Users/alistar/projects/webapp",
  "query": "用户登录后 session 是如何创建和存储的？关键词：session, create, store, login",
  "exclude_document_files": true,
  "exclude_globs": ["**/test/**", "**/__tests__/**"]
}
```

### 示例 2：理解架构

```json
{
  "project_root_path": "/Users/alistar/projects/microservice",
  "query": "服务间 RPC 通信是如何实现的？关键词：rpc, client, server, call",
  "exclude_document_files": true
}
```

### 示例 3：定位性能问题

```json
{
  "project_root_path": "/Users/alistar/projects/datapipeline",
  "query": "数据批处理时的并发控制是如何实现的？关键词：batch, concurrency, parallel, limit",
  "exclude_globs": ["benchmarks/**", "examples/**"]
}
```

---

## 快速参考卡

```
┌─────────────────────────────────────────────────────────────┐
│                  search_context 快速参考                    │
├─────────────────────────────────────────────────────────────┤
│ 必填参数：                                                   │
│   • project_root_path - 项目根目录                          │
│   • query - 自然语言查询                                    │
├─────────────────────────────────────────────────────────────┤
│ 过滤参数（可选）：                                           │
│   • exclude_document_files: true - 排除文档                 │
│   • exclude_extensions: [".md", ".txt"] - 排除扩展名        │
│   • exclude_globs: ["docs/**"] - 排除路径模式               │
├─────────────────────────────────────────────────────────────┤
│ 查询技巧：                                                   │
│   • 描述你想找什么 + 可选关键词                              │
│   • 避免太泛泛或太具体                                       │
│   • 源码搜索用 exclude_document_files: true                 │
└─────────────────────────────────────────────────────────────┘
```
