mod keymap;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::time::{Duration as StdDuration, Instant};

use crate::clipboard;
use crate::db;
use crate::db::tasks::{self, Task};
use crate::git;
use crate::jira::{self, Warnings, Worklog};
use crate::jira::Transition;
use crate::report::{self, ReportTask};
use crate::timer::{format_time, now_ms, parse_time_input};

const STATUS_TTL: StdDuration = StdDuration::from_secs(3);

pub use keymap::HELP;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Label,
    Description,
    Elapsed,
    Status,
    Code,
    Notes,
    Tags,
    Link,
    Flags,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppMode {
    Daily,
    Archive,
    Kanban,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchiveInput {
    Label,
    Tag,
    Status,
    Integration,
}

pub struct KanbanState {
    pub cards: Vec<tasks::KanbanCard>,
    pub projects: Vec<String>,
    pub selected_col: usize,
    pub selected_row: usize,
    /// Latest identity snapshot for the selected card (`focused_task` in Kanban).
    pub focus_cache: Option<Task>,
}

pub struct ArchiveState {
    pub filter_q: Option<String>,
    pub filter_tag: Option<String>,
    pub filter_status: Option<String>,
    pub filter_integration: Option<String>,
    pub tasks: Vec<Task>,
    pub selected: usize,
}

#[derive(Clone)]
pub enum JiraMode {
    Menu,
    Comment { buffer: String },
    Transition { transitions: Vec<Transition>, selected: usize },
    SyncPick,
    FetchKey { buffer: String },
    Syncing,
}

#[derive(Clone)]
pub enum GitMode {
    Menu,
    LinkBranch { buffer: String },
    Commits { commits: Vec<git::Commit>, ahead: u32, behind: u32 },
    PrStatus { prs: Vec<crate::bitbucket::PullRequest>, selected: usize },
}

pub enum Overlay {
    None,
    Help,
    ConfirmDelete,
    ConfirmResetAll,
    Detail {
        comments: Vec<crate::db::comments::Comment>,
        prs: Vec<crate::bitbucket::PullRequest>,
        statuses: Vec<crate::bitbucket::CommitStatus>,
        /// Stored `task_integrations` rows — the `jira` key/status the sprint
        /// fetch writes, and the linked `git` branch. Read from the DB rather
        /// than re-fetched, so the detail view shows what was saved.
        integrations: Vec<crate::db::integrations::Integration>,
    },
    /// The comment list for one task, optionally with a compose buffer open.
    Comments {
        task_id: i64,
        comments: Vec<crate::db::comments::Comment>,
        selected: usize,
        compose: Option<String>,
    },
    Filter { input: ArchiveInput, buffer: String },
    Form {
        edit_day_id: Option<i64>,
        field: Field,
        label: String,
        description: String,
        elapsed: String,
        status: String,
        code: String,
        notes: String,
        tags: String,
        link: String,
        is_pinned: bool,
        is_important: bool,
        is_archived: bool,
        is_cancelled: bool,
        is_deleted: bool,
        is_completed: bool,
        /// The status field's dropdown: `Some(cursor)` while it is open, where
        /// the cursor indexes `jira::JIRA_STATUSES` and its length is the
        /// "No status" row. `None` means the list is closed. It lives on the
        /// form rather than in an overlay of its own, so opening it never
        /// disturbs the other fields.
        status_pick: Option<usize>,
    },
    Jira { mode: JiraMode },
    Git { mode: GitMode },
}

struct DetailUpdate {
    prs: Vec<crate::bitbucket::PullRequest>,
    statuses: Vec<crate::bitbucket::CommitStatus>,
    warnings: Vec<String>,
}

pub struct App {
    pub conn: Connection,
    pub user_id: String,
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub timer_mode: String,
    pub mode: AppMode,
    pub archive: ArchiveState,
    pub kanban: KanbanState,
    pub jira_monitor_projects: Vec<String>,
    pub jira_pipelines: Option<crate::config::JiraPipelinesConfig>,
    pub overlay: Overlay,
    pub status: String,
    status_until: Option<Instant>,
    clipboard: Option<arboard::Clipboard>,
    /// Still read from `config.toml`, but the four JQL sprint modes scope by
    /// sprint id alone — the Agile board URL was the only consumer.
    #[allow(dead_code)]
    pub jira_board: Option<String>,
    pub jira_sprint_id: Option<String>,
    pub git_repo_path: Option<std::path::PathBuf>,
    pub bitbucket_workspace: Option<String>,
    pub bitbucket_repo: Option<String>,
    /// Agent state per task id, for the daily list's badge. Refreshed with the
    /// task list, since both come from the same DB and change together.
    pub agent_states: std::collections::HashMap<i64, crate::db::integrations::AgentState>,
    hook_listener: crate::hooks::HookListener,
    detail_updates: Option<std::sync::mpsc::Receiver<DetailUpdate>>,
}

/// Everything the app takes from `config.toml` plus the open connection and the
/// day being shown — grouped so `App::new` stays a single argument.
pub struct AppConfig {
    pub conn: Connection,
    pub user_id: String,
    pub date: NaiveDate,
    pub timer_mode: String,
    pub jira_board: Option<String>,
    pub jira_sprint_id: Option<String>,
    pub git_repo_path: Option<std::path::PathBuf>,
    pub bitbucket_workspace: Option<String>,
    pub bitbucket_repo: Option<String>,
    pub jira_monitor_projects: Vec<String>,
    pub jira_pipelines: Option<crate::config::JiraPipelinesConfig>,
}

impl App {
    pub fn new(cfg: AppConfig) -> Result<Self> {
        let AppConfig {
            conn,
            user_id,
            date,
            timer_mode,
            jira_board,
            jira_sprint_id,
            git_repo_path,
            bitbucket_workspace,
            bitbucket_repo,
            jira_monitor_projects,
            jira_pipelines,
        } = cfg;
        let mut app = Self {
            conn,
            user_id,
            date,
            tasks: Vec::new(),
            selected: 0,
            timer_mode,
            mode: AppMode::Daily,
            archive: ArchiveState {
                filter_q: None,
                filter_tag: None,
                filter_status: None,
                filter_integration: None,
                tasks: Vec::new(),
                selected: 0,
            },
            overlay: Overlay::None,
            status: String::new(),
            status_until: None,
            clipboard: None,
            jira_board,
            jira_sprint_id,
            git_repo_path,
            bitbucket_workspace,
            bitbucket_repo,
            kanban: KanbanState {
                cards: Vec::new(),
                projects: jira_monitor_projects.clone(),
                selected_col: 0,
                selected_row: 0,
                focus_cache: None,
            },
            jira_monitor_projects,
            jira_pipelines,
            agent_states: std::collections::HashMap::new(),
            hook_listener: crate::hooks::HookListener::new(),
            detail_updates: None,
        };
        app.reload()?;
        Ok(app)
    }

    pub fn date_str(&self) -> String {
        self.date.format("%Y-%m-%d").to_string()
    }

    pub fn header_date(&self) -> String {
        self.date.format("%Y-%m-%d (%a)").to_string()
    }

    pub fn total_elapsed(&self) -> i64 {
        let now = now_ms();
        self.tasks.iter().map(|t| t.current_elapsed(now)).sum()
    }

    pub fn running_label(&self) -> Option<&str> {
        self.tasks
            .iter()
            .find(|t| t.is_running)
            .map(|t| t.label.as_str())
    }

    pub fn selected_task(&self) -> Option<&Task> {
        self.tasks.get(self.selected)
    }

    fn reload(&mut self) -> Result<()> {
        self.tasks = tasks::list_tasks(&self.conn, &self.user_id, &self.date_str())?;
        // Same cadence as the task list: the orchestrator writes these rows, so a
        // badge is never more stale than the times beside it.
        self.agent_states = crate::db::integrations::agent_states(&self.conn).unwrap_or_default();
        if self.selected >= self.tasks.len() && !self.tasks.is_empty() {
            self.selected = self.tasks.len() - 1;
        }
        if self.tasks.is_empty() {
            self.selected = 0;
        }
        Ok(())
    }

    pub fn archive_selected_task(&self) -> Option<&Task> {
        self.archive.tasks.get(self.archive.selected)
    }

    /// The task the current mode's cursor is on. Daily and archive each keep
    /// their own list, so anything opened from a key press must resolve through
    /// the mode rather than assuming the daily list.
    pub fn focused_task(&self) -> Option<&Task> {
        match self.mode {
            AppMode::Archive => self.archive_selected_task(),
            AppMode::Kanban => self.kanban.focus_cache.as_ref(),
            AppMode::Daily => self.selected_task(),
        }
    }

    fn kanban_columns(&self) -> Vec<String> {
        crate::kanban::kanban_column_order()
    }

    fn update_kanban_focus_cache(&mut self) {
        let columns = self.kanban_columns();
        let card = crate::kanban::kanban_card_at(
            &self.kanban.cards,
            &columns,
            self.kanban.selected_col,
            self.kanban.selected_row,
        );
        self.kanban.focus_cache = match card {
            Some(c) => tasks::get_task_identity(&self.conn, &self.user_id, c.task_id).ok().flatten(),
            None => None,
        };
    }

    fn clamp_kanban_cursor(&mut self) {
        let columns = self.kanban_columns();
        crate::kanban::clamp_kanban_cursor(
            &mut self.kanban.selected_col,
            &mut self.kanban.selected_row,
            &columns,
            &self.kanban.cards,
        );
        self.update_kanban_focus_cache();
    }

    fn reload_archive(&mut self) -> Result<()> {
        // The status filter holds a catalog id: typing "In Progress" resolves to
        // "21" here so the query compares ids rather than labels.
        let status = self
            .archive
            .filter_status
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| jira::resolve_status_id(s).unwrap_or(s).to_string());
        let filter = tasks::ArchiveFilter {
            q: self.archive.filter_q.clone(),
            tag: self.archive.filter_tag.clone(),
            status,
            integration: self.archive.filter_integration.clone(),
        };
        self.archive.tasks = tasks::list_archive(&self.conn, &self.user_id, &filter)?;
        if self.archive.selected >= self.archive.tasks.len() && !self.archive.tasks.is_empty() {
            self.archive.selected = self.archive.tasks.len() - 1;
        }
        if self.archive.tasks.is_empty() {
            self.archive.selected = 0;
        }
        Ok(())
    }


    fn reload_kanban(&mut self) -> Result<()> {
        let today = self.date_str();
        self.kanban.cards = tasks::list_kanban_cards(&self.conn, &self.user_id, &today)?;
        self.clamp_kanban_cursor();
        Ok(())
    }

    fn enter_kanban(&mut self) -> Result<()> {
        self.mode = AppMode::Kanban;
        self.overlay = Overlay::None;
        self.reload_kanban()?;
        let columns = self.kanban_columns();
        if !self.kanban.cards.is_empty() {
            let (col, row) = crate::kanban::initial_kanban_cursor(&columns, &self.kanban.cards);
            self.kanban.selected_col = col;
            self.kanban.selected_row = row;
        } else {
            self.kanban.selected_col = 0;
            self.kanban.selected_row = 0;
        }
        self.update_kanban_focus_cache();
        Ok(())
    }

    fn kanban_add_to_today(&mut self) -> Result<()> {
        let columns = self.kanban_columns();
        let Some(card) = crate::kanban::kanban_card_at(
            &self.kanban.cards,
            &columns,
            self.kanban.selected_col,
            self.kanban.selected_row,
        ) else {
            self.set_status("no kanban card selected");
            return Ok(());
        };
        let label = card.label.clone();
        let task_id = card.task_id;
        let day = tasks::ensure_day_row(&self.conn, &self.user_id, task_id, &self.date_str())?;
        self.mode = AppMode::Daily;
        self.overlay = Overlay::None;
        self.reload()?;
        if let Some(i) = self.tasks.iter().position(|t| t.id == day.id) {
            self.selected = i;
        }
        self.set_status(format!("added '{}' to today", label));
        Ok(())
    }

    fn kanban_refresh(&mut self) -> Result<()> {
        self.set_status("kanban: refreshing jira…");
        let (_, warnings) = tasks::refresh_jira_statuses(&self.conn, &self.user_id)?;
        self.reload_kanban()?;
        if warnings.is_empty() {
            self.set_status("kanban: refreshed jira status");
        } else {
            self.set_status(format!("kanban: refreshed ({})", warnings.len()));
        }
        Ok(())
    }


    fn kanban_stub_config(&self) -> crate::config::Config {
        crate::config::Config {
            database_path: String::new(),
            user_email: String::new(),
            timer_mode: None,
            jira_board: None,
            jira_sprint_id: None,
            jira_monitor_projects: Some(self.jira_monitor_projects.clone()),
            jira_pipelines: self.jira_pipelines.clone(),
            bitbucket_workspace: None,
            bitbucket_repo: None,
            git_repo_path: None,
        }
    }

    fn kanban_pipeline_local_ok(&mut self) -> Result<()> {
        let cfg = self.kanban_stub_config();
        match crate::kanban::run_local_ok_pr_sync(&cfg, &self.jira_monitor_projects, true, false) {
            Ok(_) => self.set_status("kanban: local-ok dry-run ok"),
            Err(e) => self.set_status(format!("kanban pipeline: {e:#}")),
        }
        Ok(())
    }

    fn kanban_pull(&mut self) -> Result<()> {
        self.set_status("kanban: pulling sprint…");
        let report = crate::kanban::kanban_pull_sprint(
            &self.conn,
            &self.user_id,
            &self.date_str(),
            &self.jira_monitor_projects,
        )?;
        self.reload_kanban()?;
        self.set_status(format!("kanban: pulled {} issue(s)", report.ingested));
        Ok(())
    }

    fn toggle_archive(&mut self) -> Result<()> {
        if self.mode == AppMode::Archive || self.mode == AppMode::Kanban {
            self.mode = AppMode::Daily;
            self.reload()?;
        } else {
            self.mode = AppMode::Archive;
            self.reload_archive()?;
        }
        self.overlay = Overlay::None;
        Ok(())
    }

    fn open_archive_filter(&mut self, input: ArchiveInput) {
        let current = match input {
            ArchiveInput::Label => self.archive.filter_q.clone().unwrap_or_default(),
            ArchiveInput::Tag => self.archive.filter_tag.clone().unwrap_or_default(),
            ArchiveInput::Status => self.archive.filter_status.clone().unwrap_or_default(),
            ArchiveInput::Integration => {
                self.archive.filter_integration.clone().unwrap_or_default()
            }
        };
        self.overlay = Overlay::Filter { input, buffer: current };
    }

    fn continue_today(&mut self) -> Result<()> {
        let (label, desc) = if let Some(t) = self.archive_selected_task() {
            (t.label.clone(), t.description.clone())
        } else {
            self.set_status("no task selected in archive");
            return Ok(());
        };
        let today = self.date_str();
        let created = tasks::create_task(&self.conn, &self.user_id, &today, &label, desc.as_deref())?;
        self.mode = AppMode::Daily;
        self.overlay = Overlay::None;
        self.reload()?;
        if let Some(i) = self.tasks.iter().position(|t| t.id == created.id) {
            self.selected = i;
        }
        self.set_status(format!("continued '{}' today", label));
        Ok(())
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_until = Some(Instant::now() + STATUS_TTL);
    }

    fn err_status(&mut self, e: anyhow::Error) {
        self.set_status(e.to_string());
    }

    /// Surface JIRA hook failures in the status line, appended to whatever the
    /// caller just reported. The TUI owns an alternate screen, so a hook cannot
    /// write to stderr without corrupting the display.
    fn show_jira(&mut self, warnings: Warnings) {
        if warnings.is_empty() {
            return;
        }
        let suffix = format!("JIRA: {}", warnings.join("; "));
        let msg = if self.status.is_empty() {
            suffix
        } else {
            format!("{} — {suffix}", self.status)
        };
        self.set_status(msg);
    }

    fn tick_status(&mut self) {
        if let Some(until) = self.status_until {
            if Instant::now() >= until {
                self.status.clear();
                self.status_until = None;
            }
        }
    }

    fn shift_day(&mut self, days: i64) -> Result<()> {
        self.date += Duration::days(days);
        self.selected = 0;
        self.reload()
    }

    fn toggle_mode(&mut self) -> Result<()> {
        self.timer_mode = if self.timer_mode == "focus" {
            "parallel".into()
        } else {
            "focus".into()
        };
        db::user::set_timer_mode(&self.conn, &self.user_id, &self.timer_mode, now_ms())?;
        self.set_status(format!("mode: {}", self.timer_mode));
        Ok(())
    }

    fn export_markdown(&mut self) {
        let now = now_ms();
        let report_tasks: Vec<ReportTask<'_>> = self
            .tasks
            .iter()
            .map(|t| ReportTask {
                label: &t.label,
                description: t.description.as_deref(),
                elapsed_seconds: t.current_elapsed(now),
            })
            .collect();
        let has_time = report_tasks.iter().any(|t| t.elapsed_seconds > 0);
        if !has_time {
            self.set_status(if self.tasks.is_empty() {
                "no tasks to export"
            } else {
                "no tasks with recorded time"
            });
            return;
        }
        let markdown = report::build_markdown_report(&report_tasks, &self.date_str());
        match clipboard::set_text(&mut self.clipboard, markdown) {
            Ok(()) => self.set_status("markdown report copied to clipboard"),
            Err(e) => self.set_status(format!("clipboard error: {e}")),
        }
    }

    fn start_stop(&mut self) -> Result<()> {
        let Some((day_id, running)) = self.selected_task().map(|t| (t.day_id, t.is_running)) else {
            return Ok(());
        };
        let warnings = if running {
            self.stop_task(day_id)?
        } else {
            self.start_task(day_id)?
        };
        self.reload()?;
        self.show_jira(warnings);
        Ok(())
    }

    /// Start a task's timer for the day being viewed. In focus mode, stops every
    /// other running day row (with worklog + To Do transition), including one
    /// left running on an earlier day. Fires the In Progress transition for the
    /// started task.
    fn start_task(&mut self, day_id: i64) -> Result<Warnings> {
        let exclusive = self.timer_mode == "focus";
        let (started, stopped_rows) =
            match tasks::start_timer(&self.conn, &self.user_id, day_id, exclusive)? {
                Some(pair) => pair,
                None => return Ok(Warnings::new()),
            };
        // Every local write lands before any JIRA call: the list must read
        // correctly even while a slow or unreachable JIRA is being asked.
        jira::auto_status(&self.conn, &self.user_id, day_id);
        for victim in &stopped_rows {
            jira::auto_status(&self.conn, &self.user_id, victim.day_id);
        }
        // Fire JIRA: focus-switched tasks → worklog + To Do
        let mut warnings = Warnings::new();
        for victim in &stopped_rows {
            if let Some(worklog) = Worklog::capture(victim, now_ms()) {
                warnings.extend(worklog.record_and_reopen());
            }
        }
        // Fire JIRA: started task → In Progress
        warnings.extend(jira::fire_on_start(
            &started.label,
            started.description_text(),
        ));
        Ok(warnings)
    }

    /// Stop a day's timer. Fires worklog + To Do; Shift-D is what moves an issue to done.
    fn stop_task(&mut self, day_id: i64) -> Result<Warnings> {
        let worklog = self
            .tasks
            .iter()
            .find(|t| t.day_id == day_id)
            .and_then(|t| Worklog::capture(t, now_ms()));
        tasks::stop_timer(&self.conn, &self.user_id, day_id)?;
        // A run ending on an unfinished task reads To Do again — the row must
        // not keep showing In Progress with no timer behind it. Written before
        // the JIRA call so the list is correct even while JIRA is slow.
        jira::auto_status(&self.conn, &self.user_id, day_id);
        Ok(worklog
            .as_ref()
            .map(Worklog::record_and_reopen)
            .unwrap_or_default())
    }

    /// Stop all running tasks. Fires worklog + To Do for each, so a hook pause or
    /// session end leaves no issue stranded in In Progress.
    /// Returns whether any tasks were running, plus any JIRA hook failures.
    fn stop_all_running(&mut self) -> Result<(bool, Warnings)> {
        let now = now_ms();
        // Running timers are found across every day, not only the one on screen:
        // a run left going on an earlier date still has to be closed out.
        let running = tasks::list_running(&self.conn, &self.user_id)?;
        let stopped: Vec<Worklog> = running
            .iter()
            .filter_map(|t| Worklog::capture(t, now))
            .collect();
        let had = !stopped.is_empty();
        for task in &running {
            tasks::stop_timer(&self.conn, &self.user_id, task.day_id)?;
        }
        // Local state first, JIRA second — a slow JIRA must not delay the list.
        for task in &running {
            jira::auto_status(&self.conn, &self.user_id, task.day_id);
        }
        let mut warnings = Warnings::new();
        for worklog in &stopped {
            warnings.extend(worklog.record_and_reopen());
        }
        Ok((had, warnings))
    }

    fn open_create(&mut self) {
        self.overlay = Overlay::Form {
            edit_day_id: None,
            field: Field::Label,
            label: String::new(),
            description: String::new(),
            elapsed: "00:00:00".into(),
            status: String::new(),
            code: String::new(),
            notes: String::new(),
            tags: String::new(),
            link: String::new(),
            is_pinned: false,
            is_important: false,
            is_archived: false,
            is_cancelled: false,
            is_deleted: false,
            is_completed: false,
            status_pick: None,
        };
    }

    fn open_edit(&mut self) {
        let Some(t) = self.selected_task() else {
            return;
        };
        self.overlay = Overlay::Form {
            // `update_task` addresses the day row, and `tasks.id` and
            // `task_days.id` are separate sequences — passing `t.id` here edited
            // whichever task happened to own that day number instead.
            edit_day_id: Some(t.day_id),
            field: Field::Label,
            label: t.label.clone(),
            description: t.description.clone().unwrap_or_default(),
            elapsed: format_time(t.current_elapsed(now_ms())),
            // The field holds a catalog id; the label is for display only, so
            // storing it here would leave the dropdown unable to find its row.
            status: jira::resolve_status_id(&t.status).unwrap_or(&t.status).to_string(),
            code: t.code.clone().unwrap_or_default(),
            notes: t.notes.clone().unwrap_or_default(),
            tags: t.tags.clone().unwrap_or_default(),
            link: t.link.clone().unwrap_or_default(),
            is_pinned: t.is_pinned,
            is_important: t.is_important,
            is_archived: t.is_archived,
            is_cancelled: t.is_cancelled,
            is_deleted: t.is_deleted,
            is_completed: t.is_completed,
            status_pick: None,
        };
    }
    /// Open the form's status dropdown on the row the field already holds, or
    /// close it if it is already open. The dropdown is part of the form, so
    /// this is a field toggle rather than a separate overlay.
    fn toggle_status_pick(&mut self) {
        let Overlay::Form {
            status,
            status_pick,
            ..
        } = &mut self.overlay
        else {
            return;
        };
        if status_pick.is_some() {
            *status_pick = None;
            return;
        }
        // A task stored before the field held catalog ids can still carry a
        // label, so resolve both; anything unrecognised parks on "No status".
        let id = jira::resolve_status_id(status.trim()).unwrap_or(status.trim());
        *status_pick = Some(
            jira::JIRA_STATUSES
                .iter()
                .position(|s| s.id == id)
                .unwrap_or(jira::JIRA_STATUSES.len()),
        );
    }

    fn submit_form(&mut self) -> Result<()> {
        // Copy the form out: the save path calls `&mut self` helpers, so the
        // overlay cannot stay borrowed while it runs.
        let Overlay::Form {
            edit_day_id,
            label,
            description,
            elapsed,
            status,
            code,
            notes,
            tags,
            link,
            is_pinned,
            is_important,
            is_archived,
            is_cancelled,
            is_deleted,
            is_completed,
            ..
        } = &self.overlay
        else {
            return Ok(());
        };
        let (
            edit_day_id,
            label,
            description,
            elapsed,
            status,
            code,
            notes,
            tags,
            link,
            is_pinned,
            is_important,
            is_archived,
            is_cancelled,
            is_deleted,
            is_completed,
        ) = (
            *edit_day_id,
            label.clone(),
            description.clone(),
            elapsed.clone(),
            status.clone(),
            code.clone(),
            notes.clone(),
            tags.clone(),
            link.clone(),
            *is_pinned,
            *is_important,
            *is_archived,
            *is_cancelled,
            *is_deleted,
            *is_completed,
        );
        if label.trim().is_empty() {
            self.set_status("label is required");
            return Ok(());
        }
        let elapsed_secs = match parse_time_input(&elapsed) {
            Some(s) => s,
            None => {
                self.set_status("invalid time — use HH:MM:SS or minutes (e.g. 1.5)");
                return Ok(());
            }
        };
        // The status field only ever holds a catalog id or a legacy value from
        // before the field was a picker, so resolve defensively and keep an
        // unrecognised value rather than wiping it.
        let status_opt = jira::resolve_status_id(status.trim()).or(if status.is_empty() {
            None
        } else {
            Some(status.as_str())
        });
        let code_opt = if code.is_empty() { None } else { Some(code.as_str()) };
        let notes_opt = if notes.is_empty() { None } else { Some(notes.as_str()) };
        let tags_opt = if tags.is_empty() { None } else { Some(tags.as_str()) };
        let link_opt = if link.trim().is_empty() {
            None
        } else {
            Some(link.as_str())
        };
        let flags = tasks::TaskPatch {
            is_pinned: Some(is_pinned),
            is_important: Some(is_important),
            is_archived: Some(is_archived),
            is_cancelled: Some(is_cancelled),
            is_deleted: Some(is_deleted),
            is_completed: Some(is_completed),
            ..Default::default()
        };
        if let Some(day_id) = edit_day_id {
            tasks::update_task(
                &self.conn,
                &self.user_id,
                day_id,
                tasks::TaskPatch {
                    label: Some(&label),
                    description: Some(&description),
                    elapsed_seconds: Some(elapsed_secs),
                    code: code_opt,
                    status: status_opt,
                    notes: notes_opt,
                    tags: tags_opt,
                    link: link_opt,
                    ..flags
                },
            )?;
        } else {
            let mut final_description = description.clone();
            let mut jira_error = None;
            match jira::description_for_task(&label, &description) {
                Ok(Some(value)) => final_description = value,
                Ok(None) => {}
                Err(err) => jira_error = Some(format!("JIRA lookup failed: {err}")),
            }
            let desc = if final_description.trim().is_empty() {
                None
            } else {
                Some(final_description.as_str())
            };
            let date = self.date_str();
            // Re-adding a label already worked today returns that day's row
            // (`create_task`'s contract). The blank create form must not run
            // its defaults over it: `elapsed_seconds: Some(0)` zeroes the
            // stored time and rebases a running timer, and the flags would
            // clear pinned/important.
            let prior = tasks::day_row_for_label(&self.conn, &self.user_id, &date, &label)?;
            let created = tasks::create_task(&self.conn, &self.user_id, &date, &label, desc)?;
            if prior.is_none() {
                // `create_task` only knows label and description, so everything else
                // the form collected — including the link and the flags — lands here.
                // It writes the day row the create just made.
                tasks::update_task(
                    &self.conn,
                    &self.user_id,
                    created.day_id,
                    tasks::TaskPatch {
                        elapsed_seconds: Some(elapsed_secs),
                        code: code_opt,
                        status: status_opt,
                        notes: notes_opt,
                        tags: tags_opt,
                        link: link_opt,
                        ..flags
                    },
                )?;
            }
            self.reload()?;
            if let Some(i) = self.tasks.iter().position(|t| t.id == created.id) {
                self.selected = i;
            }
            self.overlay = Overlay::None;
            if let Some(msg) = jira_error {
                self.set_status(msg);
            }
            return Ok(());
        }
        self.overlay = Overlay::None;
        self.status.clear();
        self.status_until = None;
        self.reload()
    }

    fn reorder(&mut self, delta: isize) -> Result<()> {
        if self.tasks.len() < 2 {
            return Ok(());
        }
        let i = self.selected as isize + delta;
        if i < 0 || i >= self.tasks.len() as isize {
            return Ok(());
        }
        // Ordering is a property of the day, so the swap names day rows.
        let a = self.tasks[self.selected].day_id;
        let b = self.tasks[i as usize].day_id;
        tasks::reorder_swap(&self.conn, &self.user_id, a, b)?;
        self.selected = i as usize;
        self.reload()
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Result<bool> {
        let (input, buffer) = match &mut self.overlay {
            Overlay::Filter { input, buffer } => (*input, buffer),
            _ => return Ok(false),
        };
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
            }
            KeyCode::Enter => {
                let value = buffer.trim().to_string();
                let opt = if value.is_empty() { None } else { Some(value) };
                match input {
                    ArchiveInput::Label => self.archive.filter_q = opt,
                    ArchiveInput::Tag => self.archive.filter_tag = opt,
                    ArchiveInput::Status => self.archive.filter_status = opt,
                    ArchiveInput::Integration => self.archive.filter_integration = opt,
                }
                self.archive.selected = 0;
                self.overlay = Overlay::None;
                if let Err(e) = self.reload_archive() {
                    self.err_status(e);
                }
            }
            KeyCode::Backspace => {
                buffer.pop();
            }
            KeyCode::Char(c) if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT => {
                buffer.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn open_jira_menu(&mut self) {
        let Some(t) = self.selected_task() else {
            self.set_status("no task selected");
            return;
        };
        let key = jira::issue_key_from_task(&t.label, t.description.as_deref().unwrap_or(""));
        if key.is_none() {
            self.set_status("no JIRA key in task label/description");
            return;
        }
        self.overlay = Overlay::Jira {
            mode: JiraMode::Menu,
        };
    }

    fn jira_issue_key(&self) -> Option<String> {
        self.selected_task()
            .and_then(|t| jira::issue_key_from_task(&t.label, t.description.as_deref().unwrap_or("")))
    }

    /// Open the focused task's JIRA issue in the browser.
    ///
    /// JIRA renders its own issue better than a TUI panel can, and the PRD's
    /// non-goals rule out replicating it, so the TUI's part is the handoff: a
    /// key in the label or description is enough, and a task without one gets a
    /// status line rather than a failed spawn.
    fn open_jira_issue(&mut self) {
        if self.mode == AppMode::Kanban {
            let columns = self.kanban_columns();
            let Some(card) = crate::kanban::kanban_card_at(
                &self.kanban.cards,
                &columns,
                self.kanban.selected_col,
                self.kanban.selected_row,
            ) else {
                return;
            };
            let key = card.issue_key.clone();
            let url = jira::issue_url(&key);
            match jira::open_in_browser(&url) {
                Ok(()) => self.set_status(format!("opened {key} in browser")),
                Err(e) => self.set_status(format!("{e:#}")),
            }
            return;
        }
        let Some(task) = self.focused_task() else {
            return;
        };
        let Some(key) = jira::issue_key_from_task(&task.label, task.description_text()) else {
            self.set_status("no JIRA key in task label/description");
            return;
        };
        let url = jira::issue_url(&key);
        match jira::open_in_browser(&url) {
            Ok(()) => self.set_status(format!("opened {key} in browser")),
            Err(e) => self.set_status(format!("{e:#}")),
        }
    }

    fn handle_jira_key(&mut self, key: KeyEvent) -> Result<bool> {
        let jira = std::mem::replace(&mut self.overlay, Overlay::None);
        let Overlay::Jira { mut mode } = jira else {
            self.overlay = jira;
            return Ok(false);
        };

        // Phase 1: in-place state mutations (typing, navigation)
        match &mut mode {
            JiraMode::Comment { buffer } => match key.code {
                KeyCode::Backspace => { buffer.pop(); }
                KeyCode::Char(c)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    buffer.push(c);
                }
                _ => {}
            },
            JiraMode::FetchKey { buffer } => match key.code {
                KeyCode::Backspace => { buffer.pop(); }
                KeyCode::Char(c)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    buffer.push(c);
                }
                _ => {}
            },
            JiraMode::Transition { ref transitions, ref mut selected } => match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    *selected = (*selected + 1).min(transitions.len() - 1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    *selected = selected.saturating_sub(1);
                }
                _ => {}
            },
            _ => {}
        }

        // Phase 2: overlay state transitions
        match mode {
            JiraMode::Menu => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    // self.overlay already Overlay::None
                }
                KeyCode::Char('1') => {
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::Comment {
                            buffer: String::new(),
                        },
                    };
                }
                KeyCode::Char('2') => {
                    let key = match self.jira_issue_key() {
                        Some(k) => k,
                        None => {
                            self.set_status("no JIRA key");
                            return Ok(false);
                        }
                    };
                    match jira::get_transitions(&key) {
                        Ok(trans) if trans.is_empty() => {
                            self.set_status("no transitions available");
                        }
                        Ok(trans) => {
                            self.overlay = Overlay::Jira {
                                mode: JiraMode::Transition {
                                    transitions: trans,
                                    selected: 0,
                                },
                            };
                        }
                        Err(e) => {
                            self.set_status(format!("transitions: {e}"));
                        }
                    }
                }
                KeyCode::Char('3') => {
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::SyncPick,
                    };
                }
                _ => {
                    self.overlay = Overlay::Jira { mode: JiraMode::Menu };
                }
            },
            JiraMode::Comment { buffer } => match key.code {
                KeyCode::Esc => {
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::Menu,
                    };
                }
                KeyCode::Enter => {
                    if buffer.trim().is_empty() {
                        self.set_status("comment is empty");
                        self.overlay = Overlay::Jira {
                            mode: JiraMode::Comment { buffer },
                        };
                        return Ok(false);
                    }
                    let key = match self.jira_issue_key() {
                        Some(k) => k,
                        None => {
                            self.set_status("no JIRA key");
                            return Ok(false);
                        }
                    };
                    let text = buffer.trim().to_string();
                    match jira::post_comment(&key, &text) {
                        Ok(()) => {
                            self.set_status(format!("comment posted to {key}"));
                        }
                        Err(e) => {
                            self.set_status(format!("comment failed: {e}"));
                        }
                    }
                }
                _ => {
                    // preserve typed buffer
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::Comment { buffer },
                    };
                }
            },
            JiraMode::Transition { transitions, selected } => match key.code {
                KeyCode::Esc => {
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::Menu,
                    };
                }
                KeyCode::Enter => {
                    if let Some(t) = transitions.get(selected) {
                        let tid = t.id.clone();
                        let tname = t.to.as_ref()
                            .and_then(|to| to.name.clone())
                            .unwrap_or_else(|| t.name.clone());
                        let key = match self.jira_issue_key() {
                            Some(k) => k,
                            None => {
                                self.set_status("no JIRA key");
                                return Ok(false);
                            }
                        };
                        match jira::transition_issue(&key, &tid) {
                            Ok(()) => {
                                let status_id = t
                                    .to
                                    .as_ref()
                                    .and_then(|to| to.id.clone())
                                    .or_else(|| jira::resolve_status_id(&tname).map(str::to_string));
                                if let Some(status_id) = status_id {
                                    if let Some(task) = self.selected_task() {
                                        let _ = tasks::update_task(
                                            &self.conn,
                                            &self.user_id,
                                            task.day_id,
                                            tasks::TaskPatch {
                                                status: Some(&status_id),
                                                ..Default::default()
                                            },
                                        );
                                        self.reload().ok();
                                    }
                                }
                                self.set_status(format!("{key} → {tname}"));
                            }
                            Err(e) => {
                                self.set_status(format!("transition failed: {e}"));
                            }
                        }
                    }
                }
                _ => {
                    // preserve selected index
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::Transition { transitions, selected },
                    };
                }
            },
            JiraMode::SyncPick => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = Overlay::Jira { mode: JiraMode::Menu };
                }
                KeyCode::Char('1') => self.run_sprint_query(jira::SprintQuery::Unassigned),
                KeyCode::Char('2') => self.run_sprint_query(jira::SprintQuery::ReporterUndone),
                KeyCode::Char('3') => self.run_sprint_query(jira::SprintQuery::AssigneeUndone),
                KeyCode::Char('4') => {
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::FetchKey { buffer: String::new() },
                    };
                }
                _ => {
                    self.overlay = Overlay::Jira { mode: JiraMode::SyncPick };
                }
            },
            JiraMode::FetchKey { buffer } => match key.code {
                KeyCode::Esc => {
                    self.overlay = Overlay::Jira { mode: JiraMode::SyncPick };
                }
                KeyCode::Enter => {
                    let key = buffer.trim().to_string();
                    if key.is_empty() {
                        self.set_status("issue key is empty");
                        self.overlay = Overlay::Jira {
                            mode: JiraMode::FetchKey { buffer },
                        };
                        return Ok(false);
                    }
                    self.overlay = Overlay::Jira { mode: JiraMode::Syncing };
                    match jira::fetch_issue(&key) {
                        Ok(issue) => {
                            let issues = vec![issue];
                            match self.ingest_jira_issues(&issues) {
                                Ok(msg) => {
                                    self.set_status(msg);
                                    if let Err(e) = self.reload() { self.err_status(e); }
                                }
                                Err(e) => self.set_status(format!("fetch by key: {e}")),
                            }
                        }
                        Err(e) => self.set_status(format!("fetch by key: {e}")),
                    }
                    self.overlay = Overlay::None;
                }
                _ => {
                    // preserve typed buffer
                    self.overlay = Overlay::Jira {
                        mode: JiraMode::FetchKey { buffer },
                    };
                }
            },
            JiraMode::Syncing => match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    // self.overlay already Overlay::None
                }
                _ => {
                    self.overlay = Overlay::Jira { mode: JiraMode::Syncing };
                }
            },
        }
        Ok(false)
    }


    /// Create-or-update today's tasks for a batch of JIRA issues and link each
    /// to its issue key. Reports the split so the caller can toast it.
    ///
    /// The rows themselves are written by `tasks::ingest_jira_issues` — the same
    /// function `jira ensure` calls — so the label format and integration keying
    /// have exactly one owner.
    fn ingest_jira_issues(&mut self, issues: &[(String, String, String)]) -> Result<String> {
        let today = self.date_str();
        let mut created = 0u32;
        let mut updated = 0u32;
        let user_id = self.user_id.clone();
        let rows = tasks::ingest_jira_issues(&self.conn, &user_id, &today, issues)?;
        for (task, (_, summary, _)) in rows.iter().zip(issues.iter()) {
            if task.description.as_deref() == Some(summary.as_str()) && !summary.is_empty() {
                // newly created (description matched summary = no prior desc)
                created += 1;
            } else {
                updated += 1;
            }
        }
        let mut msg = format!("{created} created, {updated} updated, {} total", issues.len());
        if issues.len() >= 50 {
            msg.push_str(" (first 50)");
        }
        Ok(msg)
    }

    /// Run one of the four sprint modes: fetch, then ingest. Needs only the
    /// sprint id — the board is not part of JQL.
    fn run_sprint_query(&mut self, query: jira::SprintQuery) {
        let Some(sprint_id) = self.jira_sprint_id.clone() else {
            self.set_status("set jira_sprint_id in config.toml");
            return;
        };
        self.overlay = Overlay::Jira { mode: JiraMode::Syncing };
        let jql = jira::jql_for(query, &sprint_id);
        match jira::search_issues(&jql) {
            Ok(issues) => match self.ingest_jira_issues(&issues) {
                Ok(msg) => {
                    self.set_status(format!("jira: {msg}"));
                    if let Err(e) = self.reload() {
                        self.err_status(e);
                    }
                }
                Err(e) => self.set_status(format!("jira sync: {e}")),
            },
            Err(e) => self.set_status(format!("jira sync: {e}")),
        }
        self.overlay = Overlay::None;
    }

    /// Pull the newest PR comments for a branch into `task_comments`. Capped at
    /// three PRs and 20 comments each so one import cannot flood a task, and
    /// deduped on (pr, summary) so re-running it does not double rows.
    fn import_pr_comments(
        &mut self,
        task_id: i64,
        branch: &str,
        workspace: &str,
        repo: &str,
    ) -> Result<u32> {
        let prs = crate::bitbucket::list_prs(workspace, repo, branch)?;
        let existing = db::comments::list(&self.conn, task_id)?;
        let mut imported = 0u32;
        for pr in prs.iter().take(3) {
            let comments = crate::bitbucket::get_pr_comments(workspace, repo, pr.id)?;
            for c in comments {
                let summary = c
                    .content
                    .as_ref()
                    .and_then(|c| c.raw.as_deref())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if summary.is_empty() {
                    continue;
                }
                let pr_id = pr.id.to_string();
                let seen = existing.iter().any(|e| {
                    e.pr.as_deref() == Some(pr_id.as_str())
                        && e.summary.as_deref() == Some(summary.as_str())
                });
                if seen {
                    continue;
                }
                let author = c
                    .user
                    .as_ref()
                    .and_then(|u| u.display_name.as_deref())
                    .unwrap_or("unknown");
                let subject = format!("PR #{} {author}", pr.id);
                db::comments::add(
                    &self.conn,
                    task_id,
                    Some(&subject),
                    Some(&summary),
                    Some(branch),
                    Some(&pr_id),
                )?;
                imported += 1;
            }
        }
        Ok(imported)
    }

    /// Stored details are immediate; remote enrichment must never block input.
    fn open_detail(&mut self) {
        self.detail_updates = None;
        let Some(task_id) = self.focused_task().map(|t| t.id) else {
            return;
        };
        self.overlay = Overlay::Detail {
            comments: db::comments::list(&self.conn, task_id).unwrap_or_default(),
            prs: Vec::new(),
            statuses: Vec::new(),
            integrations: db::integrations::list(&self.conn, task_id).unwrap_or_default(),
        };
        let Some(branch) = self.git_task_branch().filter(|b| !b.is_empty()) else {
            return;
        };
        let (Some(workspace), Some(repo_name)) =
            (self.bitbucket_workspace.clone(), self.bitbucket_repo.clone())
        else {
            return;
        };
        let repo = self.git_repo_path.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        self.detail_updates = Some(receiver);
        std::thread::spawn(move || {
            let mut update = DetailUpdate {
                prs: Vec::new(),
                statuses: Vec::new(),
                warnings: Vec::new(),
            };
            match crate::bitbucket::list_prs(&workspace, &repo_name, &branch) {
                Ok(prs) => update.prs = prs,
                Err(e) => update.warnings.push(format!("bitbucket: {e}")),
            }
            if let Some(repo) = repo {
                if let Ok(commits) = git::commits_for_branch(&repo, &branch, 1) {
                    if let Some(head) = commits.first() {
                        match crate::bitbucket::commit_statuses(&workspace, &repo_name, &head.hash) {
                            Ok(statuses) => update.statuses = statuses,
                            Err(e) => update.warnings.push(format!("bitbucket: {e}")),
                        }
                    }
                }
            }
            // Closing or reopening detail drops this request's receiver.
            let _ = sender.send(update);
        });
    }

    fn poll_detail_updates(&mut self) {
        if !matches!(self.overlay, Overlay::Detail { .. }) {
            self.detail_updates = None;
            return;
        }
        let Some(receiver) = &self.detail_updates else {
            return;
        };
        let update = match receiver.try_recv() {
            Ok(update) => update,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.detail_updates = None;
                return;
            }
        };
        self.detail_updates = None;
        if let Overlay::Detail { prs, statuses, .. } = &mut self.overlay {
            *prs = update.prs;
            *statuses = update.statuses;
        }
        if !update.warnings.is_empty() {
            self.set_status(update.warnings.join("; "));
        }
    }

    /// Open the comment list for the selected task.
    fn open_comments(&mut self) {
        let Some(task_id) = self.focused_task().map(|t| t.id) else {
            return;
        };
        let comments = match db::comments::list(&self.conn, task_id) {
            Ok(c) => c,
            Err(e) => {
                self.err_status(e);
                return;
            }
        };
        self.overlay = Overlay::Comments {
            task_id,
            comments,
            selected: 0,
            compose: None,
        };
    }

    /// Keys for the comment list and its compose buffer.
    fn handle_comments_key(&mut self, key: KeyEvent) -> Result<()> {
        // Take the overlay out: the branches below call `&mut self` helpers, so
        // it cannot stay borrowed across them.
        let taken = std::mem::replace(&mut self.overlay, Overlay::None);
        let Overlay::Comments {
            task_id,
            comments,
            mut selected,
            mut compose,
        } = taken
        else {
            self.overlay = taken;
            return Ok(());
        };
        let restore = |comments: Vec<db::comments::Comment>, selected, compose| Overlay::Comments {
            task_id,
            comments,
            selected,
            compose,
        };
        if let Some(buffer) = compose.as_mut() {
            match key.code {
                KeyCode::Esc => {
                    self.overlay = restore(comments, selected, None);
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    self.overlay = restore(comments, selected, compose);
                }
                KeyCode::Char(c)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    buffer.push(c);
                    self.overlay = restore(comments, selected, compose);
                }
                KeyCode::Enter => {
                    let text = buffer.trim().to_string();
                    if text.is_empty() {
                        self.set_status("comment is empty");
                        self.overlay = restore(comments, selected, compose);
                        return Ok(());
                    }
                    match db::comments::add(
                        &self.conn,
                        task_id,
                        Some("comment"),
                        Some(&text),
                        None,
                        None,
                    ) {
                        Ok(_) => {
                            self.set_status("comment added");
                            let refreshed = db::comments::list(&self.conn, task_id)?;
                            self.overlay = restore(refreshed, 0, None);
                        }
                        Err(e) => {
                            self.err_status(e);
                            self.overlay = restore(comments, selected, compose);
                        }
                    }
                }
                _ => {
                    self.overlay = restore(comments, selected, compose);
                }
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('C') => {
                // self.overlay already Overlay::None
            }
            KeyCode::Char('n') => {
                self.overlay = restore(comments, selected, Some(String::new()));
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !comments.is_empty() {
                    selected = (selected + 1).min(comments.len() - 1);
                }
                self.overlay = restore(comments, selected, None);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
                self.overlay = restore(comments, selected, None);
            }
            _ => {
                self.overlay = restore(comments, selected, None);
            }
        }
        Ok(())
    }

    fn open_git_menu(&mut self) {
        if self.git_repo_path.is_none() {
            self.set_status("git repo not found (set git_repo_path in config)");
            return;
        }
        self.overlay = Overlay::Git { mode: GitMode::Menu };
    }

    fn git_task_branch(&self) -> Option<String> {
        let task = self.focused_task()?;
        let integrations = db::integrations::list(&self.conn, task.id).ok()?;
        integrations
            .iter()
            .find(|i| i.group == "git" && i.field == "branch")
            .and_then(|i| i.value.clone())
    }

    fn handle_git_key(&mut self, key: KeyEvent) -> Result<bool> {
        let mode = match &self.overlay {
            Overlay::Git { mode } => mode.clone(),
            _ => return Ok(false),
        };
        match mode {
            GitMode::Menu => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = Overlay::None;
                }
                KeyCode::Char('1') => {
                    // Link current branch to task
                    let Some(repo) = self.git_repo_path.clone() else {
                        self.set_status("no git repo");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    };
                    match git::current_branch(&repo) {
                        Ok(branch) => {
                            self.overlay = Overlay::Git {
                                mode: GitMode::LinkBranch { buffer: branch },
                            };
                        }
                        Err(e) => {
                            self.set_status(format!("git: {e}"));
                            self.overlay = Overlay::None;
                        }
                    }
                }
                KeyCode::Char('2') => {
                    // Show commits
                    let Some(repo) = self.git_repo_path.clone() else {
                        self.set_status("no git repo");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    };
                    let branch = self.git_task_branch().unwrap_or_default();
                    if branch.is_empty() {
                        self.set_status("no branch linked — use option 1 first");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    }
                    match (
                        git::commits_for_branch(&repo, &branch, 15),
                        git::branch_ahead_behind(&repo, &branch),
                    ) {
                        (Ok(commits), Ok((ahead, behind))) => {
                            self.overlay = Overlay::Git {
                                mode: GitMode::Commits { commits, ahead, behind },
                            };
                        }
                        (Err(e), _) | (_, Err(e)) => {
                            self.set_status(format!("git: {e}"));
                            self.overlay = Overlay::None;
                        }
                    }
                }
                KeyCode::Char('3') => {
                    // PR status
                    match (&self.bitbucket_workspace, &self.bitbucket_repo) {
                        (Some(ws), Some(repo_name)) => {
                            let branch = self.git_task_branch().unwrap_or_default();
                            if branch.is_empty() {
                                self.set_status("no branch linked — use option 1 first");
                                self.overlay = Overlay::None;
                                return Ok(false);
                            }
                            match crate::bitbucket::list_prs(ws, repo_name, &branch) {
                                Ok(prs) if prs.is_empty() => {
                                    self.set_status("no PRs found for this branch");
                                    self.overlay = Overlay::None;
                                }
                                Ok(prs) => {
                                    self.overlay = Overlay::Git {
                                        mode: GitMode::PrStatus { prs, selected: 0 },
                                    };
                                }
                                Err(e) => {
                                    self.set_status(format!("bitbucket: {e}"));
                                    self.overlay = Overlay::None;
                                }
                            }
                        }
                        _ => {
                            self.set_status("set bitbucket_workspace + bitbucket_repo in config");
                            self.overlay = Overlay::None;
                        }
                    }
                }
                KeyCode::Char('4') => {
                    // Import PR comments into task_comments.
                    let Some(task_id) = self.selected_task().map(|t| t.id) else {
                        self.set_status("no task selected");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    };
                    let branch = self.git_task_branch().unwrap_or_default();
                    if branch.is_empty() {
                        self.set_status("no branch linked — use option 1 first");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    }
                    let (Some(ws), Some(repo_name)) =
                        (self.bitbucket_workspace.clone(), self.bitbucket_repo.clone())
                    else {
                        self.set_status("set bitbucket_workspace + bitbucket_repo in config");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    };
                    match self.import_pr_comments(task_id, &branch, &ws, &repo_name) {
                        Ok(n) => self.set_status(format!("imported {n} comments")),
                        Err(e) => self.set_status(format!("bitbucket: {e}")),
                    }
                    self.overlay = Overlay::None;
                }
                _ => {}
            },
            GitMode::LinkBranch { mut buffer } => match key.code {
                KeyCode::Esc => {
                    self.overlay = Overlay::Git { mode: GitMode::Menu };
                }
                KeyCode::Enter => {
                    let branch = buffer.trim().to_string();
                    if branch.is_empty() {
                        self.set_status("branch name is empty");
                        return Ok(false);
                    }
                    let Some(task) = self.selected_task() else {
                        self.set_status("no task selected");
                        self.overlay = Overlay::None;
                        return Ok(false);
                    };
                    let task_id = task.id;
                    if let Err(e) = db::integrations::upsert(&self.conn, task_id, "git", "branch", Some(&branch)) {
                        self.set_status(format!("db error: {e}"));
                    } else {
                        self.set_status(format!("linked branch '{branch}'"));
                    }
                    self.overlay = Overlay::None;
                }
                KeyCode::Backspace => {
                    buffer.pop();
                }
                KeyCode::Char(c)
                    if key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT =>
                {
                    buffer.push(c);
                }
                _ => {}
            },
            GitMode::Commits { .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = Overlay::Git { mode: GitMode::Menu };
                }
                _ => {}
            },
            GitMode::PrStatus { .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.overlay = Overlay::Git { mode: GitMode::Menu };
                }
                _ => {}
            },
        }
        Ok(false)
    }

    /// Keys while the form's status dropdown is open. Consumes everything it
    /// recognises and reports whether it handled the key, so the caller never
    /// falls through to the text fields underneath.
    fn handle_status_pick_key(&mut self, key: KeyEvent) -> bool {
        // The row past the last status is "No status".
        let last = jira::JIRA_STATUSES.len();
        let Overlay::Form {
            field,
            status,
            status_pick,
            ..
        } = &mut self.overlay
        else {
            return false;
        };
        let Some(cursor) = *status_pick else {
            return false;
        };
        match key.code {
            // Esc leaves the field as it was; only Enter applies a choice.
            KeyCode::Esc => *status_pick = None,
            // Tab closes the list and lets focus move on as usual.
            KeyCode::Tab | KeyCode::BackTab => {
                *status_pick = None;
                return false;
            }
            KeyCode::Char('j') | KeyCode::Down => *status_pick = Some((cursor + 1).min(last)),
            KeyCode::Char('k') | KeyCode::Up => *status_pick = Some(cursor.saturating_sub(1)),
            KeyCode::Char(digit @ '0'..='8') => {
                *status_pick = Some((digit as usize - '0' as usize).min(last));
            }
            KeyCode::Enter => {
                *status = jira::JIRA_STATUSES
                    .get(cursor)
                    .map(|s| s.id.to_string())
                    .unwrap_or_default();
                *status_pick = None;
                // Move on like Tab would. Staying put would make the next Enter
                // reopen the list instead of saving the form the footer
                // promises Enter saves.
                *field = Field::Code;
            }
            _ => return false,
        }
        true
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> Result<bool> {
        // The dropdown owns the keyboard while it is open, so a `j` there moves
        // the cursor instead of typing into the field behind it.
        if self.handle_status_pick_key(key) {
            return Ok(false);
        }
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                return Ok(false);
            }
            KeyCode::Enter => {
                // On the status field Enter opens the catalog rather than
                // saving — the field is a dropdown, and the form's own hint
                // says so.
                if matches!(&self.overlay, Overlay::Form { field: Field::Status, .. }) {
                    self.toggle_status_pick();
                    return Ok(false);
                }
                self.submit_form()?;
                return Ok(false);
            }
            _ => {}
        }
        // Any other printable key on the status field opens the catalog too, so
        // the dropdown is reachable without knowing the Enter binding.
        let status_focused = matches!(&self.overlay, Overlay::Form { field: Field::Status, .. });
        if status_focused
            && matches!(key.code, KeyCode::Char(_))
            && (key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT)
        {
            self.toggle_status_pick();
            return Ok(false);
        }
        let Overlay::Form {
            field,
            label,
            description,
            elapsed,
            status: _,
            code,
            notes,
            tags,
            link,
            is_pinned,
            is_important,
            is_archived,
            is_cancelled,
            is_deleted,
            is_completed,
            status_pick: _,
            edit_day_id: _,
        } = &mut self.overlay
        else {
            return Ok(false);
        };
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                *field = match field {
                    Field::Label => Field::Description,
                    Field::Description => Field::Status,
                    Field::Status => Field::Code,
                    Field::Code => Field::Elapsed,
                    Field::Elapsed => Field::Notes,
                    Field::Notes => Field::Tags,
                    Field::Tags => Field::Link,
                    Field::Link => Field::Flags,
                    Field::Flags => Field::Label,
                };
            }
            // The flags field is a row of toggles, not text: these keys flip
            // one each. Daily `d`/`D` bindings are unaffected — this only fires
            // while the form is open on the flags field.
            KeyCode::Char(c)
                if *field == Field::Flags
                    && (key.modifiers == KeyModifiers::NONE
                        || key.modifiers == KeyModifiers::SHIFT) =>
            {
                let target = match c.to_ascii_lowercase() {
                    'p' => Some(is_pinned),
                    'i' => Some(is_important),
                    'a' => Some(is_archived),
                    'c' => Some(is_cancelled),
                    'x' => Some(is_deleted),
                    'd' => Some(is_completed),
                    _ => None,
                };
                if let Some(flag) = target {
                    *flag = !*flag;
                }
            }
            KeyCode::Backspace => {
                let buf = match field {
                    Field::Label => label,
                    Field::Description => description,
                    Field::Elapsed => elapsed,
                    // Status is chosen from the catalog, never typed.
                    Field::Status => return Ok(false),
                    Field::Code => code,
                    Field::Notes => notes,
                    Field::Tags => tags,
                    Field::Link => link,
                    Field::Flags => return Ok(false),
                };
                buf.pop();
            }
            KeyCode::Char(c)
                if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT =>
            {
                let buf = match field {
                    Field::Label => label,
                    Field::Description => description,
                    Field::Elapsed => elapsed,
                    Field::Status => return Ok(false),
                    Field::Code => code,
                    Field::Notes => notes,
                    Field::Tags => tags,
                    Field::Link => link,
                    Field::Flags => return Ok(false),
                };
                buf.push(c);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }
        match &self.overlay {
            Overlay::Filter { .. } => return self.handle_filter_key(key),
            Overlay::Form { .. } => return self.handle_form_key(key),
            Overlay::Help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
                ) {
                    self.overlay = Overlay::None;
                }
                return Ok(false);
            }
            Overlay::ConfirmDelete => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        if let Some(id) = self.selected_task().map(|t| t.id) {
                            tasks::delete_task(&self.conn, &self.user_id, id)?;
                        }
                        self.overlay = Overlay::None;
                        self.reload()?;
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                }
                return Ok(false);
            }
            Overlay::ConfirmResetAll => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        let date = self.date_str();
                        // A reset stops every timer, so no row stays In Progress.
                        let day_ids: Vec<i64> = self.tasks.iter().map(|t| t.day_id).collect();
                        tasks::reset_all(&self.conn, &self.user_id, &date)?;
                        for day_id in day_ids {
                            jira::auto_status(&self.conn, &self.user_id, day_id);
                        }
                        self.overlay = Overlay::None;
                        self.reload()?;
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        self.overlay = Overlay::None;
                    }
                    _ => {}
                }
                return Ok(false);
            }
            Overlay::Detail { .. } => {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('q') => {
                        self.overlay = Overlay::None;
                        self.detail_updates = None;
                    }
                    // The detail footer advertises it, so `C` works from here too.
                    KeyCode::Char('C') => self.open_comments(),
                    // The footer advertises this too: hand the issue to the browser.
                    KeyCode::Char('o') => self.open_jira_issue(),
                    _ => {}
                }
                return Ok(false);
            }
            Overlay::Comments { .. } => {
                self.handle_comments_key(key)?;
                return Ok(false);
            }
            Overlay::Jira { .. } => return self.handle_jira_key(key),
            Overlay::Git { .. } => return self.handle_git_key(key),
            Overlay::None => {}
        }

        if self.mode == AppMode::Archive {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('a') | KeyCode::Esc => {
                    self.toggle_archive()?;
                    return Ok(false);
                }
                KeyCode::Char('?') => self.overlay = Overlay::Help,
                KeyCode::Char('j') | KeyCode::Down => {
                    if !self.archive.tasks.is_empty() {
                        self.archive.selected = (self.archive.selected + 1).min(self.archive.tasks.len() - 1);
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.archive.selected = self.archive.selected.saturating_sub(1);
                }
                KeyCode::Char('/') => self.open_archive_filter(ArchiveInput::Label),
                KeyCode::Char('t') => self.open_archive_filter(ArchiveInput::Tag),
                KeyCode::Char('s') => self.open_archive_filter(ArchiveInput::Status),
                KeyCode::Char('g') => self.open_archive_filter(ArchiveInput::Integration),
                KeyCode::Char('c') | KeyCode::Enter => self.continue_today()?,
                KeyCode::Char('i') if self.archive_selected_task().is_some() => {
                    self.open_detail();
                }
                KeyCode::Char('C') if self.archive_selected_task().is_some() => {
                    self.open_comments();
                }
                KeyCode::Char('o') if self.archive_selected_task().is_some() => {
                    self.open_jira_issue();
                }
                _ => {}
            }
            return Ok(false);
        }

        if self.mode == AppMode::Kanban {
            let columns = self.kanban_columns();
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('a') | KeyCode::Esc => {
                    self.mode = AppMode::Daily;
                    self.reload()?;
                    return Ok(false);
                }
                KeyCode::Char('?') => self.overlay = Overlay::Help,
                KeyCode::Char('r') => self.kanban_refresh()?,
                KeyCode::Char('P') => self.kanban_pull()?,
                KeyCode::Char('p') => self.kanban_pipeline_local_ok()?,
                KeyCode::Left | KeyCode::Char('h') => {
                    crate::kanban::move_kanban_col(
                        &mut self.kanban.selected_col,
                        &mut self.kanban.selected_row,
                        -1,
                        &columns,
                        &self.kanban.cards,
                    );
                    self.update_kanban_focus_cache();
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    crate::kanban::move_kanban_col(
                        &mut self.kanban.selected_col,
                        &mut self.kanban.selected_row,
                        1,
                        &columns,
                        &self.kanban.cards,
                    );
                    self.update_kanban_focus_cache();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    crate::kanban::move_kanban_row(
                        self.kanban.selected_col,
                        &mut self.kanban.selected_row,
                        -1,
                        &columns,
                        &self.kanban.cards,
                    );
                    self.update_kanban_focus_cache();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    crate::kanban::move_kanban_row(
                        self.kanban.selected_col,
                        &mut self.kanban.selected_row,
                        1,
                        &columns,
                        &self.kanban.cards,
                    );
                    self.update_kanban_focus_cache();
                }
                KeyCode::Char('i') => {
                    if crate::kanban::kanban_card_at(
                        &self.kanban.cards,
                        &columns,
                        self.kanban.selected_col,
                        self.kanban.selected_row,
                    )
                    .is_some()
                    {
                        self.open_detail();
                    }
                }
                KeyCode::Char('o') => self.open_jira_issue(),
                KeyCode::Char('c') | KeyCode::Enter => self.kanban_add_to_today()?,
                _ => {}
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('a') => { self.toggle_archive()?; }
            KeyCode::Char('B') => { self.enter_kanban()?; }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('J') => self.reorder(1)?,
            KeyCode::Char('K') => self.reorder(-1)?,
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_jira_menu();
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.open_git_menu();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.tasks.is_empty() {
                    self.selected = (self.selected + 1).min(self.tasks.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Char('h') | KeyCode::Left => self.shift_day(-1)?,
            KeyCode::Char('l') | KeyCode::Right => self.shift_day(1)?,
            KeyCode::Char(' ') => self.start_stop()?,
            KeyCode::Char('n') => self.open_create(),
            KeyCode::Char('e') => self.open_edit(),
            KeyCode::Char('i') => {
                if self.selected_task().is_some() {
                    self.open_detail();
                }
            }
            KeyCode::Char('C') => {
                if self.selected_task().is_some() {
                    self.open_comments();
                }
            }
            KeyCode::Char('d') => {
                if self.selected_task().is_some() {
                    self.overlay = Overlay::ConfirmDelete;
                }
            }
            KeyCode::Char('D') => {
                // Shift-D: toggle done flag (JIRA → Cek di Local on false→true)
                if let Some(t) = self.selected_task().cloned() {
                    let new_done = !t.done;
                    let result = tasks::set_done(&self.conn, &self.user_id, t.day_id, new_done)?;
                    // Local state first: the row reads Done / To Do immediately,
                    // whether or not the JIRA hook below can be reached.
                    jira::auto_status(&self.conn, &self.user_id, t.day_id);
                    let mut warnings = Warnings::new();
                    if let Some((_, delta, start_ms)) = result {
                        if new_done {
                            warnings =
                                jira::fire_on_done(&t.label, t.description_text(), delta, start_ms);
                        }
                    }
                    self.reload()?;
                    self.set_status(if new_done { "marked done" } else { "unmarked done" });
                    self.show_jira(warnings);
                }
            }
            KeyCode::Char('r') => {
                if let Some(day_id) = self.selected_task().map(|t| t.day_id) {
                    tasks::reset_task(&self.conn, &self.user_id, day_id)?;
                    // A reset stops the timer, so the row is no longer In Progress.
                    jira::auto_status(&self.conn, &self.user_id, day_id);
                    self.reload()?;
                }
            }
            KeyCode::Char('R') => self.overlay = Overlay::ConfirmResetAll,
            KeyCode::Char('m') => self.toggle_mode()?,
            KeyCode::Char('x') => self.export_markdown(),
            KeyCode::Char('o') => self.open_jira_issue(),
            _ => {}
        }
        Ok(false)
    }

    fn handle_hook_event(&mut self, ev: crate::hooks::HookEvent) -> Result<()> {
        use crate::hooks::HookEvent;
        match ev {
            HookEvent::SessionStart { task_id } => {
                // A hook names a task, not a day: start it on the day being
                // viewed, creating that day's row if this is the first run.
                let (task_id, label) = if let Some(tid) = task_id {
                    match tasks::get_task_on(&self.conn, &self.user_id, tid, &self.date_str())? {
                        Some(t) => (tid, t.label),
                        None => {
                            self.set_status(format!("hook: task {tid} is not on this day"));
                            return Ok(());
                        }
                    }
                } else if let Some(t) = self.selected_task() {
                    (t.id, t.label.clone())
                } else {
                    self.set_status("hook: no task selected");
                    return Ok(());
                };
                let day = tasks::ensure_day_row(
                    &self.conn,
                    &self.user_id,
                    task_id,
                    &self.date_str(),
                )?;
                let warnings = self.start_task(day.day_id)?;
                self.reload()?;
                self.set_status(format!("hook: timer started ({label})"));
                self.show_jira(warnings);
            }
            HookEvent::WaitingUser => {
                let (had, warnings) = self.stop_all_running()?;
                if had {
                    self.reload()?;
                    self.set_status("hook: timer paused");
                    self.show_jira(warnings);
                }
            }
            HookEvent::SessionEnd => {
                let (had, warnings) = self.stop_all_running()?;
                if had {
                    self.reload()?;
                    self.set_status("hook: timers stopped");
                    self.show_jira(warnings);
                }
            }
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let result = (|| {
            let mut last_reload = Instant::now();
            loop {
                self.tick_status();
                self.poll_detail_updates();
                terminal.draw(|f| crate::ui::draw(f, self))?;
                let timeout = StdDuration::from_millis(250);
                if event::poll(timeout)? {
                    if let Event::Key(key) = event::read()? {
                        match self.handle_key(key) {
                            Ok(true) => break,
                            Ok(false) => {}
                            Err(e) => self.err_status(e),
                        }
                    }
                }
                // Poll hook socket for agent events.
                if let Some(ev) = self.hook_listener.poll() {
                    if let Err(e) = self.handle_hook_event(ev) {
                        self.err_status(e);
                    }
                }
                if last_reload.elapsed() >= StdDuration::from_secs(1) {
                    if self.mode == AppMode::Daily {
                        if let Err(e) = self.reload() {
                            self.err_status(e);
                        }
                    } else if let Err(e) = self.reload_archive() {
                        self.err_status(e);
                    }
                    last_reload = Instant::now();
                }
            }
            Ok(())
        })();
        ratatui::restore();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn form(status: &str) -> Overlay {
        Overlay::Form {
            edit_day_id: Some(7),
            field: Field::Status,
            label: "US-1".into(),
            description: "desc".into(),
            elapsed: "00:10:00".into(),
            status: status.into(),
            code: String::new(),
            notes: String::new(),
            tags: String::new(),
            link: String::new(),
            is_pinned: false,
            is_important: false,
            is_archived: false,
            is_cancelled: false,
            is_deleted: false,
            is_completed: false,
            status_pick: None,
        }
    }

    /// The dropdown edits the form's own field, so this reads the status the
    /// next save would write.
    fn form_status(app: &App) -> &str {
        match &app.overlay {
            Overlay::Form { status, .. } => status,
            _ => panic!("expected the form back after the picker closed"),
        }
    }

    fn pick_cursor(app: &App) -> Option<usize> {
        match &app.overlay {
            Overlay::Form { status_pick, .. } => *status_pick,
            _ => panic!("expected the form"),
        }
    }

    fn app_with_form(status: &str) -> App {
        let mut app = test_app();
        app.overlay = form(status);
        app
    }

    fn test_app() -> App {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, label TEXT NOT NULL, code TEXT, description TEXT, link TEXT, notes TEXT, tags TEXT, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE UNIQUE INDEX idx_tasks_user_label ON tasks (user_id, label);
             CREATE TABLE task_days (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, work_date TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'todo', elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0, done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0, is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0, is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0, is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0, FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE);
             CREATE UNIQUE INDEX idx_task_days_task_date ON task_days (task_id, work_date);
             CREATE TABLE user_settings (user_id TEXT PRIMARY KEY, timer_mode TEXT NOT NULL DEFAULT 'focus', updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE task_comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, subject TEXT, summary TEXT, branch TEXT, pr TEXT, created_at INTEGER NOT NULL);
             CREATE TABLE task_integrations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, \"group\" TEXT NOT NULL, field TEXT NOT NULL, value TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             INSERT INTO users VALUES ('u1','a@b.c','x',0);",
        )
        .unwrap();
        App::new(AppConfig {
            conn,
            user_id: "u1".into(),
            date: NaiveDate::from_ymd_opt(2026, 9, 17).unwrap(),
            timer_mode: "focus".into(),
            jira_board: None,
            jira_sprint_id: None,
            git_repo_path: None,
            bitbucket_workspace: None,
            bitbucket_repo: None,
            jira_monitor_projects: vec!["US".into()],
            jira_pipelines: None,
        })
        .unwrap()
    }

    fn kanban_fixture(with_day: bool) -> App {
        let mut app = test_app();
        let id = tasks::create_task_identity(
            &app.conn, "u1", "US-1234", Some("Ticket summary"),
        ).unwrap();
        if with_day {
            tasks::ensure_day_row(&app.conn, "u1", id, &app.date_str()).unwrap();
        }
        db::integrations::upsert(&app.conn, id, "jira", "issue_key", Some("US-1234")).unwrap();
        db::integrations::upsert(&app.conn, id, "jira", "status", Some("To Do")).unwrap();
        app.handle_key(key(KeyCode::Char('B'))).unwrap();
        app
    }

    fn screen(app: &App) -> String {
        let backend = ratatui::backend::TestBackend::new(180, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
        terminal.backend().buffer().content.chunks(180)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn kanban_distinct_title_uses_a_separate_line() {
        let mut app = kanban_fixture(true);
        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        assert!(screen(&app).contains("task detail"));
        app.handle_key(key(KeyCode::Esc)).unwrap();
        // A distinct task title belongs below the key, not concatenated to it.
        app.kanban.cards[0].label = "Distinct title".into();
        let rendered = screen(&app);
        assert_eq!(rendered.matches("US-1234").count(), 1);
        assert!(rendered.contains("Distinct title"));
        let key_row = rendered.lines().find(|line| line.contains("US-1234")).unwrap();
        assert!(!key_row.contains("Distinct title"));
    }

    #[test]
    fn kanban_title_does_not_repeat_the_issue_key() {
        let app = kanban_fixture(false);
        let rendered = screen(&app);
        assert_eq!(
            rendered.matches("US-1234").count(), 1,
            "a bare issue-key label must not be rendered as US-1234US-1234"
        );
    }

    #[test]
    fn kanban_detail_opens_before_adding_a_daily_row() {
        let mut app = kanban_fixture(false);
        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        let rendered = screen(&app);
        assert!(rendered.contains("task detail"), "i must display the ticket detail");
        assert!(rendered.contains("Ticket summary"), "detail must include identity content");
        app.handle_key(key(KeyCode::Esc)).unwrap();
        app.handle_key(key(KeyCode::Right)).unwrap();
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.kanban.selected_col, 1, "closing detail restores navigation");
        let days: i64 = app.conn.query_row("SELECT COUNT(*) FROM task_days", [], |r| r.get(0)).unwrap();
        assert_eq!(days, 0, "reading detail must not add the ticket to Daily");
    }

    #[test]
    fn kanban_detail_stays_responsive_when_bitbucket_stalls() {
        const CHILD: &str = "TASK_TIMER_DETAIL_REPRO_CHILD";
        if std::env::var_os(CHILD).is_some() {
            for mode in [AppMode::Kanban, AppMode::Daily, AppMode::Archive] {
                let mut app = kanban_fixture(true);
                let id = app.kanban.cards[0].task_id;
                db::integrations::upsert(&app.conn, id, "git", "branch", Some("feature/ticket")).unwrap();
                app.bitbucket_workspace = Some("fixture".into());
                app.bitbucket_repo = Some("fixture".into());
                app.mode = mode;
                app.reload().unwrap();
                app.reload_archive().unwrap();
                app.handle_key(key(KeyCode::Char('i'))).unwrap();
                assert!(screen(&app).contains("task detail"), "{mode:?} detail must appear");
                app.handle_key(key(KeyCode::Esc)).unwrap();
                assert!(matches!(app.overlay, Overlay::None), "{mode:?} must accept Escape");
            }
            return;
        }
        // Child-only proxy/credentials isolate this test from other tests and
        // guarantee that no request can reach Bitbucket or use real credentials.
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = proxy.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((_connection, _)) = proxy.accept() {
                std::thread::sleep(StdDuration::from_secs(2));
            }
        });
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "app::tests::kanban_detail_stays_responsive_when_bitbucket_stalls", "--nocapture"])
            .env(CHILD, "1")
            .env("HTTPS_PROXY", format!("http://{address}"))
            .env("ALL_PROXY", format!("http://{address}"))
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .env("BITBUCKET_EMAIL", "fixture@example.invalid")
            .env("BITBUCKET_TOKEN", "fixture-not-a-secret")
            .spawn().unwrap();
        let deadline = Instant::now() + StdDuration::from_secs(1);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "detail child failed");
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("opening detail freezes input while Bitbucket is stalled");
            }
            std::thread::sleep(StdDuration::from_millis(10));
        }
    }

    #[test]
    fn detail_enrichment_cannot_overwrite_a_reopened_ticket() {
        let mut app = kanban_fixture(true);
        let second = tasks::create_task(&app.conn, "u1", &app.date_str(), "US-9999", Some("Second ticket")).unwrap();
        db::integrations::upsert(&app.conn, second.id, "jira", "issue_key", Some("US-9999")).unwrap();
        app.enter_kanban().unwrap();
        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        let (old_sender, receiver) = std::sync::mpsc::channel();
        app.detail_updates = Some(receiver);
        old_sender.send(DetailUpdate {
            prs: Vec::new(), statuses: Vec::new(), warnings: vec!["stale failure".into()],
        }).unwrap();
        app.handle_key(key(KeyCode::Esc)).unwrap();
        let columns = app.kanban_columns();
        let other = columns.iter().position(|c| c == "Other").unwrap();
        for _ in 0..other {
            app.handle_key(key(KeyCode::Right)).unwrap();
        }
        app.handle_key(key(KeyCode::Char('i'))).unwrap();
        app.poll_detail_updates();
        assert!(screen(&app).contains("Second ticket"));
        assert!(!screen(&app).contains("stale failure"));

        let (sender, receiver) = std::sync::mpsc::channel();
        app.detail_updates = Some(receiver);
        sender.send(DetailUpdate {
            prs: vec![crate::bitbucket::PullRequest {
                id: 7, title: "Current ticket PR".into(), state: "OPEN".into(),
                author: None, url: None,
            }],
            statuses: Vec::new(),
            warnings: vec!["fixture status unavailable".into()],
        }).unwrap();
        app.poll_detail_updates();
        let rendered = screen(&app);
        assert!(rendered.contains("Second ticket"));
        assert!(rendered.contains("Current ticket PR"));
        assert!(rendered.contains("fixture status unavailable"));
        assert!(!rendered.contains("stale failure"));
    }

    #[test]
    fn picking_a_status_writes_its_catalog_id_into_the_form() {
        let mut app = app_with_form("");
        app.toggle_status_pick();
        // No stored status → the cursor sits on the "No status" row, past the
        // catalog, and `k` walks up into it (Done is the third row from the top).
        assert_eq!(pick_cursor(&app), Some(jira::JIRA_STATUSES.len()));

        for _ in 0..(jira::JIRA_STATUSES.len() - 2) {
            app.handle_form_key(key(KeyCode::Char('k'))).unwrap();
        }
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "31", "the picked status must land in the form");
        assert_eq!(pick_cursor(&app), None, "Enter closes the dropdown");
    }

    #[test]
    fn a_picker_cursor_opens_on_the_status_the_form_already_holds() {
        let mut app = app_with_form("51");
        app.toggle_status_pick();
        // Catalog order: 11, 21, 31, 41, 51 → index 4.
        assert_eq!(pick_cursor(&app), Some(4));
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "51");
    }

    /// The field holds an id, but a task stored before the catalog existed can
    /// hold a label — the dropdown must still find its row.
    #[test]
    fn a_picker_cursor_resolves_a_legacy_label() {
        let mut app = app_with_form("Cek di Local");
        app.toggle_status_pick();
        assert_eq!(pick_cursor(&app), Some(4));
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "51", "choosing must normalise to the id");
    }

    #[test]
    fn escaping_the_picker_leaves_the_form_status_untouched() {
        let mut app = app_with_form("41");
        app.toggle_status_pick();
        app.handle_form_key(key(KeyCode::Char('j'))).unwrap();
        app.handle_form_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(form_status(&app), "41", "Esc must not apply a selection");
        assert_eq!(pick_cursor(&app), None);
    }

    #[test]
    fn the_no_status_row_clears_the_field() {
        let mut app = app_with_form("81");
        app.toggle_status_pick();
        // Past the last catalog row is "No status".
        for _ in 0..jira::JIRA_STATUSES.len() {
            app.handle_form_key(key(KeyCode::Char('j'))).unwrap();
        }
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "");
    }

    #[test]
    fn movement_is_clamped_to_the_no_status_row() {
        let mut app = app_with_form("");
        app.toggle_status_pick();
        for _ in 0..(jira::JIRA_STATUSES.len() + 5) {
            app.handle_form_key(key(KeyCode::Char('j'))).unwrap();
        }
        assert_eq!(pick_cursor(&app), Some(jira::JIRA_STATUSES.len()));
        app.handle_form_key(key(KeyCode::Char('k'))).unwrap();
        app.handle_form_key(key(KeyCode::Char('k'))).unwrap();
        assert_eq!(pick_cursor(&app), Some(jira::JIRA_STATUSES.len() - 2));
    }

    /// The form's status field is not a text input: `s` must open the catalog
    /// rather than append a character, or the dropdown would be unreachable.
    #[test]
    fn the_status_field_opens_the_picker_instead_of_typing() {
        let mut app = app_with_form("21");
        app.handle_form_key(key(KeyCode::Char('s'))).unwrap();
        assert!(pick_cursor(&app).is_some());
    }

    /// The form's own hint says Enter picks the status, so Enter on that field
    /// must open the dropdown rather than save the form.
    #[test]
    fn enter_on_the_status_field_opens_the_picker_without_saving() {
        let mut app = app_with_form("21");
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert!(pick_cursor(&app).is_some(), "Enter must open the catalog");
        assert!(
            app.tasks.is_empty(),
            "Enter on the status field must not save the form"
        );
    }

    /// Committing a choice must move focus off the status field. Staying there
    /// would make the next Enter reopen the list rather than save the form,
    /// while the footer still promises Enter saves.
    #[test]
    fn committing_a_status_moves_focus_so_the_next_enter_saves() {
        let mut app = app_with_form("21");
        // The create path, so the save that follows is observable as a row.
        if let Overlay::Form { edit_day_id, label, .. } = &mut app.overlay {
            *edit_day_id = None;
            *label = "US-88".into();
        }
        app.toggle_status_pick();
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(pick_cursor(&app), None);
        assert!(
            matches!(&app.overlay, Overlay::Form { field: Field::Code, .. }),
            "focus must leave the status field once a choice is applied"
        );
        // The next Enter is now a save, not a reopen of the list.
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(app.tasks.len(), 1, "Enter must have saved the form");
        assert_eq!(app.tasks[0].status, "21");
    }

    /// Every other field still types, so the dropdown did not swallow the form.
    #[test]
    fn other_fields_still_accept_text() {
        let mut app = test_app();
        app.overlay = Overlay::Form {
            edit_day_id: Some(7),
            field: Field::Label,
            label: String::new(),
            description: String::new(),
            elapsed: "00:00:00".into(),
            status: "21".into(),
            code: String::new(),
            notes: String::new(),
            tags: String::new(),
            link: String::new(),
            is_pinned: false,
            is_important: false,
            is_archived: false,
            is_cancelled: false,
            is_deleted: false,
            is_completed: false,
            status_pick: None,
        };
        app.handle_form_key(key(KeyCode::Char('x'))).unwrap();
        let Overlay::Form { label, status, .. } = &app.overlay else {
            panic!("expected the form");
        };
        assert_eq!(label, "x");
        assert_eq!(status, "21");
    }

    /// Editing a task must open the dropdown on the task's own status, which
    /// means the form has to carry the catalog id and not the display label.
    #[test]
    fn editing_a_task_opens_the_dropdown_on_its_own_status() {
        let mut app = test_app();
        let t = tasks::create_task(&app.conn, "u1", "2026-09-17", "US-42", None).unwrap();
        app.conn
            .execute("UPDATE task_days SET status = '51' WHERE id = ?1", [t.day_id])
            .unwrap();
        app.reload().unwrap();
        app.selected = 0;
        app.open_edit();
        app.toggle_status_pick();
        assert_eq!(pick_cursor(&app), Some(4), "Cek di Local is the fifth row");
        app.handle_form_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "51");
    }

    /// Editing must address the row the user selected. `tasks.id` and
    /// `task_days.id` are separate sequences, so passing the task id as a day id
    /// edited whichever task owned that day number — and a rename then collided
    /// with the selected task's own label.
    #[test]
    fn editing_a_task_updates_the_selected_task_not_the_row_with_its_id() {
        let mut app = test_app();
        // Two days for the first task, so the second task's id (2) lands on the
        // first task's *second* day row — the sequences diverge exactly here.
        tasks::create_task(&app.conn, "u1", "2026-09-17", "First", None).unwrap();
        tasks::create_task(&app.conn, "u1", "2026-09-16", "First", None).unwrap();
        let second = tasks::create_task(&app.conn, "u1", "2026-09-17", "Second", None).unwrap();
        assert_ne!(second.id, second.day_id, "the two ids must differ for this test");

        app.reload().unwrap();
        app.selected = app
            .tasks
            .iter()
            .position(|t| t.id == second.id)
            .expect("Second is on the viewed day");
        app.open_edit();
        if let Overlay::Form { label, .. } = &mut app.overlay {
            *label = "Second renamed".into();
        }
        app.submit_form().unwrap();

        app.reload().unwrap();
        let on_viewed_day = app
            .tasks
            .iter()
            .find(|t| t.id == second.id)
            .expect("the edited task is still on the day");
        assert_eq!(on_viewed_day.label, "Second renamed");
        // The other task's day rows are untouched.
        for date in ["2026-09-16", "2026-09-17"] {
            let first = tasks::list_tasks(&app.conn, "u1", date)
                .unwrap()
                .into_iter()
                .find(|t| t.id != second.id)
                .expect("First is still there");
            assert_eq!(first.label, "First", "First must keep its own label");
        }
    }

    /// Every flag key flips exactly its own bool — this is the whole flags
    /// field, and a wrong mapping would silently write the wrong column.
    #[test]
    fn the_flags_field_toggles_one_flag_per_key() {
        let mut app = test_app();
        app.overlay = form("21");
        let Overlay::Form { field, .. } = &mut app.overlay else {
            unreachable!()
        };
        *field = Field::Flags;

        for c in ['p', 'i', 'a', 'c', 'x', 'd'] {
            app.handle_form_key(key(KeyCode::Char(c))).unwrap();
        }
        let Overlay::Form {
            is_pinned,
            is_important,
            is_archived,
            is_cancelled,
            is_deleted,
            is_completed,
            label,
            ..
        } = &app.overlay
        else {
            panic!("expected the form");
        };
        assert!(*is_pinned && *is_important && *is_archived);
        assert!(*is_cancelled && *is_deleted && *is_completed);
        assert_eq!(label, "US-1", "a flag key must not type into another field");

        // Pressing again turns it back off.
        app.handle_form_key(key(KeyCode::Char('p'))).unwrap();
        let Overlay::Form { is_pinned, .. } = &app.overlay else {
            panic!("expected the form");
        };
        assert!(!is_pinned);
    }

    /// The create path used to drop everything the form collected beyond label
    /// and description.
    #[test]
    fn saving_a_new_task_persists_link_and_flags() {
        let mut app = test_app();
        app.overlay = Overlay::Form {
            edit_day_id: None,
            field: Field::Label,
            label: "US-77 new thing".into(),
            description: String::new(),
            elapsed: "00:00:00".into(),
            status: "21".into(),
            code: "ABC".into(),
            notes: "note".into(),
            tags: "backend".into(),
            link: "https://example.com/77".into(),
            is_pinned: true,
            is_important: false,
            is_archived: false,
            is_cancelled: false,
            is_deleted: false,
            is_completed: false,
            status_pick: None,
        };
        app.submit_form().unwrap();

        let stored = &app.tasks[0];
        assert_eq!(stored.link.as_deref(), Some("https://example.com/77"));
        assert!(stored.is_pinned);
        assert_eq!(stored.code.as_deref(), Some("ABC"));
        assert_eq!(stored.status, "21");
        assert_eq!(stored.tags.as_deref(), Some("backend"));
        assert!(!stored.is_archived);
    }

    /// Re-adding today's label used to run the blank create form's defaults
    /// over the existing day row, zeroing its accumulated time and flags.
    #[test]
    fn readding_a_task_today_keeps_its_time_and_flags() {
        let mut app = test_app();
        let t = tasks::create_task(&app.conn, "u1", "2026-09-17", "US-9", None).unwrap();
        app.conn
            .execute(
                "UPDATE task_days SET elapsed_time = 1800, is_pinned = 1 WHERE id = ?1",
                [t.day_id],
            )
            .unwrap();
        app.reload().unwrap();
        app.overlay = Overlay::Form {
            edit_day_id: None,
            field: Field::Label,
            label: "US-9".into(),
            description: String::new(),
            elapsed: "00:00:00".into(),
            status: String::new(),
            code: String::new(),
            notes: String::new(),
            tags: String::new(),
            link: String::new(),
            is_pinned: false,
            is_important: false,
            is_archived: false,
            is_cancelled: false,
            is_deleted: false,
            is_completed: false,
            status_pick: None,
        };
        app.submit_form().unwrap();

        assert_eq!(app.tasks.len(), 1, "re-adding must not duplicate the task");
        let stored = &app.tasks[0];
        assert_eq!(stored.elapsed_time, 1800, "the accumulated time must survive");
        assert!(stored.is_pinned, "the blank form's flags must not clear the row");
    }

    /// `C` opens the list and `n` writes a row through to `task_comments`.
    #[test]
    fn composing_a_comment_writes_it_to_the_task() {
        let mut app = test_app();
        let task = tasks::create_task(&app.conn, "u1", "2026-09-17", "US-9", None).unwrap();
        app.reload().unwrap();
        app.selected = 0;

        app.open_comments();
        app.handle_comments_key(key(KeyCode::Char('n'))).unwrap();
        for c in "blocked on review".chars() {
            app.handle_comments_key(key(KeyCode::Char(c))).unwrap();
        }
        app.handle_comments_key(key(KeyCode::Enter)).unwrap();

        let rows = crate::db::comments::list(&app.conn, task.id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].summary.as_deref(), Some("blocked on review"));
        assert_eq!(rows[0].subject.as_deref(), Some("comment"));
        // The list is showing again with the new row.
        let Overlay::Comments { compose, .. } = &app.overlay else {
            panic!("expected the comment list");
        };
        assert!(compose.is_none());
    }

    /// Esc backs out of compose without writing, then out of the list.
    #[test]
    fn escaping_compose_writes_nothing() {
        let mut app = test_app();
        let task = tasks::create_task(&app.conn, "u1", "2026-09-17", "US-9", None).unwrap();
        app.reload().unwrap();
        app.open_comments();
        app.handle_comments_key(key(KeyCode::Char('n'))).unwrap();
        app.handle_comments_key(key(KeyCode::Char('x'))).unwrap();
        app.handle_comments_key(key(KeyCode::Esc)).unwrap();
        assert!(crate::db::comments::list(&app.conn, task.id).unwrap().is_empty());
        assert!(matches!(app.overlay, Overlay::Comments { .. }));
        app.handle_comments_key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(app.overlay, Overlay::None));
    }

    /// The archive status filter takes what the operator types and stores the
    /// catalog id, so a typed name filters the same rows as the id.
    #[test]
    fn the_archive_status_filter_resolves_a_typed_name() {
        let mut app = test_app();
        let a = tasks::create_task(&app.conn, "u1", "2026-09-16", "US-1", None).unwrap();
        tasks::create_task(&app.conn, "u1", "2026-09-16", "US-2", None).unwrap();
        app.conn
            .execute("UPDATE task_days SET status = '21' WHERE id = ?1", [a.day_id])
            .unwrap();
        app.mode = AppMode::Archive;
        app.archive.filter_status = Some("In Progress".into());
        app.reload_archive().unwrap();

        assert_eq!(app.archive.tasks.len(), 1);
        assert_eq!(app.archive.tasks[0].id, a.id);
    }

    /// A sprint fetch must keep the issue's status, not just its key — that row
    /// is what the detail view and the archive's integration filter read.
    #[test]
    fn ingesting_a_jira_issue_stores_its_key_and_status() {
        let mut app = test_app();
        let issues = vec![(
            "US-1459".to_string(),
            "Fix login".to_string(),
            "In Progress".to_string(),
        )];
        app.ingest_jira_issues(&issues).unwrap();

        let task_id: i64 = app
            .conn
            .query_row("SELECT id FROM tasks WHERE label LIKE 'US-1459%'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let rows = db::integrations::list(&app.conn, task_id).unwrap();
        let value = |field: &str| {
            rows.iter()
                .find(|i| i.group == "jira" && i.field == field)
                .and_then(|i| i.value.clone())
        };
        assert_eq!(value("issue_key").as_deref(), Some("US-1459"));
        assert_eq!(
            value("status").as_deref(),
            Some("In Progress"),
            "the fetched status must be persisted rather than discarded"
        );
    }

    /// The integration filter is passed straight through to `list_archive`, and
    /// a task matched by two integration rows is still listed once.
    #[test]
    fn the_archive_integration_filter_reaches_the_query() {
        let mut app = test_app();
        let a = tasks::create_task(&app.conn, "u1", "2026-09-16", "US-1", None).unwrap();
        tasks::create_task(&app.conn, "u1", "2026-09-16", "US-2", None).unwrap();
        db::integrations::upsert(&app.conn, a.id, "jira", "issue_key", Some("US-1459")).unwrap();
        db::integrations::upsert(&app.conn, a.id, "jira", "sprint", Some("99")).unwrap();
        app.mode = AppMode::Archive;
        app.archive.filter_integration = Some("jira".into());
        app.reload_archive().unwrap();

        assert_eq!(app.archive.tasks.len(), 1, "two matching rows must not duplicate the task");
        assert_eq!(app.archive.tasks[0].id, a.id);

        // A value-only match works too.
        app.archive.filter_integration = Some("1459".into());
        app.reload_archive().unwrap();
        assert_eq!(app.archive.tasks.len(), 1);
    }}
