---
description: 创建外部技能的标准流程——默认放到全局 ~/.minimaxcode/skills/
version: 1.2.0
---

# 创建外部技能

## 核心规则

**默认一律放全局目录。**
除非用户明确说了"仅这个项目"、"项目级"、"放在项目里"才放项目目录。

## 路径速查

**全局（默认）：**

| 平台 | 路径 |
|------|------|
| Windows | `C:\Users\<用户名>\.minimaxcode\skills\<技能名>\` |
| macOS | `/Users/<用户名>/.minimaxcode/skills/<技能名>/` |
| Linux | `/home/<用户名>/.minimaxcode/skills/<技能名>/` |

简写为 `~/.minimaxcode/skills/<技能名>/`，用 `get_env_info` 可查当前用户名。

**项目级（仅当用户明确要求）：**
```
<当前工作目录>\.minimaxcode\skills\<技能名>\
```

## 第一步：创建目录

用 `create_directory` 在全局路径下创建以技能名称命名的目录。

如 Windows：`create_directory C:\Users\<用户名>\.minimaxcode\skills\my-skill\`

## 第二步：写 SKILL.md

用 `write_file` 在刚创建的目录里写入 `SKILL.md`，格式：

```markdown
---
description: 一句话说明这个技能做什么（这是匹配依据，必填）
version: 1.0.0
---

# <技能标题>

<详细的操作指令、规范、模板、注意事项等>
```

## 第三步：可选附件

- `scripts/` 目录：放可执行脚本（.py / .sh / .js）
- `references/` 目录：放参考资料

## 完整示例（Windows）

用户说："帮我创建一个 Python 代码审查的技能"

→ 默认放全局。

```
create_directory C:\Users\Admin\.minimaxcode\skills\python-review\

write_file C:\Users\Admin\.minimaxcode\skills\python-review\SKILL.md
```

```markdown
---
description: Python 代码审查规范和常见问题检查清单
version: 1.0.0
---

# Python 代码审查

## 必查项
- 类型注解是否完整
- 异常处理是否吞掉了错误
- 是否有 SQL 注入风险

## 风格
- 遵循 PEP 8，函数不超过 30 行
```

## 生效

创建后即刻生效，无需重启。
