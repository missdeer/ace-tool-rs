# ace-tool-rs

[English](README.md) | 简体中文

一个高性能的 MCP（模型上下文协议）服务器，用于代码库索引、语义搜索和提示词增强，使用 Rust 编写。

## 概述

ace-tool-rs 是一个 Rust 实现的代码库上下文引擎，使 AI 助手能够使用自然语言查询来搜索和理解代码库。它提供：

- **实时代码库索引** - 自动索引项目文件并保持索引更新
- **语义搜索** - 使用自然语言描述查找相关代码
- **提示词增强** - 结合代码库上下文增强用户提示词，使请求更清晰、更可操作
- **多语言支持** - 支持 50+ 种编程语言和文件类型
- **增量更新** - 使用 mtime 缓存跳过未更改的文件，仅上传新增/修改的内容
- **并行处理** - 多线程文件扫描和处理，加快索引速度
- **智能排除** - 遵循 `.gitignore` 和常见的忽略模式

## 特性

- **MCP 协议支持** - 完整的 JSON-RPC 2.0 实现，基于 stdio 传输
- **自适应上传策略** - AIMD（加性增加，乘性减少）算法根据运行时指标动态调整并发度和超时时间
- **多编码支持** - 处理 UTF-8、GBK、GB18030 和 Windows-1252 编码的文件
- **并发上传** - 滑动窗口并行批量上传，加快大型项目的索引速度
- **Mtime 缓存** - 跟踪文件修改时间，避免重复处理未更改的文件
- **健壮的错误处理** - 指数退避重试逻辑和速率限制支持

## 安装

### 快速开始（推荐）

使用 npx 是安装和运行 ace-tool-rs 最简单的方式：

```bash
npx ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

这会自动下载适合你平台的二进制文件并运行。

**支持的平台：**
- Windows (x64)
- macOS (x64, ARM64)
- Linux (x64, ARM64)

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/missdeer/ace-tool-rs.git
cd ace-tool-rs

# 构建发布版本
cargo build --release

# 二进制文件位于 target/release/ace-tool-rs
```

### 环境要求

- Rust 1.70 或更高版本
- 索引服务的 API 端点
- 认证令牌

## 使用方法

### 命令行

```bash
ace-tool-rs --base-url <API_URL> --token <AUTH_TOKEN>
```

### 参数

| 参数 | 描述 |
|------|------|
| `--base-url` | 索引服务的 API 基础 URL（使用第三方端点的 `--enhance-prompt` 模式时可选） |
| `--token` | API 访问的认证令牌（使用第三方端点的 `--enhance-prompt` 模式时可选） |
| `--transport` | 传输帧格式：`auto`（默认）、`lsp`、`line` |
| `--upload-timeout` | 覆盖上传超时时间（秒），禁用自适应超时 |
| `--upload-concurrency` | 覆盖上传并发度，禁用自适应并发 |
| `--no-adaptive` | 禁用自适应策略，使用静态启发式值 |
| `--no-webbrowser-enhance-prompt` | 禁用 enhance_prompt 的浏览器交互，直接返回 API 结果 |
| `--index-only` | 仅索引当前目录并退出（不启动 MCP 服务器） |
| `--enhance-prompt` | 增强提示词并输出到标准输出，然后退出 |
| `--max-lines-per-blob` | 每个 blob 块的最大行数（默认：800） |
| `--retrieval-timeout` | 搜索检索超时时间（秒，默认：180） |

### 环境变量

| 变量 | 描述 |
|------|------|
| `RUST_LOG` | 设置日志级别（如 `info`、`debug`、`warn`） |
| `PROMPT_ENHANCER` | 控制 `enhance_prompt` 工具的暴露：设置为 `disabled`、`false`、`0` 或 `off` 可隐藏并禁用该工具 |
| `ACE_ENHANCER_ENDPOINT` | 端点选择：`new`（默认）、`old`、`claude`、`openai` 或 `gemini` |
| `PROMPT_ENHANCER_BASE_URL` | 第三方 API 的基础 URL（`claude`/`openai`/`gemini` 必需） |
| `PROMPT_ENHANCER_TOKEN` | 第三方 API 的密钥（`claude`/`openai`/`gemini` 必需） |
| `PROMPT_ENHANCER_MODEL` | 第三方 API 的模型名称覆盖（可选） |

### 示例

```bash
# 使用 debug 日志运行
RUST_LOG=debug ace-tool-rs --base-url https://api.example.com --token your-token-here
```

### 传输帧格式

默认情况下，服务器自动检测行分隔 JSON 与 LSP `Content-Length` 帧格式。
如果客户端需要特定模式，可以强制指定：

```bash
ace-tool-rs --base-url https://api.example.com --token your-token-here --transport lsp
```

## MCP 集成

### Codex CLI 配置

添加到 Codex 配置文件（通常是 `~/.codex/config.toml`）：

```toml
[mcp_servers.ace-tool]
command = "npx"
args = ["ace-tool-rs", "--base-url", "https://api.example.com", "--token", "your-token-here", "--transport", "lsp"]
env = { RUST_LOG = "info" }
startup_timeout_ms = 60000
```

### Claude Desktop 配置

添加到 Claude Desktop 配置文件：

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "npx",
      "args": [
        "ace-tool-rs",
        "--base-url", "https://api.example.com",
        "--token", "your-token-here"
      ]
    }
  }
}
```

### Claude Code

运行以下命令：

```bash
claude mcp add-json ace-tool --scope user '{"type":"stdio","command":"npx","args":["ace-tool-rs","--base-url","https://api.example.com/","--token","your-token-here"],"env":{}}'
```

修改 `~/.claude/settings.json` 添加工具权限：

```json
$ cat settings.local.json
{
  "permissions": {
    "allow": [
      "mcp__ace-tool__search_context",
      "mcp__ace-tool__enhance_prompt"
    ]
  }
}
```

### 可用工具

#### `search_context`

使用自然语言查询搜索代码库。

**参数：**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `project_root_path` | string | 是 | 项目根目录的绝对路径 |
| `query` | string | 是 | 你要查找的代码的自然语言描述 |

**查询示例：**

- "处理用户认证的函数在哪里？"
- "登录功能有哪些测试？"
- "数据库是如何连接到应用程序的？"
- "找到消息队列消费者的初始化流程"

#### `enhance_prompt`

通过结合代码库上下文和对话历史来增强用户提示词，生成更清晰、更具体、更可操作的提示词。

**参数：**

| 参数 | 类型 | 必需 | 描述 |
|------|------|------|------|
| `prompt` | string | 是 | 要增强的原始提示词 |
| `conversation_history` | string | 是 | 最近的对话历史（5-10 轮），格式：`User: xxx\nAssistant: yyy` |
| `project_root_path` | string | 否 | 项目根目录的绝对路径（可选，默认为当前工作目录） |

**特性：**

- 自动语言检测（中文输入 → 中文输出，英文输入 → 英文输出）
- 使用已索引文件的代码库上下文
- 考虑对话历史以更好地理解上下文

**API 端点：**

该工具支持多个后端端点，通过 `ACE_ENHANCER_ENDPOINT` 环境变量控制：

| 端点 | 描述 | 配置方式 |
|------|------|----------|
| `new`（默认） | Augment `/prompt-enhancer` 端点 | 使用 `--base-url` 和 `--token` CLI 参数 |
| `old` | Augment `/chat-stream` 端点（流式） | 使用 `--base-url` 和 `--token` CLI 参数 |
| `claude` | Claude API (Anthropic) | 使用 `PROMPT_ENHANCER_*` 环境变量 |
| `openai` | OpenAI API | 使用 `PROMPT_ENHANCER_*` 环境变量 |
| `gemini` | Gemini API (Google) | 使用 `PROMPT_ENHANCER_*` 环境变量 |

**第三方 API 默认模型：**

| 提供商 | 默认模型 |
|--------|----------|
| Claude | `claude-sonnet-4-20250514` |
| OpenAI | `gpt-4o` |
| Gemini | `gemini-2.0-flash-exp` |

**使用 Claude API 的示例：**

```bash
# MCP 服务器模式下，--base-url 和 --token 仍然是必需的
export ACE_ENHANCER_ENDPOINT=claude
export PROMPT_ENHANCER_BASE_URL=https://api.anthropic.com
export PROMPT_ENHANCER_TOKEN=your-anthropic-api-key
ace-tool-rs --base-url https://api.example.com --token your-token

# 使用第三方端点的 --enhance-prompt 模式下，--base-url 和 --token 是可选的
export ACE_ENHANCER_ENDPOINT=claude
export PROMPT_ENHANCER_BASE_URL=https://api.anthropic.com
export PROMPT_ENHANCER_TOKEN=your-anthropic-api-key
ace-tool-rs --enhance-prompt "添加用户认证功能"
```

## 支持的文件类型

### 编程语言

`.py`、`.js`、`.ts`、`.jsx`、`.tsx`、`.java`、`.go`、`.rs`、`.cpp`、`.c`、`.h`、`.cs`、`.rb`、`.php`、`.swift`、`.kt`、`.scala`、`.lua`、`.dart`、`.r`、`.jl`、`.ex`、`.hs`、`.zig` 等。

### 配置和数据

`.json`、`.yaml`、`.yml`、`.toml`、`.xml`、`.ini`、`.conf`、`.md`、`.txt`

### Web 技术

`.html`、`.css`、`.scss`、`.sass`、`.vue`、`.svelte`、`.astro`

### 特殊文件

`Makefile`、`Dockerfile`、`Jenkinsfile`、`.gitignore`、`.env.example`、`requirements.txt` 等。

## 默认排除项

以下模式默认被排除：

- **依赖项**：`node_modules`、`vendor`、`.venv`、`venv`
- **构建产物**：`target`、`dist`、`build`、`out`、`.next`
- **版本控制**：`.git`、`.svn`、`.hg`
- **缓存目录**：`__pycache__`、`.cache`、`.pytest_cache`
- **二进制文件**：`*.exe`、`*.dll`、`*.so`、`*.pyc`
- **媒体文件**：`*.png`、`*.jpg`、`*.mp4`、`*.pdf`
- **锁文件**：`package-lock.json`、`yarn.lock`、`Cargo.lock`

## 架构

```
ace-tool-rs/
├── src/
│   ├── main.rs          # 入口点和 CLI
│   ├── lib.rs           # 库导出
│   ├── config.rs        # 配置和上传策略
│   ├── enhancer/
│   │   ├── mod.rs
│   │   ├── prompt_enhancer.rs  # 提示词增强编排
│   │   └── templates.rs        # 增强提示词模板
│   ├── index/
│   │   ├── mod.rs
│   │   └── manager.rs   # 核心索引和搜索逻辑
│   ├── mcp/
│   │   ├── mod.rs
│   │   ├── server.rs    # MCP 服务器实现
│   │   └── types.rs     # JSON-RPC 类型
│   ├── service/
│   │   ├── mod.rs       # 服务模块导出
│   │   ├── common.rs    # 共享类型和工具
│   │   ├── augment.rs   # Augment New/Old 端点
│   │   ├── claude.rs    # Claude API (Anthropic)
│   │   ├── openai.rs    # OpenAI API
│   │   └── gemini.rs    # Gemini API (Google)
│   ├── strategy/
│   │   ├── mod.rs
│   │   ├── adaptive.rs  # AIMD 算法实现
│   │   └── metrics.rs   # EWMA 和运行时指标
│   ├── tools/
│   │   ├── mod.rs
│   │   └── search_context.rs  # 搜索工具实现
│   └── utils/
│       ├── mod.rs
│       └── project_detector.rs  # 项目工具
└── tests/               # 集成测试
    ├── config_test.rs
    ├── index_test.rs
    ├── mcp_test.rs
    ├── prompt_enhancer_test.rs
    ├── third_party_api_test.rs
    ├── tools_test.rs
    └── utils_test.rs
```

## 自适应上传策略

该工具使用受 TCP 拥塞控制启发的 AIMD（加性增加，乘性减少）算法来动态优化上传性能：

### 工作原理

1. **预热阶段**：从 concurrency=1 开始，在 5-10 个请求后评估成功率，如果成功则跳转到目标并发度
2. **加性增加**：当成功率 > 95% 且延迟健康时，并发度增加 1
3. **乘性减少**：当成功率 < 70%、被限速或高延迟时，并发度减半，超时时间增加 50%

### 指标

- **EWMA 延迟**：指数加权移动平均（α=0.2）用于延迟平滑
- **成功率**：在 20 个请求的滑动窗口上计算
- **延迟健康度**：与固定基线比较以检测退化

### 安全边界

| 参数 | 最小值 | 最大值 |
|------|--------|--------|
| 并发度 | 1 | 8 |
| 超时时间 | 15s | 180s |

### CLI 覆盖

你可以覆盖单个参数，同时保持其他参数自适应：

```bash
# 固定并发度，自适应超时
ace-tool-rs --base-url ... --token ... --upload-concurrency 4

# 固定超时，自适应并发
ace-tool-rs --base-url ... --token ... --upload-timeout 60

# 完全禁用自适应（使用静态启发式）
ace-tool-rs --base-url ... --token ... --no-adaptive
```

## 项目规模策略

该工具根据项目大小使用基于启发式的初始值。启用自适应模式（默认）时，这些值作为 AIMD 算法努力达到的目标值：

| 规模 | Blob 数量 | 批次大小 | 目标并发度 | 目标超时 |
|------|-----------|----------|------------|----------|
| 小型 | < 100 | 10 | 1 | 30s |
| 中型 | 100-499 | 30 | 2 | 45s |
| 大型 | 500-1999 | 50 | 3 | 60s |
| 超大型 | 2000+ | 70 | 4 | 90s |

使用 `--no-adaptive` 时，这些值将直接使用，不进行运行时调整。

## 开发

### 运行测试

```bash
# 运行所有测试
cargo test

# 带输出运行
cargo test -- --nocapture

# 运行特定测试
cargo test test_config_new
```

### 构建

```bash
# Debug 构建
cargo build

# Release 构建
cargo build --release

# 仅检查不构建
cargo check

# 运行 clippy 检查
cargo clippy
```

### 代码结构

- **390+ 单元测试** 覆盖所有主要组件
- 模块化架构，关注点分离清晰
- 全程使用 async/await，基于 Tokio 运行时
- 使用 Rayon 进行并行文件处理
- 使用 `anyhow` 进行全面的错误处理

## 限制

- 仅处理根目录的 `.gitignore` 文件（不支持嵌套的 `.gitignore` 文件）
- 需要网络访问索引 API
- 最大文件大小：每个文件 500KB
- 最大批次大小：每次上传批次 5MB

## 许可证

本项目采用双许可证模式：

### 非商业 / 个人使用 - GNU General Public License v3.0

可免费用于个人项目、教育目的、开源项目和非商业用途。完整的 GPLv3 许可证文本请参阅 [LICENSE](LICENSE)。

### 商业 / 工作场所使用 - 需要商业许可证

**如果您在商业环境、工作场所中使用 ace-tool-rs，或将其用于任何商业目的，您必须获取商业许可证。**

这包括但不限于：
- 在工作中使用本软件（任何组织）
- 集成到商业产品或服务中
- 用于客户工作或咨询项目
- 作为 SaaS/云服务的一部分提供

**联系方式**：商业许可证咨询请发邮件至 missdeer@gmail.com

详情请参阅 [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL)。

## 作者

[missdeer](https://github.com/missdeer)

## 贡献

欢迎贡献！请随时提交 Pull Request。

1. Fork 本仓库
2. 创建你的功能分支（`git checkout -b feature/amazing-feature`）
3. 提交你的更改（`git commit -m 'Add some amazing feature'`）
4. 推送到分支（`git push origin feature/amazing-feature`）
5. 开启 Pull Request

## Star 历史

[![Star History Chart](https://starchart.cc/missdeer/ace-tool-rs.svg)](https://starchart.cc/missdeer/ace-tool-rs)
