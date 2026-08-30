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
- [x] Unit tests (timer math, dedup)
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

- [ ] Add task columns (code, link, status, notes, tags, flags, end_time, total_time)
- [ ] Create `task_comments`, `task_integrations`
- [ ] Drizzle migration + `schema.ts`
- [ ] Update [erd.md](./erd.md)

### Web (`apps/web`)

- [ ] `taskService`: CRUD for comments and integrations
- [ ] REST: `/api/tasks/[id]/comments`, integration endpoints
- [ ] Zod schemas for new fields
- [ ] SSE payload includes new entities

### TUI (`apps/tui`)

- [ ] Extend `Task` struct + SQL in `db/tasks.rs`
- [ ] `db/comments.rs`, `db/integrations.rs`
- [ ] Edit/detail UI for new fields
- [ ] Tests

### MCP (`apps/mcp`)

- [ ] Extend `create_task` / `update_task` parameters
- [ ] New tool: `add_comment`
- [ ] Update `apps/mcp/README.md`

### Shared (`packages/shared`)

- [ ] Update `Task` / `DatabaseTask` types

---

## E3 — Archive View

### TUI

- [ ] `AppMode::Daily | Archive`
- [ ] `list_archive()` in `db/tasks.rs`
- [ ] `ui/archive_view.rs`
- [ ] Keybinding + help

### Web

- [ ] API: archive list with filters (query params or route)

### MCP

- [ ] `list_tasks` supports `done`, `archived`, status filters

---

## E4 — JIRA Integration

### Shared logic

- [ ] Decide Rust vs TS sidecar — record in tech-spec
- [ ] JIRA config (env / config file)

### TUI

- [ ] Sync menu + fetch sprint / by key
- [ ] Detail, comment, transition UI
- [ ] Store in `task_integrations`

### Web

- [ ] Expose JIRA ops via API (wrap `jira.ts`)
- [ ] Optional dashboard JIRA panel

### MCP

- [ ] `sync_jira_sprint`, `jira_comment`, `jira_transition`

---

## E5 — Git / Bitbucket

### Web

- [ ] Bitbucket API module
- [ ] Routes: commits, PR comments, status

### TUI

- [ ] Link branch, show commits, PR status
- [ ] Import PR comments → `task_comments`

### MCP

- [ ] `link_branch`, `fetch_pr_comments` (optional)

---

## E6 — AI Agent Hooks

### TUI

- [ ] Hook listener (protocol in tech-spec)
- [ ] `session_start` → start timer
- [ ] `waiting_user` → pause
- [ ] `session_end` → stop

### MCP (`apps/mcp`)

- [ ] `session_start` / `session_pause` / `session_end` tools
- [ ] Map to same REST endpoints as manual timer ops

### Web (`apps/web`)

- [ ] `POST /api/session/hook` (Bearer API key)
- [ ] Document in `docs/api.md`

### Repo

- [ ] `.cursor/hooks.json` example
- [ ] Codex `hooks.json` example
- [ ] Update `docs/ai-control.md`

---

## E7 — Web Dashboard Parity

### Web (`apps/web`)

- [ ] Dashboard filters by `work_date`; default today
- [ ] Date picker (prev/next day)
- [ ] `taskService.create_task`: dedup + continue-by-title (match TUI)
- [ ] Extended field UI (after E2)
- [ ] Archive/backlog UI (after E3)
- [ ] SSE for comments/integrations

### Verification

- [ ] Side-by-side: same DB, same day, TUI total elapsed = web total elapsed
- [ ] Create in web → visible in TUI after refresh
- [ ] Create in TUI → visible in web via SSE

---

## Documentation maintenance

- [x] `docs/tui/README.md`, `prd.md`, `epics.md`, `tasks.md`
- [x] `tech-spec.md`, `erd.md`
- [x] `apps/tui/IDEA.md` → pointer to PRD
- [x] Root `README.md` → link `docs/tui/`
