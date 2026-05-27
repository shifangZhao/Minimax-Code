---
description: 创建项目级 minimax.md 规范文件——定义代码风格、架构约定、工作流程
version: 1.0.0
---

# minimax.md 项目规范文件

## 作用

`minimax.md` 是项目级指令文件，AI Agent 启动时自动加载。用于定义：
- 代码风格和命名规范
- 项目架构和技术栈
- Git 提交规范
- 特定模块的注意事项

## 文件位置

```
<工作目录>/minimax.md
```

## 推荐模板

```markdown
# 项目名称

## 技术栈
- 框架：Vue 3 / React / Next.js 等
- 语言：TypeScript / JavaScript
- 样式：Tailwind CSS / CSS Modules 等

## 代码规范
- 组件命名：PascalCase
- 函数命名：camelCase
- 文件命名：kebab-case

## 目录结构
src/
├── components/   # 可复用组件
├── views/        # 页面组件
├── composables/  # 组合式函数
├── services/     # API 服务
└── utils/        # 工具函数

## Git 规范
- commit 格式：type: 描述
- type：feat / fix / refactor / docs / style / test

## 注意事项
- （项目特定的规则）
```

## 操作步骤

1. 询问用户项目名称和主要技术栈
2. 根据用户回答生成 minimax.md 内容
3. 写入 `<工作目录>/minimax.md`
4. 提示用户：后续对话 AI 会自动加载此文件

## 何时建议创建

- 用户首次在新项目中工作时
- 用户提到"规范"、"约定"、"风格"时
- 项目结构复杂需要说明时
