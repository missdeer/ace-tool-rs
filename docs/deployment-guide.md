# ACE-Tool-RS 编译部署指南

## 环境要求

- Rust 1.70+ (推荐使用 `rustup` 安装)
- macOS / Linux

## 编译步骤

### 1. 克隆项目

```bash
git clone https://github.com/missdeer/ace-tool-rs.git
cd ace-tool-rs
```

### 2. 编译 Release 版本

```bash
cargo build --release
```

编译产物位于：`./target/release/ace-tool-rs`

### 3. 运行测试（可选）

```bash
cargo test --test index_test --test tools_test
cargo test --lib
```

## 部署方式

### 方式一：用户目录（推荐，无需 sudo）

```bash
mkdir -p ~/.local/bin
install -m 755 ./target/release/ace-tool-rs ~/.local/bin/

# 添加到 PATH（如需要）
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### 方式二：系统目录

```bash
sudo install -m 755 ./target/release/ace-tool-rs /usr/local/bin/
```

## 验证安装

```bash
ace-tool-rs --help
```

输出应显示：
```
MCP server for codebase indexing and semantic search

Usage: ace-tool-rs [OPTIONS]

Options:
      --base-url <BASE_URL>
          API base URL for the indexing service
      --token <TOKEN>
          Authentication token
      --transport <TRANSPORT>
          Transport protocol: stdio, sse [default: stdio]
  -h, --help
          Show help
```

## 版本号

版本号定义在 `Cargo.toml` 的 `version` 字段：

```toml
[package]
name = "ace-tool-rs"
version = "0.1.15"
```

### 升级版本

1. 修改 `Cargo.toml` 中的 `version` 字段
2. 重新编译：`cargo build --release`
3. 重新部署

## 运行 MCP 服务

### 环境变量配置

```bash
export ACE_BASE_URL="https://your-api-server.com"
export ACE_TOKEN="your-auth-token"
```

### 命令行启动

```bash
ace-tool-rs --base-url https://your-api-server.com --token your-token
```

### 作为 MCP 服务使用

在 Claude Desktop 或其他 MCP 客户端配置：

```json
{
  "mcpServers": {
    "ace-tool": {
      "command": "/path/to/ace-tool-rs",
      "args": [],
      "env": {
        "ACE_BASE_URL": "https://your-api-server.com",
        "ACE_TOKEN": "your-auth-token"
      }
    }
  }
}
```

## 故障排除

### 编译失败

1. 确保 Rust 版本足够新：`rustc --version`
2. 清理并重新编译：`cargo clean && cargo build --release`

### 运行时找不到命令

1. 确认二进制文件在 PATH 中：`which ace-tool-rs`
2. 确认文件有执行权限：`ls -la $(which ace-tool-rs)`
