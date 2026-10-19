use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use super::widgets::centered;
use crate::app::{App, Overlay, HELP};
use crate::kanban;

pub fn draw(frame: &mut Frame, app: &App) {
    let footer_h = if app.status.is_empty() { 3 } else { 4 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(footer_h),
        ])
        .split(frame.area());

    let title = format!(
        " Kanban — {} card(s) — projects: {} ",
        app.kanban.cards.len(),
        app.kanban.projects.join(",")
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("Mode: "),
            Span::styled("kanban", Style::default().fg(Color::Magenta)),
            Span::raw("  ←/→:lane  ↑/↓:card  i:detail  o:jira  c:add today  r:refresh  P:pull  p:pipeline  a:daily  ?  help"),
        ]))
        .block(Block::default().borders(Borders::ALL).title(title)),
        chunks[0],
    );

    let columns = kanban::kanban_column_order();
    let col_count = columns.len().max(1);
    let col_widths: Vec<Constraint> = (0..col_count)
        .map(|_| Constraint::Ratio(1, col_count as u32))
        .collect();
    let col_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(col_widths)
        .split(chunks[1]);

    for (col_idx, col_name) in columns.iter().enumerate() {
        let indices = kanban::column_card_indices(&app.kanban.cards, col_name);
        let items: Vec<ListItem> = indices
            .iter()
            .enumerate()
            .map(|(row_idx, card_idx)| {
                let c = &app.kanban.cards[*card_idx];
                let run = if c.running_today { " *" } else { "" };
                let mut lines = vec![Line::from(format!("{}{}", c.issue_key, run))];
                if !c.label.trim().eq_ignore_ascii_case(c.issue_key.trim()) {
                    lines.push(Line::from(c.label.chars().take(40).collect::<String>()));
                }
                let selected = col_idx == app.kanban.selected_col
                    && row_idx == app.kanban.selected_row
                    && !indices.is_empty();
                let style = if selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default()
                };
                ListItem::new(lines).style(style)
            })
            .collect();
        let block = Block::default().borders(Borders::ALL).title(col_name.as_str());
        let list = List::new(items).block(block);
        frame.render_widget(list, col_areas[col_idx]);
    }

    let hints = " a:daily  ←/→:lane  j/k:move  i:detail  o:jira  c:add today  r:refresh  P:pull  p:pipeline  ?:help  q:quit ";
    let (status_area, hints_area) = if app.status.is_empty() {
        (None, chunks[2])
    } else {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(chunks[2]);
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
        Overlay::Detail {
            comments,
            prs,
            statuses,
            integrations,
        } => {
            if let Some(t) = app.focused_task() {
                super::timer_view::draw_detail(frame, t, comments, prs, statuses, integrations);
            }
        }
        Overlay::Comments {
            comments,
            selected,
            compose,
            ..
        } => super::timer_view::draw_comments(frame, comments, *selected, compose.as_deref()),
        Overlay::Form { .. } => {}
        Overlay::Filter { .. } => {}
        Overlay::Jira { mode } => super::widgets::draw_jira(frame, mode),
        Overlay::Git { .. } => {}
    }
}

fn draw_help(frame: &mut Frame) {
    let area = centered(
        frame.area(),
        52,
        (HELP.len() as u16).saturating_add(4).min(frame.area().height),
    );
    frame.render_widget(ratatui::widgets::Clear, area);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!(" {:<20} ", k), Style::default().fg(Color::Cyan)),
                Span::raw(*v),
            ])
        })
        .collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" help ");
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_confirm(frame: &mut Frame, message: &str) {
    let area = centered(frame.area(), message.len() as u16 + 6, 3);
    frame.render_widget(ratatui::widgets::Clear, area);
    frame.render_widget(
        Paragraph::new(message).block(Block::default().borders(Borders::ALL)),
        area,
    );
}
