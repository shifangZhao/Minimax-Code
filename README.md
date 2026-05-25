# MiNiMaX Code

桌面端 AI 编程助手 — 不仅仅是编辑器，而是一个能理解项目、自主编码、协作审查的全栈开发伙伴。

基于 [Tauri 2](https://tauri.app/) + Vue 3 + Rust 构建，原生支持 MiniMax API 和 Anthropic 兼容 API（Claude、DeepSeek 等）。内置技能模块与编码宪法，Agent 自动匹配加载。

## 核心亮点

- **双工作模式** — Ace 全栈单兵 与 Team 多人协作，一键切换
- **多提供商** — MiniMax 原生 + Anthropic 兼容（Claude、DeepSeek），每 Agent 可独立指定模型
- **Prompt 缓存** — MiniMax KV Cache 追加式前缀缓存，每次工具调用后重新标记断点，后续请求持续命中
- **结构化消息存储** — `message_part` 分片表将 thinking / tool_use / tool_result 独立存储，精确重建 API 消息
- **实时流式对话** — 带思考过程展示，工具调用以卡片形式实时渲染，Markdown + 语法高亮，随时中止生成
- **结构化团队协作** — Front → Explore → Plan → Work → Review，五角色标准流程：需求 → 探索 → 方案 → 执行 → 审查
- **Todo 任务板** — Agent 通过 `todo_write` 工具创建结构化任务列表，前端 TodoPanel 实时展示进度，防止 Agent 摸鱼

## 功能特性

### 核心功能
- **上下文管理** — 实时 Token 用量进度条（API 上报累计值，精确），70% 阈值自动压缩 + `/compact` 手动触发
- **Collapse Drain** — API 返回上下文溢出时自动重试，逐级激进压缩（保留最近 4→2 轮对话），模型驱动摘要
- **撤销 & 回退** — 文件修改自动保存版本化快照，支持多次撤销、回退到任意对话节点
- **快照管理** — 一键保存/恢复项目文件快照，随时回到之前的代码状态
- **斜杠命令** — `/compact` 压缩上下文（模型驱动摘要），`/mcp reload` 热重载 MCP 配置，执行过程带加载动画
- **多会话并行** — 切换 Tab 时后台会话继续运行，消息和 Token 状态按会话隔离缓存

### 工具与集成
- **文件操作** — 批量读写、查找替换、Patch 应用、多文件编辑，Diff 视图带语法高亮
- **Git 集成** — status / log / diff / branch / commit / stash / checkout，直接 `git -C` 调用无 shell 包装
- **终端命令** — 命令行执行（Windows 下 cmd 驱动，无黑框），支持后台进程、超时双阶段终止
- **MCP 协议** — 内置 MCP 客户端，stdio + HTTP 双传输模式，`/mcp reload` 热重载
- **LSP 集成** — Language Server Protocol 代码诊断与智能补全
- **Web 搜索 & 图片理解** — MiniMax 搜索 + VLM，自定义提供商使用 Anthropic Vision API
- **Ask 工具** — Agent 主动向用户发起单选/多选/文本确认，避免猜测用户意图

### 安全与质量
- **权限管理** — Normal / Guarded / Full 三种模式，敏感路径（密钥、凭证、.env、Windows 系统目录）始终拦截
- **API Key 脱敏** — 前端永远只拿到掩码 key（`sk-****abcd`），设置时自动识别掩码值保留原 key
- **网络重试** — 429/5xx/瞬断自动指数退避重试最多 10 次
- **工具配对验证** — 自定义提供商下自动校验 tool_use↔tool_result 配对，注入 stub 修复孤儿调用
- **压缩安全分割** — 压缩截断点自动调整，确保从不切断 tool_use/tool_result 配对
- **分析核验协议** — 三遍确认（溯源→交叉验证→自检），禁止凭记忆或猜测断言
- **本地持久化** — SQLite 存储对话历史、配置、API Key，数据完全本地，VACUUM 自动回收空间
- **无黑框体验** — Windows 下所有子进程使用 `CREATE_NO_WINDOW`，运行命令不闪黑框

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 |
| 前端 | Vue 3 + TypeScript + Vue Router + Vite |
| 后端 | Rust + Tokio + Reqwest |
| 数据库 | SQLite (rusqlite) |
| Markdown | marked + highlight.js |
| 协议 | MCP + LSP (JSON-RPC) |

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/) (最新 stable)
- Windows / macOS / Linux

### 安装与运行

```bash
# 安装依赖
npm install

# 开发模式
npm run tauri dev

# 生产构建
npm run tauri build
```

## 使用指南

### 1. 配置提供商

打开设置面板（标题栏齿轮图标），选择提供商：
- **MiniMax** — 填入 API Key，选择模型。支持 KV Cache 前缀缓存
- **自定义 Anthropic** — 填入 API 地址、Key、模型名、上下文窗口。可保存多套配置快速切换
- **Team 配置** — 为每个 Agent 单独指定模型（如 Front 用 Claude Opus，Work 用 MiniMax）

### 2. 设置工作目录

Agent 将在指定目录下读写文件、执行命令、分析项目。

### 3. 选择工作模式

| 模式 | 说明 |
|---|---|
| **Ace** | 独立全栈 Agent，拥有所有工具，自带 Todo 任务板 + 分析核验，端到端完成任务 |
| **Team** | 五角色结构化协作，各司其职 |

**Team 模式角色：**

| 角色 | 定位 | 职责 |
|---|---|---|
| Front | 项目经理 | 理解需求、创建 Todo 任务板、用 Ask 澄清需求、分解派发、跟踪进度、汇报结果 |
| Plan | 架构师 | 分析现状、输出技术方案（波及范围 + 步骤 + 风险 + 验收清单），方案中每个引用需有代码实证 |
| Work | 执行者 | 唯一能写文件和跑命令，收到任务第一步创建 Todo，先读后写，完成后自查，大改动提交 Review |
| Review | 审查员 | 代码审查 + Git Commit，最多 3 轮，审一处想全局 |
| Explore | 探索者 | 目录树 → 搜索入口 → 读核心文件 → 三遍核验后输出，找不到实证的结论直接删除 |

### 4. 斜杠命令

在输入框输入 `/` 弹出命令列表：

| 命令 | 说明 |
|---|---|
| `/compact` | 手动压缩对话上下文，模型驱动生成结构化摘要，显示节省的 Token 数 |
| `/mcp reload` | 热重载 MCP 服务器配置，修改 `mcp.json` 后无需重启 |

### 5. 主题切换

标题栏主题按钮支持三种配色：
- **深色** — 默认暗色主题
- **浅色** — 亮色主题
- **暖色** — 护眼暖色调

## 项目结构

```
├── src/                            # Vue 3 前端
│   ├── components/                 # UI 组件 (17 个)
│   │   ├── AgentView.vue           # Agent 对话视图
│   │   ├── TodoPanel.vue           # Todo 任务面板（实时跟踪 Agent 进度）
│   │   ├── ToolCard.vue            # 工具调用卡片（含 Diff 视图）
│   │   ├── TitleBar.vue            # 标题栏（模式切换 + 主题 + 侧边栏）
│   │   └── ...
│   ├── composables/                # 组合式函数 (9 个)
│   │   ├── useAgentConversation.ts # 对话核心逻辑 + 消息缓存
│   │   ├── useGlobalStreaming.ts   # 全局流式状态 + 监听器管理
│   │   ├── useTodoStore.ts         # Todo 状态管理
│   │   ├── useMarkdown.ts          # Markdown + 代码语法高亮 + Diff 渲染
│   │   └── ...
│   ├── views/                      # 页面视图
│   ├── services/                   # 数据库服务层 (db.ts)
│   └── router/                     # 路由配置 (带记忆)
├── src-tauri/                      # Rust 后端
│   ├── skills/                     # 内置技能模块（按需加载）
│   └── src/
│       ├── agent_service.rs        # Agent 流式对话 + 编排层
│       ├── agent_tools.rs          # 全部工具实现 (read/write/grep/git/command/MCP/LSP 等)
│       ├── mcp_service.rs          # MCP 协议客户端 (stdio + HTTP)
│       ├── lsp_manager.rs          # LSP 管理器
│       ├── lsp_client.rs           # LSP JSON-RPC 客户端
│       ├── skill_service.rs        # 技能加载、索引、匹配
│       ├── context_compressor.rs   # Token 估算 + 模型驱动摘要 + 安全压缩分割
│       ├── permission.rs           # 三层权限系统 + NTFS ADS 路径规范化
│       ├── system_prompts.rs       # Agent 系统提示词（含 Ask 指南 + 分析核验协议 + Todo 规则）
│       └── lib.rs                  # Tauri 命令注册 + 初始化 + compact_messages
└── public/                         # 静态资源
```

## License

MIT
