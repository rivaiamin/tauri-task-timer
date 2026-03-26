# Task Timer (Monorepo)

Task Timer is a time-tracking app for working on multiple tasks, with:

- **Desktop app**: Tauri + vanilla HTML/CSS/JS (`apps/desktop`)
- **Web app**: SvelteKit + Vite (`apps/web`)
- **Shared package**: reusable types/utils (`packages/shared`)

## Repository layout

```text
apps/
  desktop/        # Tauri desktop app
  web/            # SvelteKit web app
packages/
  shared/         # shared code used by desktop/web
```

## Prerequisites

- Node.js
- pnpm
- (Desktop only) Rust + system deps for Tauri

For Tauri prerequisites, see the official docs at `https://tauri.app/`.

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

### Desktop (Tauri)

```bash
pnpm --filter task-timer-desktop tauri dev
```

## Build

### Web

```bash
pnpm --filter sv-task-timer build
```

### Desktop

Build the frontend bundle:

```bash
pnpm --filter task-timer-desktop build
```

Build the Tauri app:

```bash
pnpm --filter task-timer-desktop tauri build
```

## More docs

- Desktop app details: `apps/desktop/README.md`
- Web app details: `apps/web/README.md`
- Architecture/migration notes: `migration.md`

