---
description: 外部技能存放位置——直接用 ~ 简化路径
version: 2.1.0
---

# 外部技能存放位置

## 核心规则

**默认放全局。** 仅当用户明确要求"项目级"或"仅当前项目"才放项目目录。

## 全局

```
~/.minimaxcode/skills/<技能名>/
```

`~` 系统自动展开为当前用户主目录，三平台通用。

## 项目级（仅用户明确要求时）

```
<当前工作目录>/.minimaxcode/skills/<技能名>/
```

## 示例

用户说"创建 Python 审查技能" → 默认全局：

```
create_directory ~/.minimaxcode/skills/python-review/
write_file ~/.minimaxcode/skills/python-review/SKILL.md
```
