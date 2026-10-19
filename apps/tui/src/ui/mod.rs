mod timer_view;
mod archive_view;
mod kanban_view;
mod widgets;
mod orchestrator_view;

use crate::app::AppMode;

pub fn draw(frame: &mut ratatui::Frame, app: &crate::app::App) {
    match app.mode {
        AppMode::Archive => archive_view::draw(frame, app),
        AppMode::Kanban => kanban_view::draw(frame, app),
        AppMode::Daily => timer_view::draw(frame, app),
        AppMode::Orchestrator => orchestrator_view::draw(frame, app),
    }
    if app.orchestrator.quit_confirmation {
        orchestrator_view::draw_quit_confirmation(frame);
    }
}
