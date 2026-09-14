use anyhow::{bail, Result};
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
    Report {
        #[arg(long, value_enum, default_value_t = ReportFormat::Markdown)]
        format: ReportFormat,
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
    if selector.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(id) = selector.parse::<i64>() {
            if let Some(task) = db::tasks::get_task(conn, user_id, id)? {
                return Ok(task);
            }
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
            let stopped: Vec<Worklog> = if exclusive {
                let now = now_ms();
                db::tasks::list_tasks(conn, user_id, date_iso)?
                    .iter()
                    .filter(|t| t.id != task.id)
                    .filter_map(|t| Worklog::capture(t, now))
                    .collect()
            } else {
                vec![]
            };
            let id = task.id;
            let started = db::tasks::start_timer(conn, user_id, id, exclusive)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            for worklog in &stopped {
                warn_jira(&worklog.record_and_reopen());
            }
            warn_jira(&jira::fire_on_start(&started.label, started.description_text()));
            emit_task(&started, json)
        }
        Command::Stop { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let worklog = Worklog::capture(&task, now_ms());
            let id = task.id;
            let task = db::tasks::stop_timer(conn, user_id, id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            if let Some(worklog) = &worklog {
                warn_jira(&worklog.record());
            }
            emit_task(&task, json)
        }
        Command::Reset { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
            let task = db::tasks::reset_task(conn, user_id, id)?
                .ok_or_else(|| anyhow::anyhow!("task {id} not found"))?;
            emit_task(&task, json)
        }
        Command::ResetAll => {
            db::tasks::reset_all(conn, user_id, date_iso)?;
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
            let Some((task, delta, start_ms)) = db::tasks::set_done(conn, user_id, id, !undo)? else {
                bail!("task {id} not found");
            };
            if !undo {
                warn_jira(&jira::fire_on_done(
                    &task.label,
                    task.description_text(),
                    delta,
                    start_ms,
                ));
            }
            emit_task(&task, json)
        }
        Command::Delete { task } => {
            let task = resolve_task(conn, user_id, date_iso, &task)?;
            let id = task.id;
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
