---
description: 外部技能的正确存放位置——三平台路径速查
version: 2.0.0
---

# 外部技能存放位置

## 核心规则

**默认放全局。** 仅当用户明确要求"项目级"或"仅当前项目"才放项目目录。

## 全局（~/.minimaxcode/skills/）

| 平台 | 路径 |
|------|------|
| Windows | `C:\Users\<用户名>\.minimaxcode\skills\<技能名>\` |
| macOS | `/Users/<用户名>/.minimaxcode/skills/<技能名>/` |
| Linux | `/home/<用户名>/.minimaxcode/skills/<技能名>/` |

用户名可通过 `get_env_info` 或 `echo $env:USERNAME`(Windows) / `whoami`(macOS/Linux) 获取。

## 项目级（仅用户明确要求时）

```
<当前工作目录>\.minimaxcode\skills\<技能名>\
```

## 示例

用户说"创建 Python 审查技能" → 默认全局，Windows 上：

```
create_directory C:\Users\Admin\.minimaxcode\skills\python-review\
```

不是项目级，不要放工作目录下。
