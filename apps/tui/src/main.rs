mod app;
mod config;
mod db;
mod jira;
mod timer;
mod ui;

use anyhow::{Context, Result};
use chrono::{Local, NaiveDate};
use std::env;
use std::path::{Path, PathBuf};

use app::App;

struct Args {
    config: Option<PathBuf>,
    date: Option<NaiveDate>,
    db: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut config = None;
    let mut date = None;
    let mut db = None;
    let mut argv = env::args().skip(1);
    while let Some(a) = argv.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!(
                    "task-timer-tui [--config PATH] [--date YYYY-MM-DD] [--db PATH]\n\n\
                     Config default: ~/.config/task-timer-tui/config.toml"
                );
                std::process::exit(0);
            }
            "--config" => {
                config = Some(PathBuf::from(argv.next().context("--config needs a path")?));
            }
            "--date" => {
                let s = argv.next().context("--date needs YYYY-MM-DD")?;
                date = Some(
                    NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                        .with_context(|| format!("invalid date {s}"))?,
                );
            }
            "--db" => {
                db = Some(PathBuf::from(argv.next().context("--db needs a path")?));
            }
            other => anyhow::bail!("unknown arg {other}  (try --help)"),
        }
    }
    Ok(Args { config, date, db })
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let cfg = config::load(args.config.as_deref())?;
    let db_path = args.db.unwrap_or_else(|| PathBuf::from(&cfg.database_path));
    let conn = db::open(Path::new(&db_path))?;
    let user_id = db::user::resolve_user_id(&conn, &cfg.user_email)?;
    let mut timer_mode = db::user::get_timer_mode(&conn, &user_id)?;
    if let Some(m) = cfg.timer_mode.as_deref() {
        if m == "focus" || m == "parallel" {
            timer_mode = m.to_string();
        }
    }
    let date = args.date.unwrap_or_else(|| Local::now().date_naive());
    let mut app = App::new(conn, user_id, date, timer_mode)?;
    app.run()
}
