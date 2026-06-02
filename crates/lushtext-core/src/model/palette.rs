// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette domain types — pure Rust, no GTK dependencies.

use std::path::PathBuf;
use std::sync::Arc;

/// A file entry in the palette's search index.
#[derive(Debug, Clone)]
pub struct IndexedFile {
    /// Absolute path to the file on disk.
    pub path: PathBuf,
    /// File name component (pre-extracted for fast matching).
    pub name: String,
    /// The workspace root directory that contains this file.
    /// Shared via `Arc` to avoid cloning the full path per file —
    /// a workspace with 50k files saves ~2.4MB (50k × 48 bytes/PathBuf).
    pub workspace_root: Arc<PathBuf>,
}

impl IndexedFile {
    /// Create an indexed file, deriving the name from the path's last component.
    #[must_use]
    pub fn new(path: PathBuf, workspace_root: Arc<PathBuf>) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            name,
            workspace_root,
        }
    }

    /// Path relative to the workspace root, for display purposes.
    #[must_use]
    pub fn relative_display(&self) -> String {
        self.path
            .strip_prefix(self.workspace_root.as_path())
            .map_or_else(
                |_| self.path.display().to_string(),
                |p| p.display().to_string(),
            )
    }
}

/// File-like palette entry that is not necessarily part of the workspace index.
///
/// Open file-backed tabs use this value object so the palette can search active
/// documents without pretending they belong to the current workspace index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteFileEntry {
    /// Primary row text shown in the palette.
    pub display_name: String,
    /// Secondary row text, usually a path used to disambiguate duplicates.
    pub subtitle: String,
    /// Absolute path opened when the row is activated.
    pub path: PathBuf,
}

impl PaletteFileEntry {
    /// Build a file-like palette entry from already prepared display fields.
    #[must_use]
    pub fn new(display_name: String, subtitle: String, path: PathBuf) -> Self {
        Self {
            display_name,
            subtitle,
            path,
        }
    }
}

/// A command that can be invoked from the palette.
#[derive(Debug, Clone)]
pub struct CommandDef {
    /// Full action identifier, e.g. `"win.save"` or `"app.preferences"`.
    pub id: &'static str,
    /// Human-readable label, e.g. `"Save File"`.
    pub label: &'static str,
    /// Category for grouping in results.
    pub category: CommandCategory,
    /// Keyboard shortcut hint, e.g. `"Ctrl+S"`.
    pub shortcut: Option<&'static str>,
}

impl CommandDef {
    #[must_use]
    pub fn display_subtitle(&self) -> String {
        match self.shortcut {
            Some(s) => format!("{} · {}", self.category.label(), s),
            None => self.category.label().to_string(),
        }
    }
}

/// Categories for organizing commands in the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    File,
    Edit,
    View,
    App,
}

impl CommandCategory {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::App => "App",
        }
    }
}

/// The palette's active search mode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    All,
    Files,
    Commands,
}

impl SearchMode {
    /// All search modes in selector order.
    pub const ALL: [Self; 3] = [Self::All, Self::Files, Self::Commands];

    /// Cycle to the next mode: All → Files → Commands → All.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Files,
            Self::Files => Self::Commands,
            Self::Commands => Self::All,
        }
    }

    /// Cycle to the previous mode: All → Commands → Files → All.
    #[must_use]
    pub fn previous(self) -> Self {
        match self {
            Self::All => Self::Commands,
            Self::Files => Self::All,
            Self::Commands => Self::Files,
        }
    }

    /// Convert a dropdown position into a search mode.
    #[must_use]
    pub fn from_position(position: u32) -> Self {
        usize::try_from(position)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or_default()
    }

    /// Return the dropdown position for this mode.
    #[must_use]
    pub fn position(self) -> u32 {
        match self {
            Self::All => 0,
            Self::Files => 1,
            Self::Commands => 2,
        }
    }

    /// Mode labels in dropdown order.
    #[must_use]
    pub fn labels() -> &'static [&'static str] {
        &["All", "Files", "Commands"]
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Files => "Files",
            Self::Commands => "Commands",
        }
    }

    /// Placeholder text for the search entry when this mode is active.
    #[must_use]
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::All => "Search files and commands (Tab to switch mode)",
            Self::Files => "Search files (Tab to switch mode)",
            Self::Commands => "Type a command (Tab to switch mode)",
        }
    }
}

/// A single search result with its relevance score.
#[derive(Debug)]
pub struct ScoredResult<'a> {
    pub item: SearchResultItem<'a>,
    pub score: u32,
}

/// The kind of item in a search result.
#[derive(Debug)]
pub enum SearchResultItem<'a> {
    OpenFile(&'a PaletteFileEntry),
    File(&'a IndexedFile),
    Command(&'a CommandDef),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexed_file_relative_display() {
        let file = IndexedFile {
            path: "/home/user/project/src/main.rs".into(),
            name: "main.rs".to_string(),
            workspace_root: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "src/main.rs");
    }

    #[test]
    fn test_indexed_file_relative_display_fallback() {
        let file = IndexedFile {
            path: "/other/path/file.rs".into(),
            name: "file.rs".to_string(),
            workspace_root: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "/other/path/file.rs");
    }

    #[test]
    fn test_command_category_labels() {
        assert_eq!(CommandCategory::File.label(), "File");
        assert_eq!(CommandCategory::Edit.label(), "Edit");
        assert_eq!(CommandCategory::View.label(), "View");
        assert_eq!(CommandCategory::App.label(), "App");
    }

    #[test]
    fn test_command_display_subtitle_includes_category_and_optional_shortcut() {
        let with_shortcut = CommandDef {
            id: "win.save",
            label: "Save",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+S"),
        };
        let without_shortcut = CommandDef {
            id: "app.preferences",
            label: "Preferences",
            category: CommandCategory::App,
            shortcut: None,
        };

        assert_eq!(with_shortcut.display_subtitle(), "File · Ctrl+S");
        assert_eq!(without_shortcut.display_subtitle(), "App");
    }

    #[test]
    fn test_search_mode_default_is_all() {
        assert_eq!(SearchMode::default(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_cycle() {
        assert_eq!(SearchMode::All.next(), SearchMode::Files);
        assert_eq!(SearchMode::Files.next(), SearchMode::Commands);
        assert_eq!(SearchMode::Commands.next(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_reverse_cycle() {
        assert_eq!(SearchMode::All.previous(), SearchMode::Commands);
        assert_eq!(SearchMode::Commands.previous(), SearchMode::Files);
        assert_eq!(SearchMode::Files.previous(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_selector_positions() {
        assert_eq!(SearchMode::from_position(0), SearchMode::All);
        assert_eq!(SearchMode::from_position(1), SearchMode::Files);
        assert_eq!(SearchMode::from_position(2), SearchMode::Commands);
        assert_eq!(SearchMode::from_position(99), SearchMode::All);
        assert_eq!(SearchMode::All.position(), 0);
        assert_eq!(SearchMode::Files.position(), 1);
        assert_eq!(SearchMode::Commands.position(), 2);
    }

    #[test]
    fn test_search_mode_labels() {
        assert_eq!(SearchMode::All.label(), "All");
        assert_eq!(SearchMode::Files.label(), "Files");
        assert_eq!(SearchMode::Commands.label(), "Commands");
        assert_eq!(SearchMode::labels(), &["All", "Files", "Commands"]);
    }

    #[test]
    fn test_search_mode_placeholders() {
        assert_eq!(
            SearchMode::All.placeholder(),
            "Search files and commands (Tab to switch mode)"
        );
        assert_eq!(
            SearchMode::Files.placeholder(),
            "Search files (Tab to switch mode)"
        );
        assert_eq!(
            SearchMode::Commands.placeholder(),
            "Type a command (Tab to switch mode)"
        );
    }

    #[test]
    fn test_indexed_file_at_root() {
        let file = IndexedFile {
            path: "/home/user/project/Cargo.toml".into(),
            name: "Cargo.toml".to_string(),
            workspace_root: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "Cargo.toml");
    }
}
