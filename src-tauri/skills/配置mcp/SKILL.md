---
description: 配置 MCP 服务器——配置文件位置与格式
version: 2.2.0
---

# MCP 服务器配置

## 核心规则

**默认放全局。** 仅当用户明确要求"项目级"或"仅当前项目"才放项目目录。

**API key 等敏感值必须向用户索取真实值**，绝不写入占位符（如 `MINIMAX_API_KEY`、`YOUR_KEY_HERE`）。

## 配置文件位置

| 级别 | 路径 |
|------|------|
| 全局 | `~/.minimaxcode/mcp.json` |
| 项目 | `<工作目录>/.minimaxcode/mcp.json` |

## 文件格式

```json
{
  "mcp": {
    "<服务器名>": {
      "type": "local",
      "command": ["<可执行文件>", "<参数1>", "<参数2>"]
    }
  }
}
```

**本地进程**：
```json
{ "type": "local", "command": ["python", "-m", "my_server"] }
```

**远程 HTTP**：
```json
{ "type": "remote", "url": "https://mcp.example.com", "headers": { "Authorization": "Bearer sk-xxx" } }
```

**环境变量**：
```json
{ "type": "local", "command": ["uvx", "my-mcp"], "environment": { "API_KEY": "sk-xxx" } }
```

每个服务器都有 `enabled` 开关——`true` 启用，`false` 禁用。不写默认启用。用户要求"关掉""停用"时设为 `false` 即可，不必删除配置。

## 操作步骤

1. **先判断类型**：有 `command` 字段（启动本地进程）→ `"type": "local"`；只有 `url`（连远程 HTTP）→ `"type": "remote"`。绝不给本地进程配 remote 类型。
2. `read_file` 当前配置文件（如果存在）
3. 修改/添加服务器配置
4. `write_file` 写入
5. 调 `mcp_reload` 检查状态
