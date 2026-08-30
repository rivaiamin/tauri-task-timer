mod keymap;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use rusqlite::Connection;
use std::time::{Duration as StdDuration, Instant};

use crate::db;
use crate::db::tasks::{self, Task};
use crate::timer::now_ms;

pub use keymap::HELP;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Label,
    Description,
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
        };
    }

    fn open_edit(&mut self) {
        let Some((id, label, description)) = self.selected_task().map(|t| {
            (
                t.id,
                t.label.clone(),
                t.description.clone().unwrap_or_default(),
            )
        }) else {
            return;
        };
        self.overlay = Overlay::Form {
            edit_id: Some(id),
            field: Field::Label,
            label,
            description,
        };
    }

    fn submit_form(&mut self) -> Result<()> {
        let (edit_id, label, description) = match &self.overlay {
            Overlay::Form {
                edit_id,
                label,
                description,
                ..
            } => (*edit_id, label.clone(), description.clone()),
            _ => return Ok(()),
        };
        if let Some(id) = edit_id {
            tasks::update_task(
                &self.conn,
                &self.user_id,
                id,
                Some(&label),
                Some(&description),
            )?;
        } else {
            let desc = if description.is_empty() {
                None
            } else {
                Some(description.as_str())
            };
            let date = self.date_str();
            let created = tasks::create_task(&self.conn, &self.user_id, &date, &label, desc)?;
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
}
