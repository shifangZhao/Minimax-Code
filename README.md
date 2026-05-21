# MiNiMaX Code

AI 驱动的桌面端代码编辑器，基于 [Tauri 2](https://tauri.app/) + Vue 3 + TypeScript 构建，使用 [MiniMax API](https://www.minimaxi.com/) 提供多智能体协作编程体验。

## 功能特性

- **多智能体协作** — Front、Plan、Work、Review、Explore 五种 Agent 视图，各司其职
- **流式对话** — 实时流式 AI 对话，支持 Markdown 渲染和代码高亮
- **MCP 协议** — 内置 Model Context Protocol 客户端，可接入本地/远程 MCP Server 扩展能力
- **LSP 集成** — Language Server Protocol 支持，提供代码智能补全和诊断
- **代码图谱** — 项目结构分析和依赖关系可视化
- **Skills 系统** — 可扩展的技能模块，支持内置和项目级自定义技能
- **文件操作** — 批量读写、查找替换、Patch 应用等全套文件编辑能力
- **Git 集成** — 支持 status、log、diff、branch、commit、stash 等常用 Git 操作
- **运行命令** — 在项目环境中执行终端命令，支持进程管理
- **Web 搜索 & 图片理解** — 内置网络搜索和 VLM 视觉理解能力
- **本地持久化** — SQLite 存储对话历史、会话管理和 API Key
- **权限管理** — 三种权限模式（Normal / Guarded / Full），精细控制工具调用
- **无边框窗口** — 自定义标题栏，原生窗口控制

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

1. 启动后在设置面板填入 MiniMax API Key
2. 设置工作目录（Workspace），Agent 将在此目录下进行文件操作
3. 通过顶部 Tab 切换不同 Agent 视图：
   - **Front** — 前端开发
   - **Plan** — 技术方案规划
   - **Work** — 通用编码执行
   - **Review** — 代码审查
   - **Explore** — 代码库探索
4. 左侧面板可管理多个群聊会话和对话历史

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
│   └── src/
│       ├── agent_service.rs    # AI Agent 流式对话
│       ├── mcp_service.rs      # MCP 协议客户端
│       ├── lsp_manager.rs      # LSP 管理器
│       ├── skill_service.rs    # 技能系统
│       ├── code_graph.rs       # 代码图谱
│       ├── permission.rs       # 权限管理
│       └── system_prompts.rs   # 系统提示词
├── public/                 # 静态资源
└── package.json
```

## License

MIT
