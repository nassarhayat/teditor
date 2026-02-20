use crate::app::App;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // File list
            Constraint::Length(3), // Search input
            Constraint::Length(1), // Status bar
        ])
        .split(frame.area());

    // Store list area for mouse click handling
    app.list_area = chunks[0];

    let list_height = chunks[0].height.saturating_sub(2) as usize;

    // Calculate scroll offset to keep selected item in view
    let scroll_offset = if app.selected_index >= list_height {
        app.selected_index - list_height + 1
    } else {
        0
    };

    let items: Vec<ListItem> = if app.search.search_active {
        // Search mode: flat list of matches
        let matches = app.matches();
        matches
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(list_height)
            .map(|(i, (path, score))| {
                let path_str = path
                    .strip_prefix(&app.search.root)
                    .unwrap_or(path)
                    .to_string_lossy();

                let content = format!("{} ({})", path_str, score);

                let style = if i == app.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(content).style(style)
            })
            .collect()
    } else {
        // Tree mode: show directory structure
        let entries = app.search.visible_entries();
        entries
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(list_height)
            .map(|(i, entry)| {
                let indent = "  ".repeat(entry.depth);
                let name = entry.path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.path.to_string_lossy().to_string());

                let content = if entry.is_dir {
                    let marker = if app.search.is_expanded(&entry.path) {
                        "▼"
                    } else {
                        "▶"
                    };
                    format!("{}{} {}/", indent, marker, name)
                } else {
                    format!("{}  {}", indent, name)
                };

                let style = if i == app.selected_index {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if entry.is_dir {
                    Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(content).style(style)
            })
            .collect()
    };

    // Show current path in title
    let path_display = app.search.root.to_string_lossy();
    let title = format!(" {} ", path_display);

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .title_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(list, chunks[0]);

    // Search input with match count and hidden files indicator
    let count = app.search.match_count();
    let match_info = if app.search.search_active {
        format!("{} matches", count)
    } else {
        format!("{} items", count)
    };

    let input = Paragraph::new(format!(" {}", app.search_input))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .title_style(Style::default().fg(Color::Yellow))
                .title_bottom(Line::from(match_info).right_aligned()),
        );
    frame.render_widget(input, chunks[1]);

    // Cursor position in search box
    frame.set_cursor_position(Position::new(
        chunks[1].x + app.search_input.len() as u16 + 2,
        chunks[1].y + 1,
    ));

    // Status bar
    let hidden_status = if app.search.show_hidden {
        "Hidden: SHOWN"
    } else {
        "Hidden: OFF"
    };
    let status_text = if let Some(ref msg) = app.status_message {
        format!(" {} | {}", msg, hidden_status)
    } else {
        format!(" Tab: toggle hidden | {}", hidden_status)
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status, chunks[2]);
}
