use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::widgets::{centered, vsplit};
use crate::app::{App, Field, Overlay, HELP};
use crate::timer::{format_time, now_ms};

pub fn draw(frame: &mut Frame, app: &App) {
    let footer_h = if app.status.is_empty() { 3 } else { 4 };
    let [header, body, footer] = vsplit(frame.area(), 3, footer_h);

    let running = app.running_label().unwrap_or("—");
    let title = format!(
        " Task Timer ─ {} ─ total {} ",
        app.header_date(),
        format_time(app.total_elapsed())
    );
    let head = Paragraph::new(vec![Line::from(vec![
        Span::raw("Mode: "),
        Span::styled(&app.timer_mode, Style::default().fg(Color::Yellow)),
        Span::raw("  │  Running: "),
        Span::styled(running, Style::default().fg(Color::Green)),
    ])])
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(head, header);

    let now = now_ms();
    let items: Vec<ListItem> = if app.tasks.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no tasks this day  —  press n to add",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let marker = if t.is_running { "▶ " } else { "  " };
                let elapsed = format_time(t.current_elapsed(now));
                let state = if t.is_running { "  [running]" } else { "" };
                let mut style = Style::default();
                if t.is_running {
                    style = style.fg(Color::Green);
                }
                if i == app.selected {
                    style = style
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                }
                ListItem::new(Line::from(Span::styled(
                    format!("{marker}{:<32}  {elapsed}{state}", t.label),
                    style,
                )))
            })
            .collect()
    };
    let list = List::new(items).block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    frame.render_widget(list, body);

    let hints = " n:new  Space:start/stop  e:edit  d:del  r:reset  R:reset all  x:export  ←/→:day  j/k:move  J/K:reorder  m:mode  ?:help  q:quit ";
    let (status_area, hints_area) = if app.status.is_empty() {
        (None, footer)
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(footer);
        (Some(chunks[0]), chunks[1])
    };
    if let Some(area) = status_area {
        let status = Paragraph::new(format!(" {} ", app.status))
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(status, area);
    }
    let foot = Paragraph::new(hints).block(Block::default().borders(Borders::ALL));
    frame.render_widget(foot, hints_area);

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(frame),
        Overlay::ConfirmDelete => draw_confirm(frame, "Delete this task?  y / n"),
        Overlay::ConfirmResetAll => draw_confirm(frame, "Reset all timers today?  y / n"),
        Overlay::Form {
            edit_id,
            field,
            label,
            description,
            elapsed,
        } => draw_form(
            frame,
            edit_id.is_some(),
            *field,
            label,
            description,
            elapsed,
        ),
    }
}

fn draw_help(frame: &mut Frame) {
    let area = centered(
        frame.area(),
        52,
        (HELP.len() as u16)
            .saturating_add(4)
            .min(frame.area().height),
    );
    frame.render_widget(Clear, area);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {k:<20} "), Style::default().fg(Color::Cyan)),
                Span::raw(*v),
            ])
        })
        .collect();
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" keys ")
            .title_alignment(Alignment::Center),
    );
    frame.render_widget(p, area);
}

fn draw_confirm(frame: &mut Frame, msg: &str) {
    let area = centered(frame.area(), 44, 5);
    frame.render_widget(Clear, area);
    let p = Paragraph::new(msg)
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title(" confirm "));
    frame.render_widget(p, area);
}

fn draw_form(
    frame: &mut Frame,
    editing: bool,
    field: Field,
    label: &str,
    description: &str,
    elapsed: &str,
) {
    let area = centered(frame.area(), 56, 12);
    frame.render_widget(Clear, area);
    let title = if editing { " edit task " } else { " new task " };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let active = |f: Field| {
        if field == f {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            Style::default()
        }
    };
    frame.render_widget(
        Paragraph::new("label").style(Style::default().fg(Color::DarkGray)),
        rows[0],
    );
    frame.render_widget(Paragraph::new(label).style(active(Field::Label)), rows[1]);
    frame.render_widget(
        Paragraph::new("description").style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(description)
            .style(active(Field::Description))
            .wrap(Wrap { trim: false }),
        rows[3],
    );
    frame.render_widget(
        Paragraph::new("elapsed (HH:MM:SS or minutes)").style(Style::default().fg(Color::DarkGray)),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new(elapsed).style(active(Field::Elapsed)),
        rows[5],
    );
    frame.render_widget(
        Paragraph::new("Tab: field  Enter: save  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray)),
        rows[7],
    );
}
