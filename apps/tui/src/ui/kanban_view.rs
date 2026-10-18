use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::{App, AppMode};
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
            Span::raw("  r:refresh  P:pull  p:pipeline  a:daily  ?  help"),
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

    for (i, col_name) in columns.iter().enumerate() {
        let cards: Vec<ListItem> = app
            .kanban
            .cards
            .iter()
            .filter(|c| kanban::column_for_status(&c.jira_status) == *col_name)
            .map(|c| {
                let run = if c.running_today { " *" } else { "" };
                ListItem::new(format!("{}{}\n{}", c.issue_key, run, c.label.chars().take(40).collect::<String>()))
            })
            .collect();
        let block = Block::default().borders(Borders::ALL).title(col_name.as_str());
        let list = List::new(cards).block(block);
        frame.render_widget(list, col_areas[i]);
    }

    let foot = if app.status.is_empty() {
        "Kanban: linked Jira tasks not Done".to_string()
    } else {
        app.status.clone()
    };
    frame.render_widget(Paragraph::new(foot), chunks[2]);
}
