use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::JiraMode;

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(height.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area);
    let h = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(width.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(v[1]);
    h[1]
}

pub fn vsplit(area: Rect, header: u16, footer: u16) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header),
            Constraint::Fill(1),
            Constraint::Length(footer),
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

pub fn draw_jira(frame: &mut Frame, mode: &JiraMode) {
    match mode {
        JiraMode::Menu => {
            let area = centered(frame.area(), 48, 9);
            frame.render_widget(Clear, area);
            let lines = vec![
                Line::from(Span::styled("  1", Style::default().fg(Color::Cyan))),
                Line::from("     post comment"),
                Line::from(Span::styled("  2", Style::default().fg(Color::Cyan))),
                Line::from("     transition status"),
                Line::from(Span::styled("  3", Style::default().fg(Color::Cyan))),
                Line::from("     sprint sync"),
                Line::from(""),
                Line::from(Span::styled(
                    "  Esc: close",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let p = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" jira ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
        JiraMode::Comment { buffer } => {
            let area = centered(frame.area(), 50, 5);
            frame.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" jira comment ");
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let lines = vec![
                Line::from(buffer.as_str()),
                Line::from(Span::styled(
                    "Enter: post  Esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        JiraMode::Transition { transitions, selected } => {
            let height = (transitions.len() as u16).saturating_add(5).min(frame.area().height - 2);
            let area = centered(frame.area(), 52, height);
            frame.render_widget(Clear, area);
            let mut lines: Vec<Line> = transitions
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let target = t.to.as_ref().and_then(|to| to.name.as_deref()).unwrap_or("—");
                    let spans = vec![
                        Span::styled(
                            format!(" {} ", t.name),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::styled(
                            format!("→ {target}"),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ];
                    let mut style = Style::default();
                    if i == *selected {
                        style = style
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                    }
                    Line::from(spans).style(style)
                })
                .collect();
            lines.push(Line::from(vec![]));
            lines.push(Line::from(Span::styled(
                " j/k: move  Enter: apply  Esc: back",
                Style::default().fg(Color::DarkGray),
            )));
            let p = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" transition ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
        JiraMode::Syncing => {
            let area = centered(frame.area(), 30, 3);
            frame.render_widget(Clear, area);
            let p = Paragraph::new(Line::from(Span::styled(
                "  syncing sprint...",
                Style::default().fg(Color::Cyan),
            )))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" jira ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
    }
}
