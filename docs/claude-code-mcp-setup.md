# Claude Code 配置 ACE-Tool MCP 指南

本文档说明如何在 Claude Code 中配置 ace-tool-rs 作为 MCP 服务。

---

## 前置条件

1. 已编译并部署 `ace-tool-rs` 二进制文件
2. 有可用的 API 服务端点（或使用第三方端点）

---

## 配置步骤

### 1. 确认二进制文件位置

```bash
which ace-tool-rs
# 或
ls ~/.local/bin/ace-tool-rs
```

假设路径为：`/Users/alistar/.local/bin/ace-tool-rs`

### 2. 编辑 Claude Code 配置文件

Claude Code 的 MCP 配置文件位于：

```
~/.claude/settings.json
```

如果文件不存在，创建它：

```bash
mkdir -p ~/.claude
touch ~/.claude/settings.json
```

### 3. 添加 MCP 服务配置

编辑 `~/.claude/settings.json`，添加 `mcpServers` 配置：

#### 方式一：使用命令行参数

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/Users/alistar/.local/bin/ace-tool-rs",
      "args": [
        "--base-url", "https://your-api-server.com",
        "--token", "your-auth-token"
      ]
    }
  }
}
```

#### 方式二：使用环境变量（推荐）

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/Users/alistar/.local/bin/ace-tool-rs",
      "args": [],
      "env": {
        "ACE_BASE_URL": "https://your-api-server.com",
        "ACE_TOKEN": "your-auth-token"
      }
    }
  }
}
```

#### 方式三：使用第三方端点（Claude/OpenAI/Gemini）

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/Users/alistar/.local/bin/ace-tool-rs",
      "args": [],
      "env": {
        "PROMPT_ENHANCER_ENDPOINT": "claude",
        "ANTHROPIC_API_KEY": "sk-ant-xxx"
      }
    }
  }
}
```

支持的第三方端点：
- `claude` - 使用 Anthropic Claude API
- `openai` - 使用 OpenAI API
- `gemini` - 使用 Google Gemini API

---

## 完整配置示例

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/Users/alistar/.local/bin/ace-tool-rs",
      "args": [],
      "env": {
        "ACE_BASE_URL": "https://api.example.com",
        "ACE_TOKEN": "your-token-here",
        "PROMPT_ENHANCER_ENDPOINT": "claude",
        "ANTHROPIC_API_KEY": "sk-ant-xxx"
      }
    }
  }
}
```

---

## 可用环境变量

| 环境变量 | 说明 | 示例 |
|----------|------|------|
| `ACE_BASE_URL` | API 服务地址 | `https://api.example.com` |
| `ACE_TOKEN` | 认证令牌 | `your-token` |
| `PROMPT_ENHANCER_ENDPOINT` | Prompt 增强端点 | `claude` / `openai` / `gemini` |
| `PROMPT_ENHANCER_BASE_URL` | 自定义增强服务地址 | `https://enhancer.example.com` |
| `PROMPT_ENHANCER_TOKEN` | 增强服务令牌 | `enhancer-token` |
| `PROMPT_ENHANCER_MODEL` | 增强服务模型 | `claude-sonnet-4-6` |
| `ANTHROPIC_API_KEY` | Claude API 密钥 | `sk-ant-xxx` |
| `OPENAI_API_KEY` | OpenAI API 密钥 | `sk-xxx` |
| `GEMINI_API_KEY` | Gemini API 密钥 | `xxx` |

---

## 可用命令行参数

```bash
ace-tool-rs [OPTIONS]

Options:
      --base-url <BASE_URL>           API 服务地址
      --token <TOKEN>                 认证令牌
      --transport <TRANSPORT>         传输协议: auto, lsp, line [default: auto]
      --max-lines-per-blob <N>        每个 blob 最大行数 [default: 800]
      --upload-timeout <SECONDS>      上传超时秒数
      --upload-concurrency <N>        上传并发数
      --retrieval-timeout <SECONDS>   检索超时秒数 [default: 60]
      --no-adaptive                   禁用自适应策略
      --no-webbrowser-enhance-prompt  禁用浏览器打开增强结果
      --force-xdg-open                WSL 环境强制使用 xdg-open
      --webui-addr <ADDR>             Web UI 绑定地址
      --index-only                    仅索引模式
      --enhance-prompt <PROMPT>       增强 prompt 并输出
```

---

## 验证配置

### 1. 重启 Claude Code

配置修改后，需要重启 Claude Code 使配置生效。

### 2. 检查 MCP 服务

在 Claude Code 中，使用 `/mcp` 命令查看已加载的 MCP 服务：

```
/mcp
```

应该看到 `ace-tool` 服务及其提供的工具：
- `search_context` - 语义代码检索
- `enhance_prompt` - Prompt 增强（如已配置）

### 3. 测试工具

测试 `search_context`：

```
请使用 search_context 工具在当前项目中搜索 "MCP transport 实现"
```

---

## 故障排除

### MCP 服务未加载

1. 检查配置文件语法：`cat ~/.claude/settings.json | jq .`
2. 检查二进制文件权限：`ls -la ~/.local/bin/ace-tool-rs`
3. 手动运行测试：`~/.local/bin/ace-tool-rs --help`

### 认证失败

1. 确认 `ACE_BASE_URL` 和 `ACE_TOKEN` 正确
2. 检查网络连接
3. 查看 Claude Code 日志

### 工具调用超时

1. 增加超时时间：`--retrieval-timeout 120`
2. 检查项目大小，大型项目首次索引较慢

---

## 多项目管理

如果需要为不同项目使用不同的配置，可以在项目根目录创建 `.claude/settings.json`：

```
your-project/
├── .claude/
│   └── settings.json
├── src/
└── ...
```

项目级配置会与全局配置合并。

---

## 安全建议

1. **不要** 将包含敏感令牌的配置文件提交到版本控制
2. 使用环境变量而非命令行参数传递敏感信息
3. 定期轮换 API 令牌
4. 限制令牌的权限范围
