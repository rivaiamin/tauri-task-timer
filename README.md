# Task Timer (Monorepo)

Task Timer is a time-tracking app for working on multiple tasks at once. Only one
timer runs at a time to keep tracking focused. The repo is a pnpm workspace with
two independent apps and a shared package.

- **Browser app** (`apps/desktop`) — standalone timer, no account required.
  Vanilla HTML/CSS/JS, persists to `localStorage`, deployed as a static site to
  GitHub Pages. (Originally a Tauri desktop app — see [Desktop / Tauri note](#desktop--tauri-note).)
- **Web app** (`apps/web`) — SvelteKit app with accounts, cloud sync, and
  real-time updates backed by Supabase.
- **Shared package** (`packages/shared`) — TypeScript types and utilities used by
  both apps.

## Repository layout

```text
apps/
  desktop/          # standalone browser timer (vanilla JS, localStorage)
    src/            # index.html, main.js, styles.css
    build.js        # copies src/ -> dist/ for static hosting
  web/              # SvelteKit + Supabase web app
    src/            # routes, lib (auth, supabase client, encryption)
    supabase/       # SQL migrations (schema, RLS, functions)
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
  Tailwind v4, TypeScript, [Zod](https://zod.dev/).
- Backend: [Supabase](https://supabase.com/) — auth, Postgres, and real-time
  task sync. Row-Level Security and helper functions live in
  `apps/web/supabase/migrations/`.
- Routes: `/` (redirects based on session), `/login`, `/register`,
  `/dashboard` (the timer UI with realtime sync).
- Extras: server-side encrypted webhooks and shareable view links (see the
  schema and `src/lib/encryption/`).

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
pnpm --filter sv-task-timer dev
```

Requires a `.env` in `apps/web` (see [Environment](#environment)).

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
Pages. The web app is deployed separately (SvelteKit adapter-auto).

## Environment

The web app reads Supabase and encryption settings from `apps/web/.env`
(template in `apps/web/.env.example`):

```env
PUBLIC_SUPABASE_URL=
PUBLIC_SUPABASE_PUBLISHABLE_KEY=
SUPABASE_SERVICE_ROLE_KEY=
ENCRYPTION_KEY=
```

## Desktop / Tauri note

This project started as a [Tauri](https://tauri.app/) desktop app; the browser
app still carries Tauri-compatible markup (`data-tauri-drag-region`,
`window.__TAURI__` guards) and `task-timer-desktop` still lists `@tauri-apps/cli`.
The Rust/Tauri backend (`apps/desktop/src-tauri/`) is **not present in the
current working tree**, so `tauri dev` / `tauri build` will not run as-is — the
app currently ships as a static web build to GitHub Pages. Restore `src-tauri/`
(or check out an earlier commit) to build a native desktop binary again.

## More docs

- Browser/desktop app details: `apps/desktop/README.md`
- Web app details: `apps/web/README.md`
- Architecture/migration notes: `migration.md`
