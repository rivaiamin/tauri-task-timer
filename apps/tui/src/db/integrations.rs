use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
pub struct Integration {
    pub id: i64,
    pub task_id: i64,
    pub group: String,
    pub field: String,
    pub value: Option<String>,
}

pub fn list(conn: &Connection, task_id: i64) -> Result<Vec<Integration>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, group, field, value FROM task_integrations
         WHERE task_id = ?1 ORDER BY group ASC, field ASC",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(Integration {
            id: r.get(0)?,
            task_id: r.get(1)?,
            group: r.get(2)?,
            field: r.get(3)?,
            value: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn upsert(conn: &Connection, task_id: i64, group: &str, field: &str, value: Option<&str>) -> Result<()> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM task_integrations WHERE task_id = ?1 AND group = ?2 AND field = ?3",
            params![task_id, group, field],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => {
            conn.execute("UPDATE task_integrations SET value = ?1, updated_at = ?2 WHERE id = ?3", params![value, crate::timer::now_ms(), id])?;
        }
        None => {
            let now = crate::timer::now_ms();
            conn.execute("INSERT INTO task_integrations (task_id, group, field, value, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)", params![task_id, group, field, value, now])?;
        }
    }
    Ok(())
}

use rusqlite::OptionalExtension;
