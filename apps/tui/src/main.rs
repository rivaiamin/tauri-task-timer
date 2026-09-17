mod app;
mod bitbucket;
mod cli;
mod clipboard;
mod config;
mod db;
mod git;
mod hooks;
mod jira;
mod report;
mod timer;
mod ui;

use anyhow::{bail, Result};
use chrono::Local;
use clap::Parser;
use std::path::{Path, PathBuf};

use app::App;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    if cli.json && cli.command.is_none() {
        bail!("--json requires a command");
    }
    let cfg = config::load(cli.config.as_deref())?;
    let db_path = cli.db.unwrap_or_else(|| PathBuf::from(&cfg.database_path));
    let conn = db::open(Path::new(&db_path))?;
    let user_id = db::user::resolve_user_id(&conn, &cfg.user_email)?;
    let mut timer_mode = db::user::get_timer_mode(&conn, &user_id)?;
    if let Some(m) = cfg.timer_mode.as_deref() {
        if m == "focus" || m == "parallel" {
            timer_mode = m.to_string();
        }
    }
    let date = cli.date.unwrap_or_else(|| Local::now().date_naive());
    let date_iso = date.format("%Y-%m-%d").to_string();
    match cli.command {
        None => {
            let repo_root = cfg.resolve_repo_root();
            let mut app = App::new(app::AppConfig {
                conn,
                user_id,
                date,
                timer_mode,
                jira_board: cfg.jira_board.clone(),
                jira_sprint_id: cfg.jira_sprint_id.clone(),
                git_repo_path: repo_root,
                bitbucket_workspace: cfg.bitbucket_workspace.clone(),
                bitbucket_repo: cfg.bitbucket_repo.clone(),
            })?;
            app.run()
        }
        Some(cmd) => cli::run(cmd, &conn, &user_id, &date_iso, &timer_mode, cli.json),
    }
}
