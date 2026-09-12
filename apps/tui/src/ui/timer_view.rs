use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::widgets::{centered, vsplit};
use crate::app::{App, Field, GitMode, Overlay, HELP};
use crate::db::tasks::Task;
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
                let marker = if t.is_running {
                    "▶ "
                } else if t.done {
                    "✓ "
                } else {
                    "  "
                };
                let elapsed = format_time(t.current_elapsed(now));
                let state = if t.is_running { "  [running]" } else { "" };
                let status_tag = if t.status.is_empty() || t.status == "todo" {
                    String::new()
                } else {
                    format!(" [{}", t.status)
                };
                let status_color = match t.status.as_str() {
                    "done" | "completed" => Color::Green,
                    "in_progress" | "running" => Color::Yellow,
                    _ => Color::DarkGray,
                };
                let mut spans = vec![
                    Span::raw(format!("{marker}{:<32}  {elapsed}{state}{status_tag}", t.label)),
                ];
                if !status_tag.is_empty() {
                    spans.push(Span::styled("]", Style::default().fg(status_color)));
                }
                let mut style = Style::default();
                if t.is_running {
                    style = style.fg(Color::Green);
                } else if t.done && i != app.selected {
                    style = style.fg(Color::DarkGray);
                }
                if i == app.selected {
                    style = style
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED | Modifier::BOLD);
                }
                ListItem::new(Line::from(spans).style(style))
            })
            .collect()
    };
    let list = List::new(items).block(Block::default().borders(Borders::LEFT | Borders::RIGHT));
    frame.render_widget(list, body);

    let hints = " a:archive  n:new  Space:start/stop  e:edit  i:detail  d:del  D:done  r:reset  R:reset all  x:export  ←/→:day  j/k:move  J/K:reorder  m:mode  Ctrl+J:jira  Ctrl+B:git  ?:help  q:quit ";
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
            status,
            code,
            notes,
            tags,
        } => draw_form(
            frame,
            edit_id.is_some(),
            *field,
            label,
            description,
            elapsed,
            status,
            code,
            notes,
            tags,
        ),
        Overlay::Detail => {
            if let Some(t) = app.selected_task() {
                draw_detail(frame, t);
            }
        }
        Overlay::Filter { .. } => {}
        Overlay::Jira { mode } => super::widgets::draw_jira(frame, mode),
        Overlay::Git { mode } => draw_git(frame, mode),
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
    status: &str,
    code: &str,
    notes: &str,
    tags: &str,
) {
    let area = centered(frame.area(), 56, 18);
    frame.render_widget(Clear, area);
    let title = if editing { " edit task " } else { " new task " };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label header
            Constraint::Length(1), // label value
            Constraint::Length(1), // description header
            Constraint::Length(1), // description value
            Constraint::Length(1), // status header
            Constraint::Length(1), // status value
            Constraint::Length(1), // code header
            Constraint::Length(1), // code value
            Constraint::Length(1), // elapsed header
            Constraint::Length(1), // elapsed value
            Constraint::Length(1), // notes header
            Constraint::Length(1), // notes value
            Constraint::Length(1), // tags header
            Constraint::Length(1), // tags value
            Constraint::Length(1), // spacer
            Constraint::Length(1), // footer
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
        Paragraph::new("status").style(Style::default().fg(Color::DarkGray)),
        rows[4],
    );
    frame.render_widget(Paragraph::new(status).style(active(Field::Status)), rows[5]);
    frame.render_widget(
        Paragraph::new("code").style(Style::default().fg(Color::DarkGray)),
        rows[6],
    );
    frame.render_widget(Paragraph::new(code).style(active(Field::Code)), rows[7]);
    frame.render_widget(
        Paragraph::new("elapsed (HH:MM:SS or minutes)").style(Style::default().fg(Color::DarkGray)),
        rows[8],
    );
    frame.render_widget(Paragraph::new(elapsed).style(active(Field::Elapsed)), rows[9]);
    frame.render_widget(
        Paragraph::new("notes").style(Style::default().fg(Color::DarkGray)),
        rows[10],
    );
    frame.render_widget(
        Paragraph::new(notes)
            .style(active(Field::Notes))
            .wrap(Wrap { trim: false }),
        rows[11],
    );
    frame.render_widget(
        Paragraph::new("tags").style(Style::default().fg(Color::DarkGray)),
        rows[12],
    );
    frame.render_widget(Paragraph::new(tags).style(active(Field::Tags)), rows[13]);
    frame.render_widget(
        Paragraph::new("Tab: field  Enter: save  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray)),
        rows[15],
    );
}

fn draw_git(frame: &mut Frame, mode: &GitMode) {
    match mode {
        GitMode::Menu => {
            let area = centered(frame.area(), 48, 9);
            frame.render_widget(Clear, area);
            let lines = vec![
                Line::from(Span::styled("  1", Style::default().fg(Color::Cyan))),
                Line::from("     link current branch to task"),
                Line::from(Span::styled("  2", Style::default().fg(Color::Cyan))),
                Line::from("     show commits on linked branch"),
                Line::from(Span::styled("  3", Style::default().fg(Color::Cyan))),
                Line::from("     PR status (Bitbucket)"),
                Line::from(""),
                Line::from(Span::styled(
                    "  Esc: close",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let p = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" git ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
        GitMode::LinkBranch { buffer } => {
            let area = centered(frame.area(), 50, 5);
            frame.render_widget(Clear, area);
            let block = Block::default()
                .borders(Borders::ALL)
                .title(" link branch ");
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let lines = vec![
                Line::from(buffer.as_str()),
                Line::from(Span::styled(
                    "Enter: confirm  Esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        }
        GitMode::Commits { commits, ahead, behind } => {
            let title = format!(" commits ({ahead} ahead, {behind} behind) ");
            let height = (commits.len() as u16).saturating_add(5).min(frame.area().height - 2);
            let area = centered(frame.area(), 60, height);
            frame.render_widget(Clear, area);
            let mut lines: Vec<Line> = commits
                .iter()
                .map(|c| {
                    Line::from(vec![
                        Span::styled(
                            format!(" {} ", &c.hash[..7.min(c.hash.len())]),
                            Style::default().fg(Color::Yellow),
                        ),
                        Span::raw(&c.subject),
                    ])
                })
                .collect();
            lines.push(Line::from(vec![]));
            lines.push(Line::from(Span::styled(
                " Esc / q: back",
                Style::default().fg(Color::DarkGray),
            )));
            let p = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
        GitMode::PrStatus { prs, selected } => {
            let height = (prs.len() as u16).saturating_add(5).min(frame.area().height - 2);
            let area = centered(frame.area(), 60, height);
            frame.render_widget(Clear, area);
            let mut lines: Vec<Line> = prs
                .iter()
                .enumerate()
                .map(|(i, pr)| {
                    let state_color = match pr.state.as_str() {
                        "OPEN" => Color::Green,
                        "MERGED" => Color::Magenta,
                        "DECLINED" => Color::Red,
                        _ => Color::DarkGray,
                    };
                    let spans = vec![
                        Span::styled(
                            format!(" {}", pr.state),
                            Style::default().fg(state_color),
                        ),
                        Span::raw(format!("  {}", pr.title)),
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
                " Esc / q: back",
                Style::default().fg(Color::DarkGray),
            )));
            let p = Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" pull requests ")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(p, area);
        }
    }
}

fn draw_detail(frame: &mut Frame, task: &Task) {
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
    if let Some(ref desc) = task.description {
        if !desc.is_empty() {
            for line in desc.lines() {
                lines.push(Line::from(Span::styled(
                    format!(" desc:   {line}"),
                    Style::default(),
                )));
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
            lines.push(Line::from(Span::styled(" notes:", Style::default().fg(Color::DarkGray))));
            for note_line in notes.lines() {
                lines.push(Line::from(Span::raw(format!("   {note_line}"))));
            }
        }
    }
    lines.push(Line::from(vec![]));
    lines.push(Line::from(Span::styled(
        " Esc / i / q: close",
        Style::default().fg(Color::DarkGray),
    )));

    let p = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}
