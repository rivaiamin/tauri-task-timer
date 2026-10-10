# task-timer-tui

Terminal UI for the same SQLite database as the web dashboard. Standalone —
the web server does not need to be running.

## Setup

```toml
# ~/.config/task-timer-tui/config.toml
database_path = "/home/amin/projects/tauri/tauri-task-timer/apps/web/local.db"
user_email = "you@example.com"
# timer_mode = "focus"  # optional override
```

Register the user in the web app first if the email is not already in `users`.

Apply the web migrations once. The current schema splits a task into an identity
row (`tasks`) plus one row per day worked (`task_days`); the older flat
`tasks.work_date` column no longer exists, so a stale DB fails outright:

```bash
pnpm --filter sv-task-timer db:migrate
```

## Run

From the repo root:

```bash
pnpm dev:tui
# or
cargo run -p task-timer-tui
```

```
task-timer-tui [--config PATH] [--date YYYY-MM-DD] [--db PATH]
```

Web and TUI can open the same DB (WAL). Last write wins on conflicts; the TUI
reloads every second.

## Keys

Press `?` in the app for the full map. Daily view, vim-style navigation,
`n`/`e`/`d` for CRUD, Space to start/stop, `x` to copy the daily markdown report,
`o` to open the selected task's JIRA issue in the browser.

## JIRA

`Ctrl+J` opens the JIRA menu: post a comment, transition the issue, or run a
sprint picker (unassigned / reporter undone / assignee undone) or fetch one issue
by key. A fetched issue becomes a task labelled `KEY summary`, and both its
`jira/issue_key` and the status JIRA reported are stored in `task_integrations`.

Reading the ticket in full is the browser's job: `o` (in the daily list, the
archive, or the detail view) hands `{JIRA_SITE}/browse/{KEY}` to your default
browser. The detail view (`i`) shows the stored integration rows alongside
comments and PR state.

### `jira ensure` (headless)

A sprint key that has no timer task is invisible to everything downstream: the
sprint runner writes agent state with `integration set`, which resolves a task
*identity*, so a ticket it has not got a task for is skipped as
`integration-skipped reason=no-timer-task` and never gets a badge or a
`status set`. `jira ensure` is the explicit way to create that task from a shell
— the runner itself deliberately never creates one.

```bash
task-timer-tui --json jira ensure US-2449
```

It fetches the issue, then creates-or-reuses the task labelled `KEY summary` plus
today's day row (or `--date`'s), and refreshes the `jira/issue_key` and
`jira/status` integration rows. It is idempotent — a second run duplicates
nothing — and it does **not** start the timer or move the task's status: the E6
hooks and the operator still own that. The key is validated before any request,
so a typo fails locally instead of as a JIRA 404.

The same ingest runs when you fetch by key or run a sprint picker in the TUI
(`Ctrl+J`); both go through `db::tasks::ingest_jira_issues`, so the label format
and the stored rows cannot drift between the menu and the CLI.

## Status

The `status` field in the edit form is picked from the JIRA catalog, not typed:
focus it with `Tab` and press any key to open the list (`j`/`k` to move, `0`–`8`
to jump, `Enter` to choose, `Esc` to keep what was there). The list is
`JIRA_STATUSES` in `src/jira.rs`, with the ids the web dashboard and JIRA use.

The status also follows the timer, so the list shows what is running without an
edit:

| Event | `status` becomes |
|---|---|
| start (`Space`) | In Progress (`21`) |
| stop (`Space`) | To Do (`11`) |
| reset / reset all (`r` / `R`) | To Do (`11`) |
| done (`D`) | Done (`31`) |
| un-done (`D` again) | To Do (`11`) |

A task already checked done keeps `Done` through a stop; only un-checking it
returns the row to To Do. Focus-mode switch and the agent hooks behave the same
way as a stop. The CLI (`start`, `stop`, `done`, `reset`, `reset-all`) writes the
same field.

Local status is written *before* the JIRA call, so the list is correct even while
JIRA is slow or unreachable. A hand-set status (Local OK, BLOCKED) does not
survive these events — the timer owns the field.

`status set` moves it from a shell, without touching JIRA:

```bash
task-timer-tui --json status set US-2449 "Local OK"
task-timer-tui --json status set US-2449 41          # same thing by id
```

The status is picked from the same `JIRA_STATUSES` catalog the edit form uses, so
an unknown value is rejected (with the accepted list) rather than written into the
row. Only the day named by `--date` moves; the task's other days keep their own
status. Because the timer owns the field, the next `start` / `stop` / `done` on
that day overwrites it — this is for a driver recording where a ticket actually
is, not for pinning a status the timer would fight.

## Integrations

`task_integrations` holds the per-task key/value rows the detail view shows and
the archive filters on. `integration list` / `integration set` read and write
them from a shell, which is how an external driver (the sprint orchestrator)
records what an agent is doing without touching SQLite directly.

```bash
task-timer-tui --json integration list US-2455
task-timer-tui --json integration set US-2455 jira issue_key US-2455
task-timer-tui --json integration set US-2455 agent status running
task-timer-tui --json integration set US-2455 agent status      # clears to NULL
```

- Rows are keyed by `(task, group, field)`, so `set` on an existing pair
  **updates in place**; it never appends a duplicate.
- An omitted `VALUE` writes `NULL` rather than deleting the row, so the field
  stays addressable and the next `set` updates it.
- The selector resolves the task **identity**, not a day: `--date` does not apply,
  and a task whose last day was last week is still addressable. That is deliberate
  — the rows FK to `tasks.id`, so a JIRA key or agent state does not belong to one
  day's row. A numeric selector is the identity id; a label is matched across every
  day worked.
- Nothing here writes JIRA. The `jira/*` rows are the TUI's own cache of what JIRA
  last reported (sprint fetch, transition); an external writer may set them too,
  but the task's own `status` column is still owned by the timer.

Known groups: `jira` (`issue_key`, `status`), `git` (`branch`), `agent`
(`status`, `session`, `attention`).

### Agent badge

A task carrying `agent/status` shows a badge on its daily-list row, right after
the JIRA status, so the list answers "which ticket has an agent on it" without a
keypress:

```
  [PSB] [Frontend]     00:42:00  To Do  ⚙running
✓ AIMSIS-19331         03:01:12  Done   ⚙!closed
✓ AIMSIS-19925         01:11:41  Done   ⚙closed
```

- `⚙running` (magenta) — the agent is live; `⚙closed` (grey) is history.
- `⚙!status` (red) — `agent/attention` is `true`, i.e. Paseo's
  `requiresAttention`: the agent needs you.
- The status word is Paseo's own (`running`, `idle`, `closed`, `failed`), stored
  verbatim by whoever writes the row — it is not translated here.
- A task with no `agent/status` renders exactly as before: no badge, no empty
  placeholder. A row holding only `agent/attention` gets no badge either.
- Read in `reload()` — one indexed query per second for the whole day, not one
  per task, and no network call. The `i` detail view lists every row.
- The sprint orchestrator is the writer; see `~/.agents/skills/orchestrator`.
