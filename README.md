# MiNiMaX Code

> 基于 Tauri 2 + Vue 3 + Rust 构建的本地 AI 编程 IDE，内置全栈开发 Agent。

[![Tauri](https://img.shields.io/badge/Tauri_2-FFC131?style=flat&logo=tauri&logoColor=000)](https://tauri.app/)
[![Vue 3](https://img.shields.io/badge/Vue_3-42b883?style=flat&logo=vuedotjs&logoColor=fff)](https://vuejs.org/)
[![Rust](https://img.shields.io/badge/Rust-CE412B?style=flat&logo=rust&logoColor=fff)](https://www.rust-lang.org/)
[![License MIT](https://img.shields.io/badge/License-MIT-blue)](LICENSE)

---

## 功能概述

### Agent 对话

内置 Ace 全栈 Agent，通过自然语言描述需求，Agent 会调用工具完成代码读写、命令执行、文件搜索等操作。

- 流式输出，实时显示 Agent 思考过程和工具调用
- 支持多会话并行，切换 Tab 后台继续运行
- 支持中途停止生成

### 工具调用可视化

Agent 的每次工具调用（读文件、写代码、执行命令）以卡片形式展示：

- 代码修改带 Diff 视图
- 工具调用结果实时展示
- 支持展开/折叠详情

### Todo 任务板

Agent 通过 `todo_write` 工具创建任务列表，前端 TodoPanel 展示进度。

### 回滚与快照

- 文件修改自动保存版本快照
- 支持回退到任意对话节点
- 一键保存/恢复项目文件快照

### 上下文管理

- 实时 Token 用量显示
- `/compact` 命令压缩上下文
- Prompt Cache 支持（MiniMax 原生 API）

### 多模型支持

支持 MiniMax 原生 API 和 Anthropic 兼容 API（Claude、DeepSeek 等）。每个 Agent 可独立配置模型。

---

## 技术栈

| 层 | 技术 |
|---|------|
| 前端 | Vue 3 + TypeScript + Vite |
| 后端 | Rust + Tauri 2 |
| 存储 | SQLite（本地） |
| 协议 | MCP（stdio + HTTP）、LSP |

---

## 快速开始

### 环境要求

- Node.js >= 18
- Rust（stable）
- Windows / macOS / Linux

### 安装运行

```bash
git clone https://github.com/shifangZhao/Minimax-Code.git
cd Minimax-Code
npm install
npm run tauri dev
```

### 首次使用

1. 点击标题栏齿轮，配置 API Key
2. 设置工作目录（Agent 在此目录下读写文件）
3. 开始对话

---

## 项目结构

```
src/                    # Vue 3 前端
├── components/         # UI 组件
├── composables/        # 组合式函数
└── services/           # 数据层

src-tauri/              # Rust 后端
├── skills/             # 技能模块
└── src/
    ├── agent_service.rs      # Agent 引擎
    ├── agent_tools.rs        # 工具实现
    ├── mcp_service.rs        # MCP 客户端
    ├── context_compressor.rs # 上下文压缩
    └── permission.rs         # 权限系统
```

---

## License

[MIT License](LICENSE)
