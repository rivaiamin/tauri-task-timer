pub mod tasks;
pub mod user;
pub mod comments;
pub mod integrations;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

pub fn open(path: &Path) -> Result<Connection> {
    if !path.exists() {
        anyhow::bail!(
            "database not found at {}\nRun the web app (or db:migrate) first.",
            path.display()
        );
    }
    let conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_millis(5_000))?;
    Ok(conn)
}
