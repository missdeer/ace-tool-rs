# Findings

## 本轮评测目标

- 不凭主观印象
- 每题都给真实命中证据
- 最后给总分与是否满意

## 先验观察

- 当前会话里的 `mcp__ace-tool__search_context` 已经至少成功返回过两次结果
- 之前的样例显示：源码命中是有的，但 README 会混入，存在一定噪声

## 评测记录

### Q1 工具入口与路由

- 查询：定位 `search_context` 工具入口和 `tools/call` 路由
- 命中：`src/mcp/server.rs`、`src/tools/search_context.rs`、`tests/tools_test.rs`
- 评价：
  - 核心源码命中准确
  - 直接给到了 `handle_call_tool()` 和 `SearchContextTool::execute()`
  - 结果中还混入了 transport 相关段落，片段边界略松

### Q2 transport 自动探测

- 查询：定位 `Content-Length` / `line` framing 的自动探测与写回
- 命中：`src/mcp/server.rs`、`src/main.rs`、`tests/mcp_server_test.rs`
- 评价：
  - 核心实现基本全中
  - 还带上了 README / README-zh-CN，属于低价值辅助噪声
  - 对协议类问题的召回比 Q1 更完整

### Q3 参数必填约束

- 查询：`--base-url` / `--token` 何时必填，`enhance_prompt` 是否有例外
- 命中：`src/main.rs`、`src/config.rs`、`README.md`、`README-zh-CN.md`
- 评价：
  - 关键源码确实命中，能回答问题
  - 但 README 片段权重过高，源码信号不够靠前
  - 还混入了 `npm/ace-tool-rs/README.md`，相关但不优先

### Q4 index_only 模式

- 查询：`index_only` 如何只索引不启动 MCP server
- 命中：`src/main.rs` 为主，附带 `src/index/manager.rs`
- 评价：
  - 这是本轮最干净的一题
  - 直接把 `args.index_only` 分支、成功 / partial / error 行为都带出来了
  - 噪声很少，可直接消费

### Q5 测试覆盖定位

- 查询：哪些测试覆盖 transport 解析、tool 定义、参数校验
- 命中：`tests/mcp_server_test.rs`、`tests/mcp_test.rs`、`tests/tools_test.rs`
- 评价：
  - 测试类问题召回准确
  - 片段覆盖较完整，足够回答“有哪些测试”
  - 还带了 `src/tools/search_context.rs`、`src/mcp/server.rs`，可接受

## 当前中间判断

- 优点：实现源码召回不错，适合“我不知道文件在哪”
- 缺点：片段有时过宽，README 容易挤进前排

## 逐题评分

- Q1 工具入口与路由：84/100
- Q2 transport 自动探测：88/100
- Q3 参数必填约束：74/100
- Q4 index_only 模式：92/100
- Q5 测试覆盖定位：86/100

## 初步总评

- 平均分：84.8/100
- 体感分：85/100
- 结论：能用，而且对源码定位类问题有实际价值；短板主要是 README 噪声和片段边界偏宽

## 新增设计文档

- 已输出团队评审用文档：
  - `docs/2026-03-11-search-context-dynamic-document-exclusion-design.md`
- 文档核心结论：
  - 需求可以做
  - 第一版推荐做“查询时动态过滤 blob”
  - 不建议第一版修改全局默认索引规则
