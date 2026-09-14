use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

use crate::db::tasks::Task;

#[derive(Debug)]
struct Credentials {
    email: String,
    token: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct IssueResponse {
    fields: Option<IssueFields>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct IssueFields {
    summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: Option<TransitionTarget>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransitionTarget {
    #[serde(default)]
    pub id: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TransitionsResponse {
    transitions: Vec<Transition>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SprintIssue {
    key: String,
    fields: Option<SprintIssueFields>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SprintIssueFields {
    summary: Option<String>,
    status: Option<IssueStatus>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct IssueStatus {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct SprintIssuesResponse {
    issues: Vec<SprintIssue>,
}

fn load_credentials() -> Option<Credentials> {
    let mut email = env::var("JIRA_EMAIL").ok();
    let mut token = env::var("JIRA_TOKEN").ok();

    if email.is_none() || token.is_none() {
        let path = env::var("JIRA_ENV_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_home().join(".aimsis").join("jira.env"));
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                let Some((key, value)) = line
                    .trim()
                    .strip_prefix("export ")
                    .unwrap_or_else(|| line.trim())
                    .split_once('=')
                else {
                    continue;
                };
                let value = value.trim().trim_matches(['"', '\\']);
                match key.trim() {
                    "JIRA_EMAIL" if email.is_none() => email = Some(value.to_string()),
                    "JIRA_TOKEN" if token.is_none() => token = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    match (email, token) {
        (Some(email), Some(token)) if !email.is_empty() && !token.is_empty() => {
            Some(Credentials { email, token })
        }
        _ => None,
    }
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn jira_site() -> String {
    env::var("JIRA_SITE")
        .unwrap_or_else(|_| "https://aimsis.atlassian.net".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn jira_client() -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build JIRA client")
}

/// Shared JIRA REST API fetcher. Returns the parsed JSON response.
pub fn jira_fetch(
    path: &str,
    method: &str,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let creds = load_credentials().context("JIRA not configured (set JIRA_EMAIL + JIRA_TOKEN)")?;
    let client = jira_client()?;
    let url = format!("{}/rest/api/3{}", jira_site(), path);
    let mut req = client
        .request(method.parse().unwrap_or(reqwest::Method::GET), &url)
        .basic_auth(&creds.email, Some(&creds.token))
        .header(reqwest::header::ACCEPT, "application/json");
    if let Some(b) = body {
        req = req
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .json(&b);
    }
    let resp = req.send().context("JIRA request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        bail!("JIRA {method} {path} -> {status}: {text}");
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.contains("application/json") {
        resp.json().context("parse JIRA JSON response")
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// Extract a JIRA issue key (e.g. "US-1459") from a task's label or description.
pub fn issue_key_from_task(label: &str, description: &str) -> Option<String> {
    issue_key(label, description)
}

fn issue_key(label: &str, description: &str) -> Option<String> {
    label
        .split_whitespace()
        .chain(description.split_whitespace())
        .find_map(|word| {
            let key = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            let (project, number) = key.split_once('-')?;
            if !project.is_empty()
                && project
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
            {
                Some(key.to_string())
            } else {
                None
            }
        })
}

fn merge_description(summary: &str, description: &str) -> String {
    if description.trim().is_empty() {
        summary.to_string()
    } else if description.contains(summary) {
        description.to_string()
    } else {
        format!("{summary}\n\n{description}")
    }
}

/// Fetch the JIRA summary for a key in the label/description and merge it into
/// the task description, matching the web task creation behavior.
pub fn description_for_task(label: &str, description: &str) -> Result<Option<String>> {
    let Some(key) = issue_key(label, description) else {
        return Ok(None);
    };
    if load_credentials().is_none() {
        return Ok(None);
    }
    let data = jira_fetch(&format!("/issue/{key}?fields=summary"), "GET", None)?;
    Ok(data
        .get("fields")
        .and_then(|f| f.get("summary"))
        .and_then(|s| s.as_str())
        .map(|summary| merge_description(summary, description)))
}

/// Post a plain-text comment to a JIRA issue.
pub fn post_comment(issue_key: &str, text: &str) -> Result<()> {
    let body = serde_json::json!({
        "body": {
            "type": "doc",
            "version": 1,
            "content": [{
                "type": "paragraph",
                "content": [{
                    "type": "text",
                    "text": text
                }]
            }]
        }
    });
    jira_fetch(&format!("/issue/{issue_key}/comment"), "POST", Some(body))?;
    Ok(())
}

/// Log work on a JIRA issue. `started_ms` is the epoch-millis when the run began;
/// when None, JIRA defaults to "now". Matches the web's logWork behavior.
pub fn log_work(issue_key: &str, seconds: i64, started_ms: Option<i64>) -> Result<()> {
    if seconds <= 0 {
        return Ok(());
    }
    let started = match started_ms {
        Some(ms) => {
            let dt = chrono::DateTime::from_timestamp_millis(ms)
                .context("invalid worklog start timestamp")?;
            dt.format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string()
        }
        None => {
            let dt = chrono::Utc::now();
            dt.format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string()
        }
    };
    let body = serde_json::json!({
        "timeSpentSeconds": seconds,
        "started": started,
    });
    jira_fetch(
        &format!("/issue/{issue_key}/worklog"),
        "POST",
        Some(body),
    )?;
    Ok(())
}

/// Get available transitions for a JIRA issue.
pub fn get_transitions(issue_key: &str) -> Result<Vec<Transition>> {
    let data = jira_fetch(&format!("/issue/{issue_key}/transitions"), "GET", None)?;
    let resp: TransitionsResponse =
        serde_json::from_value(data).context("parse transitions response")?;
    Ok(resp.transitions)
}

/// Apply a transition to a JIRA issue by transition ID.
pub fn transition_issue(issue_key: &str, transition_id: &str) -> Result<()> {
    let body = serde_json::json!({
        "transition": { "id": transition_id }
    });
    jira_fetch(
        &format!("/issue/{issue_key}/transitions"),
        "POST",
        Some(body),
    )?;
    Ok(())
}

/// Pick the transition whose target status matches (case-insensitive).
pub fn pick_transition_id(transitions: &[Transition], status_name: &str) -> Option<String> {
    let want = status_name.to_lowercase();
    transitions
        .iter()
        .find(|t| {
            t.to.as_ref()
                .and_then(|to| to.name.as_deref())
                .map(|n| n.to_lowercase() == want)
                .unwrap_or(false)
        })
        .map(|t| t.id.clone())
}

// ---------------------------------------------------------------------------
// Status name readers (match web's JIRA_STATUS_* env vars)
// ---------------------------------------------------------------------------

pub fn status_todo() -> String {
    std::env::var("JIRA_STATUS_TODO").unwrap_or_else(|_| "To Do".into())
}

pub fn status_in_progress() -> String {
    std::env::var("JIRA_STATUS_INPROGRESS").unwrap_or_else(|_| "In Progress".into())
}

pub fn status_done() -> String {
    std::env::var("JIRA_STATUS_DONE").unwrap_or_else(|_| "Cek di Local".into())
}

#[derive(Debug, Clone, Copy)]
pub struct JiraStatus {
    pub id: &'static str,
    pub label: &'static str,
}

pub const JIRA_STATUSES: &[JiraStatus] = &[
    JiraStatus { id: "11", label: "To Do" },
    JiraStatus { id: "21", label: "In Progress" },
    JiraStatus { id: "31", label: "Done" },
    JiraStatus { id: "41", label: "Local OK" },
    JiraStatus { id: "51", label: "Cek di Local" },
    JiraStatus { id: "61", label: "Cek di Prod" },
    JiraStatus { id: "71", label: "KABARI SEKOLAH" },
    JiraStatus { id: "81", label: "BLOCKED" },
];

/// Label for a stored status id. Unknown / empty → return `id` unchanged (empty stays empty).
pub fn status_label(id: &str) -> &str {
    JIRA_STATUSES
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.label)
        .unwrap_or(id)
}

/// Resolve typed or JIRA name/id to a catalog id.
/// Match id exactly, then label case-insensitive. Unknown → None.
pub fn resolve_status_id(value: &str) -> Option<&'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    JIRA_STATUSES
        .iter()
        .find(|s| s.id == trimmed)
        .or_else(|| {
            JIRA_STATUSES
                .iter()
                .find(|s| s.label.eq_ignore_ascii_case(trimmed))
        })
        .map(|s| s.id)
}

/// Transition an issue to `status_name`. No-op if no matching transition exists.
pub fn transition_to(issue_key: &str, status_name: &str) -> Result<()> {
    let transitions = get_transitions(issue_key)?;
    if let Some(id) = pick_transition_id(&transitions, status_name) {
        transition_issue(issue_key, &id)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Fire-and-forget wrappers — mirror web's run() + onStart/onStop/onSwitchStop/onDone
// ---------------------------------------------------------------------------

/// What to report to JIRA for a task that was running and has just stopped.
///
/// Capture it *before* the timer stops: the live seconds only exist while the
/// task runs, and a stopped task has nothing left to report.
#[derive(Debug)]
pub struct Worklog {
    pub task_id: i64,
    pub label: String,
    pub description: String,
    pub seconds: i64,
    pub started_ms: Option<i64>,
}

impl Worklog {
    /// `None` when the task is not running, so a caller cannot log work or
    /// reopen an issue for a task that has nothing to report.
    pub fn capture(task: &Task, now: i64) -> Option<Self> {
        task.is_running.then(|| Self {
            task_id: task.id,
            label: task.label.clone(),
            description: task.description_text().to_string(),
            seconds: task.running_delta(now),
            started_ms: task.start_time,
        })
    }

    /// Explicit stop or pause: worklog only, status unchanged.
    pub fn record(&self) {
        if self.seconds > 0 {
            fire_on_stop(&self.label, &self.description, self.seconds, self.started_ms);
        }
    }

    /// Displaced by another task starting in focus mode: worklog plus To Do.
    pub fn record_and_reopen(&self) {
        fire_on_switch_stop(&self.label, &self.description, self.seconds, self.started_ms);
    }
}

/// Timer started → transition issue to In Progress.
pub fn fire_on_start(label: &str, description: &str) {
    if load_credentials().is_none() { return; }
    let Some(key) = issue_key_from_task(label, description) else { return; };
    let _ = transition_to(&key, &status_in_progress());
}

/// Timer stopped (explicit) → log worklog only; keep current status.
pub fn fire_on_stop(label: &str, description: &str, seconds: i64, started_ms: Option<i64>) {
    if load_credentials().is_none() { return; }
    let Some(key) = issue_key_from_task(label, description) else { return; };
    let _ = log_work(&key, seconds, started_ms);
}

/// Focus-switch stop → log worklog, then transition to To Do.
/// If worklog fails, still attempt the To Do transition.
pub fn fire_on_switch_stop(label: &str, description: &str, seconds: i64, started_ms: Option<i64>) {
    if load_credentials().is_none() { return; }
    let Some(key) = issue_key_from_task(label, description) else { return; };
    let _ = log_work(&key, seconds, started_ms);
    let _ = transition_to(&key, &status_todo());
}

/// Mark done → log worklog, then transition to Cek di Local.
/// If worklog fails, still attempt the done transition.
pub fn fire_on_done(label: &str, description: &str, seconds: i64, started_ms: Option<i64>) {
    if load_credentials().is_none() { return; }
    let Some(key) = issue_key_from_task(label, description) else { return; };
    let _ = log_work(&key, seconds, started_ms);
    let _ = transition_to(&key, &status_done());
}

/// Fetch issues from a JIRA sprint (Agile REST API).
/// Returns (issue_key, summary, status_name) tuples.
#[allow(dead_code)]
pub fn fetch_sprint_issues(board: &str, sprint_id: &str) -> Result<Vec<(String, String, String)>> {
    let creds = load_credentials().context("JIRA not configured")?;
    let client = jira_client()?;
    let url = format!(
        "{}/rest/agile/1.0/board/{board}/sprint/{sprint_id}/issue?fields=summary,status",
        jira_site()
    );
    let resp = client
        .get(&url)
        .basic_auth(&creds.email, Some(&creds.token))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .context("JIRA sprint request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        bail!("JIRA sprint fetch -> {status}: {text}");
    }
    let data: SprintIssuesResponse = resp.json().context("parse sprint issues response")?;
    Ok(data
        .issues
        .into_iter()
        .map(|i| {
            let summary = i
                .fields
                .as_ref()
                .and_then(|f| f.summary.clone())
                .unwrap_or_default();
            let status_name = i
                .fields
                .as_ref()
                .and_then(|f| f.status.as_ref())
                .and_then(|s| s.name.clone())
                .unwrap_or_default();
            (i.key, summary, status_name)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_issue_key_in_label_or_description() {
        assert_eq!(issue_key("US-1459 fix", ""), Some("US-1459".into()));
        assert_eq!(issue_key("fix", "see AIM-22"), Some("AIM-22".into()));
        assert_eq!(issue_key("ordinary task", "no ticket"), None);
    }

    #[test]
    fn merges_summary_without_duplicating_existing_text() {
        assert_eq!(merge_description("Fix login", ""), "Fix login");
        assert_eq!(
            merge_description("Fix login", "repro steps"),
            "Fix login\n\nrepro steps"
        );
        assert_eq!(
            merge_description("Fix login", "Fix login\n\nrepro steps"),
            "Fix login\n\nrepro steps"
        );
    }

    #[test]
    fn pick_transition_matches_case_insensitive() {
        let transitions = vec![
            Transition {
                id: "11".into(),
                name: "To Do".into(),
                to: Some(TransitionTarget {
                    id: Some("11".into()),
                    name: Some("To Do".into()),
                }),
            },
            Transition {
                id: "21".into(),
                name: "Start Progress".into(),
                to: Some(TransitionTarget {
                    id: Some("21".into()),
                    name: Some("In Progress".into()),
                }),
            },
        ];
        assert_eq!(
            pick_transition_id(&transitions, "in progress"),
            Some("21".into())
        );
        assert_eq!(
            pick_transition_id(&transitions, "To Do"),
            Some("11".into())
        );
        assert_eq!(pick_transition_id(&transitions, "Cek lokal"), None);
    }

    #[test]
    fn pick_transition_handles_none_target() {
        let transitions = vec![Transition {
            id: "31".into(),
            name: "Do it".into(),
            to: None,
        }];
        assert_eq!(pick_transition_id(&transitions, "anything"), None);
    }

    #[test]
    fn status_defaults_are_nonempty() {
        // If env vars happen to be set in the test runner, we just assert non-empty.
        assert!(!status_todo().is_empty());
        assert!(!status_in_progress().is_empty());
        assert!(!status_done().is_empty());
    }

    #[test]
    fn status_label_maps_catalog_ids() {
        assert_eq!(status_label("41"), "Local OK");
        assert_eq!(status_label("11"), "To Do");
        assert_eq!(status_label("99"), "99");
        assert_eq!(status_label(""), "");
    }

    #[test]
    fn resolve_status_id_matches_id_then_label() {
        assert_eq!(resolve_status_id("51"), Some("51"));
        assert_eq!(resolve_status_id("cek di local"), Some("51"));
        assert_eq!(resolve_status_id("nope"), None);
    }

    fn task(id: i64, is_running: bool, elapsed_time: i64, start_time: Option<i64>) -> Task {
        Task {
            id,
            label: "US-2092".into(),
            description: Some("fix login".into()),
            code: None,
            link: None,
            status: "todo".into(),
            notes: None,
            tags: None,
            elapsed_time,
            total_time: 0,
            position: 0,
            is_running,
            done: false,
            is_completed: false,
            is_cancelled: false,
            is_deleted: false,
            is_archived: false,
            is_pinned: false,
            is_important: false,
            start_time,
            end_time: None,
            work_date: "2026-09-13".into(),
        }
    }

    #[test]
    fn worklog_capture_keeps_live_seconds_before_stop() {
        let now = crate::timer::now_ms();
        // 60s committed, started 5s ago: only the live 5s are reportable.
        let running = task(7, true, 60, Some(now - 5_000));
        let worklog = Worklog::capture(&running, now).expect("running task has a worklog");
        assert_eq!(worklog.task_id, 7);
        assert_eq!(worklog.seconds, 5);
        assert_eq!(worklog.label, "US-2092");
        assert_eq!(worklog.description, "fix login");
        assert_eq!(worklog.started_ms, Some(now - 5_000));

        // A stopped task has nothing to report, so no worklog and no reopen.
        let stopped = task(7, false, 65, None);
        assert!(Worklog::capture(&stopped, now).is_none());
    }
}
