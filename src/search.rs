use anyhow::Result;
use ignore::WalkBuilder;
use nucleo::{Config, Matcher, Utf32Str};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
}

pub struct FileSearch {
    pub root: PathBuf,
    files: Vec<PathBuf>,           // All files (flat, for search)
    entries: Vec<Entry>,           // All entries (files + dirs)
    expanded: HashSet<PathBuf>,    // Expanded directories
    matches: Vec<(usize, u32)>,    // Search matches (index into files, score)
    matcher: Matcher,
    pub search_active: bool,
    pub show_hidden: bool,         // Whether to show hidden files (default: true)
}

impl FileSearch {
    pub fn new(root: PathBuf) -> Result<Self> {
        let show_hidden = true; // Default: show hidden files
        let (files, entries) = Self::collect_entries(&root, show_hidden)?;
        let matches: Vec<(usize, u32)> = files.iter().enumerate().map(|(i, _)| (i, 0)).collect();

        // Start with root-level directories expanded
        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::new()); // Root is always "expanded"

        Ok(Self {
            root,
            files,
            entries,
            expanded,
            matches,
            matcher: Matcher::new(Config::DEFAULT),
            search_active: false,
            show_hidden,
        })
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.refresh()
    }

    pub fn refresh(&mut self) -> Result<()> {
        let (files, entries) = Self::collect_entries(&self.root, self.show_hidden)?;
        self.files = files;
        self.entries = entries;
        self.matches = self.files.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        Ok(())
    }

    fn sort_key(path: &PathBuf, is_dir: bool) -> Vec<(u8, String)> {
        let components: Vec<_> = path.components().collect();
        let mut key = Vec::new();

        for (i, comp) in components.iter().enumerate() {
            let name = comp.as_os_str().to_string_lossy().to_lowercase();
            let is_last = i == components.len() - 1;
            // Prefix: 0 for dirs (or non-last components which are always dirs), 1 for files
            let prefix = if is_last && !is_dir { 1 } else { 0 };
            key.push((prefix, name));
        }
        key
    }

    fn collect_entries(root: &PathBuf, show_hidden: bool) -> Result<(Vec<PathBuf>, Vec<Entry>)> {
        let mut files = Vec::new();
        let mut entries_map: std::collections::BTreeSet<(PathBuf, bool)> = std::collections::BTreeSet::new();

        for entry in WalkBuilder::new(root)
            .hidden(!show_hidden) // hidden(true) = skip hidden files
            .git_ignore(true)
            .git_exclude(true)
            .filter_entry(|e| e.file_name() != ".git")
            .build()
            .filter_map(|e| e.ok())
        {
            if let Ok(relative) = entry.path().strip_prefix(root) {
                if relative.as_os_str().is_empty() {
                    continue;
                }

                let is_file = entry.file_type().map_or(false, |ft| ft.is_file());
                let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());

                if is_file {
                    files.push(relative.to_path_buf());
                    entries_map.insert((relative.to_path_buf(), false));

                    // Add parent directories
                    let mut parent = relative.parent();
                    while let Some(p) = parent {
                        if !p.as_os_str().is_empty() {
                            entries_map.insert((p.to_path_buf(), true));
                        }
                        parent = p.parent();
                    }
                } else if is_dir {
                    entries_map.insert((relative.to_path_buf(), true));
                }
            }
        }

        // Convert to entries with depth
        let mut entries: Vec<Entry> = entries_map
            .into_iter()
            .map(|(path, is_dir)| {
                let depth = path.components().count().saturating_sub(1);
                Entry { path, is_dir, depth }
            })
            .collect();

        // Sort for proper tree display: DFS order with dirs before files at each level
        // Use a sort key that prefixes each component with 0 for dir, 1 for file
        entries.sort_by(|a, b| {
            let a_key = Self::sort_key(&a.path, a.is_dir);
            let b_key = Self::sort_key(&b.path, b.is_dir);
            a_key.cmp(&b_key)
        });

        files.sort();
        Ok((files, entries))
    }

    pub fn toggle_expanded(&mut self, path: &PathBuf) {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.clone());
        }
    }

    pub fn is_expanded(&self, path: &PathBuf) -> bool {
        self.expanded.contains(path)
    }

    fn is_visible(&self, entry: &Entry) -> bool {
        // An entry is visible if all its parent directories are expanded
        if let Some(parent) = entry.path.parent() {
            if parent.as_os_str().is_empty() {
                return true; // Root level is always visible
            }
            // Check if parent is expanded
            if !self.expanded.contains(&parent.to_path_buf()) {
                return false;
            }
            // Recursively check grandparents
            let parent_entry = Entry {
                path: parent.to_path_buf(),
                is_dir: true,
                depth: entry.depth.saturating_sub(1),
            };
            return self.is_visible(&parent_entry);
        }
        true
    }

    pub fn visible_entries(&self) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| self.is_visible(e))
            .collect()
    }

    pub fn update_query(&mut self, query: &str) {
        self.search_active = !query.is_empty();

        if query.is_empty() {
            self.matches = self.files.iter().enumerate().map(|(i, _)| (i, 0)).collect();
            return;
        }

        let mut scored: Vec<(usize, u32)> = Vec::new();
        let mut buf = Vec::new();

        for (idx, path) in self.files.iter().enumerate() {
            let path_str = path.to_string_lossy();
            let haystack = Utf32Str::new(&path_str, &mut buf);
            let mut query_buf = Vec::new();
            let needle = Utf32Str::new(query, &mut query_buf);

            if let Some(score) = self.matcher.fuzzy_match(haystack, needle) {
                scored.push((idx, score as u32));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        self.matches = scored;
    }

    pub fn all_matches(&self) -> Vec<(PathBuf, u32)> {
        self.matches
            .iter()
            .map(|(idx, score)| (self.root.join(&self.files[*idx]), *score))
            .collect()
    }

    pub fn match_count(&self) -> usize {
        if self.search_active {
            self.matches.len()
        } else {
            self.visible_entries().len()
        }
    }

    pub fn get_visible_entry(&self, index: usize) -> Option<&Entry> {
        self.visible_entries().get(index).copied()
    }

    pub fn get_match(&self, index: usize) -> Option<PathBuf> {
        if self.search_active {
            self.matches
                .get(index)
                .map(|(idx, _)| self.root.join(&self.files[*idx]))
        } else {
            self.get_visible_entry(index)
                .filter(|e| !e.is_dir)
                .map(|e| self.root.join(&e.path))
        }
    }
}
