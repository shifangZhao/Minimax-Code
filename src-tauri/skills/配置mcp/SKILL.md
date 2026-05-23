---
description: 配置 MCP 服务器——配置文件位置与格式
version: 2.0.0
---

# MCP 服务器配置

## 配置文件位置

| 级别 | 路径 |
|------|------|
| 全局 | `~/.minimaxcode/mcp.json` |
| 项目 | `<工作目录>/.minimaxcode/mcp.json` |

项目配置覆盖同名的全局配置。

## 文件格式

```json
{
  "mcpServers": {
    "<服务器名>": {
      "type": "local",
      "command": ["<可执行文件>", "<参数>", "..."],
      "enabled": true
    }
  }
}
```

**local** — 启动本地子进程，通过 stdio 通信：
```json
{ "type": "local", "command": ["python", "-m", "my_server"], "enabled": true }
```

**remote** — HTTP 连接远程服务器：
```json
{ "type": "remote", "url": "https://mcp.example.com", "headers": { "Authorization": "Bearer sk-xxx" }, "enabled": true }
```

## 操作步骤

1. `read_file` 当前配置文件（如果存在）
2. 修改/添加服务器配置
3. `write_file` 写入
4. 调 `mcp_reload` 使生效
