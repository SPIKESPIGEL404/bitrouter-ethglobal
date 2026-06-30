---
title: MCP
icon: Plug
description: Access BitRouter's documentation from MCP-aware editors and agents.
sourceHash: 99ad2a6cef613d6b834c7f26c3131ca3e38cf250adc0a47e456082ad95adc5ab
---

BitRouter publishes a public [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server so your editor or agent can search the docs, read pages, and check model availability without leaving the loop.

- **Endpoint** — `https://bitrouter.ai/mcp` (Streamable HTTP)
- **Auth** — none; it's public.

## Tools

- **`search_docs(query, limit?, locale?)`** — Full-text search across the docs; returns hits with a title, canonical URL, and a `path` to pass to `get_doc`.
- **`get_doc(path, locale?)`** — Fetch a single documentation page as clean Markdown.
- **`lookup_model(query)`** — Check whether a `provider/model` is routable through BitRouter and get its pricing, context window, and a copy-paste config snippet.

Clients that read MCP resources can also pull the curated index via the `bitrouter-docs://llms-index` resource.

## Add it to your agent

### Claude Code

```bash
claude mcp add --transport http bitrouter-docs https://bitrouter.ai/mcp
```

### Cursor and other clients

Add a remote MCP server entry to your client's config:

```json
{
  "mcpServers": {
    "bitrouter-docs": { "url": "https://bitrouter.ai/mcp" }
  }
}
```

<Callout>Prefer feeding the docs straight into context instead of calling tools? See [llms.txt](/docs/ai-resources/llms-txt).</Callout>
