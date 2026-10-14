use crate::timer::{current_elapsed_seconds, now_ms};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// One task, joined with one of its days.
///
/// `tasks` holds the identity (label, description, code, link, notes, tags);
/// `task_days` holds everything that varies per day (the timer, the day's
/// ordering, and the day's lifecycle state). A `Task` is therefore always "this
/// task, on one particular day" — `day_id` is the day it was read from, and the
/// flat timer/status fields come from that day's row.
#[derive(Clone, Debug)]
pub struct Task {
    /// The task identity's id. Stable across every day the task is worked.
    pub id: i64,
    /// The `task_days` row this snapshot came from. Day-scoped writes key on it.
    pub day_id: i64,
    pub label: String,
    pub description: Option<String>,
    pub code: Option<String>,
    pub status: String,
    pub notes: Option<String>,
    pub tags: Option<String>,
    pub elapsed_time: i64,
    pub is_running: bool,
    /// The live done flag: the dashboard checkbox writes `done`; REST still
    /// accepts `is_completed`.
    pub done: bool,
    pub start_time: Option<i64>,
    pub work_date: String,

    /// Columns this crate stores but does not read. The schema is shared with
    /// the web app, which reads and writes all of them.
    #[allow(dead_code)]
    pub end_time: Option<i64>,
    pub link: Option<String>,
    #[allow(dead_code)]
    pub total_time: i64,
    #[allow(dead_code)]
    pub position: i64,
    pub is_completed: bool,
    pub is_cancelled: bool,
    pub is_deleted: bool,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub is_important: bool,

    /// Every day this task was worked. Empty on a daily query, which only ever
    /// needs the one day; `list_archive` fills it so the archive can show the
    /// per-day breakdown under a single entry per task.
    pub days: Vec<TaskDay>,
}

impl Task {
    pub fn current_elapsed(&self, now: i64) -> i64 {
        current_elapsed_seconds(self.elapsed_time, self.is_running, self.start_time, now)
    }

    /// Seconds worked since the last start that are not yet committed to
    /// `elapsed_time`. Zero when the task is not running.
    pub fn running_delta(&self, now: i64) -> i64 {
        self.current_elapsed(now) - self.elapsed_time
    }

    /// Description as JIRA payloads want it: absent means the empty string.
    pub fn description_text(&self) -> &str {
        self.description.as_deref().unwrap_or("")
    }

    /// Recorded time across every day of this task. Only meaningful when `days`
    /// is populated; a daily query reports that one day.
    pub fn total_elapsed(&self, now: i64) -> i64 {
        if self.days.is_empty() {
            self.current_elapsed(now)
        } else {
            self.days.iter().map(|d| d.current_elapsed(now)).sum()
        }
    }
}

/// One day of work on a task, as the archive shows it.
#[derive(Clone, Debug)]
pub struct TaskDay {
    pub work_date: String,
    pub elapsed_time: i64,
    pub is_running: bool,
    pub start_time: Option<i64>,
    pub status: String,
    pub done: bool,
}

impl TaskDay {
    pub fn current_elapsed(&self, now: i64) -> i64 {
        current_elapsed_seconds(self.elapsed_time, self.is_running, self.start_time, now)
    }
}

/// The joined projection every read uses: the identity's content fields plus the
/// day row's timer, ordering, and lifecycle columns. Ordinals are pinned by
/// `map_row` — change both together.
const COLS: &str = "t.id, t.label, t.description, t.code, t.link, t.notes, t.tags, \
     td.id, td.status, td.elapsed_time, td.total_time, td.position, \
     td.is_running, td.done, td.is_completed, td.is_cancelled, td.is_deleted, \
     td.is_archived, td.is_pinned, td.is_important, td.start_time, td.end_time, td.work_date";

/// `COLS` plus the identity's `user_id`, for the few reads that must verify
/// ownership. Kept separate so the shared projection stays a fixed width.
const JOIN: &str = "FROM tasks t JOIN task_days td ON td.task_id = t.id";

fn map_row(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        label: row.get(1)?,
        description: row.get(2)?,
        code: row.get(3)?,
        link: row.get(4)?,
        notes: row.get(5)?,
        tags: row.get(6)?,
        day_id: row.get(7)?,
        status: row.get(8)?,
        elapsed_time: row.get(9)?,
        total_time: row.get(10)?,
        position: row.get(11)?,
        is_running: row.get::<_, i64>(12)? != 0,
        done: row.get::<_, i64>(13)? != 0,
        is_completed: row.get::<_, i64>(14)? != 0,
        is_cancelled: row.get::<_, i64>(15)? != 0,
        is_deleted: row.get::<_, i64>(16)? != 0,
        is_archived: row.get::<_, i64>(17)? != 0,
        is_pinned: row.get::<_, i64>(18)? != 0,
        is_important: row.get::<_, i64>(19)? != 0,
        start_time: row.get(20)?,
        end_time: row.get(21)?,
        work_date: row.get(22)?,
        days: Vec::new(),
    })
}

/// The day's tasks, in the day's order. A task with no row for `work_date` is
/// not on that day and does not appear.
pub fn list_tasks(conn: &Connection, user_id: &str, work_date: &str) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND td.work_date = ?2
         ORDER BY td.position ASC, td.id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, work_date], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Every running day row of this user, across all days. A run left going on an
/// earlier date still has to be closable, so this is deliberately not date-scoped.
pub fn list_running(conn: &Connection, user_id: &str) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND td.is_running = 1
         ORDER BY td.work_date ASC, td.id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The task's row for one specific day.
pub fn get_task_on(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    work_date: &str,
) -> Result<Option<Task>> {
    conn.query_row(
        &format!("SELECT {COLS} {JOIN} WHERE t.id = ?1 AND t.user_id = ?2 AND td.work_date = ?3"),
        params![task_id, user_id, work_date],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// One representative day row per identity, so the shared projection still
/// applies. `task_integrations` FKs to the task identity and not to a day, so a
/// caller addressing a task by id or label without naming a date needs this
/// rather than `get_task_on` — which would miss a task not worked today. Same
/// representative-day choice as `list_archive`.
const IDENTITY_DAY: &str =
    "td.id = (SELECT MAX(d2.id) FROM task_days d2 WHERE d2.task_id = t.id)";

/// The task identity by its own id, whatever days it was worked.
pub fn get_task_identity(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
) -> Result<Option<Task>> {
    conn.query_row(
        &format!("SELECT {COLS} {JOIN} WHERE t.user_id = ?1 AND t.id = ?2 AND {IDENTITY_DAY}"),
        params![user_id, task_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

/// Every identity whose label matches, whatever days they were worked. Not
/// date-scoped, so it can return more than one row for a label the user reused.
pub fn find_task_identities_by_label(
    conn: &Connection,
    user_id: &str,
    label: &str,
) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND t.label = ?2 COLLATE NOCASE AND {IDENTITY_DAY}
         ORDER BY t.id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, label], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Identities whose label *starts with* `prefix` followed by a word boundary.
///
/// The JIRA sprint fetch labels a task `KEY summary`, so the bare key is the
/// natural handle for a task whose label carries extra text. Matching on the
/// space keeps `US-1` from also matching `US-10`, and the caller rejects an
/// ambiguous result rather than picking one.
pub fn find_task_identities_by_label_prefix(
    conn: &Connection,
    user_id: &str,
    prefix: &str,
) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND t.label LIKE ?2 ESCAPE '\\' COLLATE NOCASE AND {IDENTITY_DAY}
         ORDER BY t.id ASC"
    ))?;
    let pattern = format!(
        "{} %",
        prefix.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    );
    let rows = stmt.query_map(params![user_id, pattern], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The day row by its own id — what a mutation returns after writing.
pub fn get_task_by_day(conn: &Connection, user_id: &str, day_id: i64) -> Result<Option<Task>> {
    conn.query_row(
        &format!("SELECT {COLS} {JOIN} WHERE td.id = ?1 AND t.user_id = ?2"),
        params![day_id, user_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn find_tasks_by_label(
    conn: &Connection,
    user_id: &str,
    work_date: &str,
    label: &str,
) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND td.work_date = ?2 AND t.label = ?3 COLLATE NOCASE
         ORDER BY t.id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, work_date, label], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn next_position(conn: &Connection, work_date: &str) -> Result<i64> {
    let max: Option<i64> = conn.query_row(
        "SELECT MAX(position) FROM task_days WHERE work_date = ?1",
        params![work_date],
        |r| r.get(0),
    )?;
    Ok(max.unwrap_or(-1) + 1)
}

/// The day's row for this task, created if the task has never been worked that
/// day. This is what "start on a new day" means now: the same task gains a day.
pub fn ensure_day_row(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    work_date: &str,
) -> Result<Task> {
    if let Some(existing) = get_task_on(conn, user_id, task_id, work_date)? {
        return Ok(existing);
    }
    let now = now_ms();
    let pos = next_position(conn, work_date)?;
    conn.execute(
        "INSERT INTO task_days (task_id, work_date, position, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        params![task_id, work_date, pos, now],
    )?;
    get_task_on(conn, user_id, task_id, work_date)?
        .ok_or_else(|| anyhow::anyhow!("day row vanished after insert"))
}

/// Create the task if its label is new, then make sure it has a row for
/// `work_date`. An existing day row is returned untouched.
///
/// The description belongs to the task, so it is stored on the identity: a
/// caller-supplied description fills in a task that has none, and a task that
/// already has one keeps it. That is what "the description carries over to the
/// next day" means now — there is only ever one.
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
    let now = now_ms();
    let supplied = description.map(str::trim).filter(|d| !d.is_empty());

    let existing_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM tasks WHERE user_id = ?1 AND label = ?2",
            params![user_id, label],
            |r| r.get(0),
        )
        .optional()?;

    let task_id = match existing_id {
        Some(id) => {
            if let Some(desc) = supplied {
                // Only fill a gap: an existing description is the task's, not
                // this day's to overwrite.
                conn.execute(
                    "UPDATE tasks SET description = ?1, updated_at = ?2
                     WHERE id = ?3 AND (description IS NULL OR description = '')",
                    params![desc, now, id],
                )?;
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO tasks (user_id, label, description, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![user_id, label, supplied, now],
            )?;
            conn.last_insert_rowid()
        }
    };

    ensure_day_row(conn, user_id, task_id, work_date)
}

/// The day row this label already has for `work_date`, if any.
///
/// The same exact-label lookup `create_task` does, so a caller that patches the
/// row `create_task` returns can tell "created" from "reused" and leave a
/// reused row's timer and flags alone.
pub fn day_row_for_label(
    conn: &Connection,
    user_id: &str,
    work_date: &str,
    label: &str,
) -> Result<Option<Task>> {
    let label = label.trim();
    if label.is_empty() {
        return Ok(None);
    }
    let Some(task_id) = conn
        .query_row(
            "SELECT id FROM tasks WHERE user_id = ?1 AND label = ?2",
            params![user_id, label],
            |r| r.get(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    get_task_on(conn, user_id, task_id, work_date)
}

/// One fetched JIRA issue, as the timer stores it.
///
/// The tuple shape is what `jira::fetch_issue` / `jira::search_issues` return:
/// `(key, summary, status_name)`.
pub type JiraIssue = (String, String, String);

/// Find the task a selector names, by identity rather than by day.
///
/// The lookup every `integration` command and `jira ensure` share: an exact label
/// match wins, then a `selector %` prefix (so a bare `US-2449` finds the sprint
/// fetch's `US-2449 <summary>`), then nothing. A numeric selector is the identity
/// id. An ambiguous match is an error, never a guess.
///
/// This lives here, not in the CLI, so the TUI's JIRA menu and the headless
/// command resolve a key to the same task.
pub fn find_task_identity(
    conn: &Connection,
    user_id: &str,
    selector: &str,
) -> Result<Option<Task>> {
    let selector = selector.trim();
    if selector.is_empty() {
        bail!("task selector is required");
    }
    if selector.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(id) = selector.parse::<i64>() {
            return get_task_identity(conn, user_id, id);
        }
    }
    match find_task_identities_by_label(conn, user_id, selector)?.as_slice() {
        [] => {}
        [task] => return Ok(Some(task.clone())),
        rest => return Err(ambiguous_identity(selector, rest)),
    }
    match find_task_identities_by_label_prefix(conn, user_id, selector)?.as_slice() {
        [] => Ok(None),
        [task] => Ok(Some(task.clone())),
        rest => Err(ambiguous_identity(selector, rest)),
    }
}

fn ambiguous_identity(selector: &str, rest: &[Task]) -> anyhow::Error {
    let ids = rest
        .iter()
        .map(|t| t.id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::anyhow!("ambiguous label {selector}: ids {ids} (use numeric id)")
}

/// Rename legacy sprint labels (`KEY summary`) to bare `KEY` with summary in
/// `description`. Idempotent when the label is already normalized.
pub fn normalize_legacy_jira_label(
    conn: &Connection,
    user_id: &str,
    task_id: i64,
    key: &str,
    summary: &str,
) -> Result<()> {
    let current: String = conn.query_row(
        "SELECT label FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![task_id, user_id],
        |r| r.get(0),
    )?;
    if current.eq_ignore_ascii_case(key) {
        return Ok(());
    }
    let prefix = format!("{key} ");
    if !current
        .to_ascii_uppercase()
        .starts_with(&prefix.to_ascii_uppercase())
    {
        return Ok(());
    }
    let now = now_ms();
    let desc = summary.trim();
    conn.execute(
        "UPDATE tasks SET label = ?1,
         description = CASE
           WHEN ?2 != '' THEN ?2
           WHEN description IS NULL OR description = '' THEN substr(label, length(?1) + 2)
           ELSE description
         END,
         updated_at = ?3
         WHERE id = ?4 AND user_id = ?5",
        params![key, desc, now, task_id, user_id],
    )?;
    Ok(())
}

/// Store a batch of fetched JIRA issues as timer tasks: the identity labelled
/// with the bare `KEY`, summary in `description`, its row for `work_date`, and
/// the `jira/issue_key` + `jira/status` integration rows.
///
/// This is the one writer for both entry points — the interactive sprint
/// picker / fetch-by-key and the headless `jira ensure` — so the label format and
/// the integration keying cannot drift between them.
///
/// A ticket already in the timer is *reused*, not duplicated. A bare `US-2449`
/// task and the fetch's `US-2449 <summary>` label are the same identity to a
/// human, so creating a second row would split the ticket's integration rows
/// across two ids — and the sprint runner, which resolves a key by exact label,
/// would read the empty one. `resolve` is the same exact-then-prefix lookup the
/// `integration` commands use, so the runner and `ensure` agree on which task a
/// key names.
///
/// A task with no such identity is created (upsert on `idx_tasks_user_label`, day
/// row on `idx_task_days_task_date`), which is what makes a repeated call
/// idempotent. The task's own `status` column is left alone; the timer owns it.
pub fn ingest_jira_issues(
    conn: &Connection,
    user_id: &str,
    work_date: &str,
    issues: &[JiraIssue],
) -> Result<Vec<Task>> {
    let mut tasks = Vec::with_capacity(issues.len());
    for (key, summary, status) in issues {
        // Prefer the task this key already names. An ambiguous match propagates
        // as an error rather than falling through to a create — a third row for a
        // key matching two is worse than refusing.
        let existing = find_task_identity(conn, user_id, key)?;
        let task = match existing {
            Some(task) => {
                normalize_legacy_jira_label(conn, user_id, task.id, key, summary)?;
                ensure_day_row(conn, user_id, task.id, work_date)?
            }
            None => create_task(conn, user_id, work_date, key, Some(summary))?,
        };
        crate::db::integrations::upsert(conn, task.id, "jira", "issue_key", Some(key))?;
        // Kept beside the key so the detail view and the archive's integration
        // filter can read what JIRA reported. An empty status (a fetch that
        // returned no status) must not blank a previously stored one.
        if !status.is_empty() {
            crate::db::integrations::upsert(conn, task.id, "jira", "status", Some(status))?;
        }
        tasks.push(task);
    }
    Ok(tasks)
}

/// Close a running day row, committing its live seconds. Returns the delta that
/// was added, for worklogging.
fn stop_row(conn: &Connection, user_id: &str, row: &Task, now: i64) -> Result<i64> {
    let elapsed = row.current_elapsed(now);
    conn.execute(
        "UPDATE task_days SET is_running = 0, elapsed_time = ?1, start_time = NULL, updated_at = ?2
         WHERE id = ?3
           AND task_id IN (SELECT id FROM tasks WHERE user_id = ?4)",
        params![elapsed, now, row.day_id, user_id],
    )?;
    Ok(elapsed - row.elapsed_time)
}

/// Every running day row of this user, other than `day_id`. A running timer is
/// running whatever day it was started on, so focus mode stops across days.
fn other_running(conn: &Connection, user_id: &str, day_id: i64) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} {JOIN}
         WHERE t.user_id = ?1 AND td.is_running = 1 AND td.id != ?2
         ORDER BY td.id ASC"
    ))?;
    let rows = stmt.query_map(params![user_id, day_id], map_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Start one day's timer. In exclusive mode every other running day row stops
/// first; the caller is told which ones so it can worklog and re-status them.
pub fn start_timer(
    conn: &Connection,
    user_id: &str,
    day_id: i64,
    exclusive: bool,
) -> Result<Option<(Task, Vec<Task>)>> {
    if get_task_by_day(conn, user_id, day_id)?.is_none() {
        return Ok(None);
    }
    let now = now_ms();
    let mut stopped = Vec::new();
    if exclusive {
        for t in other_running(conn, user_id, day_id)? {
            stop_row(conn, user_id, &t, now)?;
            stopped.push(t);
        }
    }
    conn.execute(
        "UPDATE task_days SET is_running = 1, start_time = ?1, updated_at = ?1 WHERE id = ?2",
        params![now, day_id],
    )?;
    Ok(get_task_by_day(conn, user_id, day_id)?.map(|t| (t, stopped)))
}

pub fn stop_timer(conn: &Connection, user_id: &str, day_id: i64) -> Result<Option<Task>> {
    let Some(row) = get_task_by_day(conn, user_id, day_id)? else {
        return Ok(None);
    };
    stop_row(conn, user_id, &row, now_ms())?;
    get_task_by_day(conn, user_id, day_id)
}

pub fn reset_task(conn: &Connection, user_id: &str, day_id: i64) -> Result<Option<Task>> {
    let now = now_ms();
    let n = conn.execute(
        "UPDATE task_days SET is_running = 0, elapsed_time = 0, start_time = NULL, updated_at = ?1
         WHERE id = ?2",
        params![now, day_id],
    )?;
    if n == 0 {
        return Ok(None);
    }
    get_task_by_day(conn, user_id, day_id)
}

/// Zero every task's time for one day. Other days keep their recorded time.
pub fn reset_all(conn: &Connection, user_id: &str, work_date: &str) -> Result<()> {
    let now = now_ms();
    conn.execute(
        "UPDATE task_days SET is_running = 0, elapsed_time = 0, start_time = NULL, updated_at = ?1
         WHERE work_date = ?2
           AND task_id IN (SELECT id FROM tasks WHERE user_id = ?3)",
        params![now, work_date, user_id],
    )?;
    Ok(())
}

/// Delete the task itself. Its day rows, comments and integrations cascade.
pub fn delete_task(conn: &Connection, user_id: &str, task_id: i64) -> Result<bool> {
    let n = conn.execute(
        "DELETE FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![task_id, user_id],
    )?;
    Ok(n > 0)
}

/// The fields a patch may change. `None` leaves the stored value alone; `status`
/// is the exception, where a blank value also keeps the stored one.
///
/// The content fields (`label`, `description`, `code`, `notes`, `tags`, `link`)
/// belong to the task and are written to `tasks`; the timer, status and flags
/// belong to the day and are written to `task_days`.
#[derive(Default, Clone, Copy)]
pub struct TaskPatch<'a> {
    pub label: Option<&'a str>,
    pub description: Option<&'a str>,
    pub elapsed_seconds: Option<i64>,
    pub code: Option<&'a str>,
    pub status: Option<&'a str>,
    pub notes: Option<&'a str>,
    pub tags: Option<&'a str>,
    pub link: Option<&'a str>,
    pub is_pinned: Option<bool>,
    pub is_important: Option<bool>,
    pub is_archived: Option<bool>,
    pub is_cancelled: Option<bool>,
    pub is_deleted: Option<bool>,
    pub is_completed: Option<bool>,
}

pub fn update_task(
    conn: &Connection,
    user_id: &str,
    day_id: i64,
    patch: TaskPatch,
) -> Result<Option<Task>> {
    let TaskPatch {
        label,
        description,
        elapsed_seconds,
        code,
        status,
        notes,
        tags,
        link,
        is_pinned,
        is_important,
        is_archived,
        is_cancelled,
        is_deleted,
        is_completed,
    } = patch;
    let Some(mut row) = get_task_by_day(conn, user_id, day_id)? else {
        return Ok(None);
    };
    if let Some(s) = elapsed_seconds {
        if s < 0 {
            bail!("elapsed time must be >= 0");
        }
    }
    let now = now_ms();

    // --- task identity -------------------------------------------------------
    let new_label: String = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| row.label.clone());
    // A blank description clears it; an absent one leaves it.
    let new_desc = match description {
        Some(d) => Some(d.trim().to_string()).filter(|s| !s.is_empty()),
        None => row.description.clone(),
    };
    let new_code = code.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let new_notes = notes.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let new_tags = tags.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    // An empty link clears the column rather than storing whitespace.
    let new_link = link.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    // A rename onto a title the user already owns on ANOTHER day continues that
    // task instead of failing: this day's row links to the existing identity and
    // the renamed identity goes away — the same link `create_task` makes when a
    // label returns on a new day, timer and all. The same day is still a
    // duplicate: one task cannot hold two rows for one date. Only a supplied
    // label can collide; a patch without one keeps its own identity.
    let renamed = label.is_some_and(|l| !l.trim().is_empty());
    let mut merged = false;
    if renamed {
        // `ORDER BY id` pins the pick when legacy case-variant identities both
        // match; the exact-case index makes exact duplicates impossible.
        let target: Option<i64> = conn
            .query_row(
                "SELECT id FROM tasks WHERE user_id = ?1 AND label = ?2 COLLATE NOCASE AND id != ?3
                 ORDER BY id ASC",
                params![user_id, new_label, row.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(target) = target {
            let same_day: Option<i64> = conn
                .query_row(
                    "SELECT id FROM task_days WHERE task_id = ?1 AND work_date = ?2",
                    params![target, row.work_date],
                    |r| r.get(0),
                )
                .optional()?;
            if same_day.is_some() {
                bail!("a task with that title already exists");
            }
            merge_identities(conn, user_id, target, row.id)?;
            row = get_task_by_day(conn, user_id, day_id)?
                .ok_or_else(|| anyhow::anyhow!("task vanished after merge"))?;
            merged = true;
        }
    }

    // --- the day -------------------------------------------------------------
    let new_elapsed = elapsed_seconds.unwrap_or(row.elapsed_time);
    let new_start = if elapsed_seconds.is_some() && row.is_running {
        Some(now)
    } else {
        row.start_time
    };
    // `status` is NOT NULL in the schema, so an absent or blank value must land
    // on the existing one — writing NULL is a constraint failure, which is what
    // made saving a status-less task impossible.
    let new_status = status
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| row.status.clone());
    let flag = |set: Option<bool>, stored: bool| i64::from(set.unwrap_or(stored));
    let new_pinned = flag(is_pinned, row.is_pinned);
    let new_important = flag(is_important, row.is_important);
    let new_archived = flag(is_archived, row.is_archived);
    let new_cancelled = flag(is_cancelled, row.is_cancelled);
    let new_deleted = flag(is_deleted, row.is_deleted);
    let new_completed = flag(is_completed, row.is_completed);

    // The day write and the identity write are separate tables now, so they are
    // wrapped so a failure in the second cannot leave the first applied.
    let tx = conn.unchecked_transaction()?;
    let day_res = tx.execute(
        "UPDATE task_days SET status = ?1, elapsed_time = ?2, start_time = ?3, updated_at = ?4,
         is_pinned = ?5, is_important = ?6, is_archived = ?7, is_cancelled = ?8,
         is_deleted = ?9, is_completed = ?10
         WHERE id = ?11",
        params![
            new_status,
            new_elapsed,
            new_start,
            now,
            new_pinned,
            new_important,
            new_archived,
            new_cancelled,
            new_deleted,
            new_completed,
            day_id
        ],
    );
    let day_res = match day_res {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    };
    let id_res = day_res.and_then(|_| {
        if merged {
            // The merged task keeps its own identity fields: the rename only
            // links the day, it does not rewrite what the task already says.
            return Ok(());
        }
        tx.execute(
            "UPDATE tasks SET label = ?1, description = ?2, code = ?3, notes = ?4, tags = ?5,
             link = ?6, updated_at = ?7
             WHERE id = ?8 AND user_id = ?9",
            params![
                new_label,
                new_desc,
                new_code,
                new_notes,
                new_tags,
                new_link,
                now,
                row.id,
                user_id
            ],
        )
        .map(|_| ())
    });
    match id_res {
        Ok(()) => {
            tx.commit()?;
            get_task_by_day(conn, user_id, day_id)
        }
        Err(e) => {
            drop(tx);
            if is_unique(&e) {
                bail!("a task with that title already exists");
            }
            Err(e.into())
        }
    }
}

/// Link everything `source_id` owns to `target_id` and delete the source
/// identity. The caller guarantees the two share no work date — the day rows
/// move wholesale, and `idx_task_days_task_date` would reject a date both have.
/// The target's identity fields win; the source's description fills the
/// target's only when the target has none, the same rule `create_task` uses.
fn merge_identities(
    conn: &Connection,
    user_id: &str,
    target_id: i64,
    source_id: i64,
) -> Result<()> {
    let now = now_ms();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE task_days SET task_id = ?1, updated_at = ?2 WHERE task_id = ?3",
        params![target_id, now, source_id],
    )?;
    // Integrations are unique per (task, group, field) in code, not in the
    // schema, so a key the target already holds keeps the target's value.
    tx.execute(
        "DELETE FROM task_integrations WHERE task_id = ?2
         AND (\"group\", field) IN
             (SELECT \"group\", field FROM task_integrations WHERE task_id = ?1)",
        params![target_id, source_id],
    )?;
    tx.execute(
        "UPDATE task_integrations SET task_id = ?1, updated_at = ?2 WHERE task_id = ?3",
        params![target_id, now, source_id],
    )?;
    tx.execute(
        "UPDATE task_comments SET task_id = ?1 WHERE task_id = ?2",
        params![target_id, source_id],
    )?;
    tx.execute(
        "UPDATE tasks SET description = (SELECT description FROM tasks WHERE id = ?2),
         updated_at = ?3
         WHERE id = ?1 AND (description IS NULL OR description = '')",
        params![target_id, source_id, now],
    )?;
    tx.execute(
        "DELETE FROM tasks WHERE id = ?1 AND user_id = ?2",
        params![source_id, user_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Set the `done` flag on one day. If becoming done while that day is running,
/// stops the timer first. Returns `(updated_task, worklog_delta_seconds, start_ms_before_stop)`.
/// If un-done, just flips the flag (delta=0, start_ms=None).
pub fn set_done(
    conn: &Connection,
    user_id: &str,
    day_id: i64,
    done: bool,
) -> Result<Option<(Task, i64, Option<i64>)>> {
    let Some(row) = get_task_by_day(conn, user_id, day_id)? else {
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
        delta = row.running_delta(now);
        stop_row(conn, user_id, &row, now)?;
    }
    conn.execute(
        "UPDATE task_days SET done = ?1, updated_at = ?2 WHERE id = ?3",
        params![done as i64, now, day_id],
    )?;
    let updated = get_task_by_day(conn, user_id, day_id)?
        .ok_or_else(|| anyhow::anyhow!("task vanished after set_done"))?;
    Ok(Some((updated, delta, start_ms)))
}

/// Swap two day rows' positions. Both ids are `task_days.id`, because ordering
/// is a property of the day, not of the task.
pub fn reorder_swap(conn: &Connection, user_id: &str, a: i64, b: i64) -> Result<()> {
    let now = now_ms();
    let owned = |day_id: i64| -> Result<bool> {
        Ok(conn
            .query_row(
                "SELECT 1 FROM task_days td JOIN tasks t ON t.id = td.task_id
                 WHERE td.id = ?1 AND t.user_id = ?2",
                params![day_id, user_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    };
    if !owned(a)? || !owned(b)? {
        bail!("cannot reorder a task that is not yours");
    }
    let pa: i64 = conn.query_row("SELECT position FROM task_days WHERE id = ?1", params![a], |r| {
        r.get(0)
    })?;
    let pb: i64 = conn.query_row("SELECT position FROM task_days WHERE id = ?1", params![b], |r| {
        r.get(0)
    })?;
    conn.execute(
        "UPDATE task_days SET position = ?1, updated_at = ?2 WHERE id = ?3",
        params![pb, now, a],
    )?;
    conn.execute(
        "UPDATE task_days SET position = ?1, updated_at = ?2 WHERE id = ?3",
        params![pa, now, b],
    )?;
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub struct ArchiveFilter {
    pub q: Option<String>,
    pub tag: Option<String>,
    /// A resolved status catalog id, not a label: the caller maps a typed name
    /// through the catalog before it gets here. Selects which days are shown.
    pub status: Option<String>,
    /// Case-insensitive substring matched against any integration group, field,
    /// or value on the task.
    pub integration: Option<String>,
}

/// Every task, each carrying the days it was worked.
///
/// The archive is a per-task view, so a task appears once with all of its days
/// rather than once per day. `q` and `tag` select tasks; `status` selects which
/// of a task's days are shown, and a task with no surviving day is dropped.
/// Ordered so the most recently worked task surfaces first.
pub fn list_archive(
    conn: &Connection,
    user_id: &str,
    filter: &ArchiveFilter,
) -> Result<Vec<Task>> {
    // The identity query. It selects one representative day row so the shared
    // `map_row` projection still applies; the real per-day data comes from the
    // second query below and replaces it.
    let mut sql = format!(
        "SELECT {COLS} {JOIN} WHERE t.user_id = ?1
         AND td.id = (SELECT MAX(d2.id) FROM task_days d2 WHERE d2.task_id = t.id)"
    );
    let mut params_vec: Vec<String> = vec![user_id.to_string()];
    let mut next_idx = 2usize;
    if let Some(q) = filter.q.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(&format!(" AND t.label LIKE ?{next_idx} COLLATE NOCASE"));
        params_vec.push(format!("%{q}%"));
        next_idx += 1;
    }
    if let Some(tag) = filter.tag.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(&format!(" AND COALESCE(t.tags, '') LIKE ?{next_idx} COLLATE NOCASE"));
        params_vec.push(format!("%{tag}%"));
        next_idx += 1;
    }
    if let Some(int) = filter
        .integration
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        // One bound value, three comparisons — a task matches when any of the
        // integration's group/field/value carries the substring, and EXISTS
        // keeps a multi-match task from being listed twice.
        sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM task_integrations ti WHERE ti.task_id = t.id
               AND (ti.\"group\" LIKE ?{next_idx} COLLATE NOCASE
                 OR ti.field LIKE ?{next_idx} COLLATE NOCASE
                 OR COALESCE(ti.value, '') LIKE ?{next_idx} COLLATE NOCASE))"
        ));
        params_vec.push(format!("%{int}%"));
        next_idx += 1;
    }
    let _ = next_idx;
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> =
        params_vec.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    let mut tasks = stmt
        .query_map(param_refs.as_slice(), map_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Every day of every task, oldest first, grouped onto its task in one pass
    // rather than one query per task.
    let mut day_stmt = conn.prepare(
        "SELECT td.task_id, td.work_date, td.elapsed_time, td.is_running,
                td.start_time, td.status, td.done
         FROM task_days td JOIN tasks t ON t.id = td.task_id
         WHERE t.user_id = ?1
         ORDER BY td.work_date ASC, td.id ASC",
    )?;
    let mut by_task: std::collections::HashMap<i64, Vec<TaskDay>> = std::collections::HashMap::new();
    let day_rows = day_stmt.query_map(params![user_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            TaskDay {
                work_date: row.get(1)?,
                elapsed_time: row.get(2)?,
                is_running: row.get::<_, i64>(3)? != 0,
                start_time: row.get(4)?,
                status: row.get(5)?,
                done: row.get::<_, i64>(6)? != 0,
            },
        ))
    })?;
    for row in day_rows {
        let (task_id, day) = row?;
        by_task.entry(task_id).or_default().push(day);
    }

    let status_filter = filter
        .status
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    tasks.retain_mut(|t| {
        let Some(days) = by_task.remove(&t.id) else {
            return false;
        };
        t.days = match status_filter {
            Some(want) => days
                .into_iter()
                .filter(|d| d.status.eq_ignore_ascii_case(want))
                .collect(),
            None => days,
        };
        !t.days.is_empty()
    });

    // Most recently worked first, then by label so the order is stable.
    tasks.sort_by(|a, b| {
        let a_newest = a.days.last().map(|d| d.work_date.as_str()).unwrap_or("");
        let b_newest = b.days.last().map(|d| d.work_date.as_str()).unwrap_or("");
        b_newest.cmp(a_newest).then_with(|| a.label.cmp(&b.label))
    });
    Ok(tasks)
}

/// True only for a label-uniqueness or same-day-uniqueness violation, so an
/// unrelated constraint failure (a NOT NULL miss, a foreign key) is reported as
/// itself rather than as a duplicate title.
///
/// SQLite names the offending *columns*, never the index, so the message is
/// `UNIQUE constraint failed: tasks.user_id, tasks.label` — matching on the
/// index name (as this once did) never fires.
fn is_unique(e: &rusqlite::Error) -> bool {
    matches!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    ) && matches!(
        e,
        rusqlite::Error::SqliteFailure(
            _,
            Some(msg),
        ) if msg.contains("tasks.user_id, tasks.label")
            || msg.contains("task_days.task_id, task_days.work_date")
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
               code TEXT,
               description TEXT,
               link TEXT,
               notes TEXT,
               tags TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
             CREATE TABLE task_days (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               task_id INTEGER NOT NULL,
               work_date TEXT NOT NULL DEFAULT (date('now')),
               status TEXT NOT NULL DEFAULT 'todo',
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
               updated_at INTEGER NOT NULL,
               FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
             );
             CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
             CREATE TABLE task_integrations (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               task_id INTEGER NOT NULL,
               \"group\" TEXT NOT NULL,
               field TEXT NOT NULL,
               value TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE task_comments (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               task_id INTEGER NOT NULL,
               subject TEXT,
               summary TEXT,
               branch TEXT,
               pr TEXT,
               created_at INTEGER NOT NULL
             );
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
        assert_eq!(a.day_id, b.day_id);
        assert_eq!(b.description.as_deref(), Some("first"));
        assert_eq!(list_tasks(&conn, "u1", "2026-08-30").unwrap().len(), 1);
    }

    /// The point of the split: a new day is the same task, with its own row.
    #[test]
    fn create_same_label_new_day_is_the_same_task_with_its_own_day() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-29", "US-1", Some("from yesterday")).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "US-1", None).unwrap();

        // One task, two days.
        assert_eq!(a.id, b.id, "the same label must not create a second task");
        assert_ne!(a.day_id, b.day_id, "each day keeps its own row");
        assert_eq!(b.work_date, "2026-08-30");
        // The description lives on the task, so the new day still sees it.
        assert_eq!(b.description.as_deref(), Some("from yesterday"));

        let day_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM task_days WHERE task_id = ?1", [a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(day_count, 2);
        assert_eq!(list_tasks(&conn, "u1", "2026-08-29").unwrap().len(), 1);
        assert_eq!(list_tasks(&conn, "u1", "2026-08-30").unwrap().len(), 1);
    }

    /// Each day carries its own recorded time, not the task's total.
    #[test]
    fn each_day_keeps_its_own_elapsed_time() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-29", "US-1", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "US-1", None).unwrap();
        conn.execute("UPDATE task_days SET elapsed_time = 3600 WHERE id = ?1", [a.day_id])
            .unwrap();
        conn.execute("UPDATE task_days SET elapsed_time = 1800 WHERE id = ?1", [b.day_id])
            .unwrap();

        let day1 = &list_tasks(&conn, "u1", "2026-08-29").unwrap()[0];
        let day2 = &list_tasks(&conn, "u1", "2026-08-30").unwrap()[0];
        assert_eq!(day1.elapsed_time, 3600);
        assert_eq!(day2.elapsed_time, 1800);
        assert_eq!(day1.id, day2.id);
    }

    #[test]
    fn a_day_without_a_row_does_not_list_the_task() {
        let conn = setup();
        create_task(&conn, "u1", "2026-08-29", "US-1", None).unwrap();
        assert!(list_tasks(&conn, "u1", "2026-08-28").unwrap().is_empty());
    }

    #[test]
    fn position_is_per_day_across_tasks() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        let c = create_task(&conn, "u1", "2026-08-29", "C", None).unwrap();
        assert_eq!(a.position, 0);
        assert_eq!(b.position, 1);
        assert_eq!(c.position, 0, "a new day starts its own ordering");
    }

    #[test]
    fn focus_start_stops_other_running_day_rows() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-30", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-30", "B", None).unwrap();
        start_timer(&conn, "u1", a.day_id, true).unwrap();
        let (_, stopped) = start_timer(&conn, "u1", b.day_id, true).unwrap().unwrap();
        assert_eq!(stopped.len(), 1);
        assert_eq!(stopped[0].day_id, a.day_id);
        assert!(!get_task_by_day(&conn, "u1", a.day_id).unwrap().unwrap().is_running);
        assert!(get_task_by_day(&conn, "u1", b.day_id).unwrap().unwrap().is_running);
    }

    /// A running timer is running whatever day it was started on, so a focus
    /// switch on today must also stop yesterday's leftover run.
    #[test]
    fn focus_start_stops_a_running_row_from_another_day() {
        let conn = setup();
        let old = create_task(&conn, "u1", "2026-08-29", "Old", None).unwrap();
        let new = create_task(&conn, "u1", "2026-08-30", "New", None).unwrap();
        start_timer(&conn, "u1", old.day_id, true).unwrap();
        start_timer(&conn, "u1", new.day_id, true).unwrap();
        assert!(!get_task_by_day(&conn, "u1", old.day_id).unwrap().unwrap().is_running);
    }

    #[test]
    fn starting_a_new_day_adds_that_day() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-08-29", "A", None).unwrap();
        let day = ensure_day_row(&conn, "u1", t.id, "2026-08-31").unwrap();
        assert_eq!(day.work_date, "2026-08-31");
        assert_eq!(day.id, t.id);
        assert_ne!(day.day_id, t.day_id);
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM task_days WHERE task_id = ?1", [t.id], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    fn set_day(conn: &Connection, day_id: i64, col: &str) {
        conn.execute(&format!("UPDATE task_days SET {col} = 1 WHERE id = ?1"), [day_id])
            .unwrap();
    }

    #[test]
    fn archive_lists_every_task_with_all_its_days() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        create_task(&conn, "u1", "2026-08-29", "A", None).unwrap();
        create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();

        let rows = list_archive(&conn, "u1", &ArchiveFilter::default()).unwrap();
        // Two tasks, not three days.
        assert_eq!(rows.len(), 2);
        let a_row = rows.iter().find(|t| t.id == a.id).unwrap();
        assert_eq!(a_row.days.len(), 2);
        assert_eq!(
            a_row.days.iter().map(|d| d.work_date.as_str()).collect::<Vec<_>>(),
            vec!["2026-08-28", "2026-08-29"]
        );
    }

    /// The archive shows finished work too — it is the whole history now.
    #[test]
    fn archive_includes_done_and_archived_tasks() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        set_day(&conn, b.day_id, "done");
        set_day(&conn, a.day_id, "is_archived");
        let rows = list_archive(&conn, "u1", &ArchiveFilter::default()).unwrap();
        assert_eq!(rows.len(), 2);
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
                ..Default::default()
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
        conn.execute("UPDATE tasks SET tags = '[\"backend\"]' WHERE id = ?1", [a.id])
            .unwrap();
        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                tag: Some("backend".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a.id);
    }

    /// The status filter selects which DAYS show, so a task worked on two days
    /// with only one of them In Progress lists that one day.
    #[test]
    fn archive_status_filter_selects_days() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let a2 = create_task(&conn, "u1", "2026-08-29", "A", None).unwrap();
        create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        conn.execute("UPDATE task_days SET status = '21' WHERE id = ?1", [a.day_id])
            .unwrap();

        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                status: Some("21".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1, "only the task with an In Progress day survives");
        assert_eq!(rows[0].id, a.id);
        assert_eq!(rows[0].days.len(), 1);
        assert_eq!(rows[0].days[0].work_date, "2026-08-28");
        // The other day of the same task is excluded by the filter.
        assert_ne!(rows[0].days[0].work_date, a2.work_date);
    }

    #[test]
    fn archive_filter_by_integration_matches_any_column_without_duplicating() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let _b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        // Two matching rows for the same task: EXISTS must still list it once.
        crate::db::integrations::upsert(&conn, a.id, "jira", "issue_key", Some("US-1")).unwrap();
        crate::db::integrations::upsert(&conn, a.id, "jira", "sprint", Some("99")).unwrap();

        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                integration: Some("jira".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a.id);
    }

    #[test]
    fn archive_filter_by_integration_matches_the_value_too() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-08-28", "A", None).unwrap();
        let _b = create_task(&conn, "u1", "2026-08-28", "B", None).unwrap();
        crate::db::integrations::upsert(&conn, a.id, "git", "branch", Some("US-1459-fix")).unwrap();

        let rows = list_archive(
            &conn,
            "u1",
            &ArchiveFilter {
                integration: Some("1459".into()),
                ..Default::default()
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
            "UPDATE task_days SET is_running = 1, start_time = ?1 WHERE id = ?2",
            [five_sec_ago, t.day_id],
        )
        .unwrap();

        let (updated, delta, start_ms) = set_done(&conn, "u1", t.day_id, true).unwrap().unwrap();
        assert!(updated.done);
        assert!(!updated.is_running);
        assert!(delta >= 4); // at least ~5s minus rounding
        assert!(start_ms.is_some());

        // Second call is idempotent (already done)
        let (again, d2, s2) = set_done(&conn, "u1", t.day_id, true).unwrap().unwrap();
        assert!(again.done);
        assert_eq!(d2, 0);
        assert!(s2.is_none());
    }

    /// Marking one day done must leave the task's other days alone.
    #[test]
    fn set_done_only_touches_that_day() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-12", "Run", None).unwrap();
        let other = create_task(&conn, "u1", "2026-09-13", "Run", None).unwrap();
        set_done(&conn, "u1", t.day_id, true).unwrap();
        assert!(get_task_by_day(&conn, "u1", t.day_id).unwrap().unwrap().done);
        assert!(!get_task_by_day(&conn, "u1", other.day_id).unwrap().unwrap().done);
    }

    /// `status` is NOT NULL, so an update that omits it must keep the stored
    /// value rather than writing NULL — saving a status-less task used to fail
    /// the constraint outright.
    #[test]
    fn update_without_a_status_keeps_the_stored_one() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Keep", None).unwrap();
        conn.execute("UPDATE task_days SET status = '51' WHERE id = ?1", [t.day_id])
            .unwrap();

        let updated = update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                label: Some("Renamed"),
                description: Some("desc"),
                elapsed_seconds: Some(60),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(updated.label, "Renamed");
        assert_eq!(updated.status, "51", "an omitted status must not be nulled out");
    }

    /// A blank status is the same case as an omitted one.
    #[test]
    fn update_with_a_blank_status_keeps_the_stored_one() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Keep", None).unwrap();
        conn.execute("UPDATE task_days SET status = '41' WHERE id = ?1", [t.day_id])
            .unwrap();

        let updated = update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                label: Some("Renamed"),
                status: Some("  "),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.status, "41");
    }

    /// And a real status still replaces it.
    #[test]
    fn update_with_a_status_replaces_the_stored_one() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Set", None).unwrap();
        let updated = update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                status: Some("31"),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(updated.status, "31");
    }

    #[test]
    fn update_sets_link_and_flags_leaving_the_rest_alone() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Link me", Some("keep")).unwrap();
        conn.execute("UPDATE task_days SET status = '21' WHERE id = ?1", [t.day_id])
            .unwrap();

        let updated = update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                link: Some("https://example.com"),
                is_pinned: Some(true),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(updated.link.as_deref(), Some("https://example.com"));
        assert!(updated.is_pinned);
        assert_eq!(updated.label, "Link me");
        assert_eq!(updated.description.as_deref(), Some("keep"));
        assert_eq!(updated.status, "21");
        // Flags the patch did not name keep their stored value.
        assert!(!updated.is_important);
        assert!(!updated.is_archived);
    }

    /// A flag can be turned back off, and a blank link clears the column.
    #[test]
    fn update_can_clear_a_flag_and_a_link() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Clear me", None).unwrap();
        conn.execute(
            "UPDATE task_days SET is_pinned = 1 WHERE id = ?1",
            [t.day_id],
        )
        .unwrap();
        conn.execute("UPDATE tasks SET link = 'https://old' WHERE id = ?1", [t.id])
            .unwrap();

        let updated = update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                link: Some("   "),
                is_pinned: Some(false),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

        assert!(updated.link.is_none());
        assert!(!updated.is_pinned);
    }

    /// A label edit is a task-level change: it must show on every day.
    #[test]
    fn renaming_a_task_changes_it_on_every_day() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-17", "Before", None).unwrap();
        create_task(&conn, "u1", "2026-09-18", "Before", None).unwrap();
        update_task(
            &conn,
            "u1",
            t.day_id,
            TaskPatch {
                label: Some("After"),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

        for date in ["2026-09-17", "2026-09-18"] {
            let rows = list_tasks(&conn, "u1", date).unwrap();
            assert_eq!(rows[0].label, "After");
        }
    }

    /// A rename onto a label the user already owns is a duplicate title, not a
    /// raw driver error. `is_unique` must recognise SQLite's message, which
    /// names the columns (`tasks.user_id, tasks.label`) and never the index.
    #[test]
    fn renaming_onto_an_existing_label_is_reported_as_a_duplicate() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-17", "Taken", None).unwrap();
        create_task(&conn, "u1", "2026-09-17", "Other", None).unwrap();

        let err = update_task(
            &conn,
            "u1",
            a.day_id,
            TaskPatch {
                label: Some("Other"),
                ..Default::default()
            },
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("already exists"),
            "a duplicate title must be named as such, got: {err}"
        );
        // The rejected rename changed nothing.
        assert_eq!(list_tasks(&conn, "u1", "2026-09-17").unwrap()[0].label, "Taken");
    }

    /// A rename onto a title owned on ANOTHER day continues that task: the
    /// renamed day links to the existing identity, its timer included, and the
    /// duplicate identity is gone. Same-day stays a duplicate (the test above).
    #[test]
    fn renaming_onto_a_label_from_another_day_links_the_task() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-17", "Alpha", Some("keep me")).unwrap();
        let b = create_task(&conn, "u1", "2026-09-18", "Beta", Some("mine")).unwrap();
        update_task(
            &conn,
            "u1",
            b.day_id,
            TaskPatch {
                elapsed_seconds: Some(90),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::integrations::upsert(&conn, a.id, "jira", "issue_key", Some("US-1")).unwrap();
        crate::db::integrations::upsert(&conn, b.id, "jira", "issue_key", Some("US-2")).unwrap();

        let merged = update_task(
            &conn,
            "u1",
            b.day_id,
            TaskPatch {
                label: Some("Alpha"),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();

        // One identity, both days, the renamed day's time kept.
        assert_eq!(merged.id, a.id);
        assert_eq!(merged.day_id, b.day_id);
        assert_eq!(merged.elapsed_time, 90);
        assert_eq!(merged.description.as_deref(), Some("keep me"));
        let on_17 = list_tasks(&conn, "u1", "2026-09-17").unwrap();
        assert_eq!(on_17.len(), 1);
        assert_eq!(on_17[0].id, a.id);
        let on_18 = list_tasks(&conn, "u1", "2026-09-18").unwrap();
        assert_eq!(on_18.len(), 1);
        assert_eq!(on_18[0].id, a.id);
        assert_eq!(on_18[0].elapsed_time, 90);
        assert!(get_task_identity(&conn, "u1", b.id).unwrap().is_none());

        // The target's integration row wins; the source's row is dropped, not
        // left orphaned on a deleted identity.
        let keys: Vec<String> = {
            let mut stmt = conn
                .prepare(
                    "SELECT value FROM task_integrations
                     WHERE task_id = ?1 AND \"group\" = 'jira' AND field = 'issue_key'",
                )
                .unwrap();
            let rows = stmt.query_map([a.id], |r| r.get::<_, String>(0)).unwrap();
            rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        };
        assert_eq!(keys, vec!["US-1".to_string()]);

        // An empty target description is filled from the source, the same rule
        // `create_task` applies.
        let c = create_task(&conn, "u1", "2026-09-17", "Gamma", None).unwrap();
        let d = create_task(&conn, "u1", "2026-09-18", "Delta", Some("delta desc")).unwrap();
        update_task(
            &conn,
            "u1",
            d.day_id,
            TaskPatch {
                label: Some("Gamma"),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        let gamma = get_task_identity(&conn, "u1", c.id).unwrap().unwrap();
        assert_eq!(gamma.description.as_deref(), Some("delta desc"));
    }

    #[test]
    fn find_by_label_is_case_insensitive_and_date_scoped() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-13", "US-2092", None).unwrap();
        create_task(&conn, "u1", "2026-09-14", "US-2092", None).unwrap();
        let rows = find_tasks_by_label(&conn, "u1", "2026-09-13", "us-2092").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, a.id);
        assert_eq!(rows[0].day_id, a.day_id);
    }

    #[test]
    fn find_by_label_returns_all_nocase_matches() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-13", "US-2092", None).unwrap();
        conn.execute("INSERT INTO tasks (user_id, label, created_at, updated_at) VALUES ('u1', 'Us-2092', 0, 0)", [])
            .unwrap();
        let other = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO task_days (task_id, work_date, position, created_at, updated_at) VALUES (?1, '2026-09-13', 1, 0, 0)",
            [other],
        )
        .unwrap();
        let rows = find_tasks_by_label(&conn, "u1", "2026-09-13", "US-2092").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, a.id);
        assert!(rows[1].id > a.id);
    }

    /// Deleting a task takes its days with it, so the archive cannot show a
    /// task whose time has nowhere to live.
    #[test]
    fn deleting_a_task_removes_every_day() {
        let conn = setup();
        let t = create_task(&conn, "u1", "2026-09-13", "Gone", None).unwrap();
        create_task(&conn, "u1", "2026-09-14", "Gone", None).unwrap();
        assert!(delete_task(&conn, "u1", t.id).unwrap());
        let days: i64 = conn
            .query_row("SELECT COUNT(*) FROM task_days WHERE task_id = ?1", [t.id], |r| r.get(0))
            .unwrap();
        assert_eq!(days, 0);
        assert!(list_archive(&conn, "u1", &ArchiveFilter::default()).unwrap().is_empty());
    }

    #[test]
    fn reset_all_zeroes_one_day_and_leaves_the_others() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-13", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-09-14", "A", None).unwrap();
        conn.execute("UPDATE task_days SET elapsed_time = 60", []).unwrap();
        reset_all(&conn, "u1", "2026-09-13").unwrap();
        assert_eq!(get_task_by_day(&conn, "u1", a.day_id).unwrap().unwrap().elapsed_time, 0);
        assert_eq!(get_task_by_day(&conn, "u1", b.day_id).unwrap().unwrap().elapsed_time, 60);
    }

    /// Reordering is a property of the day: swapping two rows on one day must
    /// not disturb the same tasks on another day.
    #[test]
    fn reorder_swaps_positions_within_a_day() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-13", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-09-13", "B", None).unwrap();
        let a2 = create_task(&conn, "u1", "2026-09-14", "A", None).unwrap();
        reorder_swap(&conn, "u1", a.day_id, b.day_id).unwrap();

        let day1 = list_tasks(&conn, "u1", "2026-09-13").unwrap();
        assert_eq!(day1.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), vec!["B", "A"]);
        // The other day keeps its own order.
        let day2 = list_tasks(&conn, "u1", "2026-09-14").unwrap();
        assert_eq!(day2[0].day_id, a2.day_id);
    }

    #[test]
    fn reorder_refuses_a_day_row_that_is_not_yours() {
        let conn = setup();
        let a = create_task(&conn, "u1", "2026-09-13", "A", None).unwrap();
        let b = create_task(&conn, "u1", "2026-09-13", "B", None).unwrap();
        conn.execute("INSERT INTO users VALUES ('u2', 'b@b.c', 'x', 0)", []).unwrap();
        assert!(reorder_swap(&conn, "u2", a.day_id, b.day_id).is_err());
    }
}
