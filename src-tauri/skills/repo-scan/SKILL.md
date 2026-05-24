---
name: repo-scan
description: 跨技术栈源代码资产审计 — 对每个文件分类、检测嵌入的第三方库，并通过交互式 HTML 报告为每个模块提供可操作的四级裁决。
origin: community
---

# repo-scan

> 每个生态系统都有自己的依赖管理器，但没有工具能跨 C++、Android、iOS 和 Web 告诉你：有多少代码是你自己的、什么是第三方、什么是死代码。

## 何时使用

- 接管大型遗留代码库并需要结构概览
- 在重大重构前 — 识别什么是核心、什么是重复、什么是死代码
- 审计直接嵌入在源代码中的第三方依赖（未在包管理器中声明）
- 为 monorepo 重组准备架构决策记录

## 安装

```bash
# 仅获取固定提交以确保可重复性
mkdir -p ~/.claude/skills/repo-scan
git init repo-scan
cd repo-scan
git remote add origin https://github.com/haibindev/repo-scan.git
git fetch --depth 1 origin 2742664
git checkout --detach FETCH_HEAD
cp -r . ~/.claude/skills/repo-scan
```

> 安装任何智能体 skill 前先审查源代码。

## 核心能力

| 能力 | 描述 |
|---|---|
| **跨技术栈扫描** | 一次通过扫描 C/C++、Java/Android、iOS (OC/Swift)、Web (TS/JS/Vue) |
| **文件分类** | 每个文件标记为项目代码、第三方或构建产物 |
| **库检测** | 50+ 已知库（FFmpeg、Boost、OpenSSL…）带版本提取 |
| **四级裁决** | 核心资产 / 提取并合并 / 重建 / 废弃 |
| **HTML 报告** | 带深入导航的交互式深色主题页面 |
| **Monorepo 支持** | 层级扫描，带摘要 + 子项目报告 |

## 分析深度级别

| 级别 | 读取文件数 | 用例 |
|---|---|---|
| `fast` | 每个模块 1-2 个 | 巨型目录的快速清单 |
| `standard` | 每个模块 2-5 个 | 带完整依赖和架构检查的默认审计 |
| `deep` | 每个模块 5-10 个 | 增加线程安全、内存管理、API 一致性 |
| `full` | 所有文件 | 预合并综合审查 |

## 工作原理

1. **分类仓库表面**：枚举文件，然后将每个文件标记为项目代码、嵌入第三方代码或构建产物。
2. **检测嵌入库**：检查目录名、头文件、许可证文件和版本标记以识别捆绑依赖和可能版本。
3. **对每个模块评分**：按模块或子系统对文件分组，然后根据所有权、重复和维护成本分配四个裁决之一。
4. **突出结构风险**：指出死代码产物、重复包装器、过时的 vendored 代码和应该被提取、重建或废弃的模块。
5. **生成报告**：返回简洁摘要以及交互式 HTML 输出，带每个模块的深入以便审计可以异步审查。

## 示例

在 50,000 文件的 C++ monorepo 上：
- 发现 FFmpeg 2.x（2015 年版本）仍在生产中
- 发现同一个 SDK 包装器重复 3 次
- 识别了 636 MB 已提交的 Debug/ipch/obj 构建产物
- 分类：3 MB 项目代码 vs 596 MB 第三方

## 最佳实践

- 首次审计从 `standard` 深度开始
- 对有 100+ 模块的 monorepo 使用 `fast` 以获得快速清单
- 对标记为重构的模块增量运行 `deep`
- 审查跨模块分析以检测子项目间的重复

## 链接

- [GitHub 仓库](https://github.com/haibindev/repo-scan)