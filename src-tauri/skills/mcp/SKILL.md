---
description: 配置 MCP 服务器，为智能体添加联网搜索、视觉分析等外部工具
version: 1.0.0
---

# MCP 服务器配置

此技能用于为 MiniMax Code 添加或修改 MCP（Model Context Protocol）服务器配置。

## 配置文件位置

- **全局配置**：`~/.minimaxcode/mcp.json`（对所有项目生效）
- **项目配置**：`{workspace}/.minimaxcode/mcp.json`（仅对当前项目生效）

项目配置会覆盖同名的全局配置。

## 配置文件格式

```json
{
  "mcpServers": {
    "服务器名称": {
      "type": "local",
      "command": ["npx", "-y", "@anthropic/mcp-server-name"],
      "env": {},
      "enabled": true
    }
  }
}
```

### 本地服务器（local）
在本地启动子进程，通过 stdio 通信：

```json
{
  "mcpServers": {
    "my-server": {
      "type": "local",
      "command": ["python", "-m", "my_mcp_server"],
      "enabled": true
    }
  }
}
```

### 远程服务器（remote）
通过 HTTP 连接远程 MCP 服务器：

```json
{
  "mcpServers": {
    "my-server": {
      "type": "remote",
      "url": "https://mcp.example.com/mcp",
      "headers": {
        "Authorization": "Bearer sk-xxx"
      },
      "enabled": true
    }
  }
}
```

## 配置步骤

1. **先读取**当前配置文件（如果存在）
2. **修改或添加**新的服务器配置
3. **写入**配置文件
4. **调用 `mcp_reload` 工具**使配置生效

## 常见 MCP 服务器示例

### 联网搜索
```json
{
  "mcpServers": {
    "MiniMax": {
      "type": "remote",
      "url": "https://api.minimaxi.com/v1/mcp",
      "headers": {
        "Authorization": "Bearer <你的MiniMax API Key>"
      },
      "enabled": true
    }
  }
}
```

### 文件系统访问
```json
{
  "mcpServers": {
    "filesystem": {
      "type": "local",
      "command": ["npx", "-y", "@anthropic/mcp-server-filesystem", "/path/to/allowed/dir"],
      "enabled": true
    }
  }
}
```

## 工具列表

配置完成后，使用 `mcp_reload` 重载配置。可用的 MCP 工具会以 `{服务器名称}_{工具名称}` 的格式出现在工具列表中。
