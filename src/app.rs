use crate::editor::Editor;
use crate::search::FileSearch;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Search,
    Edit,
}

pub struct App {
    pub mode: Mode,
    pub search: FileSearch,
    pub editor: Option<Editor>,
    pub search_input: String,
    pub selected_index: usize,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub list_area: Rect,
    pub file_changed_externally: bool,
    _watcher: Option<RecommendedWatcher>,
    watcher_rx: Option<Receiver<PathBuf>>,
}

impl App {
    pub fn new(root: PathBuf) -> Result<Self> {
        let search = FileSearch::new(root)?;
        Ok(Self {
            mode: Mode::Search,
            search,
            editor: None,
            search_input: String::new(),
            selected_index: 0,
            should_quit: false,
            status_message: None,
            list_area: Rect::default(),
            file_changed_externally: false,
            _watcher: None,
            watcher_rx: None,
        })
    }

    fn setup_watcher(&mut self, path: &PathBuf) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        let path_clone = path.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    // Check for any data modification events
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) => {
                            let _ = tx.send(path_clone.clone());
                        }
                        _ => {}
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        watcher.watch(path, RecursiveMode::NonRecursive)?;
        self._watcher = Some(watcher);
        self.watcher_rx = Some(rx);
        Ok(())
    }

    fn clear_watcher(&mut self) {
        self._watcher = None;
        self.watcher_rx = None;
        self.file_changed_externally = false;
    }

    fn check_file_changes(&mut self) {
        if let Some(ref rx) = self.watcher_rx {
            // Non-blocking check for file change events
            while rx.try_recv().is_ok() {
                self.file_changed_externally = true;
            }
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        while !self.should_quit {
            // Check for external file changes
            self.check_file_changes();

            terminal.draw(|f| ui::draw(f, self))?;

            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key.code, key.modifiers)?;
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse.kind, mouse.column, mouse.row)?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match self.mode {
            Mode::Search => self.handle_search_key(code, modifiers),
            Mode::Edit => self.handle_edit_key(code, modifiers),
        }
    }

    fn handle_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) -> Result<()> {
        match self.mode {
            Mode::Search => {
                match kind {
                    MouseEventKind::ScrollUp => {
                        if self.selected_index >= 3 {
                            self.selected_index -= 3;
                        } else {
                            self.selected_index = 0;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        let max = self.search.match_count().saturating_sub(1);
                        self.selected_index = (self.selected_index + 3).min(max);
                    }
                    MouseEventKind::Down(_) => {
                        // Check if click is in list area
                        if col >= self.list_area.x
                            && col < self.list_area.x + self.list_area.width
                            && row >= self.list_area.y + 1  // +1 for border
                            && row < self.list_area.y + self.list_area.height - 1
                        {
                            let list_height = self.list_area.height.saturating_sub(2) as usize;
                            let scroll_offset = if self.selected_index >= list_height {
                                self.selected_index - list_height + 1
                            } else {
                                0
                            };

                            let clicked_row = (row - self.list_area.y - 1) as usize;
                            let clicked_index = scroll_offset + clicked_row;

                            if clicked_index < self.search.match_count() {
                                self.selected_index = clicked_index;
                                // Also trigger action (toggle folder or open file)
                                self.handle_enter()?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Mode::Edit => {
                if let Some(ref mut editor) = self.editor {
                    match kind {
                        MouseEventKind::ScrollUp => editor.scroll_up(3),
                        MouseEventKind::ScrollDown => editor.scroll_down(3),
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_enter(&mut self) -> Result<()> {
        if self.search.search_active {
            if let Some(path) = self.search.get_match(self.selected_index) {
                self.open_file(path)?;
            }
        } else {
            if let Some(entry) = self.search.get_visible_entry(self.selected_index) {
                if entry.is_dir {
                    let path = entry.path.clone();
                    self.search.toggle_expanded(&path);
                } else {
                    let path = self.search.root.join(&entry.path);
                    self.open_file(path)?;
                }
            }
        }
        Ok(())
    }

    fn handle_search_key(&mut self, code: KeyCode, _modifiers: KeyModifiers) -> Result<()> {
        match code {
            // Toggle hidden files with Tab key
            KeyCode::Tab => {
                self.search.toggle_hidden()?;
                self.selected_index = 0;
                self.status_message = Some(format!(
                    "Hidden files: {}",
                    if self.search.show_hidden { "SHOWN" } else { "HIDDEN" }
                ));
                return Ok(());
            }
            KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Enter => {
                self.handle_enter()?;
            }
            KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
            KeyCode::Down => {
                let max = self.search.match_count().saturating_sub(1);
                if self.selected_index < max {
                    self.selected_index += 1;
                }
            }
            KeyCode::Char(c) => {
                self.search_input.push(c);
                self.search.update_query(&self.search_input);
                self.selected_index = 0;
            }
            KeyCode::Backspace => {
                self.search_input.pop();
                self.search.update_query(&self.search_input);
                self.selected_index = 0;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_edit_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        if let Some(ref mut editor) = self.editor {
            // Handle Ctrl+R to reload file
            if modifiers.contains(KeyModifiers::CONTROL) {
                if let KeyCode::Char('r') = code {
                    editor.reload()?;
                    self.file_changed_externally = false;
                    self.status_message = Some("File reloaded".to_string());
                    return Ok(());
                }
            }

            if code == KeyCode::Esc {
                if editor.is_modified() {
                    editor.save()?;
                }
                self.clear_watcher();
                self.mode = Mode::Search;
                self.editor = None;
                self.status_message = None;
            } else {
                editor.handle_input(code, modifiers);
                // Clear external change flag if user starts editing
                if self.file_changed_externally && editor.is_modified() {
                    self.file_changed_externally = false;
                }
            }
        }
        Ok(())
    }

    fn open_file(&mut self, path: PathBuf) -> Result<()> {
        self.editor = Some(Editor::open(path.clone())?);
        self.mode = Mode::Edit;
        self.status_message = None;
        self.file_changed_externally = false;

        // Setup file watcher
        if let Err(e) = self.setup_watcher(&path) {
            self.status_message = Some(format!("Watcher failed: {}", e));
        } else {
            self.status_message = Some("File watcher active".to_string());
        }
        Ok(())
    }

    pub fn matches(&self) -> Vec<(PathBuf, u32)> {
        self.search.all_matches()
    }
}
