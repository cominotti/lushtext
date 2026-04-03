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
    pub fn relative_display(&self) -> String {
        self.path
            .strip_prefix(self.workspace_root.as_path())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| self.path.display().to_string())
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
    /// Cycle to the next mode: All → Files → Commands → All.
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Files,
            Self::Files => Self::Commands,
            Self::Commands => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All ⇥",
            Self::Files => "Files ⇥",
            Self::Commands => "Commands ⇥",
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
    fn test_search_mode_labels() {
        assert_eq!(SearchMode::All.label(), "All ⇥");
        assert_eq!(SearchMode::Files.label(), "Files ⇥");
        assert_eq!(SearchMode::Commands.label(), "Commands ⇥");
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
