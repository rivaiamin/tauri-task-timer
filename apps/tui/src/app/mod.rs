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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArchiveInput {
    Label,
    Tag,
}

pub struct ArchiveState {
    pub filter_q: Option<String>,
    pub filter_tag: Option<String>,
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
        edit_id: Option<i64>,
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
    },
    Jira { mode: JiraMode },
    Git { mode: GitMode },
    /// Status picker for the form's status field: the catalog, not free text.
    /// `selected` indexes `jira::JIRA_STATUSES`; its length is "No status".
    StatusPick { selected: usize },
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
    /// The form the status picker was opened from, parked while the picker is up.
    form_under_state: Option<Overlay>,
    hook_listener: crate::hooks::HookListener,
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
            form_under_state: None,
            hook_listener: crate::hooks::HookListener::new(),
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
        if self.mode == AppMode::Archive {
            self.archive_selected_task()
        } else {
            self.selected_task()
        }
    }

    fn reload_archive(&mut self) -> Result<()> {
        let filter = tasks::ArchiveFilter {
            q: self.archive.filter_q.clone(),
            tag: self.archive.filter_tag.clone(),
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

    fn toggle_archive(&mut self) -> Result<()> {
        if self.mode == AppMode::Archive {
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
        let Some((id, running)) = self.selected_task().map(|t| (t.id, t.is_running)) else {
            return Ok(());
        };
        let warnings = if running {
            self.stop_task(id)?
        } else {
            self.start_task(id)?
        };
        self.reload()?;
        self.show_jira(warnings);
        Ok(())
    }

    /// Start a task. In focus mode, stops other running tasks (with worklog + To Do transition).
    /// Fires In Progress transition for the started task.
    fn start_task(&mut self, id: i64) -> Result<Warnings> {
        let exclusive = self.timer_mode == "focus";
        // Snapshot running tasks before exclusive start stops them
        let stopped: Vec<Worklog> = if exclusive {
            let now = now_ms();
            self.tasks
                .iter()
                .filter(|t| t.id != id)
                .filter_map(|t| Worklog::capture(t, now))
                .collect()
        } else {
            vec![]
        };
        tasks::start_timer(&self.conn, &self.user_id, id, exclusive)?;
        let mut warnings = Warnings::new();
        // Fire JIRA: focus-switched tasks → worklog + To Do
        for worklog in &stopped {
            warnings.extend(worklog.record_and_reopen());
            jira::auto_status(&self.conn, &self.user_id, worklog.task_id, false, false);
        }
        // Fire JIRA: started task → In Progress
        if let Some(t) = self.tasks.iter().find(|t| t.id == id) {
            warnings.extend(jira::fire_on_start(&t.label, t.description_text()));
        }
        jira::auto_status(&self.conn, &self.user_id, id, true, false);
        Ok(warnings)
    }

    /// Stop a task. Fires worklog + To Do; Shift-D is what moves an issue to done.
    fn stop_task(&mut self, id: i64) -> Result<Warnings> {
        let worklog = self
            .tasks
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| Worklog::capture(t, now_ms()));
        tasks::stop_timer(&self.conn, &self.user_id, id)?;
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
        let stopped: Vec<Worklog> = self
            .tasks
            .iter()
            .filter_map(|t| Worklog::capture(t, now))
            .collect();
        let had = !stopped.is_empty();
        for worklog in &stopped {
            tasks::stop_timer(&self.conn, &self.user_id, worklog.task_id)?;
        }
        let mut warnings = Warnings::new();
        for worklog in &stopped {
            warnings.extend(worklog.record_and_reopen());
        }
        Ok((had, warnings))
    }

    fn open_create(&mut self) {
        self.overlay = Overlay::Form {
            edit_id: None,
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
        };
    }

    fn open_edit(&mut self) {
        let Some(t) = self.selected_task() else {
            return;
        };
        self.overlay = Overlay::Form {
            edit_id: Some(t.id),
            field: Field::Label,
            label: t.label.clone(),
            description: t.description.clone().unwrap_or_default(),
            elapsed: format_time(t.current_elapsed(now_ms())),
            status: jira::status_label(&t.status).to_string(),
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
        };
    }

    /// Rebuild the form the picker was opened from, optionally replacing the
    /// status field. The picker is a field editor, not a separate form, so this
    /// is the only way back — and the only place the field changes.
    fn form_under(&mut self, status: Option<String>) -> Overlay {
        let Overlay::Form {
            edit_id,
            field,
            label,
            description,
            elapsed,
            status: current,
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
        } = self.form_under_state.take().unwrap_or(Overlay::None)
        else {
            return Overlay::None;
        };
        Overlay::Form {
            edit_id,
            field,
            label,
            description,
            elapsed,
            status: status.unwrap_or(current),
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
        }
    }

    /// Move the form's status field from free text to the catalog: the picker
    /// opens on whatever that field currently holds.
    fn open_status_pick(&mut self) {
        let Overlay::Form {
            status,
            edit_id,
            field,
            label,
            description,
            elapsed,
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
        } = &self.overlay
        else {
            return;
        };
        let selected = jira::JIRA_STATUSES
            .iter()
            .position(|s| s.id == status.trim())
            .unwrap_or(jira::JIRA_STATUSES.len());
        self.form_under_state = Some(Overlay::Form {
            edit_id: *edit_id,
            field: *field,
            label: label.clone(),
            description: description.clone(),
            elapsed: elapsed.clone(),
            status: status.clone(),
            code: code.clone(),
            notes: notes.clone(),
            tags: tags.clone(),
            link: link.clone(),
            is_pinned: *is_pinned,
            is_important: *is_important,
            is_archived: *is_archived,
            is_cancelled: *is_cancelled,
            is_deleted: *is_deleted,
            is_completed: *is_completed,
        });
        self.overlay = Overlay::StatusPick { selected };
    }

    fn submit_form(&mut self) -> Result<()> {
        // Copy the form out: the save path calls `&mut self` helpers, so the
        // overlay cannot stay borrowed while it runs.
        let Overlay::Form {
            edit_id,
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
            edit_id,
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
            *edit_id,
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
        if let Some(id) = edit_id {
            tasks::update_task(
                &self.conn,
                &self.user_id,
                id,
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
            let created = tasks::create_task(&self.conn, &self.user_id, &date, &label, desc)?;
            // `create_task` only knows label and description, so everything else
            // the form collected — including the link and the flags — lands here.
            tasks::update_task(
                &self.conn,
                &self.user_id,
                created.id,
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
        let a = self.tasks[self.selected].id;
        let b = self.tasks[i as usize].id;
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
                                            task.id,
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
    fn ingest_jira_issues(&mut self, issues: &[(String, String, String)]) -> Result<String> {
        let today = self.date_str();
        let mut created = 0u32;
        let mut updated = 0u32;
        let user_id = self.user_id.clone();
        for (key, summary, _status) in issues {
            let label = format!("{key} {summary}");
            let task = tasks::create_task(&self.conn, &user_id, &today, label.trim(), Some(summary))?;
            db::integrations::upsert(&self.conn, task.id, "jira", "issue_key", Some(key))?;
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

    /// Build the detail overlay: stored comments plus whatever Bitbucket and git
    /// can say about the selected task's branch. Network failures degrade to an
    /// empty section and a toast — the detail view still opens.
    fn open_detail(&mut self) {
        let Some(task) = self.focused_task().map(|t| (t.id, t.label.clone())) else {
            return;
        };
        let (task_id, _) = task;
        let comments = db::comments::list(&self.conn, task_id).unwrap_or_default();
        let branch = self.git_task_branch().unwrap_or_default();
        let mut prs = Vec::new();
        let mut statuses = Vec::new();
        if !branch.is_empty() {
            if let (Some(ws), Some(repo_name)) =
                (self.bitbucket_workspace.clone(), self.bitbucket_repo.clone())
            {
                match crate::bitbucket::list_prs(&ws, &repo_name, &branch) {
                    Ok(list) => prs = list,
                    Err(e) => self.set_status(format!("bitbucket: {e}")),
                }
                if let Some(repo) = self.git_repo_path.clone() {
                    if let Ok(commits) = git::commits_for_branch(&repo, &branch, 1) {
                        if let Some(head) = commits.first() {
                            match crate::bitbucket::commit_statuses(&ws, &repo_name, &head.hash) {
                                Ok(s) => statuses = s,
                                Err(e) => self.set_status(format!("bitbucket: {e}")),
                            }
                        }
                    }
                }
            }
        }
        self.overlay = Overlay::Detail {
            comments,
            prs,
            statuses,
        };
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

    /// Keys for the status picker. Never quits, so it reports nothing — it only
    /// ever closes back into the form it came from.
    fn handle_status_pick_key(&mut self, key: KeyEvent) -> Result<()> {
        // Copy the index out: `Overlay` is not `Copy`, so the shorthand bind
        // would move the field it needs to write back.
        let Overlay::StatusPick { selected } = &self.overlay else {
            return Ok(());
        };
        let selected = *selected;
        // The row past the last status is "No status".
        let last = jira::JIRA_STATUSES.len();
        match key.code {
            // Esc keeps the field as it was; only Enter applies a choice.
            KeyCode::Esc => {
                let restored = self.form_under(None);
                self.overlay = restored;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.overlay = Overlay::StatusPick {
                    selected: (selected + 1).min(last),
                };
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.overlay = Overlay::StatusPick {
                    selected: selected.saturating_sub(1),
                };
            }
            KeyCode::Char(digit @ '0'..='8') => {
                self.overlay = Overlay::StatusPick {
                    selected: (digit as usize - '0' as usize).min(last),
                };
            }
            KeyCode::Enter => {
                let status = jira::JIRA_STATUSES
                    .get(selected)
                    .map(|s| s.id.to_string())
                    .unwrap_or_default();
                let chosen = self.form_under(Some(status));
                self.overlay = chosen;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                return Ok(false);
            }
            KeyCode::Enter => {
                self.submit_form()?;
                return Ok(false);
            }
            _ => {}
        }
        // The status field is a picker, not a text input: any printable key on it
        // opens the catalog. Checked before the form borrow so opening can mutate.
        let status_focused = matches!(&self.overlay, Overlay::Form { field: Field::Status, .. });
        if status_focused
            && matches!(key.code, KeyCode::Char(_))
            && (key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT)
        {
            self.open_status_pick();
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
            edit_id: _,
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
            Overlay::StatusPick { .. } => {
                self.handle_status_pick_key(key)?;
                return Ok(false);
            }
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
                        tasks::reset_all(&self.conn, &self.user_id, &date)?;
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
                    }
                    // The detail footer advertises it, so `C` works from here too.
                    KeyCode::Char('C') => self.open_comments(),
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
                KeyCode::Char('c') | KeyCode::Enter => self.continue_today()?,
                KeyCode::Char('i') if self.archive_selected_task().is_some() => {
                    self.open_detail();
                }
                KeyCode::Char('C') if self.archive_selected_task().is_some() => {
                    self.open_comments();
                }
                _ => {}
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('a') => { self.toggle_archive()?; }
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
                    let result = tasks::set_done(&self.conn, &self.user_id, t.id, new_done)?;
                    let mut warnings = Warnings::new();
                    if let Some((_, delta, start_ms)) = result {
                        if new_done {
                            warnings =
                                jira::fire_on_done(&t.label, t.description_text(), delta, start_ms);
                        }
                    }
                    jira::auto_status(&self.conn, &self.user_id, t.id, false, new_done);
                    self.reload()?;
                    self.set_status(if new_done { "marked done" } else { "unmarked done" });
                    self.show_jira(warnings);
                }
            }
            KeyCode::Char('r') => {
                if let Some(id) = self.selected_task().map(|t| t.id) {
                    tasks::reset_task(&self.conn, &self.user_id, id)?;
                    self.reload()?;
                }
            }
            KeyCode::Char('R') => self.overlay = Overlay::ConfirmResetAll,
            KeyCode::Char('m') => self.toggle_mode()?,
            KeyCode::Char('x') => self.export_markdown(),
            _ => {}
        }
        Ok(false)
    }

    fn handle_hook_event(&mut self, ev: crate::hooks::HookEvent) -> Result<()> {
        use crate::hooks::HookEvent;
        match ev {
            HookEvent::SessionStart { task_id } => {
                let id = if let Some(tid) = task_id {
                    tid
                } else if let Some(t) = self.selected_task() {
                    t.id
                } else {
                    self.set_status("hook: no task selected");
                    return Ok(());
                };
                let warnings = self.start_task(id)?;
                self.reload()?;
                self.set_status(format!("hook: timer started (task {id})"));
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
            edit_id: Some(7),
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
        }
    }

    /// The picker is a field editor: whatever it applies must survive into the
    /// form, which is what `submit_form` later writes to the database.
    fn form_status(app: &App) -> &str {
        match &app.overlay {
            Overlay::Form { status, .. } => status,
            _ => panic!("expected the form back after the picker closed"),
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
             CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, user_id TEXT NOT NULL, label TEXT NOT NULL, description TEXT, code TEXT, link TEXT, status TEXT NOT NULL DEFAULT 'todo', notes TEXT, tags TEXT, elapsed_time INTEGER NOT NULL DEFAULT 0, total_time INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0, is_running INTEGER NOT NULL DEFAULT 0, done INTEGER NOT NULL DEFAULT 0, is_completed INTEGER NOT NULL DEFAULT 0, is_cancelled INTEGER NOT NULL DEFAULT 0, is_deleted INTEGER NOT NULL DEFAULT 0, is_archived INTEGER NOT NULL DEFAULT 0, is_pinned INTEGER NOT NULL DEFAULT 0, is_important INTEGER NOT NULL DEFAULT 0, start_time INTEGER, end_time INTEGER, work_date TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
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
        })
        .unwrap()
    }

    #[test]
    fn picking_a_status_writes_its_catalog_id_into_the_form() {
        let mut app = app_with_form("");
        app.open_status_pick();
        // No stored status → the cursor sits on the "No status" row, past the
        // catalog, and `k` walks up into it (Done is the third row from the top).
        let Overlay::StatusPick { selected } = app.overlay else {
            panic!("expected the picker");
        };
        assert_eq!(selected, jira::JIRA_STATUSES.len());

        for _ in 0..(jira::JIRA_STATUSES.len() - 2) {
            app.handle_status_pick_key(key(KeyCode::Char('k'))).unwrap();
        }
        app.handle_status_pick_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "31", "the picked status must land in the form");
    }

    #[test]
    fn a_picker_cursor_opens_on_the_status_the_form_already_holds() {
        let mut app = app_with_form("51");
        app.open_status_pick();
        let Overlay::StatusPick { selected } = app.overlay else {
            panic!("expected the picker");
        };
        // Catalog order: 11, 21, 31, 41, 51 → index 4.
        assert_eq!(selected, 4);
        app.handle_status_pick_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "51");
    }

    #[test]
    fn escaping_the_picker_leaves_the_form_status_untouched() {
        let mut app = app_with_form("41");
        app.open_status_pick();
        app.handle_status_pick_key(key(KeyCode::Char('j'))).unwrap();
        app.handle_status_pick_key(key(KeyCode::Esc)).unwrap();
        assert_eq!(form_status(&app), "41", "Esc must not apply a selection");
    }

    #[test]
    fn the_no_status_row_clears_the_field() {
        let mut app = app_with_form("81");
        app.open_status_pick();
        // Past the last catalog row is "No status".
        for _ in 0..jira::JIRA_STATUSES.len() {
            app.handle_status_pick_key(key(KeyCode::Char('j'))).unwrap();
        }
        app.handle_status_pick_key(key(KeyCode::Enter)).unwrap();
        assert_eq!(form_status(&app), "");
    }

    #[test]
    fn movement_is_clamped_to_the_no_status_row() {
        let mut app = app_with_form("");
        app.open_status_pick();
        for _ in 0..(jira::JIRA_STATUSES.len() + 5) {
            app.handle_status_pick_key(key(KeyCode::Char('j'))).unwrap();
        }
        let Overlay::StatusPick { selected } = app.overlay else {
            panic!("expected the picker");
        };
        assert_eq!(selected, jira::JIRA_STATUSES.len());
        app.handle_status_pick_key(key(KeyCode::Char('k'))).unwrap();
        app.handle_status_pick_key(key(KeyCode::Char('k'))).unwrap();
        let Overlay::StatusPick { selected } = app.overlay else {
            panic!("expected the picker");
        };
        assert_eq!(selected, jira::JIRA_STATUSES.len() - 2);
    }

    /// The form's status field is not a text input: `s` must open the catalog
    /// rather than append a character, or the picker would be unreachable.
    #[test]
    fn the_status_field_opens_the_picker_instead_of_typing() {
        let mut app = app_with_form("21");
        app.handle_form_key(key(KeyCode::Char('s'))).unwrap();
        assert!(matches!(app.overlay, Overlay::StatusPick { .. }));
    }

    /// Every other field still types, so the picker did not swallow the form.
    #[test]
    fn other_fields_still_accept_text() {
        let mut app = test_app();
        app.overlay = Overlay::Form {
            edit_id: Some(7),
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
        };
        app.handle_form_key(key(KeyCode::Char('x'))).unwrap();
        let Overlay::Form { label, status, .. } = &app.overlay else {
            panic!("expected the form");
        };
        assert_eq!(label, "x");
        assert_eq!(status, "21");
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
            edit_id: None,
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
}
