use crate::app::App;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let Some(ref editor) = app.editor else {
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Editor
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Calculate scroll offset to keep cursor in view
    let (cursor_row, cursor_col) = editor.cursor_position();
    let editor_height = chunks[0].height.saturating_sub(2) as usize;
    let scroll_offset = if cursor_row >= editor_height {
        cursor_row - editor_height + 1
    } else {
        0
    };

    // Use cached highlighted lines
    let visible_lines: Vec<Line> = editor
        .highlighted_lines()
        .iter()
        .skip(scroll_offset)
        .take(editor_height)
        .cloned()
        .collect();

    let editor_widget = Paragraph::new(visible_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", editor.filename()))
            .title_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(editor_widget, chunks[0]);

    // Status bar
    let modified_indicator = if editor.is_modified() { " [+]" } else { "" };
    let external_change = if app.file_changed_externally {
        " [FILE CHANGED - Ctrl+R to reload]"
    } else {
        ""
    };
    let (row, col) = editor.cursor_position();

    let status_text = format!(
        " {}{}{}  |  Ln {}, Col {}  |  Esc: save & back",
        editor.filename(),
        modified_indicator,
        external_change,
        row + 1,
        col + 1
    );

    let status_style = if app.file_changed_externally {
        Style::default().bg(Color::Yellow).fg(Color::Black)
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    };

    let status = Paragraph::new(status_text).style(status_style);
    frame.render_widget(status, chunks[1]);

    // Position cursor
    let cursor_x = chunks[0].x + 6 + cursor_col as u16;
    let cursor_y = chunks[0].y + 1 + (cursor_row - scroll_offset) as u16;
    frame.set_cursor_position(Position::new(cursor_x, cursor_y));
}
