# PRD — Task Timer TUI (and monorepo parity)

**Status:** Active  
**Owner:** Personal (raysaber37@gmail.com)  
**Last updated:** 2026-08-31

## Summary

A keyboard-driven terminal UI for daily task timing, sharing SQLite with the web dashboard and MCP server. **TUI is the reference UX** for daily workflow; web and MCP must follow when schema or timer rules change.

## Problem

Developers multitasking in the terminal need to check tasks and manage timers without context-switching to a browser. Agents already drive the timer via MCP, but session lifecycle (start/pause/stop) is manual. JIRA and PR workflows are disconnected from time tracking.

## Users

| Persona | Primary surface | Need |
|---------|-----------------|------|
| Solo developer | TUI + web | Same tasks/timers everywhere |
| AI-assisted workflow | MCP + hooks | Auto track time during agent sessions |
| JIRA-driven workflow | TUI | Sprint sync, comment, transition from terminal |

## Goals

1. **Terminal-first daily workflow** — keyboard-only timer page as the default.
2. **Shared data** — one SQLite file (`apps/web/local.db`), WAL, no sync server.
3. **Behavior parity** — timer math and focus/parallel mode identical across TUI, web, MCP.
4. **No orphan apps** — schema and feature epics explicitly list web + MCP follow-ups.

## Non-goals

- Cloud sync or multi-tenant hosting
- Replacing JIRA or Bitbucket (integrate, don't replicate)
- Tauri desktop app revival (browser `apps/desktop` stays localStorage-only)

---

## E1 — Daily Timer MVP ✅

Shipped on `feat/tui-mvp`.

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-1 | Tasks filtered by `work_date`; default today | P0 |
| FR-2 | Day navigation (`h/l`, arrows) | P0 |
| FR-3 | Start/stop/reset/reset-all | P0 |
| FR-4 | Focus stops others on same day; parallel allows multiple | P0 |
| FR-5 | CRUD + reorder | P0 |
| FR-6 | Same label new day → new row, copy description | P0 |
| FR-7 | 1s live tick while running | P0 |
| FR-8 | `?` help; modal create/edit | P0 |
| FR-9 | Config: `database_path`, `user_email` | P0 |
| FR-10 | CLI: `--config`, `--date`, `--db` | P1 |
| FR-11 | Export markdown daily report to clipboard (`x`) | P1 |

**Web note (deferred to E7):** dashboard still shows all tasks, ignores `work_date`.

**MCP note:** existing tools work; no `work_date` filter on list yet.

---

## E2 — Extended task model

From [IDEA.md](../../apps/tui/IDEA.md).

### Task fields (add to `tasks`)

`code`, `link`, `status`, `notes`, `tags`, `end_time`, `total_time`, flags: `is_pinned`, `is_important`, `is_archived`, `is_deleted`, `is_cancelled`, `is_completed`.

Rename alignment: `label` = title in IDEA; keep `label` in DB for compatibility.

### New tables

**`task_comments`**

| Column | Purpose |
|--------|---------|
| task_id | FK → tasks |
| subject | Comment / commit title |
| summary | Body / commit message |
| branch | Optional git branch |
| pr | Optional PR id/url |

**`task_integrations`**

| Column | Purpose |
|--------|---------|
| task_id | FK → tasks |
| group | `jira`, `gcp`, `bitbucket` |
| field | Arbitrary key |
| value | Arbitrary value |

### Cross-app requirements

| App | Must do |
|-----|---------|
| Web | Drizzle migration, `taskService`, REST routes, Zod types |
| TUI | Rust db modules, edit/detail UI |
| MCP | `create_task` / `update_task` accept new fields; `add_comment` tool |
| Shared | Export `Task` / `DatabaseTask` types |

---

## E3 — Archive view (TUI)

- Screen for non-finished tasks across days
- Filter by status, tag, integration
- "Continue today" uses continue-by-title rules

**Web:** optional `/dashboard/archive` or filter on main page (E7).

---

## E4 — JIRA integration

- Sprint sync: unassigned, reporter undone, assignee undone, fetch by key
- Task detail, comment, status transition
- Metadata in `task_integrations` (`group = jira`)

**Web:** reuse/extend `apps/web/src/lib/server/jira.ts` where possible; TUI may call same logic via shared TS sidecar or Rust port.

**MCP:** `sync_jira_sprint`, `jira_comment`, `jira_transition` tools (optional).

---

## E5 — Git / Bitbucket

- Commits linked to task (branch in `task_comments`)
- PR comments → `task_comments`
- PR merge + deployment status in task detail

**Web:** Bitbucket API in `taskService` or dedicated module.

---

## E6 — AI agent hooks

From IDEA.md:

- Agent API: create/assign task, update status, timer, add comment (short path — extend MCP REST)
- JIRA chained: local change propagates to JIRA when integrated
- **Session hooks:**
  - `session_start` → start timer
  - `waiting_user` → pause timer
  - `session_end` → stop timer

| App | Must do |
|-----|---------|
| MCP | Session lifecycle tools or document hook → REST mapping |
| Web | Optional `POST /api/session/hook` for agents without MCP |
| TUI | Local listener (socket or stdin) for Cursor/Codex hooks |
| Repo | `.cursor/hooks.json` / Codex hooks examples |

---

## E7 — Web dashboard parity

Catch-up epic so web is not left behind TUI.

| Requirement | Notes |
|-------------|-------|
| Daily view | Filter by `work_date`; date picker like TUI |
| Continue-by-title | Same dedup rules as TUI |
| Extended fields UI | After E2 schema |
| Archive / backlog | After E3, mirror TUI filters |
| SSE | Emit events for new tables (comments, integrations) |

---

## Success metrics

| Metric | Target |
|--------|--------|
| Daily timer without web server | ✅ E1 |
| Elapsed matches web within 1s | ✅ E1 |
| Schema change ships web + TUI same PR | E2+ |
| Agent session auto-tracks time | E6 |
| Web daily view matches TUI behavior | E7 |

## Open questions

1. JIRA: Rust in TUI vs invoke TS `jira.ts` sidecar?
2. Archive definition: `done = false` only, or full status flags from E2?
3. `tags` storage: JSON column vs join table?

## References

- [IDEA.md](../../apps/tui/IDEA.md)
- Cursor plan: `TUI Timer MVP-6105cd28` (archived into this folder)
- `apps/web/src/lib/server/taskService.ts`
- `packages/shared/src/timer.ts`
- [epics.md](./epics.md) · [tasks.md](./tasks.md)
