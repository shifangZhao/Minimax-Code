---
description: 如何创建和配置外部自定义技能——全局技能放在 C:\Users\Admin\.minimaxcode\skills\，项目技能放在 <项目根>\.minimaxcode\skills\
version: 1.0.0
---

# 配置外部技能

用户可以创建自定义技能来扩展助手的能力。技能本质是一个包含 `SKILL.md` 文件的目录。

## 技能存放位置

### 全局技能（所有项目可用）
```
C:\Users\Admin\.minimaxcode\skills\<技能名称>\
```

### 项目级技能（仅当前项目可用）
```
<项目根目录>\.minimaxcode\skills\<技能名称>\
```

## 创建步骤

1. 在对应位置创建技能目录，目录名即为技能名称（中文或英文均可）
2. 在目录内创建 `SKILL.md` 文件
3. 可选：创建 `scripts/` 子目录存放脚本（.py/.sh/.js），创建 `references/` 子目录存放参考资料

## SKILL.md 格式

```markdown
---
description: 技能的简要描述（一句话说明用途）
version: 1.0.0
---

# 技能标题

此处写技能的完整操作指令。可以是：
- 特定领域的知识和规范
- 项目特有的操作流程
- 代码生成模板
- 配置说明
- 任何希望助手遵循的指导

内容越详细越好，助手会严格遵循这里的指令。
```

## 示例

创建一个名为 `api规范` 的技能，放在 `C:\Users\Admin\.minimaxcode\skills\api规范\SKILL.md`：

```markdown
---
description: 项目 API 设计规范和命名约定
version: 1.0.0
---

# API 设计规范

## 命名约定
- REST 端点使用复数名词：`/users`、`/orders`
- 版本号放在 URL 路径：`/api/v1/users`
- 查询参数使用 snake_case：`?page_size=20`

## 响应格式
所有响应统一包装：
{
  "code": 0,
  "data": ...,
  "message": "ok"
}

## 错误码
- 0: 成功
- 1001: 参数错误
- 1002: 未授权
- 1003: 资源不存在
```

## 生效方式

技能创建后无需重启应用。下次对话时系统会自动匹配相关技能，助手也可以主动调用 `skill` 工具加载。
