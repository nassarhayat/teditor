mod editor_view;
mod search_view;

use crate::app::{App, Mode};
use ratatui::prelude::*;

pub fn draw(frame: &mut Frame, app: &mut App) {
    match app.mode {
        Mode::Search => search_view::draw(frame, app),
        Mode::Edit => editor_view::draw(frame, app),
    }
}
