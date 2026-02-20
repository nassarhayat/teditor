use anyhow::Result;
use ignore::WalkBuilder;
use nucleo::{Config, Matcher, Utf32Str};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Clone)]
struct TreeNode {
    is_dir: bool,
    children: Vec<PathBuf>,
    children_loaded: bool,
}

pub struct FileSearch {
    pub root: PathBuf,
    files: Vec<PathBuf>,           // All files (flat, for search)
    expanded: HashSet<PathBuf>,    // Expanded directories
    matches: Vec<(usize, u32)>,    // Search matches (index into files, score)
    matcher: Matcher,
    pub search_active: bool,
    pub show_hidden: bool,         // Whether to show hidden files (default: true)
    pub indexing: bool,
    tree_nodes: HashMap<PathBuf, TreeNode>,
    tree_visible: Vec<Entry>,
}

impl FileSearch {
    #[allow(dead_code)]
    pub fn new(root: PathBuf) -> Result<Self> {
        let show_hidden = true; // Default: show hidden files
        let files = Self::collect_files(&root, show_hidden)?;
        let matches: Vec<(usize, u32)> = files.iter().enumerate().map(|(i, _)| (i, 0)).collect();

        // Start with root-level directories expanded
        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::new()); // Root is always "expanded"

        let mut search = Self {
            root,
            files,
            expanded,
            matches,
            matcher: Matcher::new(Config::DEFAULT),
            search_active: false,
            show_hidden,
            indexing: false,
            tree_nodes: HashMap::new(),
            tree_visible: Vec::new(),
        };
        search.init_tree_root()?;
        Ok(search)
    }

    pub fn new_deferred(root: PathBuf) -> Result<Self> {
        let show_hidden = true; // Default: show hidden files

        // Start with root-level directories expanded
        let mut expanded = HashSet::new();
        expanded.insert(PathBuf::new()); // Root is always "expanded"

        let mut search = Self {
            root,
            files: Vec::new(),
            expanded,
            matches: Vec::new(),
            matcher: Matcher::new(Config::DEFAULT),
            search_active: false,
            show_hidden,
            indexing: true,
            tree_nodes: HashMap::new(),
            tree_visible: Vec::new(),
        };
        search.init_tree_root()?;
        Ok(search)
    }

    pub fn toggle_hidden(&mut self) -> Result<()> {
        self.show_hidden = !self.show_hidden;
        self.refresh()
    }

    pub fn refresh(&mut self) -> Result<()> {
        let files = Self::collect_files(&self.root, self.show_hidden)?;
        self.files = files;
        self.matches = self.files.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        self.indexing = false;
        let _ = self.refresh_tree_for_expanded();
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

    pub(crate) fn collect_files(root: &PathBuf, show_hidden: bool) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

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
                if is_file {
                    files.push(relative.to_path_buf());
                }
            }
        }

        files.sort();
        Ok(files)
    }

    #[allow(dead_code)]
    pub(crate) fn collect_entries(
        root: &PathBuf,
        show_hidden: bool,
    ) -> Result<(Vec<PathBuf>, Vec<Entry>)> {
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

    pub fn apply_index(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.matches = self.files.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        self.indexing = false;
    }

    fn init_tree_root(&mut self) -> Result<()> {
        self.tree_nodes.clear();
        self.tree_nodes.insert(
            PathBuf::new(),
            TreeNode {
                is_dir: true,
                children: Vec::new(),
                children_loaded: false,
            },
        );
        let _ = self.reload_children(&PathBuf::new());
        self.rebuild_tree_visible();
        Ok(())
    }

    fn refresh_tree_for_expanded(&mut self) -> Result<()> {
        let _ = self.reload_children(&PathBuf::new());
        let expanded: Vec<PathBuf> = self.expanded.iter().cloned().collect();
        for path in expanded {
            if path.as_os_str().is_empty() {
                continue;
            }
            let _ = self.reload_children(&path);
        }
        self.rebuild_tree_visible();
        Ok(())
    }

    fn load_children(&mut self, path: &PathBuf) -> Result<()> {
        let needs_load = self
            .tree_nodes
            .get(path)
            .map(|node| !node.children_loaded)
            .unwrap_or(true);
        if needs_load {
            self.reload_children(path)?;
        }
        Ok(())
    }

    fn reload_children(&mut self, path: &PathBuf) -> Result<()> {
        let full_path = self.root.join(path);
        let mut children: Vec<(u8, String, PathBuf, bool)> = Vec::new();

        for entry in WalkBuilder::new(&full_path)
            .hidden(!self.show_hidden)
            .git_ignore(true)
            .git_exclude(true)
            .filter_entry(|e| e.file_name() != ".git")
            .max_depth(Some(1))
            .min_depth(Some(1))
            .build()
            .filter_map(|e| e.ok())
        {
            let Ok(relative) = entry.path().strip_prefix(&self.root) else {
                continue;
            };
            let rel_path = relative.to_path_buf();
            let is_dir = entry.file_type().map_or(false, |ft| ft.is_dir());
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let prefix = if is_dir { 0 } else { 1 };
            children.push((prefix, name, rel_path.clone(), is_dir));

            self.tree_nodes
                .entry(rel_path)
                .or_insert(TreeNode {
                    is_dir,
                    children: Vec::new(),
                    children_loaded: !is_dir,
                });
        }

        children.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

        let child_paths: Vec<PathBuf> = children.into_iter().map(|(_, _, path, _)| path).collect();

        self.tree_nodes
            .entry(path.clone())
            .or_insert(TreeNode {
                is_dir: true,
                children: Vec::new(),
                children_loaded: false,
            });

        if let Some(node) = self.tree_nodes.get_mut(path) {
            node.is_dir = true;
            node.children = child_paths;
            node.children_loaded = true;
        }

        Ok(())
    }

    fn rebuild_tree_visible(&mut self) {
        self.tree_visible.clear();
        let root = PathBuf::new();
        let Some(root_node) = self.tree_nodes.get(&root) else {
            return;
        };
        let root_children = root_node.children.clone();
        for child in root_children {
            self.walk_tree(&child, 0);
        }
    }

    fn walk_tree(&mut self, path: &PathBuf, depth: usize) {
        let Some(node) = self.tree_nodes.get(path) else {
            return;
        };
        let is_dir = node.is_dir;
        let children = node.children.clone();
        self.tree_visible.push(Entry {
            path: path.clone(),
            is_dir,
            depth,
        });
        if is_dir && self.expanded.contains(path) {
            for child in children {
                self.walk_tree(&child, depth + 1);
            }
        }
    }

    pub fn toggle_expanded(&mut self, path: &PathBuf) -> Result<()> {
        if self.expanded.contains(path) {
            self.expanded.remove(path);
        } else {
            self.expanded.insert(path.clone());
            let _ = self.load_children(path);
        }
        self.rebuild_tree_visible();
        Ok(())
    }

    pub fn is_expanded(&self, path: &PathBuf) -> bool {
        self.expanded.contains(path)
    }

    pub fn visible_len(&self) -> usize {
        self.tree_visible.len()
    }

    pub fn visible_entry_at(&self, index: usize) -> Option<&Entry> {
        self.tree_visible.get(index)
    }

    pub fn update_query(&mut self, query: &str) {
        self.search_active = !query.is_empty();

        if query.is_empty() {
            self.matches = self.files.iter().enumerate().map(|(i, _)| (i, 0)).collect();
            return;
        }

        let mut scored: Vec<(usize, u32)> = Vec::new();
        let mut buf = Vec::new();
        let mut query_buf = Vec::new();
        let needle = Utf32Str::new(query, &mut query_buf);

        for (idx, path) in self.files.iter().enumerate() {
            let path_str = path.to_string_lossy();
            let haystack = Utf32Str::new(&path_str, &mut buf);

            if let Some(score) = self.matcher.fuzzy_match(haystack, needle) {
                scored.push((idx, score as u32));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        self.matches = scored;
    }

    pub fn match_count(&self) -> usize {
        if self.search_active {
            self.matches.len()
        } else {
            self.tree_visible.len()
        }
    }

    pub fn get_visible_entry(&self, index: usize) -> Option<&Entry> {
        self.visible_entry_at(index)
    }

    pub fn match_path_at(&self, index: usize) -> Option<(&PathBuf, u32)> {
        self.matches
            .get(index)
            .and_then(|(idx, score)| self.files.get(*idx).map(|p| (p, *score)))
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
