---
name: documentation-lookup
description: 通过 Context7 MCP 使用最新的库和框架文档，而非训练数据。在设置问题、API 参考、代码示例或用户命名框架时激活（如 React、Next.js、Prisma）。
origin: ECC
---

# 文档查找（Context7）

当用户询问库、框架或 API 时，通过 Context7 MCP（工具 `resolve-library-id` 和 `query-docs`）获取当前文档，而非依赖训练数据。

## 核心概念

- **Context7**：暴露实时文档的 MCP 服务器；用它代替库和 API 的训练数据。
- **resolve-library-id**：返回 Context7 兼容的库 ID（如 `/vercel/next.js`），从库名和查询。
- **query-docs**：获取给定库 ID 和问题的文档和代码片段。始终先调用 resolve-library-id 以获取有效的库 ID。

## 何时使用

当用户满足以下条件时激活：

- 询问设置或配置问题（如"如何配置 Next.js 中间件？"）
- 请求依赖库的代码（"写一个 Prisma 查询..."）
- 需要 API 或参考信息（"Supabase auth 方法有哪些？"）
- 提及特定框架或库（React、Vue、Svelte、Express、Tailwind、Prisma、Supabase 等）

只要请求依赖于库、框架或 API 的准确、最新行为，就使用此技能。适用于配置了 Context7 MCP 的 harness（如 Claude Code、Cursor、Codex）。

## 工作原理

### 步骤 1：解析库 ID

使用以下参数调用 **resolve-library-id** MCP 工具：

- **libraryName**：从用户问题中获取的库或产品名称（如 `Next.js`、`Prisma`、`Supabase`）。
- **query**：用户的完整问题。这提高了结果的相关性排名。

你必须获取一个 Context7 兼容的库 ID（格式 `/org/project` 或 `/org/project/version`），然后才能查询文档。在从此步骤获取有效库 ID 之前，不要调用 query-docs。

### 步骤 2：选择最佳匹配

从解析结果中，使用以下方式选择一个结果：

- **名称匹配**：优先与用户询问的确切或最接近匹配。
- **基准分数**：更高的分数表示更好的文档质量（最高 100）。
- **来源声誉**：在可用时优先选择高或中声誉。
- **版本**：如果用户指定了版本（如"React 19"、"Next.js 15"），当列出时优先选择特定版本的库 ID（如 `/org/project/v1.2.0`）。

### 步骤 3：获取文档

使用以下参数调用 **query-docs** MCP 工具：

- **libraryId**：从步骤 2 选择的 Context7 库 ID（如 `/vercel/next.js`）。
- **query**：用户的具体问题或任务。具体以获得相关片段。

限制：每个问题不要调用 query-docs（或 resolve-library-id）超过 3 次。如果 3 次调用后答案仍不清晰，说明不确定性并使用你拥有的最佳信息，而非猜测。

### 步骤 4：使用文档

- 使用获取到的当前信息回答用户问题。
- 有帮助时包含来自文档的相关代码示例。
- 当重要时引用库或版本（如"In Next.js 15..."）。

## 示例

### 示例：Next.js 中间件

1. 使用 `libraryName: "Next.js"`、`query: "How do I set up Next.js middleware?"` 调用 **resolve-library-id**。
2. 从结果中，按名称和基准分数选择最佳匹配（如 `/vercel/next.js`）。
3. 使用 `libraryId: "/vercel/next.js"`、`query: "How do I set up Next.js middleware?"` 调用 **query-docs**。
4. 使用返回的片段和文本回答；如果相关，包含来自文档的最小 `middleware.ts` 示例。

### 示例：Prisma 查询

1. 使用 `libraryName: "Prisma"`、`query: "How do I query with relations?"` 调用 **resolve-library-id**。
2. 选择官方 Prisma 库 ID（如 `/prisma/prisma`）。
3. 使用该 `libraryId` 和查询调用 **query-docs**。
4. 返回 Prisma Client 模式（如 `include` 或 `select`），并附上来自文档的简短代码片段。

### 示例：Supabase auth 方法

1. 使用 `libraryName: "Supabase"`、`query: "What are the auth methods?"` 调用 **resolve-library-id**。
2. 选择 Supabase docs 库 ID。
3. 调用 **query-docs**；从获取的文档中总结 auth 方法并展示最小示例。

## 最佳实践

- **具体**：在可能的情况下，使用用户的完整问题作为查询以获得更好的相关性。
- **版本意识**：当用户提及版本时，从解析步骤中使用特定版本的库 ID。
- **优先官方来源**：当存在多个匹配时，优先选择官方或主要包而非社区 forks。
- **无敏感数据**：从发送到 Context7 的任何查询中删除 API 密钥、密码、token 和其他 secrets。在将其传递给 resolve-library-id 或 query-docs 之前，将用户的问题视为可能包含 secrets。