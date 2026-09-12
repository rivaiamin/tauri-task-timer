use crate::timer::{current_elapsed_seconds, now_ms};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub label: String,
    pub description: Option<String>,
    pub code: Option<String>,
    pub link: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub tags: Option<String>,
    pub elapsed_time: i64,
    pub total_time: i64,
    #[allow(dead_code)]
    pub position: i64,
    pub is_running: bool,
    #[allow(dead_code)]
    pub done: bool,
    pub is_completed: bool,
    pub is_cancelled: bool,
    pub is_deleted: bool,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub is_important: bool,
    pub start_time: Option<i64>,
    pub end_time: Option<i64>,
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
        code: row.get(3)?,
        link: row.get(4)?,
        status: row.get(5)?,
        notes: row.get(6)?,
        tags: row.get(7)?,
        elapsed_time: row.get(8)?,
        total_time: row.get(9)?,
        position: row.get(10)?,
        is_running: row.get::<_, i64>(11)? != 0,
        done: row.get::<_, i64>(12)? != 0,
        is_completed: row.get::<_, i64>(13)? != 0,
        is_cancelled: row.get::<_, i64>(14)? != 0,
        is_deleted: row.get::<_, i64>(15)? != 0,
        is_archived: row.get::<_, i64>(16)? != 0,
        is_pinned: row.get::<_, i64>(17)? != 0,
        is_important: row.get::<_, i64>(18)? != 0,
        start_time: row.get(19)?,
        end_time: row.get(20)?,
        work_date: row.get(21)?,
    })
}

const COLS: &str =
    "id, label, description, code, link, status, notes, tags, elapsed_time, total_time, position, is_running, done, is_completed, is_cancelled, is_deleted, is_archived, is_pinned, is_important, start_time, end_time, work_date";

pub fn list_tasks(conn: &Connection, user_id: &str, work_date: &str) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM tasks WHERE user_id = ?1 AND work_date = ?2 ORDER BY position ASC, id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, work_date], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveFilter {
    pub q: Option<String>,
    pub tag: Option<String>,
}

/// Backlog of unfinished tasks across days: not done/archived/deleted/completed/cancelled.
/// Optional substring filters on label and tags (case-insensitive LIKE).
/// Ordered newest work_date first so recent backlog surfaces at top.
pub fn list_archive(
    conn: &Connection,
    user_id: &str,
    filter: &ArchiveFilter,
) -> Result<Vec<Task>> {
    let mut sql = format!(
        "SELECT {COLS} FROM tasks WHERE user_id = ?1
         AND COALESCE(is_deleted, 0) = 0
         AND COALESCE(done, 0) = 0
         AND COALESCE(is_archived, 0) = 0
         AND COALESCE(is_completed, 0) = 0
         AND COALESCE(is_cancelled, 0) = 0"
    );
    let mut params_vec: Vec<String> = vec![user_id.to_string()];
    let mut next_idx = 2usize;
    if let Some(q) = filter.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(&format!(" AND label LIKE ?{next_idx} COLLATE NOCASE"));
        params_vec.push(format!("%{q}%"));
        next_idx += 1;
    }
    if let Some(tag) = filter.tag.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(&format!(" AND COALESCE(tags, '') LIKE ?{next_idx} COLLATE NOCASE"));
        params_vec.push(format!("%{tag}%"));
    }
    let _ = next_idx;
    sql.push_str(" ORDER BY work_date DESC, position ASC, id ASC");
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params_vec.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(param_refs.as_slice(), map_row)?;
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
    elapsed_seconds: Option<i64>,
    code: Option<&str>,
    status: Option<&str>,
    notes: Option<&str>,
    tags: Option<&str>,
) -> Result<Option<Task>> {
    let Some(row) = get_task(conn, user_id, task_id)? else {
        return Ok(None);
    };
    if let Some(s) = elapsed_seconds {
        if s < 0 {
            bail!("elapsed time must be >= 0");
        }
    }
    let now = now_ms();
    let new_label = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&row.label);
    let new_desc = description.or(row.description.as_deref());
    let new_elapsed = elapsed_seconds.unwrap_or(row.elapsed_time);
    let new_start = if elapsed_seconds.is_some() && row.is_running {
        Some(now)
    } else {
        row.start_time
    };
    let new_code = code.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let new_status = status.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let new_notes = notes.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let new_tags = tags.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    match conn.execute(
        "UPDATE tasks SET label = ?1, description = ?2, elapsed_time = ?3, start_time = ?4, updated_at = ?5,
         code = ?6, status = ?7, notes = ?8, tags = ?9
         WHERE id = ?10 AND user_id = ?11",
        params![
            new_label,
            new_desc,
            new_elapsed,
            new_start,
            now,
            new_code,
            new_status,
            new_notes,
            new_tags,
            task_id,
            user_id
        ],
    ) {
        Ok(_) => get_task(conn, user_id, task_id),
        Err(e) if is_unique(&e) => bail!("a task with that title already exists this day"),
        Err(e) => Err(e.into()),
    }
}

/// Set the `done` flag on a task. If becoming done while the task is running,
/// stops the timer first. Returns `(updated_task, worklog_delta_seconds, start_ms_before_stop)`.
/// If un-done, just flips the flag (delta=0, start_ms=None).
pub fn set_done(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    done: bool,
) -> Result<Option<(Task, i64, Option<i64>)>> {
    let Some(row) = get_task(conn, user_id, task_id)? else {
        return Ok(None);
    };
    if row.done == done {
        return Ok(Some((row, 0, None)));
    }
    let now = now_ms();
    let mut delta: i64 = 0;
    let mut start_ms: Option<i64> = None;
    if done && row.is_running {
        start_ms = row.start_time;
        delta = row.current_elapsed(now) - row.elapsed_time;
        stop_row(conn, user_id, &row, now)?;
    }
    conn.execute(
        "UPDATE tasks SET done = ?1, updated_at = ?2 WHERE id = ?3 AND user_id = ?4",
        params![done as i64, now, task_id, user_id],
    )?;
    let updated = get_task(conn, user_id, task_id)?
        .ok_or_else(|| anyhow::anyhow!("task vanished after set_done"))?;
    Ok(Some((updated, delta, start_ms)))
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
               code TEXT,
               link TEXT,
               status TEXT NOT NULL DEFAULT 'todo',
               notes TEXT,
               tags TEXT,
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
               start_time INTEGER,
               end_time INTEGER,
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

    fn set_flag(conn: &Connection, id: i64, col: &str) {
        conn.execute(&format!("UPDATE tasks SET {col} = 1 WHERE id = ?1"), [id])
            .unwrap();
    }

    #[test]
    fn archive_excludes_done_and_archived() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        let c = create_task(&conn, "u1", "2026-08-29", "C", None).unwrap();
        set_flag(&conn, b.id, "done");
        set_flag(&conn, c.id, "is_archived");
        let rows = list_archive(&conn, "u1", &ArchiveFilter::default()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a.id);
    }

    #[test]
    fn archive_filter_by_q() {
        let conn = setup();
        create_task(&conn, "u1", "2026-08-28", "US-101 fix login", None).unwrap();
        create_task(&conn, "u1", "2026-08-28", "US-102 checkout", None).unwrap();
        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                q: Some("login".into()),
                tag: None,
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].label.contains("login"));
    }

    #[test]
    fn archive_filter_by_tag() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let _b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        conn.execute(
            "UPDATE tasks SET tags = '[\"backend\"]' WHERE id = ?1",
            [a.id],
        )
        .unwrap();
        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                q: None,
                tag: Some("backend".into()),
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a.id);
    }

    #[test]
    fn set_done_stops_running_task_and_returns_delta() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-12", "Run", None).unwrap();
        // Start the timer with a start_time 5 seconds in the past
        let five_sec_ago = crate::timer::now_ms() - 5000;
        conn.execute(
            "UPDATE tasks SET is_running = 1, start_time = ?1 WHERE id = ?2",
            [five_sec_ago, t.id],
        )
        .unwrap();

        let (updated, delta, start_ms) = set_done(&conn, "u1", t.id, true).unwrap().unwrap();
        assert!(updated.done);
        assert!(!updated.is_running);
        assert!(delta >= 4); // at least ~5s minus rounding
        assert!(start_ms.is_some());

        // Second call is idempotent (already done)
        let (again, d2, s2) = set_done(&conn, "u1", t.id, true).unwrap().unwrap();
        assert!(again.done);
        assert_eq!(d2, 0);
        assert!(s2.is_none());
    }
}
