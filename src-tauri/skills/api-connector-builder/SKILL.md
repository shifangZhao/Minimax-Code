---
name: api-connector-builder
description: 通过精确匹配目标仓库的现有集成模式来构建新的 API 连接器或提供者。用于在不使用第二套架构的情况下添加一个集成。
origin: ECC direct-port adaptation
version: "1.0.0"
---

# API 连接器构建器

当工作是为仓库添加本机集成表面，而非仅仅一个通用 HTTP 客户端时使用。

重点是匹配主机仓库的模式：

- 连接器布局
- 配置 schema
- 认证模型
- 错误处理
- 测试风格
- 注册/发现接线

## 使用场景

- "为此项目构建一个 Jira 连接器"
- "按照现有模式添加一个 Slack 提供者"
- "为此 API 创建一个新集成"
- "构建一个匹配仓库连接器风格的插件"

## 护栏

- 当仓库已有集成架构时，不要发明新的集成架构
- 不要仅从供应商文档开始；先从仓库中现有的连接器开始
- 如果仓库期望注册接线、测试和文档，不要止步于传输代码
- 如果仓库有更新的当前模式，不要照搬旧的连接器

## 工作流

### 1. 学习内部风格

检查至少 2 个现有连接器/提供者并映射：

- 文件布局
- 抽象边界
- 配置模型
- 重试/分页约定
- 注册钩子
- 测试固件和命名

### 2. 缩小目标集成

仅定义仓库实际需要的表面：

- 认证流程
- 关键实体
- 核心读写操作
- 分页和速率限制
- Webhook 或轮询模型

### 3. 按仓库原生层构建

典型切片：

- config/schema
- client/transport
- 映射层
- connector/provider 入口点
- 注册
- 测试

### 4. 对照源模式验证

新连接器在代码库中应该看起来很明显，不是从不同生态系统导入的。

## 参考形状

### 提供者风格

```text
providers/
  existing_provider/
    __init__.py
    provider.py
    config.py
```

### 连接器风格

```text
integrations/
  existing/
    client.py
    models.py
    connector.py
```

### TypeScript 插件风格

```text
src/integrations/
  existing/
    index.ts
    client.ts
    types.ts
    test.ts
```

## 质量检查清单

- [ ] 匹配仓库中现有的集成模式
- [ ] 配置验证存在
- [ ] 认证和错误处理是明确的
- [ ] 分页/重试行为遵循仓库规范
- [ ] 注册/发现接线完整
- [ ] 测试镜像仓库的风格
- [ ] 如果仓库期望，更新了文档/示例

## 相关技能

- `backend-patterns`
- `mcp-server-patterns`
- `github-ops`