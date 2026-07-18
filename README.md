# Task Timer (Monorepo)

Task Timer is a time-tracking app for working on multiple tasks at once. Only one
timer runs at a time to keep tracking focused. The repo is a pnpm workspace with
two independent apps and a shared package.

- **Browser app** (`apps/desktop`) — standalone timer, no account required.
  Vanilla HTML/CSS/JS, persists to `localStorage`, deployed as a static site to
  GitHub Pages. (Originally a Tauri desktop app — see [Desktop / Tauri note](#desktop--tauri-note).)
- **Web app** (`apps/web`) — SvelteKit app with accounts and live updates,
  backed by a **local SQLite** database. Exposes a REST + SSE API that both the
  dashboard and AI agents can drive (see `docs/ai-control.md`).
- **MCP server** (`apps/mcp`) — a Model Context Protocol server (stdio) that lets
  AI agents (Claude Desktop / Claude Code) drive the timer via the web API.
- **Shared package** (`packages/shared`) — TypeScript types and utilities used by
  both apps.

## Repository layout

```text
apps/
  desktop/          # standalone browser timer (vanilla JS, localStorage)
    src/            # index.html, main.js, styles.css
    build.js        # copies src/ -> dist/ for static hosting
  web/              # SvelteKit + local SQLite web app
    src/            # routes (+ api/*), lib/server (db, auth, taskService)
    drizzle/        # generated SQLite migrations
  mcp/              # MCP server (stdio) wrapping the web API for AI agents
packages/
  shared/           # shared TS types (Task, DatabaseTask) + utils
    src/            #   formatTime, escapeHTML
migration.md        # notes from the Tauri -> monorepo/web migration
.github/workflows/  # deploy.yml: builds apps/desktop -> GitHub Pages
```

## Apps

### Browser app (`apps/desktop`, package `task-timer-desktop`)

A single-file-per-concern static app — no build framework, no backend.

- Stack: vanilla HTML/CSS/JS, [Tailwind](https://tailwindcss.com/) and
  [SweetAlert2](https://sweetalert2.github.io/) loaded from CDN.
- Persistence: browser `localStorage` (no account, single device).
- Features: add/delete tasks, per-task timer (HH:MM:SS), single active timer,
  drag-and-drop reordering, inline edit mode (title, description, elapsed time),
  reset all, CSV export, Markdown export (copied to clipboard), light/dark mode.
- Build: `node build.js` copies `src/` into `dist/` for static hosting.

### Web app (`apps/web`, package `sv-task-timer`)

- Stack: [SvelteKit](https://svelte.dev/docs/kit) + [Vite](https://vite.dev/),
  Tailwind v4, TypeScript, [Zod](https://zod.dev/). Runs on `adapter-node`.
- Backend: **local SQLite** via [Drizzle ORM](https://orm.drizzle.team/) +
  `better-sqlite3`. Session-cookie auth (argon2 + a `sessions` table). All data
  access goes through the server; migrations live in `apps/web/drizzle/`.
- API: a REST surface under `/api/*` (tasks CRUD, start/stop/reset/reorder,
  timer-mode, key management) plus live updates over SSE at `/api/stream`.
  Requests authenticate by **session cookie** (the browser) **or `Bearer` API
  key** (AI agents). See `docs/ai-control.md`.
- Routes: `/` (redirects based on session), `/login`, `/register`,
  `/dashboard` (the timer UI).

### Shared package (`packages/shared`, package `shared`)

Consumed by both apps via `"shared": "workspace:*"`. Exposes the `Task` /
`DatabaseTask` types and the `formatTime` and `escapeHTML` helpers. Source is
imported directly (no build step).

## Prerequisites

- Node.js (workflow uses Node 20)
- pnpm

## Install

From the repo root:

```bash
pnpm install
```

## Run (development)

### Web

```bash
pnpm --filter sv-task-timer db:migrate   # create/update the local SQLite db
pnpm --filter sv-task-timer dev
```

Config is optional (see [Environment](#environment)); the database defaults to
`apps/web/local.db`.

### Browser app

The app is static HTML — open `apps/desktop/src/index.html` directly, or build
and serve the bundle:

```bash
pnpm --filter task-timer-desktop build      # copies src/ -> dist/
pnpm --filter task-timer-desktop preview    # build + serve dist/ via http-server
```

> Note: the root `package.json` convenience scripts (`dev:desktop`, `dev:web`,
> `build:*`) currently filter by `desktop`/`web`, which do not match the package
> names above and will not resolve. Use the `--filter <package-name>` commands in
> this README until those scripts are updated.

## Build

```bash
pnpm --filter sv-task-timer build           # web -> apps/web/build
pnpm --filter task-timer-desktop build      # browser app -> apps/desktop/dist
```

## Deployment

`.github/workflows/deploy.yml` runs on pushes to `main`: it builds
`apps/desktop` with `node build.js` and publishes `apps/desktop/dist` to GitHub
Pages. The web app is deployed separately as a persistent Node server
(SvelteKit `adapter-node`), since it uses a local SQLite file via the native
`better-sqlite3` driver. Run `db:migrate` as part of the deploy.

## Environment

The web app reads optional settings from `apps/web/.env` (template in
`apps/web/.env.example`):

```env
# Path to the local SQLite database file (optional; defaults to local.db)
DATABASE_PATH=local.db
```

## Desktop / Tauri note

This project started as a [Tauri](https://tauri.app/) desktop app; the browser
app still carries Tauri-compatible markup (`data-tauri-drag-region`,
`window.__TAURI__` guards) and `task-timer-desktop` still lists `@tauri-apps/cli`.
The Rust/Tauri backend (`apps/desktop/src-tauri/`) has been **removed**, so
`tauri dev` / `tauri build` will not run as-is — the app ships as a static web
build to GitHub Pages. Restore `src-tauri/` (or check out a commit before its
removal) to build a native desktop binary again.

## More docs

- Browser/desktop app details: `apps/desktop/README.md`
- Web app details: `apps/web/README.md`
- Architecture/migration notes: `migration.md`
