# sv-task-timer

The Task Timer web app — SvelteKit dashboard with accounts, local SQLite backend,
REST + SSE API, and optional JIRA sync. Part of the [monorepo](../README.md).

## What it does

- **Dashboard** (`/dashboard`) — add/reorder tasks, start/stop timers (one active
  at a time), mark tasks done, export reports.
- **Auth** — register/login with session cookies (argon2 + SQLite `sessions` table).
- **API** — same operations under `/api/*`, authenticated by session or
  `Authorization: Bearer <API key>`. Live updates via SSE at `/api/stream`.
- **API keys** — mint/revoke at `/dashboard/keys` for scripts and AI agents.
- **JIRA sync** (optional) — when configured, timer events update linked issues
  (key parsed from task label/description). See
  [`../../docs/api.md`](../../docs/api.md#jira-sync-optional).

Full API reference: [`../../docs/api.md`](../../docs/api.md).  
AI-control design: [`../../docs/ai-control.md`](../../docs/ai-control.md).  
MCP server: [`../mcp/README.md`](../mcp/README.md).

## Stack

SvelteKit 2 + Svelte 5, Vite, Tailwind v4, TypeScript, Zod. Runs on
`adapter-node`. SQLite via Drizzle ORM + `better-sqlite3`.

## Develop

From the repo root:

```bash
pnpm install
pnpm --filter sv-task-timer db:migrate   # create/update local.db
pnpm --filter sv-task-timer dev           # http://localhost:4320
```

## Build

```bash
pnpm --filter sv-task-timer build         # -> apps/web/build
pnpm --filter sv-task-timer preview       # preview production build
```

Run `db:migrate` before starting a fresh production build — the SQLite file is
not bundled into the app output.

## Database

- Default path: `apps/web/local.db` (override with `DATABASE_PATH`).
- Migrations: `apps/web/drizzle/` — generated with `db:generate`, applied with
  `db:migrate`.
- Inspect locally: `pnpm --filter sv-task-timer db:studio`.

## Environment

Copy `apps/web/.env.example` to `apps/web/.env`. All settings are optional.

| Variable | Default | Purpose |
|----------|---------|---------|
| `DATABASE_PATH` | `local.db` | SQLite file path |
| `JIRA_EMAIL` | — | JIRA account email (with token, enables sync) |
| `JIRA_TOKEN` | — | JIRA API token |
| `JIRA_SITE` | `https://aimsis.atlassian.net` | JIRA Cloud site URL |
| `JIRA_ENV_FILE` | `~/.aimsis/jira.env` | Fallback file for email + token |
| `JIRA_STATUS_TODO` | `To Do` | Status on focus-switch |
| `JIRA_STATUS_INPROGRESS` | `In Progress` | Status on timer start |
| `JIRA_STATUS_DONE` | `Cek lokal` | Status when marking task done |

Without JIRA credentials, sync hooks are no-ops — the timer works normally.

## Routes

| Path | Description |
|------|-------------|
| `/` | Redirects to dashboard or login |
| `/login`, `/register` | Auth |
| `/dashboard` | Timer UI |
| `/dashboard/keys` | API key management |
| `/api/*` | REST API (see docs) |

## Scripts

| Script | Description |
|--------|-------------|
| `dev` | Vite dev server |
| `build` | Production build |
| `check` | `svelte-check` |
| `db:generate` | Generate migration from schema changes |
| `db:migrate` | Apply migrations |
| `db:studio` | Drizzle Studio |
