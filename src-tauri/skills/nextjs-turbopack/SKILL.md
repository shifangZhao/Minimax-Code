---
name: nextjs-turbopack
description: Next.js 16+ 和 Turbopack — 增量捆绑、FS 缓存、开发速度和何时使用 Turbopack vs webpack。
origin: ECC
---

# Next.js 和 Turbopack

Next.js 16+ 默认使用 Turbopack 进行本地开发：一个用 Rust 编写的增量捆绑器，显著加快开发启动和热更新。

## 何时使用

- **Turbopack（默认开发）**：用于日常开发。更快的冷启动和 HMR，尤其是在大型应用中。
- **Webpack（遗留开发）**：仅在遇到 Turbopack bug 或依赖 dev 中的 webpack-only 插件时使用。使用 `--webpack` 禁用（或 `--no-turbopack`，取决于你的 Next.js 版本；查看发布文档）。
- **生产**：生产构建行为（`next build`）可能使用 Turbopack 或 webpack，取决于 Next.js 版本；查看你版本官方 Next.js 文档。

在以下情况使用：开发或调试 Next.js 16+ 应用、诊断慢开发启动或 HMR，或优化生产捆绑。

## 工作原理

- **Turbopack**：用于 Next.js dev 的增量捆绑器。使用文件系统缓存，因此重启快得多（例如大型项目上 5-14 倍）。
- **dev 中默认**：从 Next.js 16 起，`next dev` 使用 Turbopack 运行，除非禁用。
- **文件系统缓存**：重启重用之前的工作；缓存通常在 `.next` 下；基本使用无需额外配置。
- **捆绑分析器（Next.js 16.1+）**：实验性捆绑分析器用于检查输出和查找重依赖；通过配置或实验标志启用（参见你版本的 Next.js 文档）。

## 示例

### 命令

```bash
next dev
next build
next start
```

### 使用

运行 `next dev` 用于使用 Turbopack 的本地开发。使用捆绑分析器（参见 Next.js 文档）优化代码分割和修剪大依赖。尽可能优先使用 App Router 和服务端组件。

## 最佳实践

- 保持在最新的 Next.js 16.x 以获得稳定的 Turbopack 和缓存行为。
- 如果 dev 慢，确保你在 Turbopack 上（默认）且缓存未被不必要清除。
- 对于生产捆绑大小问题，使用官方 Next.js 捆绑分析工具用于你的版本。