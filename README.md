# MiNiMaX Code

AI 驱动的桌面端代码编辑器，基于 [Tauri 2](https://tauri.app/) + Vue 3 + TypeScript 构建，原生支持 MiniMax 和 Anthropic 兼容 API（Claude、DeepSeek 等），提供多智能体协作编程体验。

## 功能特性

- **双模式** — Ace 模式（独立全栈 Agent）和 Team 模式（多智能体协作），一键切换
- **双模式** — Ace 模式（独立全栈 Agent）和 Team 模式（多智能体协作），一键切换
- **多智能体协作** — Front / Plan / Work / Review / Explore 五大 Agent，结构化协作流程：需求 → 探索 → 方案 → 执行 → 审查
- **多提供商** — 原生 MiniMax API + Anthropic 兼容 API（Claude、DeepSeek 等），每 Agent 可独立指定模型
- **Prompt 缓存** — MiniMax KV Cache 多轮对话前缀缓存，缓存命中率极高，大幅降低延迟和成本
- **上下文管理** — 实时 Token 用量条，自动压缩 + `/compact` 命令，斜杠命令弹窗
- **Skills 系统** — 80+ 内置技能模块，覆盖 Python/Go/Rust/Java/Kotlin/Swift 等语言及架构模式、测试、安全等，Agent 自动按需匹配加载
- **MCP 协议** — 内置 MCP 客户端，`/mcp reload` 热重载，无需重启
- **流式对话** — 实时流式 AI 对话，Markdown 渲染 + 代码高亮，思考过程展示
- **撤销 & 回退** — 文件编辑自动保存快照（含二进制文件），支持撤销、回退到任意对话节点
- **快照管理** — 一键保存/恢复项目状态快照
- **文件操作** — 批量读写、查找替换、Patch 应用等全套编辑能力
- **Git 集成** — status / log / diff / branch / commit / stash 等
- **运行命令** — 终端命令执行 + 进程管理
- **Web 搜索 & 图片理解** — MiniMax 搜索 + VLM，自定义提供商走 Anthropic Vision API
- **LSP 集成** — Language Server Protocol，代码诊断与补全
- **权限管理** — Normal / Guarded / Full 三种模式，敏感路径始终拦截
- **本地持久化** — SQLite 存储对话历史、会话管理、配置

## 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri 2 |
| 前端 | Vue 3 + TypeScript + Vue Router + Vite |
| 后端 | Rust + Tokio (async runtime) |
| 数据库 | SQLite (rusqlite) |
| Markdown | marked + highlight.js |
| 协议 | MCP + LSP |

## 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/) (最新 stable)
- Windows / macOS / Linux

### 安装依赖

```bash
npm install
```

### 开发模式

```bash
npm run tauri dev
```

### 构建

```bash
npm run tauri build
```

## 使用说明

1. 启动后在设置面板选择提供商并填入 API Key
   - **MiniMax** — 填入 API Key，选择模型
   - **自定义 Anthropic** — 填入 API 地址、API Key、模型名称、上下文窗口，可保存多套配置
   - **Team 配置** — 为每个智能体单独指定模型（可选，留空使用全局配置）
2. 设置工作目录（Workspace），Agent 将在此目录下进行文件操作
3. 通过 ModeSwitcher 切换工作模式：
   - **Ace** — 独立全栈 Agent，拥有所有工具，端到端完成
   - **Team** — 五大 Agent 结构化协作：
     - **Front** — 项目经理：理解需求、协调团队、汇报结果
     - **Plan** — 架构师：分析需求、输出技术方案
     - **Work** — 执行者：写代码、跑命令
     - **Review** — 审查员：代码审查、Git 提交
     - **Explore** — 探索者：分析项目结构、理解代码逻辑
4. 左侧面板管理会话历史，支持重命名、删除、按模式过滤
5. 输入框支持斜杠命令：`/compact` 压缩上下文，`/mcp reload` 热重载 MCP 配置

## 项目结构

```
├── src/                    # Vue 3 前端
│   ├── components/         # UI 组件
│   ├── composables/        # 组合式函数 (流式、Markdown、工具调用等)
│   ├── views/              # 页面视图
│   ├── services/           # 数据库服务
│   ├── types/              # TypeScript 类型定义
│   └── router/             # 路由配置
├── src-tauri/              # Rust 后端
│   ├── skills/                 # 80+ 内置技能模块
│   └── src/
│       ├── agent_service.rs    # AI Agent 流式对话 + 工具实现
│       ├── mcp_service.rs      # MCP 协议客户端
│       ├── lsp_manager.rs      # LSP 管理器
│       ├── skill_service.rs    # 技能加载与匹配
│       ├── context_compressor.rs # 上下文压缩
│       ├── permission.rs       # 权限管理
│       └── system_prompts.rs   # Agent 系统提示词
├── public/                 # 静态资源
└── package.json
```

## License

MIT
