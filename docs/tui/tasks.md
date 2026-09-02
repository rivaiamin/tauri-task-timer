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
