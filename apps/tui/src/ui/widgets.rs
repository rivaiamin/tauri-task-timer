use ratatui::layout::{Constraint, Direction, Layout, Rect};

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
