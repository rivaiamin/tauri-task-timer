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

Apply the web migrations once so `work_date` exists:

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
`n`/`e`/`d` for CRUD, Space to start/stop, `x` to copy the daily markdown report.

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
