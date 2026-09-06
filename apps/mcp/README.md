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

### Tasks

| Tool | Description |
|------|-------------|
| `list_tasks` | List all tasks with elapsed time and the total |
| `create_task` | Create a new task (see extended fields below) |
| `update_task` | Update label, description, elapsed, status, code, notes, tags, and/or flags |
| `delete_task` | Delete a task |
| `reorder_tasks` | Set task order by giving all task ids in the desired order |

### Timer

| Tool | Description |
|------|-------------|
| `start_timer` | Start a task's timer. Omit `exclusive` to use the user's timer mode |
| `stop_timer` | Stop a task's timer, accumulating elapsed time |
| `reset_task` | Reset a single task's elapsed time to zero |
| `reset_all` | Reset every task to zero |

### Comments

| Tool | Description |
|------|-------------|
| `add_comment` | Attach a comment or delivery reference to a task (subject, summary, branch, pr) |
| `list_comments` | List all comments attached to a task |

### Integrations

| Tool | Description |
|------|-------------|
| `upsert_integration` | Create or update an integration field on a task (e.g. JIRA sprint, Bitbucket branch) |
| `list_integrations` | List all integration fields on a task |

### Settings

| Tool | Description |
|------|-------------|
| `get_timer_mode` | Get the current timer mode (focus \| parallel) |
| `set_timer_mode` | Set the timer mode |
| `get_report` | Get a Daily Report of tracked time (markdown or csv) |

## Extended task fields

`create_task` and `update_task` accept these optional fields:

| Field | Type | Description |
|-------|------|-------------|
| `code` | string \| null | Short code or identifier |
| `link` | string \| null | URL link (e.g. JIRA, PR) |
| `status` | string | Task status (default: `todo`) |
| `notes` | string \| null | Free-form notes |
| `tags` | string[] \| null | Tags for categorization |

`update_task` also supports flag fields: `is_completed`, `is_cancelled`, `is_deleted`, `is_archived`, `is_pinned`, `is_important`.

## SSE live updates

Because the web app pushes changes over SSE, anything an agent does here shows up
live in any open dashboard. The SSE payload now includes the entity type and task
ID:

```json
{"type":"change","entity":"task","taskId":42,"action":"start"}
{"type":"change","entity":"comment","taskId":42}
{"type":"change","entity":"integration","taskId":42}
```

## Notes

- Two transports: **stdio** (local agents) and **Streamable HTTP** (remote,
  `MCP_TRANSPORT=http`).
- The key carries `tasks:read` + `tasks:write`; it cannot manage other keys.
  Revoke it any time at `/dashboard/keys`.
