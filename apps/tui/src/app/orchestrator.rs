use crate::orchestrator::{
    history_entry_rendered_lines, parse_driver_start_id, parse_drivers, parse_history_page,
    CommandResponse, CommandWorker, Driver, HistoryCursor, HistoryEntry, HistoryPage,
    LIFECYCLE_COMMAND_TIMEOUT,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const DISCOVER_ARGS: &[&str] = &["ls", "-g", "--label", "orchestrator.role=driver", "--json"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    DriverList,
    History,
    Composer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryState {
    Idle,
    Sending,
    Accepted,
    Failed(String),
    Unknown(String),
}

pub struct State {
    pub previous_mode: super::AppMode,
    pub drivers: Vec<Driver>,
    pub selected: usize,
    pub error: Option<String>,
    pub(super) discovery_program: String,
    pub(super) node_program: String,
    pub(super) lifecycle_program: String,
    pub loading: bool,
    pub focus: Focus,
    pub draft: String,
    pub delivery: DeliveryState,
    pub retry_confirmation: bool,
    pub start_adopt_confirmation: bool,
    pub quit_confirmation: bool,
    pub history: Vec<HistoryEntry>,
    pub history_scroll: usize,
    history_content_width: usize,
    pub history_epoch: Option<String>,
    pub start_cursor: Option<HistoryCursor>,
    pub end_cursor: Option<HistoryCursor>,
    pub history_agent_id: Option<String>,
    pub has_older: bool,
    pub has_newer: bool,
    pub history_warning: Option<String>,
    pub history_min_seq: u64,
    pub last_success: Option<Instant>,
    pub last_history_refresh: Option<Instant>,
    generation: u64,
    pending: Option<Pending>,
    driver_confirmed: bool,
    pending_adopt_id: Option<String>,
    worker: Option<CommandWorker>,
}

enum Pending {
    Discovery,
    DriverStart,
    HistoryStatus {
        agent_id: String,
        direction: HistoryDirection,
        cursor: Option<HistoryCursor>,
    },
    HistoryPage {
        agent_id: String,
        direction: HistoryDirection,
    },
    Send {
        agent_id: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryDirection {
    Tail,
    Before,
    After,
}

impl State {
    pub fn new() -> Self {
        Self {
            previous_mode: super::AppMode::Daily,
            drivers: Vec::new(),
            selected: 0,
            error: None,
            discovery_program: "paseo".into(),
            node_program: "node".into(),
            lifecycle_program: "orchestrator-run".into(),
            loading: false,
            focus: Focus::DriverList,
            draft: String::new(),
            delivery: DeliveryState::Idle,
            retry_confirmation: false,
            start_adopt_confirmation: false,
            quit_confirmation: false,
            history: Vec::new(),
            history_scroll: 0,
            history_content_width: 80,
            history_epoch: None,
            start_cursor: None,
            end_cursor: None,
            history_agent_id: None,
            has_older: false,
            has_newer: false,
            history_warning: None,
            history_min_seq: 0,
            last_success: None,
            last_history_refresh: None,
            generation: 0,
            pending: None,
            driver_confirmed: false,
            pending_adopt_id: None,
            worker: None,
        }
    }

    pub fn leave(&mut self) {
        if matches!(self.pending, Some(Pending::Send { .. })) {
            self.delivery = DeliveryState::Unknown(
                "view closed before Paseo acknowledged delivery; check history before retrying".into(),
            );
        }
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.pending = None;
        self.start_adopt_confirmation = false;
        self.pending_adopt_id = None;
        self.worker = None;
    }
    pub(super) fn set_content_width(&mut self, width: usize) {
        self.history_content_width = width.max(1);
    }

    pub fn enter(&mut self, previous_mode: super::AppMode) {
        self.previous_mode = previous_mode;
        if let Ok(program) = std::env::var("ORCHESTRATOR_LIFECYCLE_PROGRAM") {
            let program = program.trim();
            if !program.is_empty() {
                self.lifecycle_program = program.to_owned();
            }
        }
        if self.worker.is_none() {
            match CommandWorker::new() {
                Ok(worker) => self.worker = Some(worker),
                Err(error) => {
                    self.error = Some(format!("could not start Paseo worker: {error:#}"));
                    return;
                }
            }
        }
        self.refresh();
    }


    pub fn refresh(&mut self) {
        if self.loading {
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.pending = Some(Pending::Discovery);
        if !worker.submit(
            self.generation,
            &self.discovery_program,
            DISCOVER_ARGS,
            &[],
        ) {
            self.loading = false;
            self.pending = None;
            self.error = Some("Paseo discovery is already busy".into());
        }
    }

    pub fn poll(&mut self) {
        if let Some(response) = self.worker.as_ref().and_then(CommandWorker::try_recv) {
            self.apply_response(response);
        }
        if !self.loading
            && matches!(self.focus, Focus::History | Focus::Composer)
            && self.driver_confirmed
            && self
                .last_history_refresh
                .is_some_and(|last| last.elapsed() >= Duration::from_secs(2))
        {
            self.request_history(HistoryDirection::Tail);
        }
    }

    fn apply_response(&mut self, response: CommandResponse) {
        if response.generation != self.generation {
            return;
        }
        self.loading = false;
        let pending = self.pending.take();
        match (pending, response.result) {
            (Some(Pending::Discovery), Ok(output)) => match parse_drivers(&output.stdout) {
                Ok(drivers) => self.accept_discovery(drivers),
                Err(error) => self.error = Some(error.to_string()),
            },
            (Some(Pending::Discovery), Err(error)) => self.error = Some(error),
            (Some(Pending::DriverStart), Ok(output)) => match parse_driver_start_id(&output.stdout)
            {
                Ok(id) => {
                    self.pending_adopt_id = Some(id);
                    self.refresh();
                }
                Err(error) => self.error = Some(error.to_string()),
            },
            (Some(Pending::DriverStart), Err(error)) => self.error = Some(error),
            (
                Some(Pending::HistoryStatus {
                    agent_id,
                    direction,
                    cursor,
                }),
                Ok(output),
            ) => match parse_daemon_url(&output.stdout) {
                Ok(url) => self.submit_history_page(&agent_id, direction, cursor, &url),
                Err(error) => {
                    self.error = Some(error);
                    self.last_history_refresh = Some(Instant::now());
                }
            },
            (Some(Pending::HistoryStatus { .. }), Err(error)) => {
                self.error = Some(error);
                self.last_history_refresh = Some(Instant::now());
            }
            (Some(Pending::HistoryPage { agent_id, direction }), Ok(output)) => {
                match parse_history_page(&output.stdout, &agent_id) {
                    Ok(page) => self.apply_history_page(page, direction),
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.last_history_refresh = Some(Instant::now());
                    }
                }
            }
            (Some(Pending::HistoryPage { .. }), Err(error)) => {
                self.error = Some(error);
                self.last_history_refresh = Some(Instant::now());
            }
            (Some(Pending::Send { agent_id, .. }), Ok(output)) => {
                if self.selected_driver().map(|driver| driver.id.as_str())
                    != Some(agent_id.as_str())
                {
                    self.delivery = DeliveryState::Unknown(
                        "selected driver changed during send; inspect the original driver's history".into(),
                    );
                } else {
                    match parse_send_ack(&output.stdout) {
                        Ok(()) => {
                            self.delivery = DeliveryState::Accepted;
                            self.draft.clear();
                            self.request_history(HistoryDirection::Tail);
                        }
                        Err(error) => self.delivery = DeliveryState::Unknown(error),
                    }
                }
            }
            (Some(Pending::Send { .. }), Err(error)) => {
                self.delivery = DeliveryState::Unknown(error);
            }
            (None, _) => {}
        }
    }

    fn accept_discovery(&mut self, drivers: Vec<Driver>) {
        if let Some(adopt_id) = self.pending_adopt_id.take() {
            self.drivers = drivers;
            self.error = None;
            self.last_success = Some(Instant::now());
            if let Some(position) = self.drivers.iter().position(|driver| driver.id == adopt_id) {
                self.selected = position;
                if self.drivers.len() == 1 {
                    self.driver_confirmed = true;
                    self.focus = Focus::History;
                    self.request_history(HistoryDirection::Tail);
                } else {
                    self.driver_confirmed = false;
                    self.focus = Focus::DriverList;
                    self.clear_history();
                }
            } else {
                self.selected = 0;
                self.driver_confirmed = false;
                self.focus = Focus::DriverList;
                self.clear_history();
                self.error = Some(format!(
                    "driver-start returned {adopt_id}, but discovery did not list that id"
                ));
            }
            return;
        }
        let old_id = self.selected_driver().map(|driver| driver.id.clone());
        self.drivers = drivers;
        self.selected = old_id
            .as_deref()
            .and_then(|id| self.drivers.iter().position(|driver| driver.id == id))
            .unwrap_or(0);
        self.error = None;
        self.last_success = Some(Instant::now());
        self.driver_confirmed = self.drivers.len() == 1;
        self.focus = if self.driver_confirmed {
            Focus::History
        } else {
            Focus::DriverList
        };
        if self.driver_confirmed {
            self.request_history(HistoryDirection::Tail);
        } else {
            self.clear_history();
        }
    }

    fn clear_history(&mut self) {
        self.history.clear();
        self.history_scroll = 0;
        self.history_epoch = None;
        self.history_agent_id = None;
        self.has_older = false;
        self.has_newer = false;
        self.history_warning = None;
        self.history_min_seq = 0;
        self.start_cursor = None;
        self.end_cursor = None;
        self.last_history_refresh = None;
    }

    fn request_history(&mut self, direction: HistoryDirection) {
        if self.loading || !self.driver_confirmed {
            return;
        }
        let Some(agent_id) = self.selected_driver().map(|driver| driver.id.clone()) else {
            return;
        };
        let cursor = match direction {
            HistoryDirection::Tail => None,
            HistoryDirection::Before => {
                if !self.has_older {
                    return;
                }
                self.history
                    .first()
                    .and_then(|_| self.start_cursor.as_ref())
                    .cloned()
            }
            HistoryDirection::After => {
                if !self.has_newer {
                    return;
                }
                self.end_cursor.clone()
            }
        };
        if matches!(direction, HistoryDirection::Before | HistoryDirection::After)
            && cursor.is_none()
        {
            self.error = Some("History page has no cursor for the requested direction".into());
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.pending = Some(Pending::HistoryStatus {
            agent_id,
            direction,
            cursor: cursor.clone(),
        });
        let args = ["status", "--json"];
        if !worker.submit(self.generation, &self.discovery_program, &args, &[]) {
            self.loading = false;
            self.pending = None;
            self.error = Some("Paseo command worker is busy".into());
        }
    }

    fn submit_history_page(
        &mut self,
        agent_id: &str,
        direction: HistoryDirection,
        cursor: Option<HistoryCursor>,
        url: &str,
    ) {
        let Some(worker) = self.worker.as_ref() else {
            self.loading = false;
            return;
        };
        let direction_name = match direction {
            HistoryDirection::Tail => "tail",
            HistoryDirection::Before => "before",
            HistoryDirection::After => "after",
        };
        let request = json!({
            "action": "history",
            "agentId": agent_id,
            "url": url,
            "direction": direction_name,
            "cursor": cursor,
            "limit": 50,
        });
        let bytes = match serde_json::to_vec(&request) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.loading = false;
                self.error = Some(format!("cannot encode history request: {error}"));
                return;
            }
        };
        let adapter_path = worker.adapter_path().to_string_lossy().into_owned();
        let args = [adapter_path.as_str()];
        self.loading = true;
        self.pending = Some(Pending::HistoryPage {
            agent_id: agent_id.to_owned(),
            direction,
        });
        if !worker.submit(self.generation, &self.node_program, &args, &bytes) {
            self.loading = false;
            self.pending = None;
            self.error = Some("Paseo history worker is busy".into());
        }
    }

    fn apply_history_page(&mut self, page: HistoryPage, direction: HistoryDirection) {
        if self.selected_driver().map(|driver| driver.id.as_str()) != Some(page.agent_id.as_str()) {
            return;
        }
        let epoch_changed = self.history_epoch.as_deref().is_some_and(|epoch| epoch != page.epoch);
        let epoch_reset = page.reset || epoch_changed;
        if epoch_reset || self.history_agent_id.as_deref() != Some(&page.agent_id) {
            self.clear_history();
            if epoch_reset {
                self.history_warning = Some("Paseo history epoch changed; transcript reloaded.".into());
                if direction != HistoryDirection::Tail {
                    self.request_history(HistoryDirection::Tail);
                    return;
                }
            }
        }
        if page.stale_cursor || page.gap {
            self.history_warning = Some(if page.stale_cursor {
                "History cursor is stale; showing the last valid page.".into()
            } else {
                "Paseo reported a gap in retained history.".into()
            });
            self.last_history_refresh = Some(Instant::now());
            return;
        }
        let had_history = !self.history.is_empty();
        let at_bottom = self.history_scroll == 0;
        let old_first = self.history.first().map(|entry| entry.seq_start);
        let old_last = self.history.last().map(|entry| entry.seq_end);
        let mut inserted_lines = 0usize;
        let mut appended_lines = 0usize;
        for entry in page.entries {
            let duplicate = self.history.iter().any(|old| {
                old.seq_start == entry.seq_start && old.seq_end == entry.seq_end
            });
            if !duplicate {
                let rendered_lines =
                    history_entry_rendered_lines(&entry, self.history_content_width);
                if direction == HistoryDirection::Before
                    && old_first.is_some_and(|first| entry.seq_start < first)
                {
                    inserted_lines = inserted_lines.saturating_add(rendered_lines);
                } else if matches!(direction, HistoryDirection::Tail | HistoryDirection::After)
                    && old_last.is_some_and(|last| entry.seq_start > last)
                {
                    appended_lines = appended_lines.saturating_add(rendered_lines);
                }
                self.history.push(entry);
            }
        }
        self.history.sort_by_key(|entry| entry.seq_start);
        if direction == HistoryDirection::Before {
            self.history_scroll = self.history_scroll.saturating_add(inserted_lines);
        } else if at_bottom {
            self.history_scroll = 0;
        } else if matches!(direction, HistoryDirection::Tail | HistoryDirection::After) {
            self.history_scroll = self.history_scroll.saturating_add(appended_lines);
        }
        self.history_epoch = Some(page.epoch);
        self.history_agent_id = Some(page.agent_id);
        if direction == HistoryDirection::Before {
            self.start_cursor = page.start_cursor;
        } else if self.start_cursor.is_none() {
            self.start_cursor = page.start_cursor;
        }
        if direction != HistoryDirection::Before || self.end_cursor.is_none() {
            self.end_cursor = page.end_cursor;
        }
        match direction {
            HistoryDirection::Before => self.has_older = page.has_older,
            HistoryDirection::After => self.has_newer = page.has_newer,
            HistoryDirection::Tail => {
                if !had_history {
                    self.has_older = page.has_older;
                }
                self.has_newer = page.has_newer;
            }
        }
        self.history_min_seq = page.min_seq;
        self.history_warning = if epoch_reset {
            Some("Paseo history epoch changed; transcript reloaded.".into())
        } else if !self.has_older && !self.history.is_empty() {
            Some("Oldest available retained history reached.".into())
        } else {
            None
        };
        self.error = None;
        self.last_history_refresh = Some(Instant::now());
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.drivers.is_empty() {
            return;
        }
        let last = self.drivers.len() - 1;
        let new_selected = if delta < 0 {
            self.selected.saturating_sub(delta.unsigned_abs())
        } else {
            self.selected.saturating_add(delta as usize).min(last)
        };
        if new_selected != self.selected {
            self.generation = self.generation.wrapping_add(1);
            self.loading = false;
            self.pending = None;
            self.selected = new_selected;
            self.driver_confirmed = false;
            self.clear_history();
            self.focus = Focus::DriverList;
        }
    }

    pub fn selected_driver(&self) -> Option<&Driver> {
        self.drivers.get(self.selected)
    }

    pub fn move_history(&mut self, delta: isize) {
        self.history_scroll = if delta < 0 {
            self.history_scroll.saturating_add(delta.unsigned_abs())
        } else {
            self.history_scroll.saturating_sub(delta as usize)
        };
    }

    pub fn driver_confirmed(&self) -> bool {
        self.driver_confirmed
    }

    pub fn can_offer_start_adopt(&self) -> bool {
        !self.driver_confirmed
            && self.drivers.is_empty()
            && !self.loading
            && !self.start_adopt_confirmation
            && self.pending_adopt_id.is_none()
            && !matches!(self.pending, Some(Pending::DriverStart))
    }

    fn confirm_start_adopt(&mut self) {
        self.start_adopt_confirmation = false;
        if !self.can_offer_start_adopt() {
            return;
        }
        let Some(worker) = self.worker.as_ref() else {
            self.error = Some("Paseo worker unavailable".into());
            return;
        };
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.error = None;
        self.pending = Some(Pending::DriverStart);
        if !worker.submit_with_timeout(
            self.generation,
            &self.lifecycle_program,
            &["driver-start"],
            &[],
            LIFECYCLE_COMMAND_TIMEOUT,
        ) {
            self.loading = false;
            self.pending = None;
            self.error = Some("Start/adopt is already in progress".into());
        }
    }
    pub fn needs_quit_confirmation(&self) -> bool {
        !self.draft.is_empty() || self.delivery == DeliveryState::Sending
    }


    pub fn handle_paste(&mut self, text: &str) {
        if self.focus == Focus::Composer
            && !self.retry_confirmation
            && self.delivery != DeliveryState::Sending
            && !matches!(self.delivery, DeliveryState::Unknown(_))
        {
            self.draft.push_str(text);
        }
    }

    fn submit(&mut self, retry_unknown: bool) {
        if self.delivery == DeliveryState::Sending || self.draft.is_empty() {
            return;
        }
        if matches!(self.delivery, DeliveryState::Unknown(_)) && !retry_unknown {
            self.retry_confirmation = true;
            return;
        }
        if retry_unknown && !matches!(self.delivery, DeliveryState::Unknown(_)) {
            return;
        }
        let Some(agent_id) = self
            .selected_driver()
            .filter(|_| self.driver_confirmed)
            .map(|driver| driver.id.clone())
        else {
            self.delivery = DeliveryState::Failed("no confirmed driver selected; not sent".into());
            return;
        };
        let Some(worker) = self.worker.as_ref() else {
            self.delivery = DeliveryState::Failed("Paseo worker unavailable; not sent".into());
            return;
        };
        if self.loading {
            self.delivery = DeliveryState::Failed("Paseo worker busy; not sent".into());
            return;
        }
        let prompt = self.draft.clone();
        let args = [
            "send",
            "--no-wait",
            "--json",
            agent_id.as_str(),
            "--prompt",
            prompt.as_str(),
        ];
        self.generation = self.generation.wrapping_add(1);
        if worker.submit(self.generation, &self.discovery_program, &args, &[]) {
            self.pending = Some(Pending::Send { agent_id });
            self.loading = true;
            self.delivery = DeliveryState::Sending;
            self.retry_confirmation = false;
        } else {
            self.delivery = DeliveryState::Failed("Paseo worker busy; not sent".into());
        }
    }

    fn handle_composer_key(&mut self, key: KeyEvent) -> ViewAction {
        if self.retry_confirmation {
            match key.code {
                KeyCode::Enter => self.submit(true),
                KeyCode::Esc => self.retry_confirmation = false,
                _ => {}
            }
            return ViewAction::Stay;
        }
        match key.code {
            KeyCode::Tab | KeyCode::Esc => self.focus = Focus::History,
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.submit(false)
            }
            KeyCode::Enter
                if self.delivery != DeliveryState::Sending
                    && !matches!(self.delivery, DeliveryState::Unknown(_)) =>
            {
                self.draft.push('\n')
            }
            KeyCode::Backspace
                if self.delivery != DeliveryState::Sending
                    && !matches!(self.delivery, DeliveryState::Unknown(_)) =>
            {
                self.draft.pop();
            }
            KeyCode::Char(character)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                    && self.delivery != DeliveryState::Sending
                    && !matches!(self.delivery, DeliveryState::Unknown(_)) =>
            {
                self.draft.push(character)
            }
            _ => {}
        }
        ViewAction::Stay
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        if self.start_adopt_confirmation {
            match key.code {
                KeyCode::Enter => {
                    self.confirm_start_adopt();
                    ViewAction::Stay
                }
                KeyCode::Esc => {
                    self.start_adopt_confirmation = false;
                    ViewAction::Stay
                }
                _ => ViewAction::Stay,
            }
        } else {
            self.handle_key_without_start_confirm(key)
        }
    }

    fn handle_key_without_start_confirm(&mut self, key: KeyEvent) -> ViewAction {
        match self.focus {
            Focus::DriverList => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => ViewAction::Return,
                KeyCode::Char('?') => ViewAction::Help,
                KeyCode::Char('s') if self.can_offer_start_adopt() => {
                    self.start_adopt_confirmation = true;
                    ViewAction::Stay
                }
                KeyCode::Char('r') => {
                    self.refresh();
                    ViewAction::Stay
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_selection(1);
                    ViewAction::Stay
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_selection(-1);
                    ViewAction::Stay
                }
                KeyCode::Enter => {
                    if self.selected_driver().is_some() {
                        self.driver_confirmed = true;
                        self.focus = Focus::History;
                        self.clear_history();
                        self.request_history(HistoryDirection::Tail);
                    }
                    ViewAction::Stay
                }
                _ => ViewAction::Stay,
            },
            Focus::History => match key.code {
                KeyCode::Char('q') | KeyCode::Esc => ViewAction::Return,
                KeyCode::Char('?') => ViewAction::Help,
                KeyCode::Tab => {
                    self.focus = Focus::Composer;
                    ViewAction::Stay
                }
                KeyCode::Char('r') => {
                    self.request_history(HistoryDirection::Tail);
                    ViewAction::Stay
                }
                KeyCode::Char('[') => {
                    self.request_history(HistoryDirection::Before);
                    ViewAction::Stay
                }
                KeyCode::Char(']') => {
                    self.request_history(HistoryDirection::After);
                    ViewAction::Stay
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.move_history(1);
                    ViewAction::Stay
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.move_history(-1);
                    ViewAction::Stay
                }
                _ => ViewAction::Stay,
            },
            Focus::Composer => self.handle_composer_key(key),
        }
    }
}

fn parse_send_ack(json: &[u8]) -> Result<(), String> {
    let response: Value =
        serde_json::from_slice(json).map_err(|error| format!("invalid send response: {error}"))?;
    if response.get("status").and_then(Value::as_str) == Some("sent") {
        Ok(())
    } else {
        Err("Paseo did not confirm message acceptance; delivery is unknown".into())
    }
}

fn parse_daemon_url(json: &[u8]) -> Result<String, String> {
    let status: Value = serde_json::from_slice(json)
        .map_err(|error| format!("invalid Paseo status response: {error}"))?;
    let listen = status
        .get("listen")
        .and_then(Value::as_str)
        .ok_or_else(|| "Paseo status did not include its listening address".to_owned())?;
    let address = listen
        .parse::<std::net::SocketAddr>()
        .map_err(|_| "Paseo status returned an invalid listening address".to_owned())?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err("Paseo status did not return a loopback daemon endpoint".into());
    }
    Ok(format!("ws://{listen}/ws"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewAction {
    Stay,
    Return,
    Help,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn composer_keeps_navigation_keys_as_literal_multiline_input() {
        let mut state = State::new();
        state.focus = Focus::Composer;
        state.drivers = vec![Driver {
            id: "visible-driver".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;

        for character in ['q', 'r', 'O', ' '] {
            state.handle_key(key(KeyCode::Char(character)));
        }
        state.handle_key(key(KeyCode::Enter));
        state.handle_paste("- leading dash\nquotes ' \" and shell $(not executed) 你好");

        assert_eq!(
            state.draft,
            "qrO \n- leading dash\nquotes ' \" and shell $(not executed) 你好"
        );
        assert_eq!(state.focus, Focus::Composer);
        assert_eq!(state.delivery, DeliveryState::Idle);

        state.handle_key(key(KeyCode::Tab));
        assert_eq!(state.focus, Focus::History);
        assert_eq!(state.draft.len(), "qrO \n- leading dash\nquotes ' \" and shell $(not executed) 你好".len());
    }

    #[test]
    fn empty_composer_submit_does_not_start_delivery() {
        let mut state = State::new();
        state.focus = Focus::Composer;
        state.handle_key(ctrl(KeyCode::Char('s')));
        assert_eq!(state.delivery, DeliveryState::Idle);
        assert!(state.draft.is_empty());
    }
    #[cfg(unix)]
    fn fake_send_tools(
        success: bool,
        history_response: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "task-timer-orchestrator-send-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let paseo = directory.join("paseo");
        let node = directory.join("node");
        let args_log = directory.join("args");
        let count_log = directory.join("count");
        let exit = if success { "exit 0" } else { "exit 1" };
        std::fs::write(
            &paseo,
            format!(
                "#!/bin/sh\nif [ \"$1\" = send ]; then\n  printf '%s\\000' \"$@\" >> '{}'\n  printf x >> '{}'\n  printf '{{\"status\":\"sent\"}}'\n  {exit}\nfi\nif [ \"$1\" = status ]; then printf '{{\"listen\":\"127.0.0.1:6767\"}}'; exit 0; fi\nexit 2\n",
                args_log.display(),
                count_log.display()
            ),
        )
        .unwrap();
        let node_source = format!(
            "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' {}\n",
            shell_quote(history_response)
        );
        std::fs::write(&node, node_source).unwrap();
        for executable in [&paseo, &node] {
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        (directory, paseo, node)
    }
    #[cfg(unix)]
    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    #[cfg(unix)]
    fn sending_state(paseo: &std::path::Path, node: &std::path::Path) -> State {
        let mut state = State::new();
        state.worker = Some(CommandWorker::new().unwrap());
        state.discovery_program = paseo.to_string_lossy().into_owned();
        state.node_program = node.to_string_lossy().into_owned();
        state.drivers = vec![Driver {
            id: "visible-driver".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;
        state.focus = Focus::Composer;
        state
    }

    #[cfg(unix)]
    fn poll_until(state: &mut State, expected: impl Fn(&DeliveryState) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !expected(&state.delivery) && Instant::now() < deadline {
            state.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(expected(&state.delivery), "delivery did not reach expected state");
    }

    #[cfg(unix)]
    #[test]
    fn fake_send_receives_exact_argv_once_and_ack_is_not_a_reply() {
        let (directory, paseo, node) = fake_send_tools(true, "{}");
        let args_log = directory.join("args");
        let count_log = directory.join("count");
        let prompt = "-leading dash\nquotes ' \" and shell $(not executed) 你好".to_owned();
        let expected = vec![
            "send".to_owned(),
            "--no-wait".to_owned(),
            "--json".to_owned(),
            "visible-driver".to_owned(),
            "--prompt".to_owned(),
            prompt.clone(),
        ];
        let mut state = sending_state(&paseo, &node);
        state.draft = prompt;

        state.handle_key(ctrl(KeyCode::Char('s')));
        assert_eq!(state.delivery, DeliveryState::Sending);
        state.handle_key(ctrl(KeyCode::Char('s')));
        poll_until(&mut state, |delivery| *delivery == DeliveryState::Accepted);

        let actual: Vec<String> = std::fs::read(&args_log)
            .unwrap()
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8(arg.to_vec()).unwrap())
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(std::fs::read(&count_log).unwrap(), b"x");
        assert!(state.draft.is_empty());
        assert!(state.history.is_empty(), "acknowledgment must not fabricate history");
        assert_eq!(state.delivery, DeliveryState::Accepted);
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn accepted_send_is_reflected_only_by_authoritative_history() {
        let prompt = "report status only";
        let history = json!({
            "agentId": "visible-driver",
            "epoch": "epoch-a",
            "direction": "tail",
            "startCursor": {"epoch": "epoch-a", "seq": 10},
            "endCursor": {"epoch": "epoch-a", "seq": 12},
            "hasOlder": false,
            "hasNewer": false,
            "reset": false,
            "staleCursor": false,
            "gap": false,
            "window": {"minSeq": 1, "maxSeq": 12, "nextSeq": 13},
            "entries": [
                {
                    "item": {"type": "user_message", "text": prompt, "messageId": "sent-prompt"},
                    "timestamp": "2026-10-19T10:00:00Z",
                    "seqStart": 10,
                    "seqEnd": 10,
                    "sourceSeqRanges": []
                },
                {
                    "item": {"type": "user_message", "text": "external client prompt", "messageId": "external"},
                    "timestamp": "2026-10-19T10:00:01Z",
                    "seqStart": 11,
                    "seqEnd": 11,
                    "sourceSeqRanges": []
                },
                {
                    "item": {"type": "assistant_message", "text": "driver reply", "messageId": "reply"},
                    "timestamp": "2026-10-19T10:00:02Z",
                    "seqStart": 12,
                    "seqEnd": 12,
                    "sourceSeqRanges": []
                }
            ]
        })
        .to_string();
        let (directory, paseo, node) = fake_send_tools(true, &history);
        let mut state = sending_state(&paseo, &node);
        state.draft = prompt.into();
        state.handle_key(ctrl(KeyCode::Char('s')));
        poll_until(&mut state, |delivery| *delivery == DeliveryState::Accepted);
        let deadline = Instant::now() + Duration::from_secs(3);
        while state.history.len() < 3 && Instant::now() < deadline {
            state.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            state.history.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>(),
            vec![prompt, "external client prompt", "driver reply"]
        );
        assert_eq!(state.delivery, DeliveryState::Accepted);
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }


    #[cfg(unix)]
    #[test]
    fn history_keeps_refreshing_while_the_composer_is_focused() {
        let history = json!({
            "agentId": "visible-driver", "epoch": "epoch-a", "direction": "tail",
            "startCursor": {"epoch": "epoch-a", "seq": 5},
            "endCursor": {"epoch": "epoch-a", "seq": 5},
            "hasOlder": false, "hasNewer": false, "reset": false,
            "staleCursor": false, "gap": false,
            "window": {"minSeq": 5, "maxSeq": 5, "nextSeq": 6},
            "entries": [{
                "item": {"type": "assistant_message", "text": "reply while composing"},
                "timestamp": "2026-10-19T10:00:00Z",
                "seqStart": 5, "seqEnd": 5, "sourceSeqRanges": []
            }]
        })
        .to_string();
        let (directory, paseo, node) = fake_send_tools(true, &history);
        let mut state = sending_state(&paseo, &node);
        state.draft = "half-typed".into();
        state.last_history_refresh = Instant::now().checked_sub(Duration::from_secs(3));

        let deadline = Instant::now() + Duration::from_secs(3);
        while state.history.is_empty() && Instant::now() < deadline {
            state.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(state.history[0].text, "reply while composing");
        assert_eq!(state.draft, "half-typed");
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ambiguous_send_keeps_draft_and_retries_only_after_explicit_confirmation() {
        let (directory, paseo, node) = fake_send_tools(false, "{}");
        let count_log = directory.join("count");
        let mut state = sending_state(&paseo, &node);
        state.draft = "status only".into();

        state.handle_key(ctrl(KeyCode::Char('s')));
        poll_until(&mut state, |delivery| matches!(delivery, DeliveryState::Unknown(_)));
        assert_eq!(std::fs::read(&count_log).unwrap(), b"x");
        assert_eq!(state.draft, "status only");

        state.handle_key(ctrl(KeyCode::Char('s')));
        assert!(state.retry_confirmation);
        assert_eq!(std::fs::read(&count_log).unwrap(), b"x");
        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.delivery, DeliveryState::Sending);
        poll_until(&mut state, |delivery| matches!(delivery, DeliveryState::Unknown(_)));
        assert_eq!(std::fs::read(&count_log).unwrap(), b"xx");
        assert_eq!(state.draft, "status only");

        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn disappeared_send_target_cannot_turn_ack_into_success_or_clear_draft() {
        let mut state = State::new();
        state.drivers = vec![Driver {
            id: "old-driver".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;
        state.generation = 4;
        state.loading = true;
        state.pending = Some(Pending::Send {
            agent_id: "old-driver".into(),
        });
        state.delivery = DeliveryState::Sending;
        state.draft = "keep me".into();
        state.drivers[0].id = "replacement-driver".into();

        state.apply_response(CommandResponse {
            generation: 4,
            result: Ok(crate::orchestrator::CommandOutput {
                stdout: br#"{"status":"sent"}"#.to_vec(),
            }),
        });

        assert!(matches!(state.delivery, DeliveryState::Unknown(_)));
        assert_eq!(state.draft, "keep me");
        assert!(state.history.is_empty());
    }



    #[test]
    fn explicit_selection_clamps_at_each_end() {
        let mut state = State::new();
        state.drivers = vec![
            Driver { id: "one".into(), name: "driver".into(), status: "idle".into() },
            Driver { id: "two".into(), name: "driver".into(), status: "running".into() },
        ];
        state.move_selection(-1);
        assert_eq!(state.selected_driver().unwrap().id, "one");
        state.handle_key(key(KeyCode::Down));
        assert_eq!(state.selected_driver().unwrap().id, "two");
        state.move_selection(1);
        assert_eq!(state.selected_driver().unwrap().id, "two");
    }

    #[test]
    fn exit_keys_return_from_view_without_quitting_application() {
        let mut state = State::new();
        assert_eq!(state.handle_key(key(KeyCode::Esc)), ViewAction::Return);
        assert_eq!(state.handle_key(key(KeyCode::Char('q'))), ViewAction::Return);
        assert_eq!(state.handle_key(key(KeyCode::Char('r'))), ViewAction::Stay);
    }
    fn entry(seq: u64, text: &str) -> HistoryEntry {
        HistoryEntry {
            role: "user_message".into(),
            text: text.into(),
            timestamp: "2026-10-19T10:00:00Z".into(),
            seq_start: seq,
            seq_end: seq,
            message_id: None,
            is_wake: false,
        }
    }

    fn page(direction: &str, seqs: &[u64], has_older: bool) -> HistoryPage {
        HistoryPage {
            agent_id: "one".into(),
            epoch: "epoch-a".into(),
            direction: direction.into(),
            start_cursor: Some(HistoryCursor {
                epoch: "epoch-a".into(),
                seq: seqs.first().copied().unwrap_or(0),
            }),
            end_cursor: Some(HistoryCursor {
                epoch: "epoch-a".into(),
                seq: seqs.last().copied().unwrap_or(0),
            }),
            has_older,
            has_newer: false,
            reset: false,
            stale_cursor: false,
            gap: false,
            min_seq: seqs.first().copied().unwrap_or(0),
            max_seq: seqs.last().copied().unwrap_or(0),
            entries: seqs.iter().map(|seq| entry(*seq, "same text")).collect(),
        }
    }

    #[test]
    fn ambiguous_driver_list_requires_enter_before_transcript_selection() {
        let mut state = State::new();
        state.drivers = vec![
            Driver { id: "one".into(), name: "driver".into(), status: "idle".into() },
            Driver { id: "two".into(), name: "driver".into(), status: "running".into() },
        ];
        state.accept_discovery(state.drivers.clone());
        assert!(!state.driver_confirmed());
        assert_eq!(state.focus, Focus::DriverList);
        state.handle_key(key(KeyCode::Enter));
        assert!(state.driver_confirmed());
        assert_eq!(state.history_agent_id, None);
        assert_eq!(state.focus, Focus::History);
    }
    #[test]
    fn changing_drivers_invalidates_the_old_transcript_and_request() {
        let mut state = State::new();
        state.drivers = vec![
            Driver { id: "one".into(), name: "driver".into(), status: "idle".into() },
            Driver { id: "two".into(), name: "driver".into(), status: "running".into() },
        ];
        state.selected = 0;
        state.generation = 7;
        state.loading = true;
        state.pending = Some(Pending::HistoryPage {
            agent_id: "one".into(),
            direction: HistoryDirection::Tail,
        });
        state.driver_confirmed = true;
        state.focus = Focus::History;
        state.history = vec![entry(10, "first driver's private history")];
        state.history_agent_id = Some("one".into());
        state.history_epoch = Some("epoch-a".into());

        state.move_selection(1);

        assert_eq!(state.selected_driver().unwrap().id, "two");
        assert_eq!(state.generation, 8);
        assert!(!state.loading);
        assert!(state.pending.is_none());
        assert!(!state.driver_confirmed());
        assert_eq!(state.focus, Focus::DriverList);
        assert!(state.history.is_empty());
        assert_eq!(state.history_agent_id, None);
    }

    #[test]
    fn late_worker_error_from_old_generation_is_ignored() {
        let mut state = State::new();
        state.generation = 2;
        state.loading = true;
        state.pending = Some(Pending::HistoryPage {
            agent_id: "two".into(),
            direction: HistoryDirection::Tail,
        });

        state.apply_response(CommandResponse {
            generation: 1,
            result: Err("stale failure".into()),
        });
        assert!(state.loading);
        assert!(state.pending.is_some());
        assert_eq!(state.error, None);

        state.apply_response(CommandResponse {
            generation: 2,
            result: Err("current failure".into()),
        });
        assert!(!state.loading);
        assert!(state.pending.is_none());
        assert_eq!(state.error.as_deref(), Some("current failure"));
    }


    #[test]
    fn older_history_is_prepended_by_sequence_without_collapsing_equal_text() {
        let mut state = State::new();
        state.drivers = vec![Driver {
            id: "one".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;
        state.apply_history_page(page("tail", &[20, 21], true), HistoryDirection::Tail);
        state.history_scroll = 5;
        let mut older = page("before", &[18, 19], false);
        older.has_newer = true;
        state.apply_history_page(older, HistoryDirection::Before);
        assert_eq!(
            state.history.iter().map(|entry| entry.seq_start).collect::<Vec<_>>(),
            vec![18, 19, 20, 21]
        );
        assert!(state.history.iter().all(|entry| entry.text == "same text"));
        assert_eq!(state.history_scroll, 11);
        assert!(!state.has_older);
        assert!(!state.has_newer, "before-page newer entries are already loaded");
        assert_eq!(state.start_cursor.as_ref().unwrap().seq, 18);
        assert_eq!(state.end_cursor.as_ref().unwrap().seq, 21);

        state.apply_history_page(page("tail", &[20, 21], true), HistoryDirection::Tail);
        assert!(!state.has_older, "tail refresh must retain the loaded history boundary");
        assert_eq!(state.start_cursor.as_ref().unwrap().seq, 18);
        assert_eq!(state.end_cursor.as_ref().unwrap().seq, 21);
        let mut newer = page("after", &[22], false);
        newer.start_cursor.as_mut().unwrap().seq = 22;
        newer.end_cursor.as_mut().unwrap().seq = 22;
        newer.has_older = false;
        newer.has_newer = false;
        state.apply_history_page(newer, HistoryDirection::After);
        assert_eq!(state.history_scroll, 14);
        assert!(!state.has_older);
        assert!(!state.has_newer);
        assert_eq!(state.start_cursor.as_ref().unwrap().seq, 18);
        assert_eq!(state.end_cursor.as_ref().unwrap().seq, 22);
    }

    #[test]
    fn tail_refresh_preserves_read_position_and_only_follows_at_bottom() {
        let mut state = State::new();
        state.drivers = vec![Driver {
            id: "one".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;
        state.set_content_width(5);
        state.apply_history_page(page("tail", &[10, 11], true), HistoryDirection::Tail);

        state.history_scroll = 7;
        let mut newer = page("tail", &[10, 11, 12], true);
        newer.entries[2].text = "你好你好你好".into();
        state.apply_history_page(newer, HistoryDirection::Tail);
        assert_eq!(state.history_scroll, 17);
        assert_eq!(
            state.history.iter().map(|entry| entry.seq_start).collect::<Vec<_>>(),
            vec![10, 11, 12]
        );

        state.history_scroll = 0;
        state.apply_history_page(page("tail", &[10, 11, 12, 13], true), HistoryDirection::Tail);
        assert_eq!(state.history_scroll, 0);
        assert_eq!(
            state.history.iter().map(|entry| entry.seq_start).collect::<Vec<_>>(),
            vec![10, 11, 12, 13]
        );
    }

    #[test]
    fn stale_and_gapped_pages_keep_last_good_history_visible() {
        let mut state = State::new();
        state.drivers = vec![Driver {
            id: "one".into(),
            name: "driver".into(),
            status: "idle".into(),
        }];
        state.driver_confirmed = true;
        state.apply_history_page(page("tail", &[10, 11], true), HistoryDirection::Tail);

        let mut stale = page("tail", &[12], true);
        stale.stale_cursor = true;
        state.apply_history_page(stale, HistoryDirection::Tail);
        assert_eq!(state.history.len(), 2);
        assert_eq!(
            state.history_warning.as_deref(),
            Some("History cursor is stale; showing the last valid page.")
        );

        let mut gap = page("tail", &[12], true);
        gap.gap = true;
        state.apply_history_page(gap, HistoryDirection::Tail);
        assert_eq!(state.history.len(), 2);
        assert_eq!(
            state.history_warning.as_deref(),
            Some("Paseo reported a gap in retained history.")
        );
    }

    #[test]
    fn daemon_status_must_resolve_to_loopback_socket() {
        assert_eq!(
            parse_daemon_url(br#"{"listen":"127.0.0.1:6767"}"#).unwrap(),
            "ws://127.0.0.1:6767/ws"
        );
        assert!(parse_daemon_url(br#"{"listen":"0.0.0.0:6767"}"#).is_err());
    }

    #[test]
    fn driver_start_confirmation_cancel_does_not_invoke_lifecycle() {
        let mut state = State::new();
        state.worker = Some(CommandWorker::new().unwrap());
        assert!(state.can_offer_start_adopt());
        state.handle_key(key(KeyCode::Char('s')));
        assert!(state.start_adopt_confirmation);
        state.handle_key(key(KeyCode::Esc));
        assert!(!state.start_adopt_confirmation);
        assert!(state.can_offer_start_adopt());
    }

    #[cfg(unix)]
    fn fake_driver_start_tools(
        driver_id: &str,
        lifecycle_fails: bool,
    ) -> (
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "task-timer-orchestrator-start-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let paseo = directory.join("paseo");
        let lifecycle = directory.join("orchestrator-run");
        let lifecycle_log = directory.join("lifecycle.log");
        let discovery_log = directory.join("discovery.log");
        let node = directory.join("node");
        let after_start = directory.join("after_start");
        let fail = if lifecycle_fails { "1" } else { "0" };
        let history = json!({
            "agentId": driver_id,
            "epoch": "epoch-a",
            "direction": "tail",
            "startCursor": {"epoch": "epoch-a", "seq": 1},
            "endCursor": {"epoch": "epoch-a", "seq": 1},
            "hasOlder": false,
            "hasNewer": false,
            "reset": false,
            "staleCursor": false,
            "gap": false,
            "window": {"minSeq": 1, "maxSeq": 1, "nextSeq": 2},
            "entries": []
        })
        .to_string();
        std::fs::write(
            &paseo,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nif [ \"$1\" = ls ]; then\n  if [ -f '{}' ]; then\n    printf '[{{\"id\":\"{}\",\"name\":\"driver\",\"status\":\"idle\"}}]'\n  else\n    printf '[]'\n  fi\n  exit 0\nfi\nif [ \"$1\" = status ]; then printf '{{\"listen\":\"127.0.0.1:6767\"}}'; exit 0; fi\nexit 2\n",
                discovery_log.display(),
                after_start.display(),
                driver_id
            ),
        )
        .unwrap();
        std::fs::write(
            &lifecycle,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nif [ \"$1\" = driver-start ]; then\n  if [ '{fail}' = 1 ]; then\n    echo lifecycle failed >&2\n    exit 1\n  fi\n  touch '{}'\n  printf '%s\\n' '{}'\n  exit 0\nfi\nexit 2\n",
                lifecycle_log.display(),
                after_start.display(),
                driver_id
            ),
        )
        .unwrap();
        std::fs::write(
            &node,
            format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' {}\n",
                shell_quote(&history)
            ),
        )
        .unwrap();
        for executable in [&paseo, &lifecycle, &node] {
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        (directory, paseo, lifecycle, lifecycle_log, node)
    }

    #[cfg(unix)]
    #[test]
    fn driver_start_adopts_returned_driver_after_rediscovery() {
        let driver_id = "550e8400-e29b-41d4-a716-446655440000";
        let (directory, paseo, lifecycle, lifecycle_log, node) =
            fake_driver_start_tools(driver_id, false);
        let mut state = State::new();
        state.worker = Some(CommandWorker::new().unwrap());
        state.discovery_program = paseo.to_string_lossy().into_owned();
        state.lifecycle_program = lifecycle.to_string_lossy().into_owned();
        state.node_program = node.to_string_lossy().into_owned();
        state.refresh();
        wait_until_idle(&mut state);
        assert!(state.drivers.is_empty());

        state.handle_key(key(KeyCode::Char('s')));
        state.handle_key(key(KeyCode::Enter));
        wait_until_idle(&mut state);
        assert!(state.driver_confirmed);
        assert_eq!(state.selected_driver().unwrap().id, driver_id);
        assert_eq!(
            std::fs::read_to_string(&lifecycle_log).unwrap().trim(),
            "driver-start"
        );

        state.handle_key(key(KeyCode::Char('r')));
        wait_until_idle(&mut state);
        assert_eq!(
            std::fs::read_to_string(&lifecycle_log).unwrap().trim(),
            "driver-start"
        );
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn driver_start_malformed_identity_surfaces_error_without_adoption() {
        use std::os::unix::fs::PermissionsExt;

        let directory = std::env::temp_dir().join(format!(
            "task-timer-orchestrator-start-bad-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let lifecycle = directory.join("orchestrator-run");
        std::fs::write(
            &lifecycle,
            "#!/bin/sh\nprintf 'not-a-uuid\\n'\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&lifecycle, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut state = State::new();
        state.worker = Some(CommandWorker::new().unwrap());
        state.lifecycle_program = lifecycle.to_string_lossy().into_owned();
        state.handle_key(key(KeyCode::Char('s')));
        state.handle_key(key(KeyCode::Enter));
        wait_until_idle(&mut state);
        assert!(!state.driver_confirmed);
        assert!(state.error.as_ref().is_some_and(|error| error.contains("malformed")));
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn driver_start_failure_shows_explicit_error_without_follow_up_discovery() {
        let driver_id = "550e8400-e29b-41d4-a716-446655440111";
        let (directory, paseo, lifecycle, lifecycle_log, node) =
            fake_driver_start_tools(driver_id, true);
        let mut state = State::new();
        state.worker = Some(CommandWorker::new().unwrap());
        state.discovery_program = paseo.to_string_lossy().into_owned();
        state.lifecycle_program = lifecycle.to_string_lossy().into_owned();
        state.node_program = node.to_string_lossy().into_owned();
        state.handle_key(key(KeyCode::Char('s')));
        state.handle_key(key(KeyCode::Enter));
        wait_until_idle(&mut state);
        assert!(!state.driver_confirmed);
        assert!(state.error.is_some());
        assert_eq!(
            std::fs::read_to_string(&lifecycle_log).unwrap().trim(),
            "driver-start"
        );
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    fn wait_until_idle(state: &mut State) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            state.poll();
            if !state.loading {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!state.loading, "orchestrator state did not settle");
    }
}

