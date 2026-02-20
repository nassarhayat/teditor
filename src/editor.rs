use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use std::fs;
use std::path::PathBuf;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style as SyntectStyle, Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use tui_textarea::TextArea;

pub struct Editor {
    pub path: PathBuf,
    pub textarea: TextArea<'static>,
    pub syntax_set: SyntaxSet,
    pub theme: Theme,
    modified: bool,
    original_content: String,
    highlighted_lines: Vec<Line<'static>>,
    content_hash: u64,
}

impl Editor {
    pub fn open(path: PathBuf) -> Result<Self> {
        let content = fs::read_to_string(&path)?;
        let original_content = content.clone();

        let lines: Vec<String> = content.lines().map(String::from).collect();
        let mut textarea = TextArea::new(lines);

        textarea.set_cursor_line_style(ratatui::style::Style::default());
        textarea.set_line_number_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray));

        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set.themes["base16-ocean.dark"].clone();

        let mut editor = Self {
            path,
            textarea,
            syntax_set,
            theme,
            modified: false,
            original_content,
            highlighted_lines: Vec::new(),
            content_hash: 0,
        };
        editor.update_highlighting();
        Ok(editor)
    }

    fn update_highlighting(&mut self) {
        let content = self.textarea.lines().join("\n");
        let new_hash = simple_hash(&content);

        if new_hash == self.content_hash && !self.highlighted_lines.is_empty() {
            return;
        }
        self.content_hash = new_hash;

        let extension = self.extension().unwrap_or_default();
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(&extension)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut styled_lines: Vec<Line<'static>> = Vec::new();

        for (line_num, line) in LinesWithEndings::from(&content).enumerate() {
            let ranges = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();

            let mut spans: Vec<Span<'static>> = Vec::new();

            spans.push(Span::styled(
                format!("{:4} ", line_num + 1),
                Style::default().fg(Color::DarkGray),
            ));

            for (style, text) in ranges {
                spans.push(Span::styled(
                    text.trim_end_matches('\n').to_string(),
                    syntect_to_ratatui_style(style),
                ));
            }

            styled_lines.push(Line::from(spans));
        }

        self.highlighted_lines = styled_lines;
    }

    pub fn highlighted_lines(&self) -> &[Line<'static>] {
        &self.highlighted_lines
    }

    pub fn handle_input(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let input = crossterm::event::KeyEvent::new(code, modifiers);
        self.textarea.input(input);
        self.modified = self.textarea.lines().join("\n") != self.original_content;
        self.update_highlighting();
    }

    pub fn save(&mut self) -> Result<()> {
        let content = self.textarea.lines().join("\n");
        fs::write(&self.path, &content)?;
        self.original_content = content;
        self.modified = false;
        Ok(())
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Reload file from disk, preserving cursor position if possible
    pub fn reload(&mut self) -> Result<()> {
        let content = fs::read_to_string(&self.path)?;
        let cursor = self.textarea.cursor();

        let lines: Vec<String> = content.lines().map(String::from).collect();
        self.textarea = TextArea::new(lines);
        self.textarea.set_cursor_line_style(ratatui::style::Style::default());
        self.textarea.set_line_number_style(ratatui::style::Style::default().fg(ratatui::style::Color::DarkGray));

        // Try to restore cursor position
        let max_row = self.textarea.lines().len().saturating_sub(1);
        let row = cursor.0.min(max_row);
        let max_col = self.textarea.lines().get(row).map(|l| l.len()).unwrap_or(0);
        let col = cursor.1.min(max_col);

        // Move cursor to restored position
        for _ in 0..row {
            self.textarea.input(crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }
        for _ in 0..col {
            self.textarea.input(crossterm::event::KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        }

        self.original_content = content;
        self.modified = false;
        self.content_hash = 0;
        self.update_highlighting();
        Ok(())
    }


    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".into())
    }

    pub fn extension(&self) -> Option<String> {
        self.path
            .extension()
            .map(|s| s.to_string_lossy().to_string())
    }

    pub fn cursor_position(&self) -> (usize, usize) {
        self.textarea.cursor()
    }

    pub fn scroll_up(&mut self, lines: usize) {
        for _ in 0..lines {
            self.textarea.input(crossterm::event::KeyEvent::new(
                KeyCode::Up,
                KeyModifiers::NONE,
            ));
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        for _ in 0..lines {
            self.textarea.input(crossterm::event::KeyEvent::new(
                KeyCode::Down,
                KeyModifiers::NONE,
            ));
        }
    }
}

fn syntect_to_ratatui_style(style: SyntectStyle) -> Style {
    let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
    Style::default().fg(fg)
}

fn simple_hash(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
