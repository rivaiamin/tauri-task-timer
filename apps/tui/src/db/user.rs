use anyhow::{bail, Result};
use rusqlite::Connection;

pub fn resolve_user_id(conn: &Connection, email: &str) -> Result<String> {
    match conn.query_row("SELECT id FROM users WHERE email = ?1", [email], |r| {
        r.get::<_, String>(0)
    }) {
        Ok(id) => Ok(id),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            bail!("No user with email {email}. Register via the web app first.")
        }
        Err(e) => Err(e.into()),
    }
}

pub fn get_timer_mode(conn: &Connection, user_id: &str) -> Result<String> {
    let mode: Result<String, _> = conn.query_row(
        "SELECT timer_mode FROM user_settings WHERE user_id = ?1",
        [user_id],
        |r| r.get(0),
    );
    Ok(mode.unwrap_or_else(|_| "focus".into()))
}

pub fn set_timer_mode(conn: &Connection, user_id: &str, mode: &str, now_ms: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO user_settings (user_id, timer_mode, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(user_id) DO UPDATE SET timer_mode = excluded.timer_mode, updated_at = excluded.updated_at",
        rusqlite::params![user_id, mode, now_ms],
    )?;
    Ok(())
}
