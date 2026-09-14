---
name: task-timer
description: Drive the local Task Timer CLI (task-timer-tui) to list, create, start, stop, reset, complete, delete, and report daily tasks. Use when tracking time, starting or stopping a timer, using JIRA keys as task labels (US-2092, AIMSIS-16872), or exporting a daily markdown/CSV report. Requires task-timer-tui on PATH; do not open the TUI.
compatibility: Requires task-timer-tui on PATH and ~/.config/task-timer-tui/config.toml
---

# Task Timer CLI

Drive daily time tracking with `task-timer-tui`. Do not open the TUI, call MCP, hit HTTP `/api`, or run `scripts/task-timer-hook`.

## Preconditions

1. First action: `command -v task-timer-tui`.
2. If missing, tell the user to put the release binary on PATH (`pnpm build:tui` → `target/release/task-timer-tui`). Do not `cargo run` from a random project.
3. Config is `~/.config/task-timer-tui/config.toml` (`database_path`, `user_email`). Do not pass `--config` or `--db` unless the user names a path.
4. Default work date is today. Pass `--date YYYY-MM-DD` only when the user names another day.

## Invocation rules

- Never invoke the binary with no subcommand (that opens ratatui and hangs).
- `--json` without a subcommand errors: `--json requires a command`.
- Always pass `--json` on one-shot commands except `report` (markdown/csv text, not JSON).
- Hyphenated JIRA keys are not clap flags. If a parser eats `US-2092`, use `task-timer-tui start -- US-2092`.

## TASK selector

One positional `TASK`:

- All digits → numeric id (any date).
- Else exact same-day label, ASCII case-insensitive.
- Prefer the JIRA key as `TASK` when the user names one (`US-2092`, `AIMSIS-16872`).
- Full labels with extra text (`US-2092 fix login`) must be quoted as the whole label or use the id.
- Missing → stderr `task {selector} not found`, exit 1. Do not auto-create; `add` first.
- Ambiguous case-only duplicates → `ambiguous label {selector}: ids a, b (use numeric id)`.

## Commands

```bash
task-timer-tui --json list
task-timer-tui --json show TASK
task-timer-tui --json add LABEL
task-timer-tui --json add LABEL --description TEXT
task-timer-tui --json start TASK
task-timer-tui --json start TASK --exclusive
task-timer-tui --json start TASK --parallel
task-timer-tui --json stop TASK
task-timer-tui --json reset TASK
task-timer-tui --json reset-all
task-timer-tui --json done TASK
task-timer-tui --json done TASK --undo
task-timer-tui --json delete TASK
task-timer-tui report
task-timer-tui report --format csv
```

`--exclusive` and `--parallel` are mutually exclusive. Omit both to use config/user timer mode.

## Output

Without `--json`: `{id}\t{HH:MM:SS}\t{*|-}\t{label}`. `list` then prints `total\t{HH:MM:SS}`. `delete` prints `deleted\t{id}`. `reset-all` prints `reset-all\t{date}`. `*` means running, `-` means stopped.

JSON single task (camelCase): `{id,label,description,status,isRunning,done,elapsedSeconds,currentElapsedSeconds,workDate}`.

- `list`: `{tasks:[...], totalElapsedSeconds}`
- `delete`: `{deleted: id}`
- `reset-all`: `{ok: true, workDate}`

## Workflow

1. `list` if the target is unknown.
2. `add` the JIRA key/label if missing.
3. `start` when work begins; then check `isRunning` is true.
4. `stop` when pausing.
5. `done` only if the user says the work is finished.
6. `delete` / `reset` / `reset-all` only if the user asked.

Prefer labels over memorizing ids.

JIRA side effects (same fire-and-forget hooks as the TUI; CLI stdout stays the task JSON/line):

- `start` → JIRA **In Progress** (and exclusive peers → worklog + **To Do**).
- `stop` → JIRA worklog only; status unchanged.
- `done` → JIRA worklog + **Cek di Local** (`JIRA_STATUS_DONE`, default `Cek di Local`). Still only when the user/pipeline says the work is finished.
