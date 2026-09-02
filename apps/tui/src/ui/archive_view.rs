use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::widgets::{centered, vsplit};
use crate::app::{App, ArchiveInput, Overlay, HELP};
use crate::timer::{format_time, now_ms};

pub fn draw(frame: &mut Frame, app: &App) {
    let footer_h = if app.status.is_empty() { 3 } else { 4 };
    let [header, body, footer] = vsplit(frame.area(), 3, footer_h);

    let running = app.running_label().unwrap_or("—");
    let title = format!(
        " Archive ─ filters q={} tag={} ─ {} results ",
        display_filter(&app.archive.filter_q),
        display_filter(&app.archive.filter_tag),
        app.archive.tasks.len()
    );
    let head = Paragraph::new(vec![Line::from(vec![
        Span::raw("Mode: "),
        Span::styled("archive", Style::default().fg(Color::Magenta)),
        Span::raw("  │  Running: "),
        Span::styled(running, Style::default().fg(Color::Green)),
    ])])
    .block(Block::default().borders(Borders::ALL).title(title));
    frame.render_widget(head, header);

    let filter_bar = format!(
        " /:filter label  t:filter tag  c:continue today  a:back to daily  (active: q={} tag={})",
        display_filter(&app.archive.filter_q),
        display_filter(&app.archive.filter_tag),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Fill(1)])
        .split(body);
    frame.render_widget(
        Paragraph::new(filter_bar).style(Style::default().fg(Color::DarkGray)),
        chunks[0],
    );

    let now = now_ms();
    let items: Vec<ListItem> = if app.archive.tasks.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "  no unfinished tasks matching filters — try clearing with Esc",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.archive
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let elapsed = format_time(t.current_elapsed(now));
                let date = &t.work_date;
                let tags = t
                    .tags
                    .as_deref()
                    .map(|s| format!(" [{}]", s))
                    .unwrap_or_default();
                let style = if i == app.archive.selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![Span::raw(format!(
                    "{}  {}  {}{}",
                    date, elapsed, t.label, tags
                ))])
                .style(style))
            })
            .collect()
    };
    let list = List::new(items).block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    frame.render_widget(list, chunks[1]);

    let hints = " a:daily  c:continue today  /:filter  t:tag  j/k:move  i:detail  ?:help  q:quit ";
    let (status_area, hints_area) = if app.status.is_empty() {
        (None, footer)
    } else {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(footer);
        (Some(parts[0]), parts[1])
    };
    if let Some(area) = status_area {
        frame.render_widget(
            Paragraph::new(format!(" {} ", app.status)).style(Style::default().fg(Color::Yellow)),
            area,
        );
    }
    frame.render_widget(
        Paragraph::new(hints).block(Block::default().borders(Borders::ALL)),
        hints_area,
    );

    match &app.overlay {
        Overlay::None => {}
        Overlay::Help => draw_help(frame),
        Overlay::ConfirmDelete => draw_confirm(frame, "Delete this task?  y / n"),
        Overlay::ConfirmResetAll => draw_confirm(frame, "Reset all timers today?  y / n"),
        Overlay::Detail => {
            if let Some(t) = app.archive_selected_task().or_else(|| app.selected_task()) {
                draw_detail(frame, t);
            }
        }
        Overlay::Form { .. } => {}
        Overlay::Filter { input, buffer } => draw_filter(frame, input, buffer),
    }
}

fn display_filter(v: &Option<String>) -> String {
    v.as_deref().filter(|s| !s.is_empty()).unwrap_or("—").to_string()
}

fn draw_help(frame: &mut Frame) {
    let area = centered(
        frame.area(),
        52,
        (HELP.len() as u16).saturating_add(4).min(frame.area().height),
    );
    frame.render_widget(Clear, area);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {:<20} ", k), Style::default().fg(Color::Cyan)),
                Span::raw(*v),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" keys ")
                .title_alignment(Alignment::Center),
        ),
        area,
    );
}

fn draw_confirm(frame: &mut Frame, msg: &str) {
    let area = centered(frame.area(), 44, 5);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(msg)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title(" confirm ")),
        area,
    );
}

fn draw_filter(frame: &mut Frame, input: &ArchiveInput, buffer: &str) {
    let area = centered(frame.area(), 56, 5);
    frame.render_widget(Clear, area);
    let title = match input {
        ArchiveInput::Label => " filter: label substring ",
        ArchiveInput::Tag => " filter: tag substring ",
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(buffer.to_string()),
            Line::from(Span::styled(
                "Enter: apply  Esc: cancel",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_detail(frame: &mut Frame, task: &crate::db::tasks::Task) {
    let area = centered(frame.area(), 70, 20);
    frame.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title(" task detail ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(" label:  ", Style::default().fg(Color::DarkGray)),
        Span::raw(&task.label),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" date:   ", Style::default().fg(Color::DarkGray)),
        Span::raw(&task.work_date),
    ]));
    if let Some(ref d) = task.description {
        if !d.is_empty() {
            for l in d.lines() {
                lines.push(Line::from(Span::raw(format!(" desc:   {}", l))));
            }
        }
    }
    lines.push(Line::from(vec![
        Span::styled(" status: ", Style::default().fg(Color::DarkGray)),
        Span::raw(&task.status),
    ]));
    if let Some(ref code) = task.code {
        lines.push(Line::from(vec![
            Span::styled(" code:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(code.as_str()),
        ]));
    }
    if let Some(ref tags) = task.tags {
        lines.push(Line::from(vec![
            Span::styled(" tags:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(tags.as_str()),
        ]));
    }
    if let Some(ref notes) = task.notes {
        if !notes.is_empty() {
            lines.push(Line::from(vec![]));
            lines.push(Line::from(Span::styled(
                " notes:",
                Style::default().fg(Color::DarkGray),
            )));
            for nl in notes.lines() {
                lines.push(Line::from(Span::raw(format!("   {}", nl))));
            }
        }
    }
    lines.push(Line::from(vec![]));
    lines.push(Line::from(Span::styled(
        " c: continue today  Esc / i / q: close",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
