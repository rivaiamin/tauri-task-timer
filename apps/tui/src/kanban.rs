use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::db::tasks;
use crate::jira::{self, jql_open_sprint_assignee_undone};

/// Sprint pull summary. `ingested` counts Jira search hits processed through ingest
/// (idempotent re-pulls included), not net-new tasks.
#[derive(Debug, Serialize)]
pub struct KanbanPullReport {
    pub ingested: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct PipelineOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn kanban_pull_sprint(
    conn: &Connection,
    user_id: &str,
    work_date: &str,
    projects: &[String],
) -> Result<KanbanPullReport> {
    if projects.is_empty() {
        bail!("jira_monitor_projects is empty");
    }
    let jql = jql_open_sprint_assignee_undone(projects);
    let issues = jira::search_issues(&jql).with_context(|| format!("JQL: {jql}"))?;
    let ingested = tasks::ingest_jira_issues_with_options(
        conn,
        user_id,
        work_date,
        &issues,
        tasks::IngestJiraOptions {
            ensure_day_row: false,
        },
    )?
    .len();
    let (_, warnings) = tasks::refresh_jira_statuses(conn, user_id)?;
    Ok(KanbanPullReport { ingested, warnings })
}

pub fn kanban_column_order() -> Vec<String> {
    let mut cols: Vec<String> = jira::JIRA_STATUSES
        .iter()
        .filter(|s| !s.label.eq_ignore_ascii_case("Done"))
        .map(|s| s.label.to_string())
        .collect();
    cols.push("Other".to_string());
    cols
}

pub fn column_for_status(status: &str) -> String {
    let trimmed = status.trim();
    if trimmed.is_empty() {
        return "Other".to_string();
    }
    for s in jira::JIRA_STATUSES {
        if s.label.eq_ignore_ascii_case(trimmed) {
            return s.label.to_string();
        }
    }
    "Other".to_string()
}

pub fn column_card_indices(cards: &[tasks::KanbanCard], column: &str) -> Vec<usize> {
    cards
        .iter()
        .enumerate()
        .filter(|(_, c)| column_for_status(&c.jira_status) == column)
        .map(|(i, _)| i)
        .collect()
}

pub fn clamp_kanban_cursor(
    col: &mut usize,
    row: &mut usize,
    columns: &[String],
    cards: &[tasks::KanbanCard],
) {
    if columns.is_empty() {
        *col = 0;
        *row = 0;
        return;
    }
    if *col >= columns.len() {
        *col = columns.len() - 1;
    }
    let indices = column_card_indices(cards, &columns[*col]);
    if indices.is_empty() {
        *row = 0;
    } else if *row >= indices.len() {
        *row = indices.len() - 1;
    }
}

pub fn initial_kanban_cursor(columns: &[String], cards: &[tasks::KanbanCard]) -> (usize, usize) {
    for (col, name) in columns.iter().enumerate() {
        if !column_card_indices(cards, name).is_empty() {
            return (col, 0);
        }
    }
    (0, 0)
}

pub fn move_kanban_col(
    col: &mut usize,
    row: &mut usize,
    delta: i32,
    columns: &[String],
    cards: &[tasks::KanbanCard],
) {
    if columns.is_empty() {
        return;
    }
    let next = (*col as i32 + delta).clamp(0, columns.len() as i32 - 1) as usize;
    *col = next;
    clamp_kanban_cursor(col, row, columns, cards);
}

pub fn move_kanban_row(
    col: usize,
    row: &mut usize,
    delta: i32,
    columns: &[String],
    cards: &[tasks::KanbanCard],
) {
    if columns.is_empty() {
        return;
    }
    let col_name = &columns[col.min(columns.len() - 1)];
    let len = column_card_indices(cards, col_name).len();
    if len == 0 {
        return;
    }
    let next = (*row as i32 + delta).clamp(0, len as i32 - 1) as usize;
    *row = next;
}

pub fn kanban_card_at<'a>(
    cards: &'a [tasks::KanbanCard],
    columns: &[String],
    col: usize,
    row: usize,
) -> Option<&'a tasks::KanbanCard> {
    if columns.is_empty() || col >= columns.len() {
        return None;
    }
    let indices = column_card_indices(cards, &columns[col]);
    let idx = indices.get(row)?;
    cards.get(*idx)
}

fn resolve_bin(configured: Option<String>, default_name: &str) -> Result<PathBuf> {
    if let Some(p) = configured {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Ok(pb);
        }
    }
    let out = Command::new("which").arg(default_name).output()?;
    if out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let s = stdout.trim();
        if !s.is_empty() {
            return Ok(PathBuf::from(s));
        }
    }
    bail!("{default_name} not found on PATH (configure jira_pipelines in config.toml)")
}

fn run_cmd(bin: &PathBuf, args: &[&str]) -> Result<PipelineOutput> {
    let proc = Command::new(bin)
        .args(args)
        .output()
        .with_context(|| format!("spawn {}", bin.display()))?;
    Ok(PipelineOutput {
        stdout: String::from_utf8_lossy(&proc.stdout).to_string(),
        stderr: String::from_utf8_lossy(&proc.stderr).to_string(),
        exit_code: proc.status.code().unwrap_or(-1),
    })
}

pub fn run_local_ok_pr_sync(cfg: &Config, projects: &[String], dry_run: bool, json: bool) -> Result<PipelineOutput> {
    let bin = resolve_bin(
        cfg.jira_pipelines.as_ref().and_then(|p| p.local_ok_pr_sync.clone()),
        "jira-local-ok-pr-sync",
    )?;
    let mut args: Vec<String> = Vec::new();
    if projects.len() == 1 {
        args.push(projects[0].clone());
    }
    if dry_run {
        args.push("--dry-run".into());
    }
    if json {
        args.push("--json".into());
    }
    if projects.len() != 1 {
        args.push("--all".into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_cmd(&bin, &refs)?;
    if out.exit_code != 0 {
        bail!("jira-local-ok-pr-sync failed: {}", out.stderr);
    }
    Ok(out)
}

pub fn run_cek_di_prod_done(
    cfg: &Config,
    projects: &[String],
    days: u32,
    dry_run: bool,
    apply: bool,
    json: bool,
) -> Result<PipelineOutput> {
    let bin = resolve_bin(
        cfg.jira_pipelines.as_ref().and_then(|p| p.cek_di_prod_done.clone()),
        "jira-cek-di-prod-done",
    )?;
    let mut args = vec![format!("--days={days}")];
    if dry_run {
        args.push("--dry-run".into());
    }
    if apply {
        args.push("--apply".into());
    }
    if json {
        args.push("--json".into());
    }
    if projects.len() == 1 {
        args.push(format!("--project={}", projects[0]));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_cmd(&bin, &refs)?;
    if out.exit_code != 0 {
        bail!("jira-cek-di-prod-done failed: {}", out.stderr);
    }
    Ok(out)
}

pub fn run_unassigned_todo(cfg: &Config, project: Option<&str>, json: bool) -> Result<PipelineOutput> {
    let bin = resolve_bin(
        cfg.jira_pipelines.as_ref().and_then(|p| p.unassigned_todo.clone()),
        "jira-unassigned-todo",
    )?;
    let mut args: Vec<&str> = Vec::new();
    if let Some(p) = project {
        args.push(p);
    }
    if json {
        args.push("--json");
    }
    let out = run_cmd(&bin, &args)?;
    if out.exit_code != 0 {
        bail!("jira-unassigned-todo failed: {}", out.stderr);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tasks::KanbanCard;

    fn card(key: &str, status: &str) -> KanbanCard {
        KanbanCard {
            task_id: 1,
            label: key.to_string(),
            issue_key: key.to_string(),
            jira_status: status.to_string(),
            running_today: false,
        }
    }

    fn sample_cards() -> Vec<KanbanCard> {
        vec![
            card("US-1", "To Do"),
            card("US-2", "To Do"),
            card("US-3", "In Progress"),
        ]
    }

    #[test]
    fn initial_cursor_skips_empty_leading_columns() {
        let cols = kanban_column_order();
        let cards = sample_cards();
        let (c, r) = initial_kanban_cursor(&cols, &cards);
        let name = &cols[c];
        assert!(!column_card_indices(&cards, name).is_empty());
        assert_eq!(r, 0);
    }

    #[test]
    fn move_row_does_not_wrap() {
        let cols = kanban_column_order();
        let cards = sample_cards();
        let todo = "To Do".to_string();
        let col = cols.iter().position(|c| c == &todo).unwrap();
        let mut row = 0usize;
        move_kanban_row(col, &mut row, -1, &cols, &cards);
        assert_eq!(row, 0);
        let len = column_card_indices(&cards, &todo).len();
        row = len - 1;
        move_kanban_row(col, &mut row, 1, &cols, &cards);
        assert_eq!(row, len - 1);
    }

    #[test]
    fn move_col_into_empty_lane_yields_no_card() {
        let cols = kanban_column_order();
        let cards = sample_cards();
        let todo = "To Do".to_string();
        let mut col = cols.iter().position(|c| c == &todo).unwrap();
        let mut row = 0usize;
        for _ in 0..cols.len() {
            move_kanban_col(&mut col, &mut row, 1, &cols, &cards);
            if column_card_indices(&cards, &cols[col]).is_empty() {
                assert!(kanban_card_at(&cards, &cols, col, row).is_none());
                return;
            }
        }
    }

    #[test]
    fn clamp_row_after_shrink() {
        let cols = kanban_column_order();
        let cards = sample_cards();
        let todo = "To Do".to_string();
        let mut col = cols.iter().position(|c| c == &todo).unwrap();
        let mut row = 5usize;
        clamp_kanban_cursor(&mut col, &mut row, &cols, &cards);
        let len = column_card_indices(&cards, &todo).len();
        assert_eq!(row, len - 1);
    }
}
