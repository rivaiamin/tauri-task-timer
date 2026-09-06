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
