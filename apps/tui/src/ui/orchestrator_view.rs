use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::orchestrator::{DeliveryState, Focus};
use crate::app::{App, Overlay};
use crate::orchestrator::{display_text, history_entry_header, history_entry_rendered_lines};


pub fn draw(frame: &mut Frame, app: &App) {
    let state = &app.orchestrator;
    let composer_focused = state.focus == Focus::Composer;
    let footer_height = 6;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(if composer_focused { 5 } else { 0 }),
            Constraint::Length(footer_height),
        ])
        .split(frame.area());

    let connection = if state.delivery == DeliveryState::Sending && state.last_success.is_some() {
        "connected"
    } else if state.loading {
        "connecting"
    } else if state.error.is_some() && state.last_success.is_some() {
        "stale"
    } else if state.error.is_some() {
        "unavailable"
    } else if state.last_success.is_some() {
        "connected"
    } else {
        "not checked"
    };
    let mut header = vec![Line::from(vec![
        Span::raw("Mode: "),
        Span::styled("orchestrator", Style::default().fg(Color::Magenta)),
        Span::raw(format!("  Paseo: {connection}")),
    ])];
    if let Some(driver) = state.selected_driver() {
        header.push(Line::from(vec![
            Span::raw("Driver: "),
            Span::styled(display_text(&driver.id), Style::default().fg(Color::Cyan)),
            Span::raw("  status: "),
            Span::styled(display_text(&driver.status), Style::default().fg(Color::Green)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(header).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Orchestrator — {} labeled driver(s) ", state.drivers.len())),
        ),
        chunks[0],
    );

    if !state.driver_confirmed() {
        let items: Vec<ListItem> = if state.drivers.is_empty() {
            let message = if state.loading {
                "Discovering labeled drivers…"
            } else if state.error.is_some() {
                "No successful discovery result; the last successful data is retained."
            } else {
                "No active driver labeled orchestrator.role=driver."
            };
            vec![ListItem::new(message)]
        } else {
            state
                .drivers
                .iter()
                .enumerate()
                .map(|(index, driver)| {
                    let selected = index == state.selected;
                    let style = if selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                    } else {
                        Style::default()
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(if selected { "> " } else { "  " }),
                        Span::styled(display_text(&driver.name), style),
                        Span::raw("  "),
                        Span::styled(display_text(&driver.id), style),
                        Span::raw("  ["),
                        Span::styled(display_text(&driver.status), style),
                        Span::raw("]"),
                    ]))
                })
                .collect()
        };
        frame.render_widget(
            List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(if state.drivers.len() > 1 {
                        " Select a driver — Enter to open "
                    } else {
                        " Drivers "
                    }),
            ),
            chunks[1],
        );
    } else {
        let mut lines = Vec::new();
        for entry in &state.history {
            lines.push(Line::from(Span::styled(
                history_entry_header(entry),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            lines.extend(
                display_text(&entry.text)
                    .lines()
                    .map(|line| Line::from(line.to_owned())),
            );
            lines.push(Line::from(""));
        }
        if lines.is_empty() {
            lines.push(Line::from(if state.loading {
                "Loading retained history…"
            } else {
                "No retained timeline entries."
            }));
        }
        let content_width = chunks[1].width.saturating_sub(2).max(1) as usize;
        let total_lines = if state.history.is_empty() {
            1
        } else {
            state
                .history
                .iter()
                .map(|entry| history_entry_rendered_lines(entry, content_width))
                .sum()
        };
        let paragraph = Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Driver conversation "));
        let visible_lines = chunks[1].height.saturating_sub(2) as usize;
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let scroll = max_scroll.saturating_sub(state.history_scroll)
        .min(u16::MAX as usize) as u16;
        frame.render_widget(paragraph.scroll((scroll, 0)), chunks[1]);
    }
    if composer_focused {
        let body = if state.retry_confirmation {
            format!(
                "WARNING: delivery may already have occurred. Enter retries this exact message to the same driver; Esc cancels.\n\n{}",
                display_text(&state.draft)
            )
        } else if state.delivery == DeliveryState::Sending {
            format!("Sending; draft is locked.\n{}", display_text(&state.draft))
        } else if matches!(&state.delivery, DeliveryState::Unknown(_)) {
            format!(
                "Delivery unknown; draft locked. Check history before retrying.\n{}",
                display_text(&state.draft)
            )
        } else if state.draft.is_empty() {
            "Type or paste an instruction. Enter adds a newline.".to_owned()
        } else {
            display_text(&state.draft)
        };
        frame.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title(" Compose ")),
            chunks[2],
        );
    }
    let mut footer = Vec::new();
    if let Some(error) = state.error.as_deref() {
        footer.push(Line::from(Span::styled(
            display_text(error),
            Style::default().fg(Color::Yellow),
        )));
    }
    if let Some(warning) = state.history_warning.as_deref() {
        footer.push(Line::from(Span::styled(
            display_text(warning),
            Style::default().fg(Color::Yellow),
        )));
    }
    match &state.delivery {
        DeliveryState::Idle => {}
        DeliveryState::Sending => footer.push(Line::from("Sending to the selected driver…")),
        DeliveryState::Accepted => footer.push(Line::from(
            "Paseo accepted the message; this is not a reply or completion.",
        )),
        DeliveryState::Failed(error) => footer.push(Line::from(Span::styled(
            format!("Not sent: {}", display_text(error)),
            Style::default().fg(Color::Yellow),
        ))),
        DeliveryState::Unknown(error) => footer.push(Line::from(Span::styled(
            format!(
                "Delivery unknown: {}. Check history before retrying.",
                display_text(error)
            ),
            Style::default().fg(Color::Yellow),
        ))),
    }
    footer.push(Line::from(if composer_focused {
        if state.retry_confirmation {
            "Enter:retry  Esc:cancel retry"
        } else {
            "Enter:newline  Ctrl+S:send  Tab/Esc:history"
        }
    } else if state.driver_confirmed() {
        "Tab:compose  [:older  ]:newer  j/k:scroll  r:refresh  ?:help  q/Esc:back"
    } else {
        "j/k:select  Enter:open  r:refresh  ?:help  q/Esc:back"
    }));
    if let Some(when) = if state.driver_confirmed() {
        state.last_history_refresh
    } else {
        state.last_success
    } {
        let label = if state.driver_confirmed() {
            "Last history refresh"
        } else {
            "Last driver discovery"
        };
        footer.push(Line::from(format!("{label} {}s ago", when.elapsed().as_secs())));
    }
    frame.render_widget(
        Paragraph::new(footer).block(Block::default().borders(Borders::ALL)),
        chunks[3],
    );


    if matches!(&app.overlay, Overlay::Help) {
        super::timer_view::draw_help(frame);
    }
}

pub fn draw_quit_confirmation(frame: &mut Frame) {
    let area = super::widgets::centered(frame.area(), 58, 5);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(
            "Quit and discard the local Orchestrator draft or pending send?  y / n",
        )
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Confirm quit ")),
        area,
    );
}
