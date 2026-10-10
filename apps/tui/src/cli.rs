use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;

use crate::db;
use crate::db::tasks::Task;
use crate::jira::{self, Worklog};
use crate::report::{self, ReportTask};
use crate::timer::{format_time, now_ms};

#[derive(Parser, Debug)]
#[command(name = "task-timer-tui", subcommand_required = false)]
pub struct Cli {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[arg(long, global = true, value_parser = parse_date, value_name = "YYYY-MM-DD")]
    pub date: Option<NaiveDate>,

    #[arg(long, global = true)]
    pub db: Option<PathBuf>,

    /// JSON output (one-shot commands only)
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("invalid date {s}"))
}

/// JIRA side effects must not fail the command — the local timer already moved —
/// but a transition that never landed must not be silent either. Exit stays 0;
/// the wrappers do not redirect stderr, so this reaches the operator.
fn warn_jira(warnings: &[String]) {
    for warning in warnings {
        eprintln!("JIRA WARNING: {warning}");
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    List,
    Show {
        #[arg(value_name = "TASK")]
        task: String,
    },
    Add {
        label: String,
        #[arg(long)]
        description: Option<String>,
    },
    Start {
        #[arg(value_name = "TASK")]
        task: String,
        #[arg(long, conflicts_with = "parallel")]
        exclusive: bool,
        #[arg(long)]
        parallel: bool,
    },
    Stop {
        #[arg(value_name = "TASK")]
        task: String,
    },
    Reset {
        #[arg(value_name = "TASK")]
        task: String,
    },
    ResetAll,
    Done {
        #[arg(value_name = "TASK")]
        task: String,
        #[arg(long)]
        undo: bool,
    },
    Delete {
        #[arg(value_name = "TASK")]
        task: String,
    },
    /// Read and write `task_integrations` rows (the `jira` / `git` / `agent`
    /// groups the detail view shows and the archive filters on).
    Integration {
        #[command(subcommand)]
        action: IntegrationAction,
    },
    /// Move the selected day's own status without touching JIRA.
    Status {
        #[command(subcommand)]
        action: StatusAction,
    },
    /// Materialize a JIRA ticket as a timer task (see `jira ensure`).
    Jira {
        #[command(subcommand)]
        action: JiraAction,
    },
    Report {
        #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
        format: ReportFormat,
    },
}

#[derive(Subcommand, Debug)]
pub enum IntegrationAction {
    /// Every stored row for one task, ordered by group then field.
    List {
        #[arg(value_name = "TASK")]
        task: String,
    },
    /// Set one `(task, group, field)` row. An omitted VALUE clears the row to
    /// NULL rather than deleting it, so the field stays addressable.
    Set {
        #[arg(value_name = "TASK")]
        task: String,
        group: String,
        field: String,
        #[arg(value_name = "VALUE")]
        value: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum JiraAction {
    /// Fetch one issue by key and make sure the timer has a task for it.
    ///
    /// Creates the identity (labelled `KEY summary`) and today's day row if they
    /// are missing, and refreshes the `jira/issue_key` + `jira/status`
    /// integration rows. Idempotent: a second run duplicates nothing. Does not
    /// start the timer — that stays the operator's or the agent hook's call.
    Ensure {
        #[arg(value_name = "KEY")]
        key: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum StatusAction {
    /// Set the day row's `status`. Accepts a catalog id (`41`) or its label
    /// (`Local OK`); the timer's own events still overwrite it.
    Set {
        #[arg(value_name = "TASK")]
        task: String,
        /// Catalog id or label, e.g. `41` or `Local OK`.
        #[arg(value_name = "STATUS")]
        status: String,
    },
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ReportFormat {
    #[default]
    Markdown,
    Csv,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskOut {
    id: i64,
    label: String,
    description: Option<String>,
    status: String,
    is_running: bool,
    done: bool,
    elapsed_seconds: i64,
    current_elapsed_seconds: i64,
    work_date: String,
}

impl TaskOut {
    fn from_task(task: &Task) -> Self {
        Self {
            id: task.id,
            label: task.label.clone(),
            description: task.description.clone(),
            status: task.status.clone(),
            is_running: task.is_running,
            done: task.done,
            elapsed_seconds: task.elapsed_time,
            current_elapsed_seconds: task.current_elapsed(now_ms()),
            work_date: task.work_date.clone(),
        }
    }
}

fn print_task_line(task: &Task) {
    let elapsed = format_time(task.current_elapsed(now_ms()));
    let marker = if task.is_running { "*" } else { "-" };
    println!("{}\t{}\t{}\t{}", task.id, elapsed, marker, task.label);
}

fn emit_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn emit_task(task: &Task, json: bool) -> Result<()> {
    if json {
        emit_json(&TaskOut::from_task(task))
    } else {
        print_task_line(task);
        Ok(())
    }
}

fn resolve_task(
    conn: &Connection,
    user_id: &str,
    date_iso: &str,
    selector: &str,
) -> Result<Task> {
    let selector = selector.trim();
    if selector.is_empty() {
        bail!("task selector is required");
    }
    // A numeric selector names a task; the day comes from `--date`, so a task
    // with no row for that day is not resolvable on it.
    if selector.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(id) = selector.parse::<i64>() {
            if let Some(task) = db::tasks::get_task_on(conn, user_id, id, date_iso)? {
                return Ok(task);
            }
            bail!("task {selector} is not worked on {date_iso}");
        }
    }
    let matches = db::tasks::find_tasks_by_label(conn, user_id, date_iso, selector)?;
    match matches.as_slice() {
        [] => bail!("task {selector} not found"),
        [task] => Ok(task.clone()),
        rest => {
            let ids = rest
                .iter()
                .map(|t| t.id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("ambiguous label {selector}: ids {ids} (use numeric id)")
        }
    }
}

/// Resolve a `TASK` selector to the task *identity*, not to a day.
///
/// Integration rows FK to the identity, so `--date` must not decide whether the
/// row is addressable: a task not worked today still has a JIRA key and still
/// shows agent state. A numeric selector is the identity id; a label is matched
/// across every day the task was worked. Ambiguity is possible here (unlike the
/// day-scoped `resolve_task`), so it errors rather than guessing.
///
/// The lookup itself is `db::tasks::find_task_identity`, shared with
/// `db::tasks::ingest_jira_issues` so `jira ensure` and the `integration`
/// commands cannot disagree about which task a key names.
fn resolve_task_identity(conn: &Connection, user_id: &str, selector: &str) -> Result<Task> {
    let selector = selector.trim();
    db::tasks::find_task_identity(conn, user_id, selector)?
        .ok_or_else(|| anyhow::anyhow!("task {selector} not found"))
}

pub fn run(
    cmd: Command,
    conn: &Connection,
    user_id: &str,
    date_iso: &str,
    timer_mode: &str,
    json: bool,
) -> Result<()> {
    match cmd {
        Command::List => {
            let tasks = db::tasks::list_tasks(conn, user_id, date_iso)?;
            let now = now_ms();
            let total: i64 = tasks.iter().map(|t| t.current_elapsed(now)).sum();
            if json {
                emit_json(&serde_json::json!({
                    "tasks": tasks.iter().map(TaskOut::from_task).collect::<Vec<_>>(),
                    "totalElapsedSeconds": total,
                }))
            } else {
                for t in &tasks {
                    print_task_line(t);
                }
                println!("total\t{}", format_time(total));
                Ok(())
            }
        }
        Command::Show { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            emit_task(&task, json)
        }
        Command::Add { label, description } => {
            let task = db::tasks::create_task(
                conn,
                user_id,
                date_iso,
                &label,
                description.as_deref(),
            )?;
            emit_task(&task, json)
        }
        Command::Start {
            task,
            exclusive,
            parallel,
        } => {
            let exclusive = if exclusive {
                true
            } else if parallel {
                false
            } else {
                timer_mode == "focus"
            };
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
            let day_id = task.day_id;
            let (started, stopped_rows) =
                db::tasks::start_timer(conn, user_id, day_id, exclusive)?
                    .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            // Local state first: the list is correct even while JIRA is slow.
            // The victim's run is over, so its own day row stops reading In
            // Progress; this is the same demotion the TUI applies on a switch.
            jira::auto_status(conn, user_id, started.day_id);
            for victim in &stopped_rows {
                jira::auto_status(conn, user_id, victim.day_id);
            }
            for victim in &stopped_rows {
                if let Some(worklog) = Worklog::capture(victim, now_ms()) {
                    warn_jira(&worklog.record_and_reopen());
                }
            }
            warn_jira(&jira::fire_on_start(&started.label, started.description_text()));
            // Re-read so the emitted task carries the status the list will show.
            let started = db::tasks::get_task_by_day(conn, user_id, started.day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            emit_task(&started, json)
        }
        Command::Stop { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let worklog = Worklog::capture(&task, now_ms());
            let id = task.id;
            let day_id = task.day_id;
            db::tasks::stop_timer(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            // A stop on an unfinished task reads To Do again. Written before the
            // JIRA call so the row is correct even while JIRA is slow.
            jira::auto_status(conn, user_id, day_id);
            if let Some(worklog) = &worklog {
                warn_jira(&worklog.record_and_reopen());
            }
            let task = db::tasks::get_task_by_day(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            emit_task(&task, json)
        }
        Command::Reset { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
            let day_id = task.day_id;
            db::tasks::reset_task(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            // A reset stops the timer, so the row is no longer In Progress.
            jira::auto_status(conn, user_id, day_id);
            let task = db::tasks::get_task_by_day(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            emit_task(&task, json)
        }
        Command::ResetAll => {
            let day_ids: Vec<i64> = db::tasks::list_tasks(conn, user_id, date_iso)?
                .iter()
                .map(|t| t.day_id)
                .collect();
            db::tasks::reset_all(conn, user_id, date_iso)?;
            for day_id in day_ids {
                jira::auto_status(conn, user_id, day_id);
            }
            if json {
                emit_json(&serde_json::json!({ "ok": true, "workDate": date_iso }))
            } else {
                println!("reset-all\t{date_iso}");
                Ok(())
            }
        }
        Command::Done { task, undo } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
            let day_id = task.day_id;
            let Some((task, delta, start_ms)) = db::tasks::set_done(conn, user_id, day_id, !undo)?
            else {
                bail!("task {id} not found");
            };
            // Local state first: the row reads Done / To Do immediately, whether
            // or not the JIRA hook below can be reached.
            jira::auto_status(conn, user_id, day_id);
            if !undo {
                warn_jira(&jira::fire_on_done(
                    &task.label,
                    task.description_text(),
                    delta,
                    start_ms,
                ));
            }
            let task = db::tasks::get_task_by_day(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            emit_task(&task, json)
        }
        Command::Delete { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
            // Deleting removes the task itself, and its days with it — the
            // selector's day only decided which task was meant.
            if !db::tasks::delete_task(conn, user_id, id)? {
                bail!("task {id} not found");
            }
            if json {
                emit_json(&serde_json::json!({ "deleted": id }))
            } else {
                println!("deleted\t{id}");
                Ok(())
            }
        }
        Command::Integration { action } => {
            match action {
                IntegrationAction::List { task } => {
                    let task = resolve_task_identity(conn, user_id, &task)?;
                    let rows = db::integrations::list(conn, task.id)?;
                    if json {
                        emit_json(
                            &rows
                                .iter()
                                .map(|r| {
                                    serde_json::json!({
                                        "group": r.group,
                                        "field": r.field,
                                        "value": r.value,
                                    })
                                })
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        for r in &rows {
                            println!(
                                "{}\t{}\t{}",
                                r.group,
                                r.field,
                                r.value.as_deref().unwrap_or("")
                            );
                        }
                        Ok(())
                    }
                }
                IntegrationAction::Set {
                    task,
                    group,
                    field,
                    value,
                } => {
                    let task = resolve_task_identity(conn, user_id, &task)?;
                    // `upsert` keys on (task, group, field), so re-setting a field
                    // updates in place — the agent's current state replaces its
                    // last, it does not accumulate rows.
                    db::integrations::upsert(conn, task.id, &group, &field, value.as_deref())?;
                    if json {
                        emit_json(&serde_json::json!({
                            "group": group,
                            "field": field,
                            "value": value,
                        }))
                    } else {
                        println!("set\t{}\t{}\t{}", group, field, value.as_deref().unwrap_or(""));
                        Ok(())
                    }
                }
            }
        }
        Command::Status { action } => {
            match action {
                StatusAction::Set { task, status } => {
                    let task = resolve_task(conn, user_id, date_iso, &task)?;
                    // The catalog is the single source of the ids the dashboard and
                    // JIRA use, so a typo is rejected here rather than written into
                    // the row and rendered as a raw id everywhere downstream.
                    let Some(status_id) = jira::resolve_status_id(&status) else {
                        bail!(
                            "unknown status {status:?}; expected one of: {}",
                            jira::JIRA_STATUSES
                                .iter()
                                .map(|s| format!("{} ({})", s.label, s.id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    };
                    let id = task.id;
                    let day_id = task.day_id;
                    // Only the day row's status moves: `update_task` also writes the
                    // identity, and this must not touch a field it was not asked to.
                    let changed = conn.execute(
                        "UPDATE task_days SET status = ?1, updated_at = ?2 WHERE id = ?3",
                        rusqlite::params![status_id, now_ms(), day_id],
                    )?;
                    if changed == 0 {
                        bail!("task {id} not found");
                    }
                    let task = db::tasks::get_task_by_day(conn, user_id, day_id)?
                        .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
                    emit_task(&task, json)
                }
            }
        }
        Command::Jira { action } => match action {
            JiraAction::Ensure { key } => {
                // `fetch_issue` validates the key shape before any request, so a
                // typo fails here rather than as a JIRA 404.
                let issue = jira::fetch_issue(&key)
                    .with_context(|| format!("fetch JIRA issue {key}"))?;
                let rows = db::tasks::ingest_jira_issues(conn, user_id, date_iso, &[issue])?;
                let task = rows
                    .into_iter()
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("JIRA returned no issue for {key}"))?;
                emit_task(&task, json)
            }
        },
        Command::Report { format } => {
            let tasks = db::tasks::list_tasks(conn, user_id, date_iso)?;
            let now = now_ms();
            let report_tasks: Vec<ReportTask<'_>> = tasks
                .iter()
                .map(|t| ReportTask {
                    label: &t.label,
                    description: t.description.as_deref(),
                    elapsed_seconds: t.current_elapsed(now),
                })
                .collect();
            let text = match format {
                ReportFormat::Markdown => report::build_markdown_report(&report_tasks, date_iso),
                ReportFormat::Csv => report::build_csv_report(&report_tasks),
            };
            println!("{text}");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema the CLI writes against: identity + day rows + integrations,
    /// mirroring `apps/web/drizzle`. Integration rows FK to the identity, which
    /// is the whole point of these tests.
    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL,
                                 label TEXT NOT NULL, description TEXT, code TEXT, link TEXT,
                                 notes TEXT, tags TEXT, created_at INTEGER NOT NULL DEFAULT 0,
                                 updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
             CREATE TABLE task_days (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL,
                                     work_date TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'todo',
                                     elapsed_time INTEGER NOT NULL DEFAULT 0,
                                     total_time INTEGER NOT NULL DEFAULT 0,
                                     position INTEGER NOT NULL DEFAULT 0,
                                     is_running INTEGER NOT NULL DEFAULT 0,
                                     done INTEGER NOT NULL DEFAULT 0,
                                     is_completed INTEGER NOT NULL DEFAULT 0,
                                     is_cancelled INTEGER NOT NULL DEFAULT 0,
                                     is_deleted INTEGER NOT NULL DEFAULT 0,
                                     is_archived INTEGER NOT NULL DEFAULT 0,
                                     is_pinned INTEGER NOT NULL DEFAULT 0,
                                     is_important INTEGER NOT NULL DEFAULT 0,
                                     start_time INTEGER, end_time INTEGER,
                                     created_at INTEGER NOT NULL DEFAULT 0,
                                     updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
             CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT,
                                             task_id INTEGER NOT NULL, \"group\" TEXT NOT NULL,
                                             field TEXT NOT NULL, value TEXT,
                                             created_at INTEGER NOT NULL,
                                             updated_at INTEGER NOT NULL);",
        )
        .unwrap();
        conn
    }

    /// One identity worked on two days, the older day being the one a
    /// date-scoped lookup would miss.
    fn seed(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-1');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01'), (1, '2026-09-17');",
        )
        .unwrap();
    }

    fn integration_rows(conn: &Connection) -> Vec<(String, String, Option<String>)> {
        db::integrations::list(conn, 1)
            .unwrap()
            .into_iter()
            .map(|r| (r.group, r.field, r.value))
            .collect()
    }

    /// `jira ensure`'s write path: one identity, one day row, two `jira/*` rows.
    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn ingest_creates_the_identity_the_day_row_and_the_jira_rows() {
        let conn = setup();
        let issues = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "In Progress".to_string(),
        )];
        let rows = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &issues).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "US-1459 Fix login");
        assert_eq!(rows[0].work_date, "2026-09-17");
        // The fetched status is cached as an integration row, and the task's own
        // status column is left to the timer.
        assert_eq!(
            integration_rows(&conn),
            vec![
                ("jira".into(), "issue_key".into(), Some("US-1459".into())),
                ("jira".into(), "status".into(), Some("In Progress".into())),
            ]
        );
        assert_eq!(rows[0].status, "todo", "a fetch must not move the timer's status");
        assert!(!rows[0].is_running, "a fetch must not start the timer");
    }

    /// The idempotency claim: running it twice adds no row and no duplicate, and
    /// the stored status is refreshed in place rather than appended.
    #[test]
    fn ingest_twice_duplicates_nothing_and_updates_the_status_in_place() {
        let conn = setup();
        let first = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "In Progress".to_string(),
        )];
        let a = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &first).unwrap();

        let second = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "Local OK".to_string(),
        )];
        let b = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &second).unwrap();

        assert_eq!(a[0].id, b[0].id, "the same label must resolve to the same identity");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM task_days"), 1);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM task_integrations"),
            2,
            "a re-run must update the two rows, not append two more"
        );
        assert_eq!(
            integration_rows(&conn)[1].2.as_deref(),
            Some("Local OK"),
            "the refreshed JIRA status replaces the stored one"
        );
    }

    /// The same ticket worked on a second day gains a day row, not a second task
    /// — the identity upsert is what keeps `integration set` addressable across
    /// days.
    #[test]
    fn ingest_on_a_new_day_adds_a_day_row_to_the_same_identity() {
        let conn = setup();
        let issues = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "In Progress".to_string(),
        )];
        let a = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &issues).unwrap();
        let b = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-18", &issues).unwrap();

        assert_eq!(a[0].id, b[0].id);
        assert_eq!(b[0].work_date, "2026-09-18");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM task_days"), 2);
    }

    /// An issue JIRA reports without a status must not blank the status already
    /// stored — a later fetch that omits the field is not evidence it changed.
    #[test]
    fn ingest_keeps_the_stored_status_when_the_fetch_reports_none() {
        let conn = setup();
        let with_status = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "In Progress".to_string(),
        )];
        db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &with_status).unwrap();

        let without = vec![("US-1459".to_string(), "Fix login".to_string(), String::new())];
        db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &without).unwrap();

        assert_eq!(
            integration_rows(&conn)[1].2.as_deref(),
            Some("In Progress"),
            "an empty fetched status must not overwrite a known one"
        );
    }

    /// A ticket already in the timer must be reused, not duplicated. A bare
    /// `US-2449` and the fetch's `US-2449 <summary>` are the same ticket, and
    /// creating a second identity would split its integration rows — the sprint
    /// runner resolves the key by exact label and would read the empty one.
    #[test]
    fn ingest_reuses_an_existing_bare_key_task_instead_of_creating_a_second() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-2449');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-17');",
        )
        .unwrap();

        let issues = vec![(
            "US-2449".to_string(),
            "Upload survey files".to_string(),
            "In Progress".to_string(),
        )];
        let rows = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &issues).unwrap();

        assert_eq!(rows[0].id, 1, "the existing identity must be reused");
        assert_eq!(
            rows[0].label, "US-2449",
            "an existing label is the operator's, not the fetch's to rewrite"
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks"), 1, "no second identity");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM task_days"), 1, "no second day row");
        // The rows land on the identity the runner would resolve the key to.
        assert_eq!(integration_rows(&conn)[0].2.as_deref(), Some("US-2449"));
    }

    /// The same key fetched on a later day reuses the identity and gains a day
    /// row, so agent state stays addressable across days.
    #[test]
    fn ingest_reuses_the_identity_across_days() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-2449');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-16');",
        )
        .unwrap();

        let issues = vec![(
            "US-2449".to_string(),
            "Upload survey files".to_string(),
            "In Progress".to_string(),
        )];
        let rows = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &issues).unwrap();

        assert_eq!(rows[0].id, 1);
        assert_eq!(rows[0].work_date, "2026-09-17");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM task_days"), 2);
    }

    /// Two tasks matching the key prefix is ambiguous: the ingest must refuse
    /// rather than pick one or create a third.
    #[test]
    fn ingest_refuses_an_ambiguous_key_rather_than_guessing() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-7 alpha'), ('u1', 'US-7 beta');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-17'), (2, '2026-09-17');",
        )
        .unwrap();

        let issues = vec![(
            "US-7".to_string(),
            "Some summary".to_string(),
            "In Progress".to_string(),
        )];
        let err = db::tasks::ingest_jira_issues(&conn, "u1", "2026-09-17", &issues).unwrap_err();

        assert!(err.to_string().contains("ambiguous"), "got: {err}");
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM tasks"), 2, "nothing may be created");
    }

    #[test]
    fn identity_lookup_finds_a_task_not_worked_on_the_requested_date() {
        let conn = setup();
        seed(&conn);

        // The day-scoped resolver the timer commands use cannot see this task on
        // a date it has no row for...
        assert!(resolve_task(&conn, "u1", "2026-10-10", "US-1").is_err());
        // ...but integration rows belong to the identity, so this must.
        let task = resolve_task_identity(&conn, "u1", "US-1").unwrap();
        assert_eq!(task.id, 1);
        assert_eq!(
            resolve_task_identity(&conn, "u1", "1").unwrap().id,
            1,
            "numeric selector names the identity too"
        );
    }

    #[test]
    fn integration_set_upserts_one_row_per_group_and_field() {
        let conn = setup();
        seed(&conn);

        let set = |group: &str, field: &str, value: Option<&str>| {
            run(
                Command::Integration {
                    action: IntegrationAction::Set {
                        task: "US-1".into(),
                        group: group.into(),
                        field: field.into(),
                        value: value.map(str::to_string),
                    },
                },
                &conn,
                "u1",
                "2026-10-10",
                "parallel",
                true,
            )
            .unwrap();
        };

        set("jira", "issue_key", Some("US-1"));
        set("jira", "status", Some("To Do"));
        set("jira", "status", Some("Local OK"));

        assert_eq!(
            integration_rows(&conn),
            vec![
                ("jira".into(), "issue_key".into(), Some("US-1".into())),
                ("jira".into(), "status".into(), Some("Local OK".into())),
            ],
            "re-setting a field replaces its value instead of adding a row"
        );
    }

    #[test]
    fn clearing_a_value_keeps_the_row_addressable() {
        let conn = setup();
        seed(&conn);

        let set = |value: Option<&str>| {
            run(
                Command::Integration {
                    action: IntegrationAction::Set {
                        task: "US-1".into(),
                        group: "agent".into(),
                        field: "status".into(),
                        value: value.map(str::to_string),
                    },
                },
                &conn,
                "u1",
                "2026-10-10",
                "parallel",
                true,
            )
            .unwrap();
        };

        set(Some("running"));
        set(None);

        assert_eq!(
            integration_rows(&conn),
            vec![("agent".into(), "status".into(), None)],
            "a cleared field is still listed, so the next write updates it"
        );
    }

    #[test]
    fn integration_list_reads_back_what_set_wrote() {
        let conn = setup();
        seed(&conn);

        for (group, field, value) in [("jira", "issue_key", "US-1"), ("git", "branch", "US-1-fix")] {
            run(
                Command::Integration {
                    action: IntegrationAction::Set {
                        task: "US-1".into(),
                        group: group.into(),
                        field: field.into(),
                        value: Some(value.into()),
                    },
                },
                &conn,
                "u1",
                "2026-10-10",
                "parallel",
                true,
            )
            .unwrap();
        }

        assert_eq!(
            integration_rows(&conn),
            vec![
                ("git".into(), "branch".into(), Some("US-1-fix".into())),
                ("jira".into(), "issue_key".into(), Some("US-1".into())),
            ],
            "list is ordered by group then field"
        );
    }

    #[test]
    fn an_unknown_selector_is_an_error_not_an_empty_result() {
        let conn = setup();
        seed(&conn);

        let err = resolve_task_identity(&conn, "u1", "US-404").unwrap_err();
        assert_eq!(err.to_string(), "task US-404 not found");
    }

    #[test]
    fn a_label_is_scoped_to_its_own_user() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-2'), ('u2', 'US-2');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01'), (2, '2026-09-01');",
        )
        .unwrap();

        // The unique index is per (user_id, label), so a label can only repeat
        // across users — which is exactly what the scoping has to guard.
        assert_eq!(resolve_task_identity(&conn, "u1", "US-2").unwrap().id, 1);
        assert_eq!(resolve_task_identity(&conn, "u2", "US-2").unwrap().id, 2);
    }

    /// The JIRA sprint fetch writes `KEY summary` labels, so a bare key has to
    /// reach them or the orchestrator cannot address half the sprint.
    #[test]
    fn a_bare_key_reaches_a_task_labelled_key_and_summary() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-7 [Rework]');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01');",
        )
        .unwrap();

        assert_eq!(resolve_task_identity(&conn, "u1", "US-7").unwrap().id, 1);
    }

    #[test]
    fn an_exact_label_wins_over_a_longer_label_sharing_its_prefix() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-7'), ('u1', 'US-7 [Rework]');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01'), (2, '2026-09-01');",
        )
        .unwrap();

        assert_eq!(
            resolve_task_identity(&conn, "u1", "US-7").unwrap().id,
            1,
            "the task actually labelled US-7 is not shadowed by its longer sibling"
        );
    }

    #[test]
    fn a_prefix_does_not_match_a_longer_number() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-10 fixed');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01');",
        )
        .unwrap();

        // `US-1 %` must not match `US-10 fixed`, or US-1 would write rows onto
        // the wrong ticket.
        let err = resolve_task_identity(&conn, "u1", "US-1").unwrap_err();
        assert_eq!(err.to_string(), "task US-1 not found");
        assert_eq!(resolve_task_identity(&conn, "u1", "US-10").unwrap().id, 1);
    }

    #[test]
    fn two_tasks_sharing_a_key_prefix_are_ambiguous_rather_than_guessed() {
        let conn = setup();
        conn.execute_batch(
            "INSERT INTO tasks (user_id, label) VALUES ('u1', 'US-7 alpha'), ('u1', 'US-7 beta');
             INSERT INTO task_days (task_id, work_date) VALUES (1, '2026-09-01'), (2, '2026-09-01');",
        )
        .unwrap();

        let err = resolve_task_identity(&conn, "u1", "US-7").unwrap_err();
        assert!(
            err.to_string().starts_with("ambiguous label US-7: ids 1, 2"),
            "{err}"
        );
    }

    fn day_status(conn: &Connection, day_id: i64) -> String {
        conn.query_row(
            "SELECT status FROM task_days WHERE id = ?1",
            [day_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn set_status(conn: &Connection, selector: &str, status: &str) -> Result<()> {
        run(
            Command::Status {
                action: StatusAction::Set {
                    task: selector.into(),
                    status: status.into(),
                },
            },
            conn,
            "u1",
            "2026-09-17",
            "parallel",
            true,
        )
    }

    #[test]
    fn status_set_accepts_a_catalog_label_or_id() {
        let conn = setup();
        seed(&conn);
        // The selector resolves the day named by --date, so this is day 2.
        set_status(&conn, "US-1", "Local OK").unwrap();
        assert_eq!(day_status(&conn, 2), "41");
        set_status(&conn, "US-1", "51").unwrap();
        assert_eq!(day_status(&conn, 2), "51");
    }

    #[test]
    fn status_set_rejects_a_status_outside_the_catalog() {
        let conn = setup();
        seed(&conn);

        let err = set_status(&conn, "US-1", "Bogus").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown status"), "{msg}");
        assert!(msg.contains("Local OK (41)"), "names the accepted values: {msg}");
        assert_eq!(day_status(&conn, 2), "todo", "nothing was written");
    }

    #[test]
    fn status_set_moves_only_the_selected_day() {
        let conn = setup();
        seed(&conn);

        set_status(&conn, "US-1", "Local OK").unwrap();

        assert_eq!(day_status(&conn, 2), "41", "the --date day moved");
        assert_eq!(
            day_status(&conn, 1),
            "todo",
            "the task's other day is untouched"
        );
    }
}
