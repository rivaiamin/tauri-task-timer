# Task Timer — API & AI-control guide

How to drive the web app's timer programmatically — from scripts, curl, or an AI
agent. For the *why* (architecture/design), see [`ai-control.md`](./ai-control.md).

- [Authentication](#authentication)
- [Conventions](#conventions)
- [The task object](#the-task-object)
- [Endpoints](#endpoints)
- [Live updates (SSE)](#live-updates-sse)
- [Quickstart (curl)](#quickstart-curl)
- [Driving it from an AI agent (MCP)](#driving-it-from-an-ai-agent-mcp)

## Authentication

Every `/api/*` request is authenticated one of two ways:

| Caller | How | Notes |
|---|---|---|
| The app's browser | **Session cookie** (set at login) | Sent automatically. |
| Scripts / AI agents | **`Authorization: Bearer <API key>`** | Mint a key in the UI. |

**Mint an API key:** sign in and go to **`/dashboard/keys`** → *Create key*. The
raw key (`sk_live_…`) is shown **once** — copy it; only its hash is stored. Keys
can be revoked there at any time.

Key-management endpoints (`/api/keys*`) require a **logged-in session** — an API
key cannot mint or list other keys.

## Conventions

- **Base URL:** your web app origin, e.g. `http://localhost:4320`. All paths below
  are relative to it.
- **Bodies & responses:** JSON.
- **Errors:** non-2xx responses return `{ "message": "…" }`. Common codes: `400`
  (bad input), `401` (missing/invalid auth), `403` (missing scope / not a
  session), `404` (task/key not found).
- **Scopes:** keys currently carry `tasks:read` + `tasks:write`.

## The task object

```jsonc
{
  "id": 42,
  "label": "Write RFC",
  "description": null,
  "position": 0,
  "isRunning": true,
  "startTime": 1784354591461,   // epoch ms of the current run, or null
  "elapsedSeconds": 120,        // accumulated (saved) seconds
  "currentElapsedSeconds": 135  // accumulated + live time while running
}
```

List responses wrap tasks with a total:

```jsonc
{ "tasks": [ /* Task[] */ ], "totalElapsedSeconds": 255 }
```

## Endpoints

### Tasks

| Method | Path | Scope | Body | Returns |
|---|---|---|---|---|
| GET | `/api/tasks` | read | — | `{ tasks, totalElapsedSeconds }` |
| POST | `/api/tasks` | write | `{ label, description? }` | Task (201) |
| PATCH | `/api/tasks/:id` | write | `{ label?, description?, elapsed_seconds? }` | Task |
| DELETE | `/api/tasks/:id` | write | — | `204` |
| POST | `/api/tasks/reorder` | write | `{ ids: number[] }` (all ids in new order) | `{ tasks, … }` |

### Timer control

| Method | Path | Scope | Body | Returns |
|---|---|---|---|---|
| POST | `/api/tasks/:id/start` | write | `{ exclusive? }` | Task |
| POST | `/api/tasks/:id/stop` | write | — | Task |
| POST | `/api/tasks/:id/reset` | write | — | Task |
| POST | `/api/tasks/reset-all` | write | — | `{ tasks, … }` |

`exclusive` on **start**: `true` stops all other running timers first (focus);
`false` leaves them running (parallel). Omit it to use the user's saved timer mode.

### Settings

| Method | Path | Scope | Body | Returns |
|---|---|---|---|---|
| GET | `/api/settings/timer-mode` | read | — | `{ timer_mode }` |
| PUT | `/api/settings/timer-mode` | write | `{ timer_mode: "focus" \| "parallel" }` | `{ timer_mode }` |

### Reports

| Method | Path | Scope | Returns |
|---|---|---|---|
| GET | `/api/report?format=markdown\|csv` | read | raw report **text** (`text/markdown` or `text/csv`) |

A "Daily Report" over each task's **current** elapsed time (dated today, UTC).
`markdown` is the Google-Chat-friendly format (bold header; only tasks with
recorded time; `- [storyPoints] label - description`); `csv` has a
`Task, Description, Story Points` header row for all tasks. Story points =
seconds / 3600. Unlike the other endpoints this returns raw text, not JSON.

```bash
curl -H "Authorization: Bearer $KEY" "$BASE/api/report?format=markdown"
```

### API keys (session only)

| Method | Path | Body | Returns |
|---|---|---|---|
| GET | `/api/keys` | — | `{ keys: [...] }` (no hashes) |
| POST | `/api/keys` | `{ name? }` | `{ id, key, keyPrefix, name }` — `key` shown once (201) |
| DELETE | `/api/keys/:id` | — | `204` |

## Live updates (SSE)

`GET /api/stream` is a [Server-Sent Events](https://developer.mozilla.org/docs/Web/API/Server-sent_events)
stream (authenticated by session cookie). It emits `{"type":"connected"}` on
open and `{"type":"tasks-changed"}` whenever any of the user's tasks change —
from this browser, another device, **or an agent**. The dashboard uses it to stay
live; on `tasks-changed` it refetches `/api/tasks`.

```js
const es = new EventSource('/api/stream');
es.onmessage = (e) => {
  if (JSON.parse(e.data).type === 'tasks-changed') refetchTasks();
};
```

## Quickstart (curl)

```bash
KEY=sk_live_...                     # from /dashboard/keys
BASE=http://localhost:4320

# list
curl -H "Authorization: Bearer $KEY" $BASE/api/tasks

# create, then start it exclusively
ID=$(curl -s -H "Authorization: Bearer $KEY" -H 'content-type: application/json' \
  -d '{"label":"Write RFC"}' $BASE/api/tasks | jq .id)
curl -X POST -H "Authorization: Bearer $KEY" -H 'content-type: application/json' \
  -d '{"exclusive":true}' $BASE/api/tasks/$ID/start

# stop it
curl -X POST -H "Authorization: Bearer $KEY" $BASE/api/tasks/$ID/stop
```

## Driving it from an AI agent (MCP)

For Claude Desktop / Claude Code and other MCP clients, use the bundled **MCP
server** (`apps/mcp`) instead of raw HTTP — it exposes the same operations as
tools (`start_timer`, `create_task`, …). It runs over **stdio** (local) or
**Streamable HTTP** (remote). Setup and config: **[`apps/mcp/README.md`](../apps/mcp/README.md)**.
