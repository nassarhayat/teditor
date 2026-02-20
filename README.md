# teditor

A minimal, fast TUI file browser and text editor with fuzzy search and syntax highlighting. Built in Rust.

## Features

- **Fuzzy file search** - type to filter files instantly using nucleo (same fuzzy matcher as Helix editor)
- **Quick navigation** - arrow keys to scroll through matches
- **Built-in editor** - edit code and markdown directly in the terminal
- **Syntax highlighting** - powered by syntect (same engine as bat/delta)
- **Respects .gitignore** - automatically hides ignored files
- **Hidden files toggle** - show/hide dotfiles with `Tab`
- **File watching** - detects external changes with option to reload

## Installation

### From source

```bash
git clone https://github.com/nassarhayat/teditor.git
cd teditor
cargo build --release

# Optionally, copy to your PATH
cp target/release/teditor ~/.local/bin/
```

### Using Cargo

```bash
cargo install --git https://github.com/nassarhayat/teditor.git
```

## Usage

```bash
# Run in current directory
teditor

# Or specify a path
teditor /path/to/project
```

### Keybindings

**Search Mode:**
| Key | Action |
|-----|--------|
| `Type` | Filter files |
| `↑/↓` | Navigate results |
| `Enter` | Open file / expand folder |
| `Ctrl+N` | Create file/folder |
| `Tab` | Toggle hidden files |
| `Esc` | Quit |

**Edit Mode:**
| Key | Action |
|-----|--------|
| `Ctrl+S` | Save file |
| `Ctrl+R` | Reload file (if changed externally) |
| `Ctrl+Q` | Quit without saving |
| `Esc` | Back to search (auto-saves if modified) |
| Arrows, Home, End | Standard text navigation |

## Architecture

```
src/
├── main.rs          # Entry point, CLI args
├── app.rs           # App state machine (Search ↔ Edit modes)
├── search.rs        # File walking + fuzzy matching
├── editor.rs        # Editor state, file I/O, modifications
└── ui/
    ├── mod.rs
    ├── search_view.rs   # Search input + file list
    └── editor_view.rs   # Text editor + syntax highlighting
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend |
| `nucleo` | Fuzzy matching (from Helix) |
| `ignore` | .gitignore-aware file walking |
| `tui-textarea` | Text editor widget |
| `syntect` | Syntax highlighting |
| `notify` | File system watching |

## License

MIT