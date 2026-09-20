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

A task is two things: an **identity** row (`label`, `description`, `code`, `link`,
`notes`, `tags`) keyed by `(user_id, label)`, and one **day row** per date it was
worked, carrying that day's time and lifecycle. `id` is the identity's id and
stays the public handle for every `/api/tasks/:id/*` route. On a dated query the
viewed day is flattened onto the task, so the daily UI reads it as before; `dayId`
is that day row's own id.

```jsonc
{
  "id": 42,                     // task identity id
  "label": "Write RFC",
  "description": null,
  "code": null,
  "link": null,
  "notes": null,
  "tags": null,
  "workDate": "2026-09-21",     // the viewed day; null on an archive query
  "dayId": 907,                 // task_days.id of the viewed day
  "status": "todo",
  "position": 0,
  "isRunning": true,
  "done": false,                // marked finished (moves the JIRA issue to Cek di Local)
  "startTime": 1784354591461,   // epoch ms of the current run, or null
  "elapsedSeconds": 120,        // accumulated (saved) seconds
  "currentElapsedSeconds": 135  // accumulated + live time while running
}
```

List responses wrap tasks with a total:

```jsonc
{ "tasks": [ /* Task[] */ ], "totalElapsedSeconds": 255 }
```

**Archive shape.** `GET /api/tasks` with **no `date`** returns every task — not
just unfinished ones — each carrying a `days[]` array with one entry per day it
was worked (`work_date` ASC):

```jsonc
{
  "tasks": [
    {
      "id": 42,
      "label": "Write RFC",
      "days": [
        // Each entry is a full TaskDayDTO; only the fields the archive UI needs
        // are shown here.
        { "id": 907, "workDate": "2026-09-21", "elapsedSeconds": 120,
          "currentElapsedSeconds": 135, "status": "todo", "done": false, "position": 0 },
        { "id": 812, "workDate": "2026-09-18", "elapsedSeconds": 3600,
          "currentElapsedSeconds": 3600, "status": "done", "done": true, "position": 2 }
      ]
    }
  ],
  "totalElapsedSeconds": 3735
}
```

`totalElapsedSeconds` is the sum of every day's current elapsed. `q` filters the
label, `tag` filters tags, and `status`/`done`/`archived` filter **which days
appear in `days[]`** — a task with no surviving day is dropped.

On an archive task `workDate` and `dayId` are `null`, and the task's own
`elapsedSeconds`/`currentElapsedSeconds`/`totalTime` are the roll-up **sum** of its
`days[]` (so the totals are meaningful without walking the array).

## Endpoints

### Tasks

| Method | Path | Scope | Body | Returns |
|---|---|---|---|---|
| GET | `/api/tasks?date=YYYY-MM-DD` | read | — | `{ tasks, totalElapsedSeconds }` — that day's view |
| GET | `/api/tasks` | read | — | `{ tasks, totalElapsedSeconds }` — archive, each task with `days[]` |
| POST | `/api/tasks` | write | `{ label, description?, workDate?, code?, link?, status?, notes?, tags? }` | Task (201) |
| PATCH | `/api/tasks/:id` | write | identity: `{ label?, description?, code?, link?, notes?, tags? }`; day: `{ elapsed_seconds?, done?, status?, is_*?, workDate? }` | Task |
| DELETE | `/api/tasks/:id` | write | — | `204` |
| POST | `/api/tasks/reorder` | write | `{ ids: number[], workDate? }` (all ids in new order) | `{ tasks, … }` |

`GET` filters: `archived`, `done`, `status`, `q`, `tag` (see the archive shape above).

**`PATCH` splits identity from day.** `label`, `description`, `code`, `link`,
`notes`, `tags` write the task identity; `elapsed_seconds`, `done`, `status` and
the flags write a **day row**. An optional `workDate` selects which day (default
today), so `PATCH` is how you correct one day's time without touching the task's
other days. `DELETE` removes the identity and cascades its days.

### Timer control

| Method | Path | Scope | Body | Returns |
|---|---|---|---|---|
| POST | `/api/tasks/:id/start` | write | `{ exclusive?, workDate? }` | Task |
| POST | `/api/tasks/:id/stop` | write | `{ workDate? }` | Task |
| POST | `/api/tasks/:id/reset` | write | `{ workDate? }` | Task |
| POST | `/api/tasks/reset-all` | write | `{ workDate? }` | `{ tasks, … }` |

`exclusive` on **start**: `true` stops all other running timers first (focus);
`false` leaves them running (parallel). Omit it to use the user's saved timer mode.
Focus mode stops other **running day rows** regardless of date — a running timer is
running whatever day it belongs to.

`workDate` on any timer route defaults to today and picks the day row to mutate.

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

## JIRA sync (optional)

If a JIRA API token is present on the server, timer events drive an AIMSIS JIRA
issue for you — no need to open the (slow) JIRA UI. The issue **key is parsed** from
each task's label or description (first `US-1234` / `AIM-56`-style match); there's no
key field.

| Timer event | JIRA effect |
|---|---|
| **create** a task | auto-fetches JIRA issue title and sets as task description |
| **start** a task | issue → **In Progress**; that day's `status` → **In Progress** |
| **stop** a task | worklog for the run, then issue → **To Do**; that day's `status` → **To Do** |
| focus-mode **switch** (start B, auto-stops A) | A → **To Do** + worklog for A's run; A's day `status` → **To Do** |
| agent hook pause / session end | worklog + **To Do** for every stopped task; each day `status` → **To Do** |
| **reset** / **reset all** | stops the timer; each day `status` → **To Do** |
| mark **`done`** (PATCH `{done:true}`) | stops any live run + final worklog, issue → **Cek di Local**; that day's `status` → **Done** |
| un-mark **`done`** | issue untouched; that day's `status` → **To Do** |

A stopped task only goes back to **To Do** when it is *not* checked done. Checking
it done is the one thing that leaves the issue at the done status, so a stop can
never silently undo a completed ticket.

The **day row's** `status` follows the same events, so the list shows what the
timer is doing without an edit: **start** writes In Progress, **stop**/reset writes
To Do, and **done** writes Done (un-done returns to To Do). Status is per day, so
marking one day done leaves the task's other days alone. Local state is written
before the JIRA call, so the list stays correct while JIRA is slow or unreachable.
All three clients write it — the TUI and CLI via `auto_status`
(`apps/tui/src/jira.rs`), the web dashboard via `autoStatus`
(`apps/web/src/lib/server/taskStatus.ts`). An explicit `status` in a PATCH wins
over the automatic write, matching the TUI's edit form, which picks a status from
the JIRA catalog (`e`, then the status field) and is not auto-corrected.

Details:

- **Credentials:** `JIRA_EMAIL` + `JIRA_TOKEN` (server env, or the file at
  `JIRA_ENV_FILE`, default `~/.aimsis/jira.env`). Site defaults to
  `https://aimsis.atlassian.net` (override `JIRA_SITE`). Status names are
  overridable via `JIRA_STATUS_TODO` / `JIRA_STATUS_INPROGRESS` / `JIRA_STATUS_DONE`.
- **Silent when unconfigured:** with no token, every hook is a no-op — the timer
  works normally and touches nothing.
- **Fire-and-forget:** JIRA calls never block or fail a timer operation; a failed
  call is logged and dropped (no retry). Worklogs under 60s are skipped (JIRA minimum).

## Live updates (SSE)

`GET /api/stream` is a [Server-Sent Events](https://developer.mozilla.org/docs/Web/API/Server-sent_events)
stream (authenticated by session cookie). It emits `{"type":"connected"}` on
open and `{"type":"change", "entity":"task"|"comment"|"integration", "taskId":N}`
whenever any of the user's tasks change — from this browser, another device,
**or an agent**. The dashboard uses it to stay live; on `change` it refetches
`/api/tasks`.

```js
const es = new EventSource('/api/stream');
es.onmessage = (e) => {
  if (JSON.parse(e.data).type === 'change') refetchTasks();
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
