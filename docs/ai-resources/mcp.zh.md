---
title: MCP
icon: Plug
description: 从支持 MCP 的编辑器与 agent 中访问 BitRouter 文档。
sourceHash: 99ad2a6cef613d6b834c7f26c3131ca3e38cf250adc0a47e456082ad95adc5ab
---

BitRouter 提供了一个公开的 [Model Context Protocol](https://modelcontextprotocol.io)（MCP）服务器，让你的编辑器或 agent 无需离开工作流即可搜索文档、读取页面并查询模型可用性。

- **Endpoint** — `https://bitrouter.ai/mcp`（Streamable HTTP）
- **认证** — 无；完全公开。

## 工具

- **`search_docs(query, limit?, locale?)`** — 全文搜索文档，返回带标题、规范 URL 以及可传给 `get_doc` 的 `path` 的结果。
- **`get_doc(path, locale?)`** — 以干净的 Markdown 获取单个文档页面。
- **`lookup_model(query)`** — 检查某个 `provider/model` 是否可通过 BitRouter 路由，并返回其价格、上下文窗口以及可直接复制的配置片段。

支持读取 MCP resource 的客户端还可以通过 `bitrouter-docs://llms-index` resource 获取精选索引。

## 接入你的 agent

### Claude Code

```bash
claude mcp add --transport http bitrouter-docs https://bitrouter.ai/mcp
```

### Cursor 及其他客户端

在客户端配置中添加一个远程 MCP 服务器：

```json
{
  "mcpServers": {
    "bitrouter-docs": { "url": "https://bitrouter.ai/mcp" }
  }
}
```

<Callout>更想把文档直接喂进上下文，而不是调用工具？参见 [llms.txt](/docs/ai-resources/llms-txt)。</Callout>
