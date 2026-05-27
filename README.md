# 🚀 MiNiMaX Code

> **不只是编辑器，而是你的 AI 编程搭档**
>
> 一个真正理解你的代码、能自主思考、会主动汇报进度的全栈开发 Agent。

[![Tauri](https://img.shields.io/badge/Tauri_2-FFC131?style=flat&logo=tauri&logoColor=000)](https://tauri.app/)
[![Vue 3](https://img.shields.io/badge/Vue_3-42b883?style=flat&logo=vuedotjs&logoColor=fff)](https://vuejs.org/)
[![Rust](https://img.shields.io/badge/Rust-CE412B?style=flat&logo=rust&logoColor=fff)](https://www.rust-lang.org/)
[![License MIT](https://img.shields.io/badge/License-MIT-blue)](LICENSE)

---

## 🤔 为什么选择 MiNiMaX Code？

你是否遇到过这些问题？

- ❌ AI 助手**不懂你的项目**，上下文总是丢失
- ❌ 工具调用**不透明**，不知道 Agent 到底在做什么
- ❌ **修改没有记录**，出了问题无法回滚
- ❌ 长对话越聊越慢，**Token 费用失控**

**MiNiMaX Code 为这些问题而生。** 我们用 Rust 构建了可靠的后端引擎，用 Vue 3 打造了流畅的交互体验，让 AI 编程回归本质——**高效、可控、可信赖**。

---

## ✨ 产品亮点

### 🎯 一个 Agent，干完整件事

Ace 是我们的全栈 Agent，**独立完成从需求理解 → 代码实现 → 质量验证**的全部工作。你只需要描述需求，它自己规划、执行、验证，最后给你结果。

> **"不是聊天，而是交付。"**

### 🛠️ 工具调用全程可视化

Agent 每次读文件、写代码、执行命令，**你都能实时看到**：
- 工具调用以卡片形式展开
- 代码修改带 **Diff 视图**
- 进度条实时更新
- **可随时中止**生成过程

### 📋 Todo 任务板——Agent 自己建任务清单

Agent 通过内置的 `todo_write` 工具创建结构化任务列表，前端 **TodoPanel 实时展示进度**。

```
✅ 分析项目结构
✅ 设计数据库 Schema  
⏳ 编写 API 接口...
⬜ 编写单元测试
⬜ 更新文档
```

**再也不用猜 Agent 在干什么。**

### 💾 安全回滚，一键回到任意时刻

- 文件修改自动保存**版本化快照**
- 支持**多次撤销**，回到任意对话节点
- 一键保存/恢复项目文件快照
- **再也不怕 AI 改错代码**

### 🧠 智能上下文管理

- **实时 Token 用量**进度条（API 上报累计值，精确）
- **70% 阈值自动压缩**，无需手动干预
- `/compact` 一键压缩，显示节省的 Token 数
- **Prompt Cache** 命中率优化，省钱！

### 🔌 多模型，随意切换

支持 **MiniMax 原生 + Anthropic 兼容**（Claude、DeepSeek 等）：

| 提供商 | 特点 |
|-------|------|
| **MiniMax** | 原生 KV Cache，Prompt 缓存命中率高 |
| **Anthropic 兼容** | Claude、DeepSeek 等，可保存多套配置 |

每个 Agent 可**独立指定模型**，灵活组合。

---

## 🏆 核心能力

### 💻 编码与工具

| 能力 | 说明 |
|------|------|
| **文件操作** | 批量读写、查找替换、Patch 应用、多文件编辑 |
| **Git 集成** | status / log / diff / branch / commit，无需离开编辑器 |
| **终端命令** | 命令行执行，Windows 无黑框体验 |
| **MCP 协议** | 内置客户端，stdio + HTTP 双传输 |
| **LSP 集成** | Language Server Protocol 代码诊断与补全 |
| **Web 搜索** | 内置搜索能力，Agent 可查询最新资料 |

### 🔒 安全防护

| 机制 | 说明 |
|------|------|
| **三层权限** | Normal / Guarded / Full，敏感路径始终拦截 |
| **Key 脱敏** | 前端永远只看到掩码 key（`sk-****abcd`） |
| **网络重试** | 429/5xx 自动指数退避，最多 10 次 |
| **本地存储** | SQLite 存储，数据完全本地，VACUUM 自动回收 |

### 🎨 体感优化

| 特性 | 说明 |
|------|------|
| **三套主题** | 深色 / 浅色 / 暖色，护眼配色 |
| **多会话并行** | 切换 Tab 后台继续运行 |
| **斜杠命令** | `/compact` 压缩、`/mcp reload` 重载，输入 `/` 即可 |
| **Ask 工具** | Agent 主动向你确认意图，避免猜测 |

---

## ⚡ 快速开始

### 环境要求

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://www.rust-lang.org/)（最新 stable）
- Windows / macOS / Linux

### 安装运行

```bash
# 克隆仓库
git clone https://github.com/shifangZhao/Minimax-Code.git
cd Minimax-Code

# 安装依赖
npm install

# 开发模式
npm run tauri dev

# 生产构建
npm run tauri build
```

### 首次使用

1. **配置 API** — 点击标题栏齿轮，填入 MiniMax 或自定义 API Key
2. **设置工作目录** — Agent 将在此目录下读写文件
3. **开始对话** — 描述你的需求，Ace 会自动规划执行

---

## 📁 技术架构

```
src/                    # Vue 3 前端
├── components/         # 17 个 UI 组件
│   ├── AgentView.vue   # 对话主视图
│   ├── TodoPanel.vue   # 任务进度面板
│   └── ...
├── composables/        # 9 个组合式函数
└── services/           # SQLite 数据层

src-tauri/              # Rust 后端
├── skills/             # 技能模块（按需加载）
└── src/
    ├── agent_service.rs      # Agent 核心引擎
    ├── agent_tools.rs        # 工具实现
    ├── mcp_service.rs        # MCP 客户端
    ├── context_compressor.rs # 上下文压缩
    └── permission.rs         # 权限系统
```

---

## 📄 License

[MIT License](LICENSE) — 完全免费，可商用，可修改。

---

<div align="center">

**⭐ 如果觉得有帮助，请点个 Star 支持一下！**

[![GitHub Stars](https://img.shields.io/github/stars/shifangZhao/Minimax-Code?style=social)](https://github.com/shifangZhao/Minimax-Code/stargazers)

</div>
