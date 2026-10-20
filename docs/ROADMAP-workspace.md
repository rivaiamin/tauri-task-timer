# Task Timer workspace roadmap

Status: living proposal (updated 2026-10-20). Originally written 2026-10-19 from a read-only survey of this repo.

Task Timer began as a per-task timer. It is now the hub of a personal engineering workspace. One SQLite file is shared by a Rust TUI, a SvelteKit web app, an MCP server and a CLI skill. Around that core sit Jira, Git/Bitbucket, Cursor and Codex session hooks, and the Paseo sprint orchestrator. The roadmap below ranks the work that makes the hub dependable and observable across all of those surfaces. It ranks by value, effort and dependency order, and it stays inside what the PRD allows. The PRD non-goals are no cloud sync and no replacing Jira or Bitbucket (see docs/tui/prd.md).

## Current state

What exists today, with the file that implements it:

- Daily timer, focus and parallel modes, and the task-identity plus per-day row model in the TUI: `apps/tui/src/app/mod.rs`
- TUI database layer for tasks, days, comments and integrations: `apps/tui/src/db/tasks.rs`
- Headless CLI used by the agent skill (list, start, stop, done, status set, integration set, jira ensure, kanban): `apps/tui/src/cli.rs`
- Web dashboard, archive and Kanban pages: `apps/web/src/routes/dashboard/+page.svelte`
- Web archive page with filters and "continue today": `apps/web/src/routes/dashboard/archive/+page.svelte`
- Web Kanban page for Jira-linked tasks: `apps/web/src/routes/dashboard/kanban/+page.svelte`
- Domain service shared by REST, MCP and the dashboard: `apps/web/src/lib/server/taskService.ts`
- Drizzle schema (users, sessions, tasks, task_days, task_comments, task_integrations, user_settings, api_keys): `apps/web/src/lib/server/db/schema.ts`
- Session cookie or Bearer API key auth with scopes: `apps/web/src/lib/server/actor.ts`
- Live updates over SSE: `apps/web/src/routes/api/stream/+server.ts`
- Agent session hook endpoint: `apps/web/src/routes/api/session/hook/+server.ts`
- MCP server with task, comment, integration, Kanban, Jira ensure and session tools (stdio and HTTP): `apps/mcp/src/server.ts`
- Unix-socket hook client for Cursor and Codex: `scripts/task-timer-hook`
- TUI hook listener: `apps/tui/src/hooks/mod.rs`
- Jira REST client in Rust (comment, worklog, transition, sprint pull): `apps/tui/src/jira.rs`
- Jira client in TypeScript (start, stop and done hooks, ingest): `apps/web/src/lib/server/jira.ts`
- Bitbucket PR status and comments: `apps/tui/src/bitbucket.rs`
- Git branch and commit lookup: `apps/tui/src/git.rs`
- Orchestrator driver conversation view (history, composer, delivery states): `apps/tui/src/app/orchestrator.rs`
- Bundled Paseo SDK adapter: `apps/tui/src/orchestrator_adapter.mjs`
- Agent badge from task integrations: `apps/tui/src/db/integrations.rs`
- Markdown and CSV daily reports (TypeScript and a Rust port): `packages/shared/src/report.ts`
- Shared timer math: `packages/shared/src/timer.ts`
- Data export tool: `scripts/export-task-timer.py`
- Agent skill that drives the CLI: `skills/task-timer/SKILL.md`
- Gate ledgers with executable verifiers: `GATES.md` (day identity), `GATES-kanban.md`, `GATES-jira-ensure.md`, `GATES-orchestrator.md` (contract), `GATES-orchestrator-feature.md` (implementation); see `scripts/verify-*.mjs` including `scripts/verify-orchestrator-smoke.mjs`
- Orchestrator operator docs in the TUI README (`apps/tui/README.md` § Orchestrator); Paseo contract in `docs/tui/orchestrator-contract.md`
- Static browser timer (localStorage only): `apps/desktop/src/index.html`
- Pages deployment workflow (the only CI workflow): `.github/workflows/deploy.yml`

## Ranking rationale

Items 1 to 3 are cheap or already started and unblock the rest. Item 1 is **partially delivered** (see below). Items 4 to 6 build the event and reliability layer that the later items reuse. Items 7 to 9 close parity and reporting gaps. Items 10 and 11 are hygiene that should not be skipped but need no urgency.

## 1. Finish the Orchestrator driver view (Tasks 5 and 6, Checkpoint C)

- **Problem:** **Start/adopt** when no driver exists is implemented (`GATES-orchestrator-task5.md`), but in-app `?` help still shows the daily key map, not Orchestrator-specific keys. Task 6’s broader safety regression matrix (resize, slow CLI, full cross-mode checks) and Task 5’s optional PTY lifecycle smoke are not automated beyond Rust fakes. Checkpoint B still has an open human review line; Checkpoint C and Task 6 remain open in `tasks/todo.md`.
- **Delivered (do not re-do):** Tasks 1–4; contract gate `GATES-orchestrator.md`; implementation ledger `GATES-orchestrator-feature.md` (G0–G5 all met, including `cargo test … orchestrator`, release build, `node scripts/verify-orchestrator-smoke.mjs`, and README contract lint via `--docs`). Manual Task 3–4 TUI smokes are recorded in `tasks/todo.md`. Send/delivery over a real PTY `Ctrl+S` is intentionally not asserted in the smoke script (terminal flow-control); those behaviours stay on the Rust orchestrator tests and the Task 4 manual evidence.
- **Proposal:** Complete Tasks 5 and 6 as specified in `tasks/todo.md`, close Checkpoint B (human review; optional live send recorded separately), then Checkpoint C. Do not widen scope; the TUI stays a Paseo client with no transcript storage.
- **Effort:** M (remaining slice only)
- **Dependencies:** None for the open work (Tasks 1–4 and the feature gate ledger are done)
- **Evidence:** `tasks/todo.md` (Tasks 5–6, Checkpoints B and C), `tasks/plan.md`, `apps/tui/src/app/orchestrator.rs`, `apps/tui/src/orchestrator.rs`, `docs/tui/orchestrator-contract.md`, `GATES-orchestrator-feature.md`, `apps/tui/README.md` (Orchestrator section documents `O`, `Ctrl+S`, delivery meanings, Node 22+, and states Start/adopt is not shipped yet).
- **Acceptance test:** (draft) **Already met:** `node /home/amin/.agents/skills/unlazy/scripts/gate-check.mjs GATES-orchestrator-feature.md` reports all met; `grep -c "Orchestrator" apps/tui/README.md` ≥ 1; README lists entry (`O`) and send (`Ctrl+S`). **Still required:** `cargo test -p task-timer-tui orchestrator` includes cancel, adoption, double activation, failure, and malformed identity (Task 5). UI offers Start/adopt with confirmation when discovery is empty; README and `?` help describe Start/adopt once implemented. Every checkbox under Task 5, Task 6, and Checkpoint C in `tasks/todo.md` is ticked; any skipped live round trip is recorded as unverified, not passed.

## 2. Run the existing gates in CI

- **Problem:** The repo already has executable gate ledgers and verifier scripts, but the only workflow deploys the static browser app. Regressions in the TUI, web service, MCP and the day-identity migration are caught only when someone remembers to run the gates by hand.
- **Proposal:** Add a pull-request workflow that runs the Rust tests, clippy, the web Vitest suite, svelte-check and the verify scripts. The Jira verifiers should use the bundled Jira stub so CI needs no credentials. Verifiers that need a real database copy should build a throwaway one from the migrations. Add a check that fails when a ledger entry has no runnable CHECK line.
- **Effort:** S
- **Dependencies:** None
- **Evidence:** `.github/workflows/deploy.yml` is the only workflow. Ledgers: `GATES.md`, `GATES-kanban.md`, `GATES-jira-ensure.md`, `GATES-orchestrator-feature.md`. Verifiers under `scripts/verify-*.mjs` (including orchestrator smoke, day identity, kanban, jira ensure, status sync/picker, clippy). `scripts/jira-stub.mjs` is the offline Jira stand-in.
- **Acceptance test:** (draft) A new file under `.github/workflows/` exists besides `deploy.yml`. On a clean checkout with no Jira credentials set, `cargo test -p task-timer-tui`, `pnpm --filter sv-task-timer test`, `pnpm --filter sv-task-timer check`, representative `node scripts/verify-*.mjs` runs (at least kanban, day-identity, status-sync, orchestrator smoke on Linux), and `gate-check` on the ledgers that have CHECK lines all exit 0. Deliberately breaking one gate makes the workflow fail on a throwaway branch.

## 3. Attribute timers to agent sessions instead of stopping everything

- **Problem:** Session hooks carry no session identity. `waiting_user` and `session_end` stop every running timer in the TUI and in the web endpoint. With the orchestrator running several agents in parallel, one agent finishing or waiting stops the timers of the others, and a human timer started by hand is stopped too.
- **Proposal:** Add an optional session id to the hook payload in the hook script, the TUI listener, the web endpoint and the MCP session tools. Record the session id against the day row it started, for example as an agent session integration row. Make `waiting_user` and `session_end` stop only the timers that session started. Keep the current stop-all behaviour when no session id is sent, so existing hook configs still work.
- **Effort:** M
- **Dependencies:** None
- **Evidence:** `apps/web/src/routes/api/session/hook/+server.ts` stops every running day for session_end, `apps/tui/src/hooks/mod.rs` defines hook events with no session field, `apps/tui/src/app/mod.rs` calls stop_all_running for both events, `scripts/task-timer-hook` sends only an event and an optional task id, `.cursor/hooks.json` and `.codex/hooks.json` are the configs that call it, `apps/mcp/src/server.ts` defines the session tools, `apps/tui/src/db/integrations.rs` can hold the session row.
- **Acceptance test:** (draft) A Vitest case in the web service tests starts two timers through the hook with different session ids, sends session_end for the first id, and asserts that only the first timer stopped and the second is still running. A second case sends session_end with no id and asserts both stop. A Rust unit test in the hooks module parses a payload with a session id and one without. `cargo test -p task-timer-tui` and `pnpm --filter sv-task-timer test` exit 0.

## 4. Durable activity log for every write

- **Problem:** The database records only current state. When a timer moves, a status flips or a Jira call fails, there is no record of who did it (TUI, web, an API key, a hook, the orchestrator) or when. The web Jira client logs a failure to the console and drops it, and the CLI prints a warning to stderr that disappears with the terminal.
- **Proposal:** Add an append-only events table through a new Drizzle migration, written by the web service and by the TUI for timer, status, integration, comment and Jira-sync outcomes. Each row has a timestamp, an actor kind and name, an action, the task identity and a small JSON payload. Expose it through a read-only API route, an MCP tool and a TUI detail-view section. Keep it bounded with a retention setting.
- **Effort:** M
- **Dependencies:** None
- **Evidence:** `apps/web/src/lib/server/db/schema.ts` has no event table, `apps/web/src/lib/server/actor.ts` already resolves who is calling, `apps/web/src/lib/server/jira.ts` drops sync failures after console output, `apps/web/src/lib/server/events.ts` is the in-memory SSE bus that keeps nothing, `apps/tui/src/db/tasks.rs` is where the TUI writes timer and status changes, `apps/web/drizzle` holds the migrations.
- **Dependencies note:** Items 5, 6 and 9 build on this table.

## 5. One attention inbox across TUI, web and MCP

- **Problem:** Things that need a human are scattered. An agent that needs you is a badge in the TUI only. A failed Jira write is a stderr line. New PR comments are imported on demand. The web dashboard shows none of it.
- **Proposal:** Define an attention item as a row derived from the event log and the integration rows: agent attention true, a failed or skipped Jira sync, a PR comment newer than the last viewed time, and a ticket whose Jira status changed under you. Add an Attention list to the TUI, the dashboard and an MCP list tool, with acknowledge actions. Push changes over the existing SSE stream.
- **Effort:** M
- **Dependencies:** Item 4
- **Evidence:** `apps/tui/src/db/integrations.rs` reads the agent status and attention rows, `apps/tui/src/ui/timer_view.rs` renders the badge, `apps/web/src/lib/server/events.ts` and `apps/web/src/routes/api/stream/+server.ts` carry live changes, `apps/tui/src/bitbucket.rs` imports PR comments, `skills/task-timer/SKILL.md` documents the Jira warning that is easy to miss.

## 6. Jira write outbox with retry

- **Problem:** Worklogs and transitions are fire-and-forget in both clients. If Jira is unreachable at stop time, the worklog is lost for good, so the local time and Jira's time drift apart without any signal.
- **Proposal:** Write each outbound Jira action (worklog, transition, comment) to an outbox table first, then send it from a single drain routine with backoff and idempotency keys. Failures stay visible in the attention inbox until they succeed or are dismissed. Local status keeps being written before the Jira call, as it is today.
- **Effort:** M
- **Dependencies:** Items 4 and 5
- **Evidence:** `apps/web/src/lib/server/jira.ts` catches errors and drops them, `apps/tui/src/jira.rs` has the log_work and transition_issue calls, `apps/tui/src/cli.rs` runs the CLI hooks that only warn, `scripts/jira-stub.mjs` can fail on demand for tests, `scripts/verify-jira-stop.mjs` is the existing stop verifier to extend.

## 7. Bring agent, git and PR context to the web dashboard and MCP

- **Problem:** The PRD says no app should be left behind, yet the web app has no knowledge of agent status, branches or pull requests. Those exist only in the TUI. An agent using MCP cannot ask which tickets have an agent running or which branch a task is on.
- **Proposal:** Show the agent badge, branch and PR state on dashboard rows, the archive and the Kanban cards, reading the existing integration rows. Add MCP read tools for the same data and for the orchestrator driver status. Writes stay with the orchestrator and the CLI. Do not add transcript storage to the web app.
- **Effort:** M
- **Dependencies:** Item 1 **remaining** work for Start/adopt and any MCP “driver status” surface; the Paseo read/send **contract** (`docs/tui/orchestrator-contract.md`) is already frozen
- **Evidence:** `apps/web/src/routes/dashboard/kanban/+page.svelte` has no agent or PR fields, `apps/web/src/lib/server/kanban.ts` builds the cards, `apps/tui/src/ui/kanban_view.rs` is the reference rendering, `apps/tui/src/git.rs` and `apps/tui/src/bitbucket.rs` hold the lookups the web side lacks, `apps/mcp/src/server.ts` lists the current tools, `docs/tui/prd.md` states the parity goal.

## 8. Cross-implementation contract tests for the Rust and TypeScript ports

- **Problem:** The TUI and the web service implement the same rules twice. The report builder, the timer math, the automatic status rule and the Jira ingest rules are hand ports kept equal by comment ("keep output identical"). Drift is found by a user, not by a test.
- **Proposal:** Create golden vectors in a shared fixtures folder: inputs and expected outputs for report formatting, elapsed and story-point math, status transitions and Jira ingest labels. Run the same vectors from a Rust test and a Vitest test. Add vectors whenever a rule changes.
- **Effort:** M
- **Dependencies:** Item 2, so the tests actually run
- **Evidence:** `packages/shared/src/report.ts` and `apps/tui/src/report.rs` are a documented port pair, `packages/shared/src/timer.ts` and `apps/tui/src/timer.rs` are the timer pair, `apps/web/src/lib/server/taskStatus.ts` is a port of the TUI status rule, `apps/web/src/lib/server/jiraIngest.ts` and `apps/tui/src/jira.rs` share ingest rules, `apps/tui/src/app/mod.rs` is the largest module and holds much of the Rust-side behaviour.

## 9. Range reports and Jira worklog reconciliation

- **Problem:** Reporting is one day at a time, in markdown or CSV. A weekly or sprint view needs manual stitching. There is no way to see whether the time recorded locally matches what was logged in Jira.
- **Proposal:** Add a date range and a group-by option (task, tag, Jira project) to the report in the CLI, the REST route and the MCP tool, reusing the shared formatters. Add a reconcile report that compares each day's local seconds with worklogs recorded in the event log and the outbox, and lists the differences.
- **Effort:** M
- **Dependencies:** Items 4 and 6 for the reconcile part; the range report alone has none
- **Evidence:** `apps/web/src/routes/api/report/+server.ts` takes a single date and format, `packages/shared/src/report.ts` builds the daily output, `apps/tui/src/report.rs` is the Rust copy, `apps/tui/src/cli.rs` exposes the report command, `apps/mcp/src/server.ts` defines get_report.

## 10. Operational hardening of the shared database and remote MCP

- **Problem:** Rate limiting on the API is listed as not done, while the MCP server can run in a multi-tenant HTTP mode. The only backup is a manual export. The live database sits inside the repo checkout next to the app. Key scopes exist but only two are used.
- **Proposal:** Add per-key rate limits on the API routes. Schedule the existing export with a retention count, using the SQLite online-backup approach it already has. Move the default database path out of the repo tree in the docs and config examples. Add read-only and per-tool scopes for API keys, and show last-used time on the keys page.
- **Effort:** S
- **Dependencies:** None
- **Evidence:** `docs/ai-control.md` lists rate limiting as not yet done, `apps/web/src/lib/server/apiKeys.ts` handles key creation and checks, `apps/web/src/routes/dashboard/keys/+page.svelte` is the key management page, `scripts/export-task-timer.py` is the existing backup tool, `Makefile` wires it up, `apps/web/ecosystem.config.cjs` is the production process config, `apps/mcp/README.md` documents the remote HTTP mode.

## 11. Settle the repo's identity and retire stale docs

- **Problem:** The repo name and several docs still describe an earlier product. The README says the repo has two apps and then lists five. The Tauri backend is gone, yet the desktop package keeps the Tauri CLI dependency. The AI-control design doc still describes Supabase and row-level security. The browser app keeps its own copy of timer logic and cannot join the shared database.
- **Proposal:** Write a short architecture overview that names the hub, its clients and the integration boundaries, and link it from the README. Mark the Supabase sections of the AI-control doc as historical or prune them. Decide the browser app's fate: keep it as a static demo with a clear label, or point it at the web API. Remove the unused Tauri dependency if the demo stays.
- **Effort:** S
- **Dependencies:** None
- **Evidence:** `README.md` has the outdated app count and the Tauri note, `docs/ai-control.md` still carries Supabase-era sections, `apps/desktop/README.md` documents the localStorage-only app, `apps/desktop/src/main.js` holds its separate timer logic, `migration.md` is a long historical note, `docs/tui/prd.md` lists Tauri revival as a non-goal.
