use anyhow::Result;
use rusqlite::{params, Connection};

/// One `task_comments` row. The schema is shared with the web app, so the type
/// mirrors it; the TUI renders `subject`/`summary` and dedupes imports on
/// `pr`, leaving the rest along for the ride.
#[derive(Clone, Debug)]
pub struct Comment {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub task_id: i64,
    pub subject: Option<String>,
    pub summary: Option<String>,
    #[allow(dead_code)]
    pub branch: Option<String>,
    pub pr: Option<String>,
    #[allow(dead_code)]
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

/// Append a comment. `branch`/`pr` are set for imported PR comments and left
/// NULL for ones written by hand in the TUI.
pub fn add(
    conn: &Connection,
    task_id: i64,
    subject: Option<&str>,
    summary: Option<&str>,
    branch: Option<&str>,
    pr: Option<&str>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO task_comments (task_id, subject, summary, branch, pr, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![task_id, subject, summary, branch, pr, crate::timer::now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE task_comments (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               task_id INTEGER NOT NULL,
               subject TEXT,
               summary TEXT,
               branch TEXT,
               pr TEXT,
               created_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn add_then_list_round_trips_every_column() {
        let conn = setup();
        let id = add(
            &conn,
            7,
            Some("PR #12 alice"),
            Some("please rebase"),
            Some("US-1-fix"),
            Some("12"),
        )
        .unwrap();
        let rows = list(&conn, 7).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].task_id, 7);
        assert_eq!(rows[0].subject.as_deref(), Some("PR #12 alice"));
        assert_eq!(rows[0].summary.as_deref(), Some("please rebase"));
        assert_eq!(rows[0].branch.as_deref(), Some("US-1-fix"));
        assert_eq!(rows[0].pr.as_deref(), Some("12"));
        assert!(rows[0].created_at > 0);
    }

    #[test]
    fn a_hand_written_comment_leaves_branch_and_pr_null() {
        let conn = setup();
        add(&conn, 7, Some("comment"), Some("note"), None, None).unwrap();
        let rows = list(&conn, 7).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].branch.is_none());
        assert!(rows[0].pr.is_none());
    }

    #[test]
    fn list_is_empty_for_an_unknown_task() {
        let conn = setup();
        add(&conn, 7, Some("s"), Some("m"), None, None).unwrap();
        assert!(list(&conn, 99).unwrap().is_empty());
    }
}
