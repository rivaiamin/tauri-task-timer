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

Copy `.env.example` to `.env`, set `BASE_URL` and `API_KEY`, then:

```bash
make claude-add
```

Or manually:

```bash
claude mcp add task-timer \
  --env BASE_URL=http://localhost:4320 \
  --env API_KEY=sk_live_your_key_here \
  -- node /absolute/path/to/apps/mcp/dist/index.js
```

To re-register: `make claude-remove` then `make claude-add`.

## Remote (Streamable HTTP)

Set `MCP_TRANSPORT=http` to serve the MCP endpoint over HTTP instead of stdio —
for hosted / remote connectors. Each client authenticates with **its own** Task
Timer API key sent as `Authorization: Bearer <key>` (multi-tenant); if a client
sends no key, the server falls back to the `API_KEY` env (single-user hosting).

```bash
MCP_TRANSPORT=http PORT=3010 BASE_URL=http://localhost:4320 \
  pnpm --filter task-timer-mcp start
# -> POST/GET http://127.0.0.1:3010/mcp
```

Env: `PORT` (default 3010), `HOST` (default `127.0.0.1`), `MCP_PATH` (default
`/mcp`). **Security:** an unauthenticated request is rejected (401), but the
endpoint is otherwise unencrypted — put it behind TLS + a trusted network / proxy
before exposing it publicly (the default bind is loopback only).

## Tools

`list_tasks`, `create_task`, `start_timer`, `stop_timer`, `reset_task`,
`reset_all`, `update_task`, `delete_task`, `reorder_tasks`, `get_timer_mode`,
`set_timer_mode`, `get_report`.

Because the web app pushes changes over SSE, anything an agent does here shows up
live in any open dashboard.

## Notes

- Two transports: **stdio** (local agents) and **Streamable HTTP** (remote,
  `MCP_TRANSPORT=http`).
- The key carries `tasks:read` + `tasks:write`; it cannot manage other keys.
  Revoke it any time at `/dashboard/keys`.
