// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette domain types — pure Rust, no GTK dependencies.

use std::path::PathBuf;
use std::sync::Arc;

use super::bookmark::BookmarkRecord;

/// Canonical filesystem identity used only for palette deduplication.
///
/// Display and activation continue to use the caller's original path. An
/// unavailable identity remains explicit so callers never mistake a raw path
/// for a canonical one after metadata resolution fails.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaletteFileIdentity {
    /// Canonical path resolved through the filesystem boundary.
    Canonical(PathBuf),
    /// Canonical identity could not be resolved for this source snapshot.
    Unavailable(PaletteFileIdentityFailure),
}

impl PaletteFileIdentity {
    /// Build a resolved canonical identity.
    #[must_use]
    pub fn canonical(path: PathBuf) -> Self {
        Self::Canonical(path)
    }

    /// Return the canonical path when identity resolution succeeded.
    #[must_use]
    pub fn canonical_path(&self) -> Option<&std::path::Path> {
        match self {
            Self::Canonical(path) => Some(path),
            Self::Unavailable(_) => None,
        }
    }
}

/// Stable, content-free classification for palette identity failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteFileIdentityFailure {
    NotResolved,
    NotFound,
    PermissionDenied,
    InvalidInput,
    Other,
}

impl From<std::io::ErrorKind> for PaletteFileIdentityFailure {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::InvalidInput => Self::InvalidInput,
            _ => Self::Other,
        }
    }
}

/// A file entry in the palette's search index.
#[derive(Debug, Clone)]
pub struct IndexedFile {
    /// Absolute path to the file on disk.
    pub path: PathBuf,
    /// Canonical identity captured during background index construction.
    pub identity: PaletteFileIdentity,
    /// File name component (pre-extracted for fast matching).
    pub name: String,
    /// The workspace folder that contains this file.
    /// Shared via `Arc` to avoid cloning the full path per file —
    /// a workspace with 50k files saves ~2.4MB (50k × 48 bytes/PathBuf).
    pub workspace_folder: Arc<PathBuf>,
}

impl IndexedFile {
    /// Create an indexed file, deriving the name from the path's last component.
    #[must_use]
    pub fn new(
        path: PathBuf,
        identity: PaletteFileIdentity,
        workspace_folder: Arc<PathBuf>,
    ) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            identity,
            name,
            workspace_folder,
        }
    }

    /// Path relative to the workspace folder, for display purposes.
    #[must_use]
    pub fn relative_display(&self) -> String {
        self.path
            .strip_prefix(self.workspace_folder.as_path())
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
    /// Canonical identity already known by the owning editor.
    pub identity: PaletteFileIdentity,
}

impl PaletteFileEntry {
    /// Build a file-like palette entry from already prepared display fields.
    #[must_use]
    pub fn new(
        display_name: String,
        subtitle: String,
        path: PathBuf,
        identity: PaletteFileIdentity,
    ) -> Self {
        Self {
            display_name,
            subtitle,
            path,
            identity,
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
    /// High-level command family shown in result subtitles.
    pub category: CommandCategory,
    /// Keyboard shortcut hint, e.g. `"Ctrl+S"`.
    pub shortcut: Option<&'static str>,
}

impl CommandDef {
    /// Build the subtitle shown for a command result.
    ///
    /// The subtitle combines the command's display category with its optional
    /// shortcut hint so all palette modes present command metadata consistently.
    #[must_use]
    pub fn display_subtitle(&self) -> String {
        match self.shortcut {
            Some(s) => format!("{} · {}", self.category.label(), s),
            None => self.category.label().to_string(),
        }
    }
}

/// Display categories carried by command definitions.
///
/// Categories stay in the model because they are pure row/subtitle metadata.
/// Workflow-specific membership, such as Notes-mode sections, belongs in the
/// palette service where command registry policy already lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    File,
    Edit,
    View,
    Notes,
    App,
}

impl CommandCategory {
    /// Human-readable label used in command subtitles.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Notes => "Notes",
            Self::App => "App",
        }
    }
}

/// The palette's shared launcher-mode vocabulary.
///
/// `Notes` searches persisted note and bookmark rows. Note-related actions
/// remain available through `Commands` mode so commands do not duplicate the
/// first-class note record source.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    #[default]
    All,
    Files,
    Notes,
    Commands,
}

impl SearchMode {
    /// All search modes in selector order.
    pub const ALL: [Self; 4] = [Self::All, Self::Files, Self::Notes, Self::Commands];

    /// Cycle to the next mode: All → Files → Notes → Commands → All.
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Files,
            Self::Files => Self::Notes,
            Self::Notes => Self::Commands,
            Self::Commands => Self::All,
        }
    }

    /// Cycle to the previous mode: All → Commands → Notes → Files → All.
    #[must_use]
    pub fn previous(self) -> Self {
        match self {
            Self::All => Self::Commands,
            Self::Files => Self::All,
            Self::Notes => Self::Files,
            Self::Commands => Self::Notes,
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
            Self::Notes => 2,
            Self::Commands => 3,
        }
    }

    /// Mode labels in dropdown order.
    #[must_use]
    pub fn labels() -> &'static [&'static str] {
        &["All", "Files", "Notes", "Commands"]
    }

    /// Human-readable label used by palette controls and mode text.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Files => "Files",
            Self::Notes => "Notes",
            Self::Commands => "Commands",
        }
    }

    /// Placeholder text for the search entry when this mode is active.
    #[must_use]
    pub fn placeholder(self) -> &'static str {
        match self {
            Self::All => "Search files, notes, and commands (Tab to switch mode)",
            Self::Files => "Search files (Tab to switch mode)",
            Self::Notes => "Search note contents (Tab to switch mode)",
            Self::Commands => "Type a command (Tab to switch mode)",
        }
    }

    /// Stable lowercase name used by automation snapshots and target-state actions.
    #[must_use]
    pub fn stable_name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Files => "files",
            Self::Notes => "notes",
            Self::Commands => "commands",
        }
    }

    /// Parse the stable action/snapshot spelling for command-palette mode.
    #[must_use]
    pub fn from_stable_name(value: &str) -> Option<Self> {
        match value.trim() {
            "all" => Some(Self::All),
            "files" => Some(Self::Files),
            "notes" => Some(Self::Notes),
            "commands" => Some(Self::Commands),
            _ => None,
        }
    }
}

/// A single search result with its relevance score.
#[derive(Debug)]
pub struct ScoredResult<'a> {
    /// Borrowed result item returned from the searched source.
    pub item: SearchResultItem<'a>,
    /// Fuzzy-match score; higher scores sort earlier.
    pub score: u32,
    /// Original ordinal in the searched source, used as the deterministic tie-break.
    pub source_ordinal: usize,
}

/// The kind of item in a search result.
#[derive(Debug)]
pub enum SearchResultItem<'a> {
    /// File-backed tab already open in the editor.
    OpenFile(&'a PaletteFileEntry),
    /// File discovered through the current workspace file index.
    File(&'a IndexedFile),
    /// Static command registry entry.
    Command(&'a CommandDef),
    /// Cached bookmark or note record.
    Note(&'a PaletteNoteEntry),
}

/// Semantic group for note rows in the command palette and Notes browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaletteNoteCategory {
    /// Saved bookmarks attached to files in the current workspace scope.
    Bookmarks,
    /// Notes attached to configured workspace folders.
    FolderNotes,
    /// Document notes attached to files in the current workspace scope.
    DocumentNotes,
    /// Saved open-tab notes outside the current workspace scope.
    OpenTabs,
}

impl PaletteNoteCategory {
    /// Browser and Notes-mode category order.
    pub const ALL: [Self; 4] = [
        Self::Bookmarks,
        Self::FolderNotes,
        Self::DocumentNotes,
        Self::OpenTabs,
    ];

    /// Header label used in dedicated Notes surfaces.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Bookmarks => "Bookmarks",
            Self::FolderNotes => "Folder Notes",
            Self::DocumentNotes => "Document Notes",
            Self::OpenTabs => "Open Tabs",
        }
    }

    /// Header label used in mixed All mode.
    ///
    /// Open file-backed tabs already use "Open Tabs" there, so note rows from
    /// saved open tabs get a more explicit group label.
    #[must_use]
    pub fn all_mode_label(self) -> &'static str {
        match self {
            Self::OpenTabs => "Open Tab Notes",
            _ => self.label(),
        }
    }
}

/// Activation target carried by one note palette row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteNoteTarget {
    /// Jump to a saved file bookmark. The line is stored zero-based.
    Bookmark {
        path: PathBuf,
        line: u32,
        workspace_folders: Vec<PathBuf>,
    },
    /// Open the folder note attached to one configured workspace folder.
    FolderNote {
        workspace_name: String,
        folder: PathBuf,
    },
    /// Open the document note attached to one saved file.
    DocumentNote {
        path: PathBuf,
        workspace_folders: Vec<PathBuf>,
    },
}

/// Owned, GTK-free row emitted by grouped command-palette search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteSearchRow {
    Header {
        label: String,
    },
    File {
        display_name: String,
        subtitle: String,
        file_path: PathBuf,
    },
    Command {
        display_name: String,
        subtitle: String,
        action_id: String,
    },
    Note {
        display_name: String,
        subtitle: String,
        target: PaletteNoteTarget,
    },
}

/// One searchable note row shared by the Notes browser and command palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteNoteEntry {
    /// Semantic group that controls section ordering.
    pub category: PaletteNoteCategory,
    /// Primary row text.
    pub title: String,
    /// Secondary visible metadata such as workspace, folder, path, or line.
    pub subtitle: String,
    /// Optional first meaningful note-body line shown as row detail.
    pub detail: Option<String>,
    /// Stored note body used only for search and preview workflows.
    pub note_text: Option<String>,
    /// Action payload used when the row is activated.
    pub target: PaletteNoteTarget,
}

impl PaletteNoteEntry {
    /// Return heap bytes reachable through this row, excluding its slice shell.
    #[must_use]
    pub fn retained_heap_byte_weight(&self) -> u64 {
        retained_bytes(
            self.title
                .capacity()
                .saturating_add(self.subtitle.capacity())
                .saturating_add(self.detail.as_ref().map_or(0, String::capacity))
                .saturating_add(self.note_text.as_ref().map_or(0, String::capacity)),
        )
        .saturating_add(self.target.retained_heap_byte_weight())
    }

    /// Build the subtitle shown in compact palette rows.
    #[must_use]
    pub fn display_subtitle(&self) -> String {
        self.detail.as_ref().map_or_else(
            || self.subtitle.clone(),
            |detail| format!("{} · {detail}", self.subtitle),
        )
    }

    /// Return the stored note text, or an empty string for bookmark rows.
    #[must_use]
    pub fn note_text(&self) -> &str {
        self.note_text.as_deref().unwrap_or("")
    }
}

impl PaletteNoteTarget {
    /// Return every heap allocation retained by this activation payload.
    #[must_use]
    pub fn retained_heap_byte_weight(&self) -> u64 {
        match self {
            Self::Bookmark {
                path,
                workspace_folders,
                ..
            }
            | Self::DocumentNote {
                path,
                workspace_folders,
            } => retained_bytes(path.capacity())
                .saturating_add(retained_bytes(
                    workspace_folders
                        .capacity()
                        .saturating_mul(std::mem::size_of::<PathBuf>()),
                ))
                .saturating_add(workspace_folders.iter().fold(0u64, |total, folder| {
                    total.saturating_add(retained_bytes(folder.capacity()))
                })),
            Self::FolderNote {
                workspace_name,
                folder,
            } => retained_bytes(workspace_name.capacity().saturating_add(folder.capacity())),
        }
    }
}

/// Return the exact retained weight of a compact note-entry slice.
#[must_use]
pub fn palette_note_entries_retained_byte_weight(entries: &[PaletteNoteEntry]) -> u64 {
    retained_bytes(
        entries
            .len()
            .saturating_mul(std::mem::size_of::<PaletteNoteEntry>()),
    )
    .saturating_add(entries.iter().fold(0u64, |total, entry| {
        total.saturating_add(entry.retained_heap_byte_weight())
    }))
}

fn retained_bytes(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// Main-thread snapshot of one open editor's live note-related state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteOpenEditorNoteSnapshot {
    /// Saved file path shown in rows and used to resolve sidecar identity.
    pub path: PathBuf,
    /// Current live bookmark records from the editor projection.
    pub bookmarks: Vec<BookmarkRecord>,
    /// Supplemental source used only when the path is outside the current scope.
    pub open_tab_source: Option<PaletteOpenTabSource>,
}

/// Source metadata for a saved open tab outside the current workspace scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteOpenTabSource {
    /// Restored workspace that owns this path, when it is merely outside the active scope.
    pub workspace_name: Option<String>,
    /// Real restored workspace folder for Markdown context, never synthesized.
    pub workspace_folder: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indexed_file_relative_display() {
        let file = IndexedFile {
            path: "/home/user/project/src/main.rs".into(),
            identity: PaletteFileIdentity::canonical("/home/user/project/src/main.rs".into()),
            name: "main.rs".to_string(),
            workspace_folder: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "src/main.rs");
    }

    #[test]
    fn test_indexed_file_relative_display_fallback() {
        let file = IndexedFile {
            path: "/other/path/file.rs".into(),
            identity: PaletteFileIdentity::canonical("/other/path/file.rs".into()),
            name: "file.rs".to_string(),
            workspace_folder: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "/other/path/file.rs");
    }

    #[test]
    fn test_command_category_labels() {
        assert_eq!(CommandCategory::File.label(), "File");
        assert_eq!(CommandCategory::Edit.label(), "Edit");
        assert_eq!(CommandCategory::View.label(), "View");
        assert_eq!(CommandCategory::Notes.label(), "Notes");
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
            id: "win.open-document-note",
            label: "Open Document Note",
            category: CommandCategory::Notes,
            shortcut: None,
        };

        assert_eq!(with_shortcut.display_subtitle(), "File · Ctrl+S");
        assert_eq!(without_shortcut.display_subtitle(), "Notes");
    }

    #[test]
    fn test_search_mode_default_is_all() {
        assert_eq!(SearchMode::default(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_cycle() {
        assert_eq!(SearchMode::All.next(), SearchMode::Files);
        assert_eq!(SearchMode::Files.next(), SearchMode::Notes);
        assert_eq!(SearchMode::Notes.next(), SearchMode::Commands);
        assert_eq!(SearchMode::Commands.next(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_reverse_cycle() {
        assert_eq!(SearchMode::All.previous(), SearchMode::Commands);
        assert_eq!(SearchMode::Commands.previous(), SearchMode::Notes);
        assert_eq!(SearchMode::Notes.previous(), SearchMode::Files);
        assert_eq!(SearchMode::Files.previous(), SearchMode::All);
    }

    #[test]
    fn test_search_mode_selector_positions() {
        assert_eq!(SearchMode::from_position(0), SearchMode::All);
        assert_eq!(SearchMode::from_position(1), SearchMode::Files);
        assert_eq!(SearchMode::from_position(2), SearchMode::Notes);
        assert_eq!(SearchMode::from_position(3), SearchMode::Commands);
        assert_eq!(SearchMode::from_position(99), SearchMode::All);
        assert_eq!(SearchMode::All.position(), 0);
        assert_eq!(SearchMode::Files.position(), 1);
        assert_eq!(SearchMode::Notes.position(), 2);
        assert_eq!(SearchMode::Commands.position(), 3);
    }

    #[test]
    fn test_search_mode_labels() {
        assert_eq!(SearchMode::All.label(), "All");
        assert_eq!(SearchMode::Files.label(), "Files");
        assert_eq!(SearchMode::Notes.label(), "Notes");
        assert_eq!(SearchMode::Commands.label(), "Commands");
        assert_eq!(SearchMode::labels(), &["All", "Files", "Notes", "Commands"]);
    }

    #[test]
    fn test_search_mode_placeholders() {
        assert_eq!(
            SearchMode::All.placeholder(),
            "Search files, notes, and commands (Tab to switch mode)"
        );
        assert_eq!(
            SearchMode::Files.placeholder(),
            "Search files (Tab to switch mode)"
        );
        assert_eq!(
            SearchMode::Notes.placeholder(),
            "Search note contents (Tab to switch mode)"
        );
        assert_eq!(
            SearchMode::Commands.placeholder(),
            "Type a command (Tab to switch mode)"
        );
    }

    #[test]
    fn test_search_mode_stable_names_round_trip_exactly() {
        let cases = [
            (SearchMode::All, "all"),
            (SearchMode::Files, "files"),
            (SearchMode::Notes, "notes"),
            (SearchMode::Commands, "commands"),
        ];

        for (mode, stable_name) in cases {
            assert_eq!(mode.stable_name(), stable_name);
            assert_eq!(SearchMode::from_stable_name(stable_name), Some(mode));
            assert_eq!(
                SearchMode::from_stable_name(&format!("  {stable_name}\t")),
                Some(mode),
                "stable-name parsing should trim surrounding whitespace"
            );
        }

        assert_eq!(SearchMode::from_stable_name("All"), None);
        assert_eq!(SearchMode::from_stable_name("unknown"), None);
    }

    #[test]
    fn test_note_category_labels_are_stable_for_notes_and_all_modes() {
        let cases = [
            (PaletteNoteCategory::Bookmarks, "Bookmarks", "Bookmarks"),
            (
                PaletteNoteCategory::FolderNotes,
                "Folder Notes",
                "Folder Notes",
            ),
            (
                PaletteNoteCategory::DocumentNotes,
                "Document Notes",
                "Document Notes",
            ),
            (PaletteNoteCategory::OpenTabs, "Open Tabs", "Open Tab Notes"),
        ];

        assert_eq!(
            PaletteNoteCategory::ALL,
            [
                PaletteNoteCategory::Bookmarks,
                PaletteNoteCategory::FolderNotes,
                PaletteNoteCategory::DocumentNotes,
                PaletteNoteCategory::OpenTabs,
            ]
        );
        for (category, label, all_mode_label) in cases {
            assert_eq!(category.label(), label);
            assert_eq!(category.all_mode_label(), all_mode_label);
        }
    }

    #[test]
    fn test_note_entry_subtitle_and_search_text_preserve_detail_and_empty_bookmarks() {
        let document_note = PaletteNoteEntry {
            category: PaletteNoteCategory::DocumentNotes,
            title: "main.rs".to_string(),
            subtitle: "src/main.rs".to_string(),
            detail: Some("Refactor reminder".to_string()),
            note_text: Some("Remember the split adapter".to_string()),
            target: PaletteNoteTarget::DocumentNote {
                path: PathBuf::from("/workspace/src/main.rs"),
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        };
        let bookmark = PaletteNoteEntry {
            category: PaletteNoteCategory::Bookmarks,
            title: "Line 8".to_string(),
            subtitle: "src/main.rs:8".to_string(),
            detail: None,
            note_text: None,
            target: PaletteNoteTarget::Bookmark {
                path: PathBuf::from("/workspace/src/main.rs"),
                line: 7,
                workspace_folders: vec![PathBuf::from("/workspace")],
            },
        };

        assert_eq!(
            document_note.display_subtitle(),
            "src/main.rs · Refactor reminder"
        );
        assert_eq!(document_note.note_text(), "Remember the split adapter");
        assert_eq!(bookmark.display_subtitle(), "src/main.rs:8");
        assert_eq!(bookmark.note_text(), "");
    }

    #[test]
    fn test_indexed_file_at_workspace_folder_top_level() {
        let file = IndexedFile {
            path: "/home/user/project/Cargo.toml".into(),
            identity: PaletteFileIdentity::canonical("/home/user/project/Cargo.toml".into()),
            name: "Cargo.toml".to_string(),
            workspace_folder: Arc::new("/home/user/project".into()),
        };
        assert_eq!(file.relative_display(), "Cargo.toml");
    }
}
