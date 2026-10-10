use anyhow::Result;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
pub struct Integration {
    pub group: String,
    pub field: String,
    pub value: Option<String>,
}

pub fn list(conn: &Connection, task_id: i64) -> Result<Vec<Integration>> {
    let mut stmt = conn.prepare(
        "SELECT id, task_id, \"group\", field, value FROM task_integrations
         WHERE task_id = ?1 ORDER BY \"group\" ASC, field ASC",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(Integration {
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
            "SELECT id FROM task_integrations WHERE task_id = ?1 AND \"group\" = ?2 AND field = ?3",
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
            conn.execute("INSERT INTO task_integrations (task_id, \"group\", field, value, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)", params![task_id, group, field, value, now])?;
        }
    }
    Ok(())
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE task_integrations (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               task_id INTEGER NOT NULL,
               \"group\" TEXT NOT NULL,
               field TEXT NOT NULL,
               value TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        conn
    }

    /// `group` is a SQLite keyword, so every statement naming it must quote it.
    #[test]
    fn upsert_and_list_round_trip_through_a_quoted_group_column() {
        let conn = setup();
        upsert(&conn, 1, "jira", "issue_key", Some("US-1")).unwrap();
        let rows = list(&conn, 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].group, "jira");
        assert_eq!(rows[0].value.as_deref(), Some("US-1"));

        // Second upsert on the same (task, group, field) updates in place.
        upsert(&conn, 1, "jira", "issue_key", Some("US-2")).unwrap();
        let rows = list(&conn, 1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value.as_deref(), Some("US-2"));
    }
}
