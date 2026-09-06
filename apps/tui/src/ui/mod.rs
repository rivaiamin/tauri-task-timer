mod timer_view;
mod archive_view;
mod widgets;

use crate::app::AppMode;

pub fn draw(frame: &mut ratatui::Frame, app: &crate::app::App) {
    if app.mode == AppMode::Archive {
        archive_view::draw(frame, app);
    } else {
        timer_view::draw(frame, app);
    }
}
