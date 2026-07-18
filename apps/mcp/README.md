# task-timer-mcp

A [Model Context Protocol](https://modelcontextprotocol.io/) server that lets AI
agents (Claude Desktop, Claude Code, etc.) drive the Task Timer. It's a thin
stdio wrapper over the web app's REST API — every tool maps to an `/api/*`
endpoint and authenticates with a per-user API key.

## Setup

1. Run the web app and mint an API key at **`/dashboard/keys`** (copy it once).
2. Build this server:
   ```bash
   pnpm --filter task-timer-mcp build
   ```
3. Point your MCP client at `dist/index.js` with two env vars:
   - `BASE_URL` — the web app's origin (e.g. `http://localhost:4320`)
   - `API_KEY` — the `sk_live_…` key you minted

### Claude Desktop (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "task-timer": {
      "command": "node",
      "args": ["/absolute/path/to/apps/mcp/dist/index.js"],
      "env": {
        "BASE_URL": "http://localhost:4320",
        "API_KEY": "sk_live_your_key_here"
      }
    }
  }
}
```

### Claude Code

```bash
claude mcp add task-timer \
  --env BASE_URL=http://localhost:4320 \
  --env API_KEY=sk_live_your_key_here \
  -- node /absolute/path/to/apps/mcp/dist/index.js
```

## Tools

`list_tasks`, `create_task`, `start_timer`, `stop_timer`, `reset_task`,
`reset_all`, `update_task`, `delete_task`, `reorder_tasks`, `get_timer_mode`,
`set_timer_mode`.

Because the web app pushes changes over SSE, anything an agent does here shows up
live in any open dashboard.

## Notes

- stdio transport only (local agents). A remote Streamable-HTTP transport is a
  possible follow-up.
- The key carries `tasks:read` + `tasks:write`; it cannot manage other keys.
  Revoke it any time at `/dashboard/keys`.
