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
        let match_count = app.search.match_count();
        let end = (scroll_offset + list_height).min(match_count);
        (scroll_offset..end)
            .filter_map(|i| app.search.match_path_at(i).map(|(p, s)| (i, p, s)))
            .map(|(i, path, score)| {
                let path_str = path.to_string_lossy();
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
        let visible_count = app.search.visible_len();
        let end = (scroll_offset + list_height).min(visible_count);
        (scroll_offset..end)
            .filter_map(|i| app.search.visible_entry_at(i).map(|e| (i, e)))
            .map(|(i, entry)| {
                let indent = "  ".repeat(entry.depth);
                let name = entry.path
                    .file_name()
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

    // Search / create input with match count and hidden files indicator
    let count = app.search.match_count();
    let match_info = if app.search.search_active {
        format!("{} matches", count)
    } else {
        format!("{} items", count)
    };

    let base_display = if app.create_base.as_os_str().is_empty() {
        ".".to_string()
    } else {
        app.create_base.to_string_lossy().to_string()
    };

    let (input_text, input_title, input_bottom) = if app.create_active {
        (
            app.create_input.as_str(),
            format!(" New (in {}/) ", base_display),
            "Enter: create | Esc: cancel".to_string(),
        )
    } else {
        (
            app.search_input.as_str(),
            " Search ".to_string(),
            match_info,
        )
    };

    let input = Paragraph::new(format!(" {}", input_text))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(input_title)
                .title_style(Style::default().fg(Color::Yellow))
                .title_bottom(Line::from(input_bottom).right_aligned()),
        );
    frame.render_widget(input, chunks[1]);

    // Cursor position in input box
    let cursor_len = if app.create_active {
        app.create_input.len()
    } else {
        app.search_input.len()
    };
    frame.set_cursor_position(Position::new(
        chunks[1].x + cursor_len as u16 + 2,
        chunks[1].y + 1,
    ));

    // Status bar
    let hidden_status = if app.search.show_hidden {
        "Hidden: SHOWN"
    } else {
        "Hidden: OFF"
    };
    let status_text = if app.create_active {
        format!(" New in {}/ | Enter: create | Esc: cancel | {}", base_display, hidden_status)
    } else if let Some(ref msg) = app.status_message {
        format!(" {} | {}", msg, hidden_status)
    } else {
        format!(" Tab: toggle hidden | Ctrl+N: new | {}", hidden_status)
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status, chunks[2]);
}
