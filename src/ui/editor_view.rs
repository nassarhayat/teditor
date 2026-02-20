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

    let editor_height = chunks[0].height.saturating_sub(2) as usize;
    let inner_width = chunks[0].width.saturating_sub(2) as usize;
    let line_count = editor.highlighted_lines().len().max(1);
    let line_number_digits = line_count.to_string().len();
    let line_number_width = line_number_digits + 1; // digits + space
    let text_width = inner_width.saturating_sub(line_number_width).max(1);

    // Calculate scroll offset to keep cursor in view (accounting for wrapping)
    let (cursor_row, cursor_col) = editor.cursor_position();
    let line_lengths = editor.line_lengths();
    let safe_row = cursor_row.min(line_lengths.len().saturating_sub(1));
    let mut cursor_visual_row = 0;
    for i in 0..safe_row {
        cursor_visual_row += wrapped_line_count(line_lengths.get(i).copied().unwrap_or(0), text_width);
    }
    let line_len = line_lengths.get(safe_row).copied().unwrap_or(0);
    let effective_col = cursor_col.min(line_len);
    let (wrap_row, col_in_wrap) = wrap_position(effective_col, text_width);
    cursor_visual_row += wrap_row;

    let scroll_offset = if cursor_visual_row >= editor_height {
        cursor_visual_row - editor_height + 1
    } else {
        0
    };

    let mut visible_lines: Vec<Line> = Vec::with_capacity(editor_height);
    let mut visual_row = 0;
    let number_style = Style::default().fg(Color::DarkGray);

    for (line_idx, spans) in editor.highlighted_lines().iter().enumerate() {
        let wrap_count = wrapped_line_count(line_lengths.get(line_idx).copied().unwrap_or(0), text_width);
        if visual_row + wrap_count <= scroll_offset {
            visual_row += wrap_count;
            continue;
        }

        let line_number = format!("{:width$} ", line_idx + 1, width = line_number_digits);
        let number_span = Span::styled(line_number, number_style);
        let pad_span = Span::styled(" ".repeat(line_number_width), number_style);
        let wrapped = wrap_spans(spans, text_width);
        let start_in_line = scroll_offset.saturating_sub(visual_row);

        for (wrap_idx, wrapped_spans) in wrapped.into_iter().enumerate().skip(start_in_line) {
            if visible_lines.len() >= editor_height {
                break;
            }
            let mut line_spans = Vec::new();
            if wrap_idx == 0 {
                line_spans.push(number_span.clone());
            } else {
                line_spans.push(pad_span.clone());
            }
            line_spans.extend(wrapped_spans);
            visible_lines.push(Line::from(line_spans));
        }

        visual_row += wrap_count;
        if visible_lines.len() >= editor_height {
            break;
        }
    }

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
        " [CONFLICT - external change]"
    } else {
        ""
    };
    let (row, col) = editor.cursor_position();

    let status_text = format!(
        " {}{}{}  |  Ln {}, Col {}  |  Esc: save & back | Ctrl+R: reload",
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
    let cursor_visual_col = line_number_width + col_in_wrap;
    let cursor_screen_row = cursor_visual_row.saturating_sub(scroll_offset);
    if cursor_screen_row < editor_height {
        let cursor_x = chunks[0].x + 1 + cursor_visual_col as u16;
        let cursor_y = chunks[0].y + 1 + cursor_screen_row as u16;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn wrapped_line_count(len: usize, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    if len == 0 {
        return 1;
    }
    (len - 1) / width + 1
}

fn wrap_position(col: usize, width: usize) -> (usize, usize) {
    if width == 0 || col == 0 {
        return (0, 0);
    }
    let wrap_row = (col - 1) / width;
    let col_in_wrap = (col - 1) % width + 1;
    (wrap_row, col_in_wrap)
}

fn wrap_spans(spans: &[(Style, String)], width: usize) -> Vec<Vec<Span<'static>>> {
    if width == 0 {
        return vec![Vec::new()];
    }
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut cur_width = 0usize;

    for (span_idx, (style, text)) in spans.iter().enumerate() {
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let available = width.saturating_sub(cur_width);
            if available == 0 {
                lines.push(Vec::new());
                cur_width = 0;
                continue;
            }

            let (take, rest) = split_at_char_count(remaining, available);
            if !take.is_empty() {
                lines
                    .last_mut()
                    .unwrap()
                    .push(Span::styled(take.to_string(), *style));
                cur_width += take.chars().count();
            }

            remaining = rest;
            if cur_width >= width && !remaining.is_empty() {
                lines.push(Vec::new());
                cur_width = 0;
            }
        }

        if cur_width >= width && span_idx + 1 < spans.len() {
            lines.push(Vec::new());
            cur_width = 0;
        }
    }

    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

fn split_at_char_count(s: &str, count: usize) -> (&str, &str) {
    if count == 0 {
        return ("", s);
    }
    let mut chars = s.char_indices();
    let mut end = s.len();
    let mut seen = 0usize;
    while let Some((idx, _)) = chars.next() {
        if seen == count {
            end = idx;
            break;
        }
        seen += 1;
    }
    if seen < count {
        return (s, "");
    }
    (&s[..end], &s[end..])
}
