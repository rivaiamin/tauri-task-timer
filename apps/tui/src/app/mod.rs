mod keymap;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::time::{Duration as StdDuration, Instant};

use crate::clipboard;
use crate::db;
use crate::db::tasks::{self, Task};
use crate::jira;
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

pub enum Overlay {
    None,
    Help,
    ConfirmDelete,
    ConfirmResetAll,
    Detail,
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
    },
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
}

impl App {
    pub fn new(
        conn: Connection,
        user_id: String,
        date: NaiveDate,
        timer_mode: String,
    ) -> Result<Self> {
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
        if running {
            tasks::stop_timer(&self.conn, &self.user_id, id)?;
        } else {
            let exclusive = self.timer_mode == "focus";
            tasks::start_timer(&self.conn, &self.user_id, id, exclusive)?;
        }
        self.reload()
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
            status: t.status.clone(),
            code: t.code.clone().unwrap_or_default(),
            notes: t.notes.clone().unwrap_or_default(),
            tags: t.tags.clone().unwrap_or_default(),
        };
    }

    fn submit_form(&mut self) -> Result<()> {
        let (edit_id, label, description, elapsed, status, code, notes, tags) = match &self.overlay {
            Overlay::Form {
                edit_id,
                label,
                description,
                elapsed,
                status,
                code,
                notes,
                tags,
                ..
            } => (
                *edit_id,
                label.clone(),
                description.clone(),
                elapsed.clone(),
                status.clone(),
                code.clone(),
                notes.clone(),
                tags.clone(),
            ),
            _ => return Ok(()),
        };
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
        let status_opt = if status.is_empty() { None } else { Some(status.as_str()) };
        let code_opt = if code.is_empty() { None } else { Some(code.as_str()) };
        let notes_opt = if notes.is_empty() { None } else { Some(notes.as_str()) };
        let tags_opt = if tags.is_empty() { None } else { Some(tags.as_str()) };
        if let Some(id) = edit_id {
            tasks::update_task(
                &self.conn,
                &self.user_id,
                id,
                Some(&label),
                Some(&description),
                Some(elapsed_secs),
                code_opt,
                status_opt,
                notes_opt,
                tags_opt,
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
            if elapsed_secs > 0 {
                tasks::update_task(
                    &self.conn,
                    &self.user_id,
                    created.id,
                    None,
                    None,
                    Some(elapsed_secs),
                    None,
                    None,
                    None,
                    None,
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
        let Overlay::Form {
            field,
            label,
            description,
            elapsed,
            status,
            code,
            notes,
            tags,
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
                    Field::Tags => Field::Label,
                };
            }
            KeyCode::Backspace => {
                let buf = match field {
                    Field::Label => label,
                    Field::Description => description,
                    Field::Elapsed => elapsed,
                    Field::Status => status,
                    Field::Code => code,
                    Field::Notes => notes,
                    Field::Tags => tags,
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
                    Field::Status => status,
                    Field::Code => code,
                    Field::Notes => notes,
                    Field::Tags => tags,
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
            Overlay::Detail => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('i') | KeyCode::Char('q')) {
                    self.overlay = Overlay::None;
                }
                return Ok(false);
            }
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
                KeyCode::Char('i') => {
                    if self.archive_selected_task().is_some() {
                        self.overlay = Overlay::Detail;
                    }
                }
                _ => {}
            }
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('a') => { self.toggle_archive()?; }
            KeyCode::Char('?') => self.overlay = Overlay::Help,
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.tasks.is_empty() {
                    self.selected = (self.selected + 1).min(self.tasks.len() - 1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Char('J') => self.reorder(1)?,
            KeyCode::Char('K') => self.reorder(-1)?,
            KeyCode::Char('h') | KeyCode::Left => self.shift_day(-1)?,
            KeyCode::Char('l') | KeyCode::Right => self.shift_day(1)?,
            KeyCode::Char(' ') => self.start_stop()?,
            KeyCode::Char('n') => self.open_create(),
            KeyCode::Char('e') => self.open_edit(),
            KeyCode::Char('i') => {
                if self.selected_task().is_some() {
                    self.overlay = Overlay::Detail;
                }
            }
            KeyCode::Char('d') => {
                if self.selected_task().is_some() {
                    self.overlay = Overlay::ConfirmDelete;
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
