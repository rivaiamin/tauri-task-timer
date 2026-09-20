# Task Checklist — Task Timer (TUI-led)

Update when shipping epics. Each section names **all apps** so web/MCP are not skipped.

**Legend:** `[x]` done · `[ ]` pending

---

## E1 — Daily Timer MVP

### Schema (`apps/web`)

- [x] Migration `0002`: `work_date`, `done`, indexes (superseded by the day-identity split below)
- [x] `schema.ts` updated

### TUI (`apps/tui`)

- [x] Cargo workspace + crate scaffold
- [x] `config.rs`, `db/`, `timer.rs`, `app/`, `ui/`
- [x] Port taskService timer ops to `db/tasks.rs`
- [x] Keymap + `?` help + modals
- [x] Export markdown report (`report.rs`, `x` key, clipboard)
- [x] Unit tests (timer math, identity upsert + day-row ensure, report)
- [ ] Operator sign-off: elapsed matches web
- [x] Merge `feat/tui-mvp` → `main` (PR #9)

### Web (`apps/web`)

- [x] Migration only (no daily UI yet — E7)
- [x] Verify `taskService` still works with the day-scoped columns (`workDate` throughout; `taskCreate.test.ts` covers the identity upsert + per-day position)

### MCP (`apps/mcp`)

- [ ] Smoke test: list/create/start/stop against DB with `task_days` rows

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
- [x] `db/comments.rs` (list/add, with `branch`/`pr`), `db/integrations.rs`
- [x] Edit/detail UI for new fields (link + the six flags on the form; `p/i/a/c/x/d` on the flags field)
- [x] Status is picked from `JIRA_STATUSES` via an inline dropdown (no free text); status follows the timer (start → In Progress, stop → To Do, done → Done, un-done → To Do)
- [x] `C` comment list + compose → `task_comments`
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
- [x] `list_archive()` in `db/tasks.rs` (q/tag/status/integration filters; grouped — one entry per task with its days)
- [x] `ui/archive_view.rs` (filter bar, `c` continue today — resolves the task by label and adds today's day entry)
- [x] Keybinding + help (`a`, `/`, `t`, `s`, `g`, `c`, daily `a:archive` hint)

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

- [x] Sprint picker: unassigned / reporter undone / assignee undone / fetch by key (JQL, no board)
- [x] Detail, comment, transition UI
- [x] Store in `task_integrations` (`jira/issue_key` + the fetched `jira/status`)
- [x] `o` opens the issue in the browser (`{JIRA_SITE}/browse/{KEY}`); the detail view lists the stored rows

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
- [x] Import PR comments → `task_comments` (git menu 4, deduped on pr + summary)
- [x] Detail view: comments, PR merge state, commit statuses

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
- [x] `addTask` posts `workDate` of the viewed day (not always calendar today)
- [x] `resetAll` scoped to the viewed date (TUI resets that day only)

### 2. Create Path (`apps/web/src/lib/server/taskService.ts`)

- [x] `createTask`: upsert the task identity by `(user_id, label)`; an existing identity is reused, never duplicated
- [x] `createTask`: `ensureDayRow` for the requested date — an existing day row is returned unchanged, a missing one is inserted with `position = MAX(position)+1` for that date
- [x] Unit tests matching TUI `create_same_label_same_day_returns_existing` / `create_same_label_new_day_adds_a_day_row`

### 3. Extended Field UI (`apps/web/src/routes/dashboard/`)

- [x] Edit modal: add code, link, status, notes, tags fields
- [x] Task card: show code badge, status pill, tags if present
- [x] Create form: optional status/tags fields
- [x] Wire to existing `PATCH /api/tasks/[id]` endpoint

### 4. Archive Page (`apps/web/src/routes/dashboard/archive/`)

- [x] New route `+page.svelte` + `+page.server.ts`
- [x] Fetch from `GET /api/tasks` (no `date`) with filter params (`q`, `tag`, `status`); each task carries its `days[]`
- [x] Filter bar: text search, tag select, status select
- [x] "Continue today" button per task → POST create (upserts the identity, ensures today's day row)
- [x] Navigation link from daily dashboard ↔ archive

### 5. SSE Fix + Extend (`apps/web/src/lib/server/`)

- [x] Fix `+page.svelte` SSE handler: listen for `type === 'change'` (currently checks `'tasks-changed'` — never fires)
- [x] `events.ts`: publish on comment add, integration upsert (not just task mutations)
- [x] Dashboard + archive subscribe to SSE; refresh on relevant entity changes

### Verification

- [x] Side-by-side: same DB, same day, TUI total elapsed = web total elapsed
- [x] Create in web → visible in TUI after refresh
- [x] Create in TUI → visible in web via SSE
- [x] Date navigation: prev/next day loads correct tasks, URL bookmarkable
- [x] Same label twice same day → returns the same task and its existing day row, no duplicate
- [x] Archive: one entry per task with its days listed; "continue today" adds today's day entry
- [x] Extended fields: edit/save code/link/status/notes/tags, persist across refresh

### Status column parity (follow-up to E7)

- [x] **Web writes `status` on start/stop/reset/done.** `apps/web/src/lib/server/taskStatus.ts` ports the TUI's `auto_status` (`apps/tui/src/jira.rs`) and is wired into `startTimer` (including the focus-switch victim), `stopTimer`, `resetTask`, `resetAll`, and the done toggle in `updateTask`. Start → `21`, stop/reset → `11`, done → `31`, un-done → `11`; an explicit `status` in a PATCH wins.
- [x] `taskStatus.test.ts` mirrors the TUI's `auto_status_follows_the_rows_own_state` oracle (negative-controlled: each of the three rules fails the suite when broken).
- [x] Verified end-to-end over the REST API against a copy of `local.db`: full lifecycle, done-task stop keeps `31`, focus-switch victim demoted to `11`, explicit status wins.

---

## Verification scripts

Runnable oracles for the TUI's timer/status behavior. Each has a negative control
(`--self-test`) that must fail on the behavior the oracle rejects, so a green run
means the check discriminates.

| Script | Proves |
|--------|--------|
| `scripts/verify-status-sync.mjs` | start → `21`, stop → `11`, done → `31`, un-done → `11` |
| `scripts/verify-status-picker.mjs` | the status picked in a real pty session is what gets stored |
| `scripts/verify-jira-stop.mjs` | every stop path worklogs then returns the issue to To Do (`--paths` covers each path) |
| `scripts/verify-clippy-clean.mjs` | the crate is clean under `-D warnings` |

`GATES.md` at the repo root is the current completion ledger for the status work.

---

## Documentation maintenance

- [x] `docs/tui/README.md`, `prd.md`, `epics.md`, `tasks.md`
- [x] `tech-spec.md`, `erd.md`
- [x] `apps/tui/IDEA.md` → pointer to PRD
- [x] Root `README.md` → link `docs/tui/`
