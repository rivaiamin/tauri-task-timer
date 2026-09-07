# Epics — Task Timer (TUI-led, full monorepo)

TUI leads UX and schema decisions. Each epic lists **all apps** that must change before the epic is done.

---

## E1 — Daily Timer MVP

**Status:** Done (`feat/tui-mvp`)

**Goal:** Keyboard daily timer sharing SQLite with web.

### Apps

| App | Deliverable |
|-----|-------------|
| TUI | Full daily timer UX |
| Web | `work_date` + `done` migration only |
| MCP | Unchanged (uses existing API) |
| Shared | `timer.ts` reference for Rust port |

### Acceptance criteria

- [x] TUI lists today's tasks for configured user
- [x] Elapsed while running matches web within 1s
- [x] Day navigation filters by `work_date`
- [x] Continue-by-title copies description
- [x] Focus mode stops other tasks same day
- [x] `cargo test -p task-timer-tui` passes
- [x] Export markdown report to clipboard (`x`; matches web/MCP format)
- [x] Merge `feat/tui-mvp` → `main`

---

## E2 — Extended Schema

**Status:** Done (`feat/tui-mvp`) · **Depends on:** E1

**Goal:** Rich tasks, comments, integrations — **one migration, three consumers**.

### Apps

| App | Deliverable |
|-----|-------------|
| Web | Drizzle schema + migration, `taskService`, REST routes, Zod |
| TUI | `db/comments.rs`, `db/integrations.rs`, extended task UI |
| MCP | Extended `create_task`/`update_task`; `add_comment` tool |
| Shared | Updated `Task` / `DatabaseTask` types |

### Acceptance criteria

- [x] Single migration; `pnpm db:migrate` succeeds on existing DB
- [x] Web API CRUD for comments and integrations
- [x] TUI create/edit shows code, notes, tags, status
- [x] MCP can set extended fields and add comments
- [x] [erd.md](./erd.md) matches deployed schema

---

## E3 — Archive View

**Status:** Done (`main`) · **Depends on:** E2

**Goal:** Backlog of unfinished work across days.

### Apps

| App | Deliverable |
|-----|-------------|
| TUI | Archive screen, filters, "continue today" |
| Web | `GET /api/tasks/archive` or query params (can slip to E7 UI) |
| MCP | `list_tasks` filter by `done`, `status` |

### Acceptance criteria

- [x] TUI toggles daily ↔ archive
- [x] Archive lists non-finished tasks per agreed rules
- [x] Filter by label substring and tag
- [x] Continue today creates row via E1 dedup rules

---

## E4 — JIRA Integration

**Status:** Done (`feat/e4-jira`) · **Depends on:** E2

**Goal:** Sprint sync and JIRA mutations from TUI; web/MCP can reuse.

### Existing work

`apps/tui/src/jira.rs` already provides: credential loading (env + `~/.aimsis/jira.env`), issue-key extraction from label/description, and summary fetch + description merge.

### Apps

| App | Deliverable |
|-----|-------------|
| TUI | JIRA sync menu, detail, comment, transition |
| Web | Shared `jira.ts` logic callable from API |
| MCP | `sync_jira_sprint`, `jira_comment`, `jira_transition` (optional) |

### Acceptance criteria

- [x] Credential loading from env vars or `~/.aimsis/jira.env`
- [x] Issue-key extraction from label or description
- [x] Summary fetch merges into task description
- [x] Sprint fetch creates/updates tasks + `task_integrations`
- [x] Comment from TUI appears in JIRA
- [x] Status transition updates JIRA + local task
- [x] Auth failure is non-destructive (toast, not crash)
- [x] `cargo test -p task-timer-tui` passes

---

## E5 — Git / Bitbucket

**Status:** Planned · **Depends on:** E2, E4 (optional)

**Goal:** Commits and PRs linked to tasks.

### Apps

| App | Deliverable |
|-----|-------------|
| TUI | Branch link, commit list, PR status in detail |
| Web | Bitbucket API module + routes |
| MCP | `link_branch`, `fetch_pr_comments` (optional) |

### Acceptance criteria

- [ ] Task linked to branch name
- [ ] Commits listed for branch
- [ ] PR comments imported to `task_comments`
- [ ] PR merge / deploy status visible

---

## E6 — AI Agent Hooks

**Status:** Planned · **Depends on:** E1 (E4 for JIRA chain)

**Goal:** Timer follows agent session lifecycle; MCP stays the primary agent API.

### Apps

| App | Deliverable |
|-----|-------------|
| TUI | Hook listener → start/pause/stop |
| MCP | Session tools: `session_start`, `session_pause`, `session_end` mapping to timer |
| Web | `POST /api/session/hook` (Bearer key) for non-MCP agents |
| Repo | `.cursor/hooks.json` + Codex hook examples in docs |

### Acceptance criteria

- [ ] Documented hook protocol in [tech-spec.md](./tech-spec.md)
- [ ] Cursor session start starts timer on selected task
- [ ] Waiting for user pauses timer
- [ ] Session end stops timer
- [ ] MCP tools mirror hook behavior
- [ ] Hook failure does not crash TUI or web

---

## E7 — Web Dashboard Parity

**Status:** Planned · **Depends on:** E1, E2 (E3 for archive UI)

**Goal:** Web dashboard matches TUI daily workflow — **explicit so it is not forgotten**.

### Apps

| App | Deliverable |
|-----|-------------|
| Web | Daily date picker, `work_date` filter on dashboard |
| Web | Continue-by-title on create (match `taskService` + TUI) |
| Web | Extended field UI after E2 |
| Web | Archive/backlog page after E3 |
| Web | SSE events for comments/integrations |

### Acceptance criteria

- [ ] Dashboard defaults to today; only shows that day's tasks
- [ ] Date navigation matches TUI semantics
- [ ] Create duplicate label same day returns existing task
- [ ] Create same label new day copies description
- [ ] Extended fields editable in UI
- [ ] Archive view or filter equivalent to TUI E3

---

## Dependency graph

```mermaid
graph TD
  E1[E1 Daily MVP]
  E2[E2 Schema]
  E3[E3 Archive]
  E4[E4 JIRA]
  E5[E5 Bitbucket]
  E6[E6 AI Hooks]
  E7[E7 Web Parity]

  E1 --> E2
  E1 --> E6
  E1 --> E7
  E2 --> E3
  E2 --> E4
  E2 --> E5
  E2 --> E7
  E3 --> E7
  E4 -.-> E5
  E4 -.-> E6
```
