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

/// The agent state the daily list shows as a badge, for every task that has any.
///
/// One query per reload rather than one per task: the list draws every row of the
/// day, and the alternative is N queries on a 1s tick. `idx_task_integrations_task`
/// covers the join, and a task with no `agent` rows is simply absent from the map.
pub fn agent_states(conn: &Connection) -> Result<std::collections::HashMap<i64, AgentState>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, field, value FROM task_integrations
         WHERE \"group\" = 'agent' AND field IN ('status', 'attention')",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;

    let mut out: std::collections::HashMap<i64, AgentState> = std::collections::HashMap::new();
    for row in rows {
        let (task_id, field, value) = row?;
        let state = out.entry(task_id).or_default();
        match field.as_str() {
            "status" => state.status = value.filter(|v| !v.is_empty()),
            "attention" => state.attention = value.as_deref() == Some("true"),
            _ => {}
        }
    }
    // A row that only ever carried `attention` has no status to show, so it is not
    // a badge.
    out.retain(|_, s| s.status.is_some());
    Ok(out)
}

/// The `agent` rows that matter to the daily list: what the agent is doing, and
/// whether it needs the operator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentState {
    /// Paseo's own word — `running`, `idle`, `closed`, `failed` — stored verbatim
    /// by the orchestrator rather than translated here.
    pub status: Option<String>,
    pub attention: bool,
}

impl AgentState {
    /// The badge text for a daily-list row, or None when there is nothing to say.
    ///
    /// `attention` is marked with a leading `!` rather than a second badge: the
    /// row already carries a status and an elapsed time, and one token that
    /// distinguishes "needs you" from "working" is what the list has room for.
    pub fn badge(&self) -> Option<String> {
        let status = self.status.as_deref().filter(|s| !s.is_empty())?;
        Some(if self.attention {
            format!("!{status}")
        } else {
            status.to_string()
        })
    }

    /// A badge is only worth colouring differently when the agent is alive or
    /// stuck; a closed agent is history.
    pub fn is_live(&self) -> bool {
        matches!(self.status.as_deref(), Some("running") | Some("working"))
    }
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

    #[test]
    fn agent_states_gathers_the_badge_fields_and_ignores_everything_else() {
        let conn = setup();
        upsert(&conn, 1, "agent", "status", Some("running")).unwrap();
        upsert(&conn, 1, "agent", "attention", Some("false")).unwrap();
        upsert(&conn, 2, "agent", "status", Some("closed")).unwrap();
        upsert(&conn, 2, "agent", "attention", Some("true")).unwrap();
        // Other groups and other agent fields are not badge material.
        upsert(&conn, 1, "jira", "issue_key", Some("US-1")).unwrap();
        upsert(&conn, 1, "agent", "session", Some("abc-123")).unwrap();

        let states = agent_states(&conn).unwrap();
        assert_eq!(states.len(), 2);
        assert_eq!(states[&1].badge().as_deref(), Some("running"));
        assert_eq!(states[&2].badge().as_deref(), Some("!closed"));
        assert!(states[&1].is_live());
        assert!(!states[&2].is_live(), "a closed agent is history, not live");
    }

    #[test]
    fn a_task_with_no_agent_status_gets_no_badge() {
        let conn = setup();
        // An attention-only row has nothing to display, so it must not produce a
        // badge — the alternative is an empty `⚙` on the row.
        upsert(&conn, 1, "agent", "attention", Some("true")).unwrap();
        upsert(&conn, 2, "agent", "status", Some("")).unwrap();

        assert!(agent_states(&conn).unwrap().is_empty());
    }

    #[test]
    fn a_cleared_status_removes_the_badge() {
        let conn = setup();
        upsert(&conn, 1, "agent", "status", Some("running")).unwrap();
        assert!(agent_states(&conn).unwrap().contains_key(&1));

        upsert(&conn, 1, "agent", "status", None).unwrap();
        assert!(
            agent_states(&conn).unwrap().is_empty(),
            "clearing the status must drop the badge, not render an empty one"
        );
    }
}
