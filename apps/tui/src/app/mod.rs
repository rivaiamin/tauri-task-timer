mod keymap;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::time::{Duration as StdDuration, Instant};

use crate::db;
use crate::db::tasks::{self, Task};
use crate::timer::{format_time, now_ms, parse_time_input};

pub use keymap::HELP;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Label,
    Description,
    Elapsed,
}

pub enum Overlay {
    None,
    Help,
    ConfirmDelete,
    ConfirmResetAll,
    Form {
        edit_id: Option<i64>,
        field: Field,
        label: String,
        description: String,
        elapsed: String,
    },
}

pub struct App {
    pub conn: Connection,
    pub user_id: String,
    pub date: NaiveDate,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub timer_mode: String,
    pub overlay: Overlay,
    pub status: String,
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
            overlay: Overlay::None,
            status: String::new(),
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

    fn err_status(&mut self, e: anyhow::Error) {
        self.status = e.to_string();
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
        self.status = format!("mode: {}", self.timer_mode);
        Ok(())
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
        };
    }

    fn open_edit(&mut self) {
        let Some((id, label, description, elapsed)) = self.selected_task().map(|t| {
            (
                t.id,
                t.label.clone(),
                t.description.clone().unwrap_or_default(),
                format_time(t.current_elapsed(now_ms())),
            )
        }) else {
            return;
        };
        self.overlay = Overlay::Form {
            edit_id: Some(id),
            field: Field::Label,
            label,
            description,
            elapsed,
        };
    }

    fn submit_form(&mut self) -> Result<()> {
        let (edit_id, label, description, elapsed) = match &self.overlay {
            Overlay::Form {
                edit_id,
                label,
                description,
                elapsed,
                ..
            } => (*edit_id, label.clone(), description.clone(), elapsed.clone()),
            _ => return Ok(()),
        };
        if label.trim().is_empty() {
            self.status = "label is required".into();
            return Ok(());
        }
        let elapsed_secs = match parse_time_input(&elapsed) {
            Some(s) => s,
            None => {
                self.status = "invalid time — use HH:MM:SS or minutes (e.g. 1.5)".into();
                return Ok(());
            }
        };
        if let Some(id) = edit_id {
            tasks::update_task(
                &self.conn,
                &self.user_id,
                id,
                Some(&label),
                Some(&description),
                Some(elapsed_secs),
            )?;
        } else {
            let desc = if description.is_empty() {
                None
            } else {
                Some(description.as_str())
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
                )?;
            }
            self.reload()?;
            if let Some(i) = self.tasks.iter().position(|t| t.id == created.id) {
                self.selected = i;
            }
            self.overlay = Overlay::None;
            self.status.clear();
            return Ok(());
        }
        self.overlay = Overlay::None;
        self.status.clear();
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
            ..
        } = &mut self.overlay
        else {
            return Ok(false);
        };
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                *field = match field {
                    Field::Label => Field::Description,
                    Field::Description => Field::Elapsed,
                    Field::Elapsed => Field::Label,
                };
            }
            KeyCode::Backspace => {
                let buf = match field {
                    Field::Label => label,
                    Field::Description => description,
                    Field::Elapsed => elapsed,
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
            Overlay::None => {}
        }

        match key.code {
            KeyCode::Char('q') => return Ok(true),
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
            _ => {}
        }
        Ok(false)
    }

    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let result = (|| {
            let mut last_reload = Instant::now();
            loop {
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
                    if let Err(e) = self.reload() {
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
