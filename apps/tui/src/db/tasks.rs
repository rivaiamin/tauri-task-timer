use crate::timer::{current_elapsed_seconds, now_ms};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub label: String,
    pub description: Option<String>,
    pub elapsed_time: i64,
    #[allow(dead_code)]
    pub position: i64,
    pub is_running: bool,
    #[allow(dead_code)]
    pub done: bool,
    pub start_time: Option<i64>,
    pub work_date: String,
}

impl Task {
    pub fn current_elapsed(&self, now: i64) -> i64 {
        current_elapsed_seconds(self.elapsed_time, self.is_running, self.start_time, now)
    }
}

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        label: row.get(1)?,
        description: row.get(2)?,
        elapsed_time: row.get(3)?,
        position: row.get(4)?,
        is_running: row.get::<_, i64>(5)? != 0,
        done: row.get::<_, i64>(6)? != 0,
        start_time: row.get(7)?,
        work_date: row.get(8)?,
    })
}

const COLS: &str =
    "id, label, description, elapsed_time, position, is_running, done, start_time, work_date";

pub fn list_tasks(conn: &Connection, user_id: &str, work_date: &str) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM tasks WHERE user_id = ?1 AND work_date = ?2 ORDER BY position ASC, id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, work_date], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_task(conn: &Connection, user_id: &str, task_id: i64) -> Result<Option<Task>> {
    conn.query_row(
        &format!("SELECT {COLS} FROM tasks WHERE id = ?1 AND user_id = ?2"),
        params![task_id, user_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

fn next_position(conn: &Connection, user_id: &str, work_date: &str) -> Result<i64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(position) FROM tasks WHERE user_id = ?1 AND work_date = ?2",
        params![user_id, work_date],
        |r| r.get(0),
    )?;
    Ok(max.unwrap_or(-1) + 1)
}

/// Same label on this day → return existing. Else insert; copy description from
/// the most recent same-label row if the caller didn't supply one.
pub fn create_task(
    conn: &Connection,
    user_id: &str,
    work_date: &str,
    label: &str,
    description: Option<&str>,
) -> Result<Task> {
    let label = label.trim();
    if label.is_empty() {
        bail!("label is required");
    }
    if let Some(existing) = conn
        .query_row(
            &format!(
                "SELECT {COLS} FROM tasks WHERE user_id = ?1 AND work_date = ?2 AND label = ?3"
            ),
            params![user_id, work_date, label],
            map_row,
        )
        .optional()?
    {
        return Ok(existing);
    }

    let desc: Option<String> = match description {
        Some(d) if !d.is_empty() => Some(d.to_string()),
        _ => conn
            .query_row(
                "SELECT description FROM tasks
                 WHERE user_id = ?1 AND label = ?2 AND description IS NOT NULL AND description != ''
                 ORDER BY work_date DESC, id DESC LIMIT 1",
                params![user_id, label],
                |r| r.get(0),
            )
            .optional()?,
    };

    let now = now_ms();
    let pos = next_position(conn, user_id, work_date)?;
    conn.execute(
        "INSERT INTO tasks (user_id, label, work_date, description, elapsed_time, position, is_running, done, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, 0, 0, ?6, ?6)",
        params![user_id, label, work_date, desc, pos, now],
    )?;
    let id = conn.last_insert_rowid();
    get_task(conn, user_id, id)?.ok_or_else(|| anyhow::anyhow!("insert vanished"))
}

fn stop_row(conn: &Connection, user_id: &str, row: &Task, now: i64) -> Result<i64> {
    let elapsed = row.current_elapsed(now);
    conn.execute(
        "UPDATE tasks SET is_running = 0, elapsed_time = ?1, start_time = NULL, updated_at = ?2
         WHERE id = ?3 AND user_id = ?4",
        params![elapsed, now, row.id, user_id],
    )?;
    Ok(elapsed - row.elapsed_time)
}

pub fn start_timer(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    exclusive: bool,
) -> Result<Option<Task>> {
    let Some(row) = get_task(conn, user_id, task_id)? else {
        return Ok(None);
    };
    let now = now_ms();
    if exclusive {
        let running = list_tasks(conn, user_id, &row.work_date)?
            .into_iter()
            .filter(|t| t.is_running && t.id != task_id)
            .collect::<Vec<_>>();
        for t in running {
            stop_row(conn, user_id, &t, now)?;
        }
    }
    conn.execute(
        "UPDATE tasks SET is_running = 1, start_time = ?1, updated_at = ?1 WHERE id = ?2 AND user_id = ?3",
        params![now, task_id, user_id],
    )?;
    get_task(conn, user_id, task_id)
}

pub fn stop_timer(conn: &Connection, user_id: &str, task_id: i64) -> Result<Option<Task>> {
    let Some(row) = get_task(conn, user_id, task_id)? else {
        return Ok(None);
    };
    stop_row(conn, user_id, &row, now_ms())?;
    get_task(conn, user_id, task_id)
}

pub fn reset_task(conn: &Connection, user_id: &str, task_id: i64) -> Result<Option<Task>> {
    let now = now_ms();
    let n = conn.execute(
        "UPDATE tasks SET is_running = 0, elapsed_time = 0, start_time = NULL, updated_at = ?1
         WHERE id = ?2 AND user_id = ?3",
        params![now, task_id, user_id],
    )?;
    if n == 0 {
        return Ok(None);
    }
    get_task(conn, user_id, task_id)
}

pub fn reset_all(conn: &Connection, user_id: &str, work_date: &str) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "UPDATE tasks SET is_running = 0, elapsed_time = 0, start_time = NULL, updated_at = ?1
         WHERE user_id = ?2 AND work_date = ?3",
        params![now, user_id, work_date],
    )?;
    Ok(())
}

pub fn delete_task(conn: &Connection, user_id: &str, task_id: i64) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![task_id, user_id],
    )?;
    Ok(n > 0)
}

pub fn update_task(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    label: Option<&str>,
    description: Option<&str>,
) -> Result<Option<Task>> {
    let Some(row) = get_task(conn, user_id, task_id)? else {
        return Ok(None);
    };
    let now = now_ms();
    let new_label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&row.label);
    let new_desc = description.or(row.description.as_deref());
    match conn.execute(
        "UPDATE tasks SET label = ?1, description = ?2, updated_at = ?3 WHERE id = ?4 AND user_id = ?5",
        params![new_label, new_desc, now, task_id, user_id],
    ) {
        Ok(_) => get_task(conn, user_id, task_id),
        Err(e) if is_unique(&e) => bail!("a task with that title already exists this day"),
        Err(e) => Err(e.into()),
    }
}

pub fn reorder_swap(conn: &Connection, user_id: &str, a: i64, b: i64) -> Result<()> {
    let now = now_ms();
    let pa: i64 = conn.query_row(
        "SELECT position FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![a, user_id],
        |r| r.get(0),
    )?;
    let pb: i64 = conn.query_row(
        "SELECT position FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![b, user_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE tasks SET position = ?1, updated_at = ?2 WHERE id = ?3 AND user_id = ?4",
        params![pb, now, a, user_id],
    )?;
    conn.execute(
        "UPDATE tasks SET position = ?1, updated_at = ?2 WHERE id = ?3 AND user_id = ?4",
        params![pa, now, b, user_id],
    )?;
    Ok(())
}

fn is_unique(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE tasks (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               user_id TEXT NOT NULL,
               label TEXT NOT NULL,
               work_date TEXT NOT NULL DEFAULT (date('now')),
               description TEXT,
               elapsed_time INTEGER NOT NULL DEFAULT 0,
               position INTEGER NOT NULL DEFAULT 0,
               is_running INTEGER NOT NULL DEFAULT 0,
               done INTEGER NOT NULL DEFAULT 0,
               start_time INTEGER,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX idx_tasks_user_date_label ON tasks (user_id, work_date, label);
             INSERT INTO users VALUES ('u1', 'a@b.c', 'x', 0);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn create_same_label_same_day_returns_existing() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-30", "US-1", Some("first")).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "US-1", Some("ignored")).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(b.description.as_deref(), Some("first"));
        assert_eq!(list_tasks(&conn, "u1", "2026-08-30").unwrap().len(), 1);
    }

    #[test]
    fn create_same_label_new_day_copies_description() {
        let conn = setup();
        create_task(&conn, "u1", "2026-08-29", "US-1", Some("from yesterday")).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "US-1", None).unwrap();
        assert_eq!(b.description.as_deref(), Some("from yesterday"));
        assert_ne!(b.work_date, "2026-08-29");
    }

    #[test]
    fn focus_start_stops_other_running_same_day() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-30", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "B", None).unwrap();
        start_timer(&conn, "u1", a.id, true).unwrap();
        start_timer(&conn, "u1", b.id, true).unwrap();
        let a2 = get_task(&conn, "u1", a.id).unwrap().unwrap();
        let b2 = get_task(&conn, "u1", b.id).unwrap().unwrap();
        assert!(!a2.is_running);
        assert!(b2.is_running);
    }
}
