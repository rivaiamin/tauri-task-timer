# Task Checklist — Task Timer (TUI-led)

Update when shipping epics. Each section names **all apps** so web/MCP are not skipped.

**Legend:** `[x]` done · `[ ]` pending

---

## E1 — Daily Timer MVP

### Schema (`apps/web`)

- [x] Migration `0002`: `work_date`, `done`, indexes, unique `(user_id, work_date, label)`
- [x] `schema.ts` updated

### TUI (`apps/tui`)

- [x] Cargo workspace + crate scaffold
- [x] `config.rs`, `db/`, `timer.rs`, `app/`, `ui/`
- [x] Port taskService timer ops to `db/tasks.rs`
- [x] Keymap + `?` help + modals
- [x] Export markdown report (`report.rs`, `x` key, clipboard)
- [x] Unit tests (timer math, dedup, report)
- [ ] Operator sign-off: elapsed matches web
- [ ] Merge `feat/tui-mvp` → `main`

### Web (`apps/web`)

- [x] Migration only (no daily UI yet — E7)
- [ ] Verify `taskService` still works with `work_date` column

### MCP (`apps/mcp`)

- [ ] Smoke test: list/create/start/stop against DB with `work_date` rows

### Monorepo

- [x] `pnpm dev:tui`, `pnpm build:tui`
- [x] Root README + `apps/tui/README.md`
- [x] `docs/tui/` planning set

---

## E2 — Extended Schema

### Schema (`apps/web`)

- [x] Add task columns (code, link, status, notes, tags, flags, end_time, total_time)
- [x] Create `task_comments`, `task_integrations`
- [x] Drizzle migration + `schema.ts`
- [x] Update [erd.md](./erd.md)

### Web (`apps/web`)

- [x] `taskService`: CRUD for comments and integrations
- [x] REST: `/api/tasks/[id]/comments`, integration endpoints
- [x] Zod schemas for new fields
- [x] SSE payload includes new entities

### TUI (`apps/tui`)

- [x] Extend `Task` struct + SQL in `db/tasks.rs`
- [x] `db/comments.rs`, `db/integrations.rs`
- [x] Edit/detail UI for new fields
- [x] Tests

### MCP (`apps/mcp`)

- [x] Extend `create_task` / `update_task` parameters
- [x] New tool: `add_comment`
- [x] Update `apps/mcp/README.md`

### Shared (`packages/shared`)

- [x] Update `Task` / `DatabaseTask` types

---

## E3 — Archive View

### TUI

- [x] `AppMode::Daily | Archive` (`a` toggle)
- [x] `list_archive()` in `db/tasks.rs` (+ 3 tests, q/tag filters, unfinished exclusion)
- [x] `ui/archive_view.rs` (filter bar, `c` continue today via E1 dedup rules)
- [x] Keybinding + help (`a`, `/`, `t`, `c`, daily `a:archive` hint)

### Web

- [x] API: archive list with filters (`GET /api/tasks?archived&done&q&tag&status`)

### MCP

- [x] `list_tasks` supports `date`, `q`, `tag`, `status`, `done`, `archived` filters

---

## E4 — JIRA Integration

### Shared logic

- [x] Decide Rust vs TS sidecar — record in tech-spec
- [x] JIRA config (env / config file)

### TUI

- [x] Sync menu + fetch sprint / by key
- [x] Detail, comment, transition UI
- [x] Store in `task_integrations`

### Web

- [x] Expose JIRA ops via API (wrap `jira.ts`)
- [x] Optional dashboard JIRA panel

### MCP

- [x] `sync_jira_sprint`, `jira_comment`, `jira_transition`

---

## E5 — Git / Bitbucket

### Web

- [x] Bitbucket API module
- [x] Routes: commits, PR comments, status

### TUI

- [x] Link branch, show commits, PR status
- [x] Import PR comments → `task_comments`

### MCP

- [x] `link_branch`, `fetch_pr_comments` (optional)

---

## E6 — AI Agent Hooks

### TUI

- [x] Hook listener (protocol in tech-spec)
- [x] `session_start` → start timer
- [x] `waiting_user` → pause
- [x] `session_end` → stop

### MCP (`apps/mcp`)

- [x] `session_start` / `session_pause` / `session_end` tools
- [x] Map to same REST endpoints as manual timer ops

### Web (`apps/web`)

- [x] `POST /api/session/hook` (Bearer API key)
- [x] Document in `docs/api.md`

### Repo

- [x] `.cursor/hooks.json` example
- [x] Codex `hooks.json` example
- [x] Update `docs/ai-control.md`

---

## E7 — Web Dashboard Parity

Implementation plan: [e7-plan.md](./e7-plan.md).

### 1. Date Navigation (`apps/web`)

- [x] `+page.server.ts`: accept `?date=YYYY-MM-DD` param; default to today
- [x] `+page.svelte`: date picker bar — prev day / date input / next day
- [x] URL reflects date (`/dashboard?date=2026-09-07`); bookmarkable
- [ ] `addTask` posts `workDate` of the viewed day (not always calendar today)
- [ ] `resetAll` scoped to viewed `work_date` (TUI resets that day only)

### 2. Create Dedup (`apps/web/src/lib/server/taskService.ts`)

- [ ] `createTask`: same label + same `work_date` → return existing row (no insert)
- [ ] `createTask`: same label + different `work_date` → insert new row, copy description from most recent same-label row if caller didn't supply one
- [ ] Unit tests matching TUI `create_same_label_same_day_returns_existing` / `create_same_label_new_day_copies_description`

### 3. Extended Field UI (`apps/web/src/routes/dashboard/`)

- [ ] Edit modal: add code, link, status, notes, tags fields
- [ ] Task card: show code badge, status pill, tags if present
- [ ] Create form: optional status/tags fields
- [ ] Wire to existing `PATCH /api/tasks/[id]` endpoint

### 4. Archive Page (`apps/web/src/routes/dashboard/archive/`)

- [ ] New route `+page.svelte` + `+page.server.ts`
- [ ] Fetch from `GET /api/tasks?archived=true` with filter params (`q`, `tag`, `status`)
- [ ] Filter bar: text search, tag select, status select
- [ ] "Continue today" button per task → POST create with dedup (task 2)
- [ ] Navigation link from daily dashboard ↔ archive

### 5. SSE Fix + Extend (`apps/web/src/lib/server/`)

- [ ] Fix `+page.svelte` SSE handler: listen for `type === 'change'` (currently checks `'tasks-changed'` — never fires)
- [x] `events.ts`: publish on comment add, integration upsert (not just task mutations)
- [ ] Dashboard + archive subscribe to SSE; refresh on relevant entity changes

### Verification

- [ ] Side-by-side: same DB, same day, TUI total elapsed = web total elapsed
- [ ] Create in web → visible in TUI after refresh
- [ ] Create in TUI → visible in web via SSE
- [ ] Date navigation: prev/next day loads correct tasks, URL bookmarkable
- [ ] Dedup: create same label twice same day → returns existing, no duplicate
- [ ] Archive: non-done tasks visible, "continue today" creates row with dedup
- [ ] Extended fields: edit/save code/link/status/notes/tags, persist across refresh

---

## Documentation maintenance

- [x] `docs/tui/README.md`, `prd.md`, `epics.md`, `tasks.md`
- [x] `tech-spec.md`, `erd.md`
- [x] `apps/tui/IDEA.md` → pointer to PRD
- [x] Root `README.md` → link `docs/tui/`
