use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
pub struct Comment {
    pub id: i64,
    pub task_id: i64,
    pub subject: Option<String>,
    pub summary: Option<String>,
    pub branch: Option<String>,
    pub pr: Option<String>,
    pub created_at: i64,
}

pub fn list(conn: &Connection, task_id: i64) -> Result<Vec<Comment>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, subject, summary, branch, pr, created_at
         FROM task_comments WHERE task_id = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(Comment {
            id: r.get(0)?,
            task_id: r.get(1)?,
            subject: r.get(2)?,
            summary: r.get(3)?,
            branch: r.get(4)?,
            pr: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add(conn: &Connection, task_id: i64, subject: Option<&str>, summary: Option<&str>) -> Result<i64> {
    conn.execute(
        "INSERT INTO task_comments (task_id, subject, summary, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, subject, summary, crate::timer::now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}
