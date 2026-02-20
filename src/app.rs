use crate::editor::Editor;
use crate::search::FileSearch;
use crate::ui;
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEventKind};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::prelude::*;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

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
    _root_watcher: Option<RecommendedWatcher>,
    root_watcher_rx: Option<Receiver<()>>,
    index_rx: Option<Receiver<Result<Vec<PathBuf>>>>,
    pub create_active: bool,
    pub create_input: String,
    pub create_base: PathBuf,
}

impl App {
    pub fn new(root: PathBuf) -> Result<Self> {
        let search = FileSearch::new_deferred(root.clone())?;
        let (tx, rx) = mpsc::channel();
        let root_clone = root.clone();
        let show_hidden = search.show_hidden;
        thread::spawn(move || {
            let result = FileSearch::collect_files(&root_clone, show_hidden);
            let _ = tx.send(result);
        });

        let mut app = Self {
            mode: Mode::Search,
            search,
            editor: None,
            search_input: String::new(),
            selected_index: 0,
            should_quit: false,
            list_area: Rect::default(),
            file_changed_externally: false,
            _watcher: None,
            watcher_rx: None,
            _root_watcher: None,
            root_watcher_rx: None,
            index_rx: Some(rx),
            create_active: false,
            create_input: String::new(),
            create_base: PathBuf::new(),
            status_message: Some("Indexing...".to_string()),
        };
        let root = app.search.root.clone();
        if let Err(e) = app.setup_root_watcher(&root) {
            app.status_message = Some(format!("Root watcher failed: {}", e));
        }
        Ok(app)
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
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::NonRecursive)?;
        self._watcher = Some(watcher);
        self.watcher_rx = Some(rx);
        Ok(())
    }

    fn setup_root_watcher(&mut self, path: &PathBuf) -> Result<()> {
        let (tx, rx) = mpsc::channel();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    use notify::EventKind;
                    use notify::event::ModifyKind;
                    match event.kind {
                        EventKind::Create(_) | EventKind::Remove(_) => {
                            let _ = tx.send(());
                        }
                        EventKind::Modify(ModifyKind::Name(_)) => {
                            let _ = tx.send(());
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::Recursive)?;
        self._root_watcher = Some(watcher);
        self.root_watcher_rx = Some(rx);
        Ok(())
    }

    fn clear_watcher(&mut self) {
        self._watcher = None;
        self.watcher_rx = None;
        self.file_changed_externally = false;
    }

    fn check_file_changes(&mut self) {
        if self.editor.is_none() {
            return;
        }

        let mut changed = false;
        if let Some(ref rx) = self.watcher_rx {
            // Non-blocking check for file change events
            while rx.try_recv().is_ok() {
                changed = true;
            }
        }

        if !changed {
            return;
        }

        if let Some(ref mut editor) = self.editor {
            if editor.is_modified() {
                self.file_changed_externally = true;
                self.status_message = Some("External change detected (unsaved edits)".to_string());
            } else {
                match editor.reload() {
                    Ok(()) => {
                        self.file_changed_externally = false;
                        self.status_message = Some("File reloaded (external change)".to_string());
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Reload failed: {}", e));
                    }
                }
            }
        }
    }

    fn check_root_changes(&mut self) -> bool {
        let mut changed = false;
        if let Some(ref rx) = self.root_watcher_rx {
            while rx.try_recv().is_ok() {
                changed = true;
            }
        }
        changed
    }

    fn refresh_search(&mut self) {
        if let Err(e) = self.search.refresh() {
            self.status_message = Some(format!("Refresh failed: {}", e));
            return;
        }
        self.search.update_query(&self.search_input);
        let max = self.search.match_count().saturating_sub(1);
        if self.selected_index > max {
            self.selected_index = max;
        }
    }

    fn check_indexing(&mut self) -> bool {
        let Some(ref rx) = self.index_rx else {
            return false;
        };

        match rx.try_recv() {
            Ok(result) => {
                self.index_rx = None;
                match result {
                    Ok(files) => {
                        self.search.apply_index(files);
                        self.search.update_query(&self.search_input);
                        let max = self.search.match_count().saturating_sub(1);
                        if self.selected_index > max {
                            self.selected_index = max;
                        }
                        if matches!(self.status_message.as_deref(), Some("Indexing...")) {
                            self.status_message = None;
                        }
                    }
                    Err(e) => {
                        self.search.indexing = false;
                        self.status_message = Some(format!("Indexing failed: {}", e));
                    }
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.index_rx = None;
                self.search.indexing = false;
                self.status_message = Some("Indexing failed: worker disconnected".to_string());
                true
            }
        }
    }

    pub fn run(&mut self, terminal: &mut Terminal<impl Backend>) -> Result<()> {
        let tick_rate = Duration::from_millis(100);
        let refresh_interval = Duration::from_millis(300);
        let mut last_tick = Instant::now();
        let mut last_root_refresh = Instant::now();
        let mut should_draw = true;
        let mut root_refresh_pending = false;

        while !self.should_quit {
            if should_draw {
                terminal.draw(|f| ui::draw(f, self))?;
                should_draw = false;
            }

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or(Duration::from_secs(0));

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key.code, key.modifiers)?;
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse(mouse.kind, mouse.column, mouse.row)?;
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
                should_draw = true;
            }

            if last_tick.elapsed() >= tick_rate {
                let before = self.file_changed_externally;
                self.check_file_changes();
                if self.file_changed_externally != before {
                    should_draw = true;
                }

                if self.check_root_changes() {
                    root_refresh_pending = true;
                }
                if self.check_indexing() {
                    should_draw = true;
                }
                if root_refresh_pending
                    && !self.search.indexing
                    && last_root_refresh.elapsed() >= refresh_interval
                {
                    self.refresh_search();
                    root_refresh_pending = false;
                    last_root_refresh = Instant::now();
                    if self.mode == Mode::Search {
                        should_draw = true;
                    }
                }
                last_tick = Instant::now();
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
                if self.create_active {
                    return Ok(());
                }
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
                    if let Err(e) = self.search.toggle_expanded(&path) {
                        self.status_message = Some(format!("Expand failed: {}", e));
                    }
                } else {
                    let path = self.search.root.join(&entry.path);
                    self.open_file(path)?;
                }
            }
        }
        Ok(())
    }

    fn handle_search_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        if self.create_active {
            return self.handle_create_key(code, modifiers);
        }

        if modifiers.contains(KeyModifiers::CONTROL) {
            if let KeyCode::Char('n') = code {
                self.start_create_mode();
                return Ok(());
            }
        }

        match code {
            // Toggle hidden files with Tab key
            KeyCode::Tab => {
                self.search.toggle_hidden()?;
                self.search.update_query(&self.search_input);
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

    fn start_create_mode(&mut self) {
        self.create_active = true;
        self.create_input.clear();
        self.create_base = self.current_base_dir();
    }

    fn stop_create_mode(&mut self) {
        self.create_active = false;
        self.create_input.clear();
    }

    fn handle_create_key(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Result<()> {
        match code {
            KeyCode::Esc => {
                self.stop_create_mode();
            }
            KeyCode::Enter => {
                self.apply_create_input();
                self.stop_create_mode();
            }
            KeyCode::Backspace => {
                self.create_input.pop();
            }
            KeyCode::Char(c) => {
                if !modifiers.contains(KeyModifiers::CONTROL) {
                    self.create_input.push(c);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn apply_create_input(&mut self) {
        let raw = self.create_input.trim();
        if raw.is_empty() {
            return;
        }

        let is_dir = raw.ends_with('/') || raw.ends_with(std::path::MAIN_SEPARATOR);
        let trimmed = raw
            .trim_end_matches('/')
            .trim_end_matches(std::path::MAIN_SEPARATOR);

        if trimmed.is_empty() {
            self.status_message = Some("Invalid path".to_string());
            return;
        }

        let input_path = PathBuf::from(trimmed);
        if input_path.is_absolute() {
            self.status_message = Some("Absolute paths are not allowed".to_string());
            return;
        }

        let base = self.create_base.clone();
        let combined = if input_path.starts_with(&base) {
            input_path
        } else {
            base.join(input_path)
        };

        let normalized = match normalize_relative(&combined) {
            Some(path) => path,
            None => {
                self.status_message = Some("Path escapes root".to_string());
                return;
            }
        };

        let target = self.search.root.join(&normalized);
        let existed = target.exists();

        let result = if is_dir {
            fs::create_dir_all(&target)
        } else {
            if let Some(parent) = target.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    self.status_message = Some(format!("Create failed: {}", e));
                    return;
                }
            }
            if existed {
                Ok(())
            } else {
                fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&target)
                    .map(|_| ())
            }
        };

        if let Err(e) = result {
            self.status_message = Some(format!("Create failed: {}", e));
            return;
        }

        self.refresh_search();
        let display = normalized.to_string_lossy();
        self.status_message = Some(if is_dir {
            format!("Created folder: {}", display)
        } else if existed {
            format!("File exists: {}", display)
        } else {
            format!("Created file: {}", display)
        });
    }

    fn current_base_dir(&self) -> PathBuf {
        if self.search.search_active {
            if let Some((path, _)) = self.search.match_path_at(self.selected_index) {
                return path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
            }
            return PathBuf::new();
        }

        if let Some(entry) = self.search.get_visible_entry(self.selected_index) {
            if entry.is_dir {
                entry.path.clone()
            } else {
                entry.path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
            }
        } else {
            PathBuf::new()
        }
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
                    self.file_changed_externally = false;
                }
                self.clear_watcher();
                self.mode = Mode::Search;
                self.editor = None;
                self.status_message = None;
            } else {
                editor.handle_input(code, modifiers);
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

}

fn normalize_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => {
                return None;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::Normal(part) => out.push(part),
        }
    }
    Some(out)
}
