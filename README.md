# MiNiMaX Code

桌面端 AI 编程助手 — 不仅仅是编辑器，而是一个能理解项目、自主编码、协作审查的全栈开发伙伴。

基于 [Tauri 2](https://tauri.app/) + Vue 3 + Rust 构建，原生支持 MiniMax API 和 Anthropic 兼容 API（Claude、DeepSeek 等）。内置 80+ 技能模块，覆盖主流语言与开发范式。

## 核心亮点

- **双工作模式** — Ace 全栈单兵 与 Team 多人协作，一键切换
- **多提供商** — MiniMax 原生 + Anthropic 兼容（Claude、DeepSeek），每 Agent 可独立指定模型
- **Prompt 缓存** — MiniMax KV Cache 实现多轮前缀缓存，极高命中率，大幅节省延迟与成本
- **80+ 内置技能** — Python / Go / Rust / Java / Kotlin / Swift / 架构模式 / 测试 / 安全 等，Agent 自动匹配加载
- **12 条编码宪法** — Ace 内置完整编程原则，确保输出质量一致
- **实时流式对话** — 带思考过程展示，Markdown 渲染 + 代码高亮，支持随时中止生成
- **结构化团队协作** — Front → Explore → Plan → Work → Review，五角色标准流程：需求 → 探索 → 方案 → 执行 → 审查

## 功能特性

### 核心功能
- **上下文管理** — 实时 Token 用量进度条，自动压缩 + `/compact` 命令手动触发
- **撤销 & 回退** — 文件修改自动保存快照（含二进制），支持逐次撤销、回退到任意对话节点
- **快照管理** — 一键保存/恢复项目文件快照，随时回到之前的代码状态
- **斜杠命令** — `/compact` 压缩上下文，`/mcp reload` 热重载 MCP 配置

### 工具与集成
- **文件操作** — 批量读写、查找替换、Patch 应用、多文件编辑
- **Git 集成** — status / log / diff / branch / commit / stash / checkout
- **终端命令** — 命令行执行（Windows 下 Powershell 驱动，无黑框），支持后台进程管理
- **MCP 协议** — 内置 MCP 客户端，接入本地/远程 MCP Server，`/mcp reload` 热重载
- **LSP 集成** — Language Server Protocol 代码诊断与智能补全
- **Web 搜索 & 图片理解** — MiniMax 搜索 + VLM，自定义提供商使用 Anthropic Vision API

### 安全与质量
- **权限管理** — Normal / Guarded / Full 三种模式，敏感路径（密钥、凭证、.env）始终拦截
- **本地持久化** — SQLite 存储对话历史、配置、API Key，数据完全本地
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

打开设置面板，选择提供商：
- **MiniMax** — 填入 API Key，选择模型
- **自定义 Anthropic** — 填入 API 地址、Key、模型名、上下文窗口。可保存多套配置快速切换
- **Team 配置** — 为每个 Agent 单独指定模型（如 Front 用 Claude Opus，Work 用 MiniMax）

### 2. 设置工作目录

Agent 将在指定目录下读写文件、执行命令、分析项目。

### 3. 选择工作模式

| 模式 | 说明 |
|---|---|
| **Ace** | 独立全栈 Agent，拥有所有工具，端到端完成任务，不委派不等待 |
| **Team** | 五角色结构化协作，各司其职 |

**Team 模式角色：**

| 角色 | 定位 | 职责 |
|---|---|---|
| Front | 项目经理 | 理解需求、分解任务、协调团队、汇报结果 |
| Plan | 架构师 | 分析现状、输出技术方案：涉及文件 + 步骤 + 风险 + 验收清单 |
| Work | 执行者 | 唯一能写文件和跑命令，完成后自查，大改动提交 Review |
| Review | 审查员 | 代码审查 + Git Commit，最多 3 轮 |
| Explore | 探索者 | 目录树 → 搜索入口 → 读核心文件 → 输出结构化分析 |

### 4. 斜杠命令

在输入框输入 `/` 弹出命令列表：

| 命令 | 说明 |
|---|---|
| `/compact` | 手动压缩对话上下文，将中间消息替换为摘要，显示节省的 Token 数 |
| `/mcp reload` | 热重载 MCP 服务器配置，修改 `mcp.json` 后无需重启 |

## 项目结构

```
├── src/                         # Vue 3 前端
│   ├── components/              # UI 组件 (20+)
│   ├── composables/             # 组合式函数 (流式、缓存、书签、撤销等)
│   ├── views/                   # 页面视图
│   ├── services/                # 数据库服务层
│   └── router/                  # 路由配置 (带记忆)
├── src-tauri/                   # Rust 后端
│   ├── skills/                  # 80+ 内置技能模块
│   └── src/
│       ├── agent_service.rs     # Agent 流式对话 + 全部工具实现
│       ├── mcp_service.rs       # MCP 协议客户端 (stdio + HTTP)
│       ├── lsp_manager.rs       # LSP 管理器
│       ├── skill_service.rs     # 技能加载、索引、匹配
│       ├── context_compressor.rs # Token 估算 + 上下文压缩
│       ├── permission.rs        # 三层权限系统
│       └── system_prompts.rs    # Agent 系统提示词
└── public/                      # 静态资源
```

## License

MIT
