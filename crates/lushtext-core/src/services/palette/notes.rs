// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free note row construction and search for command-palette consumers.
//!
//! The command palette and Browse Notes surface both need the same note taxonomy:
//! bookmarks, folder notes, document notes, and saved open-tab notes. This
//! service owns that source policy while GTK adapters decide how to render and
//! activate the rows.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::note::{RichNoteBody, note_preview_line};
use crate::model::palette::{
    PaletteNoteCategory, PaletteNoteEntry, PaletteNoteTarget, PaletteOpenEditorNoteSnapshot,
    PaletteOpenTabSource,
};
use crate::model::workspace::{WorkspaceConfig, WorkspaceScopeSnapshot};
use crate::services::fuzzy::FuzzyQuery;
use crate::services::recovery_metadata::RecoveryDiagnostic;
use crate::services::{bookmark_service, document_note_service, folder_note_service};

/// Complete palette note source plus diagnostics from partially recovered sidecars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteNoteSourceLoad {
    /// Rows safe to show in note search surfaces.
    pub entries: Vec<PaletteNoteEntry>,
    /// Recovery diagnostics for malformed or unreadable note/bookmark sidecars.
    pub diagnostics: Vec<RecoveryDiagnostic>,
}

/// Load all note rows covered by the current workspace scope.
///
/// # Errors
///
/// Returns an error only when a sidecar directory cannot be scanned or a
/// workspace folder identity cannot be resolved.
pub fn load_note_entries_for_scope(
    data_dir: &Path,
    scope_snapshot: &WorkspaceScopeSnapshot,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
) -> Result<PaletteNoteSourceLoad> {
    let visible_workspaces = scope_snapshot.visible_workspaces();
    let scope_folders = scope_snapshot.folder_paths();
    let folder_notes = if visible_workspaces.is_empty() {
        folder_note_service::FolderNoteListing {
            notes: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        folder_note_service::list_folder_notes_for_scope_recovering(
            data_dir,
            visible_workspaces,
            scope_snapshot.scope(),
        )?
    };
    let bookmark_listing = if scope_folders.is_empty() {
        bookmark_service::WorkspaceBookmarkListing {
            bookmarks: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        bookmark_service::list_workspace_bookmarks_recovering(data_dir, scope_folders)?
    };
    let live_bookmarks = open_editor_snapshots
        .iter()
        .filter(|snapshot| snapshot.open_tab_source.is_none())
        .map(|snapshot| PaletteOpenEditorNoteSnapshot {
            path: snapshot.path.clone(),
            bookmarks: snapshot.bookmarks.clone(),
            open_tab_source: None,
        })
        .collect();
    let bookmarks = merge_live_bookmark_snapshots(bookmark_listing.bookmarks, live_bookmarks);
    let document_notes = if scope_folders.is_empty() {
        document_note_service::WorkspaceDocumentNoteListing {
            notes: Vec::new(),
            diagnostics: Vec::new(),
        }
    } else {
        document_note_service::list_workspace_document_notes_recovering(data_dir, scope_folders)?
    };

    let mut diagnostics = Vec::new();
    diagnostics.extend(folder_notes.diagnostics);
    diagnostics.extend(bookmark_listing.diagnostics);
    diagnostics.extend(document_notes.diagnostics);
    let entries = build_note_entries(
        visible_workspaces,
        bookmarks,
        folder_notes.notes,
        document_notes.notes,
        open_editor_snapshots,
        data_dir,
    );

    Ok(PaletteNoteSourceLoad {
        entries,
        diagnostics,
    })
}

/// Merge bookmarks plus folder and document notes into one section-ordered row list.
#[must_use]
pub fn build_note_entries(
    visible_workspaces: &[WorkspaceConfig],
    bookmarks: Vec<bookmark_service::WorkspaceBookmark>,
    folder_notes: Vec<folder_note_service::ListedFolderNote>,
    document_notes: Vec<document_note_service::WorkspaceDocumentNote>,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
    data_dir: &Path,
) -> Vec<PaletteNoteEntry> {
    let mut bookmark_entries = Vec::new();
    let mut folder_note_entries = Vec::new();
    let mut document_entries = Vec::new();
    let mut scoped_document_ids = HashSet::new();

    for bookmark in bookmarks {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &bookmark.path) {
            remember_document_identity(&mut scoped_document_ids, &bookmark.path);
            let workspace_folder = workspace_folder_for_path(workspace, &bookmark.path)
                .unwrap_or_else(|| bookmark.path.clone());
            let source = PaletteNoteDocumentSource::Workspace {
                workspace_name: workspace.name.clone(),
                workspace_folder,
            };
            let bookmark_service::WorkspaceBookmark {
                path, line, label, ..
            } = bookmark;
            bookmark_entries.push(bookmark_entry(&source, path, line, label.as_deref()));
        }
    }

    folder_note_entries.extend(
        folder_notes
            .into_iter()
            .map(|note| folder_note_entry(note.workspace_name, note.folder, note.note)),
    );

    for note in document_notes {
        if let Some(workspace) = workspace_for_path(visible_workspaces, &note.path) {
            remember_document_identity(&mut scoped_document_ids, &note.path);
            let workspace_folder = workspace_folder_for_path(workspace, &note.path)
                .unwrap_or_else(|| note.path.clone());
            let source = PaletteNoteDocumentSource::Workspace {
                workspace_name: workspace.name.clone(),
                workspace_folder,
            };
            document_entries.push(document_note_entry(&source, note.path, note.note));
        }
    }

    let mut open_tab_entries =
        build_open_tab_note_entries(data_dir, open_editor_snapshots, &scoped_document_ids);

    sort_note_entries_by_label(&mut bookmark_entries);
    sort_note_entries_by_label(&mut document_entries);
    sort_note_entries_by_label(&mut open_tab_entries);

    let mut entries = Vec::new();
    entries.extend(bookmark_entries);
    entries.extend(folder_note_entries);
    entries.extend(document_entries);
    entries.extend(open_tab_entries);
    entries
}

/// Search prepared note rows by visible metadata and stored note body text.
#[must_use]
pub fn search_note_entries<'a>(
    entries: &'a [PaletteNoteEntry],
    query: &str,
    max: usize,
) -> Vec<&'a PaletteNoteEntry> {
    if max == 0 {
        return Vec::new();
    }
    let Some(text_query) = PaletteNoteTextQuery::new(query) else {
        return entries.iter().take(max).collect();
    };

    let mut fuzzy_query = FuzzyQuery::new(text_query.as_str());
    search_scored_note_entries(entries.iter(), &text_query, &mut fuzzy_query, max)
}

/// Search prepared note rows within one semantic Notes category.
#[must_use]
pub fn search_note_entries_in_category<'a>(
    entries: &'a [PaletteNoteEntry],
    category: PaletteNoteCategory,
    query: &str,
    max: usize,
) -> Vec<&'a PaletteNoteEntry> {
    if max == 0 {
        return Vec::new();
    }
    let Some(text_query) = PaletteNoteTextQuery::new(query) else {
        return entries
            .iter()
            .filter(|entry| entry.category == category)
            .take(max)
            .collect();
    };

    let mut fuzzy_query = FuzzyQuery::new(text_query.as_str());
    search_scored_note_entries(
        entries.iter().filter(|entry| entry.category == category),
        &text_query,
        &mut fuzzy_query,
        max,
    )
}

fn search_scored_note_entries<'a>(
    entries: impl Iterator<Item = &'a PaletteNoteEntry>,
    text_query: &PaletteNoteTextQuery,
    fuzzy_query: &mut FuzzyQuery,
    max: usize,
) -> Vec<&'a PaletteNoteEntry> {
    let mut results: Vec<_> = entries
        .filter_map(|entry| {
            note_entry_score(entry, text_query, fuzzy_query).map(|score| (entry, score))
        })
        .collect();
    results.sort_unstable_by_key(|(_, score)| std::cmp::Reverse(*score));
    results.iter().map(|(entry, _)| *entry).take(max).collect()
}

fn note_entry_score(
    entry: &PaletteNoteEntry,
    text_query: &PaletteNoteTextQuery,
    fuzzy_query: &mut FuzzyQuery,
) -> Option<u32> {
    [
        Some(entry.title.as_str()),
        Some(entry.subtitle.as_str()),
        entry.detail.as_deref(),
        entry.note_text.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|candidate| text_query.matches(candidate))
    .map(|candidate| fuzzy_query.score(candidate).unwrap_or(0))
    .max()
}

/// Case-insensitive full-text query used to decide note-row eligibility.
struct PaletteNoteTextQuery {
    /// Trimmed query text for nucleo score calculation after an exact match.
    text: String,
    /// Query represented as Unicode scalar values so note bodies do not need to
    /// allocate their own lowercased copies on every keystroke.
    needle: Vec<char>,
    /// Knuth-Morris-Pratt prefix table for streaming substring matching.
    prefix: Vec<usize>,
}

impl PaletteNoteTextQuery {
    /// Prepare one non-empty query for repeated row checks.
    #[must_use]
    fn new(query: &str) -> Option<Self> {
        let text = query.trim().to_string();
        let lower_text = text.to_lowercase();
        if lower_text.is_empty() {
            return None;
        }

        let needle: Vec<_> = lower_text.chars().collect();
        let prefix = Self::prefix_table(&needle);
        Some(Self {
            text,
            needle,
            prefix,
        })
    }

    /// Return the original trimmed query text for palette-style scoring.
    #[must_use]
    fn as_str(&self) -> &str {
        &self.text
    }

    /// Build the KMP prefix table once per query instead of once per note body.
    fn prefix_table(needle: &[char]) -> Vec<usize> {
        let mut prefix = vec![0; needle.len()];
        let mut matched = 0;
        for index in 1..needle.len() {
            while matched > 0 && needle[index] != needle[matched] {
                matched = prefix[matched - 1];
            }
            if needle[index] == needle[matched] {
                matched += 1;
                prefix[index] = matched;
            }
        }
        prefix
    }

    /// Match without allocating a lowercased copy of large note bodies.
    fn matches(&self, haystack: &str) -> bool {
        if haystack.is_empty() {
            return false;
        }

        let mut matched = 0;
        for character in haystack.chars().flat_map(char::to_lowercase) {
            while matched > 0 && character != self.needle[matched] {
                matched = self.prefix[matched - 1];
            }
            if character == self.needle[matched] {
                matched += 1;
                if matched == self.needle.len() {
                    return true;
                }
            }
        }
        false
    }
}

/// Return whether one path is inside any folder in the current browse scope.
#[must_use]
pub fn path_is_in_folders(path: &Path, folders: &[PathBuf]) -> bool {
    folders.iter().any(|folder| path.starts_with(folder))
}

/// Classify an out-of-scope saved open tab for note source metadata.
#[must_use]
pub fn open_tab_source_for_path(
    all_workspaces: &[WorkspaceConfig],
    path: &Path,
) -> PaletteOpenTabSource {
    let owning_workspace = workspace_for_path(all_workspaces, path);
    PaletteOpenTabSource {
        workspace_name: owning_workspace.map(|workspace| workspace.name.clone()),
        workspace_folder: owning_workspace
            .and_then(|workspace| workspace_folder_for_path(workspace, path)),
    }
}

/// Origin of a row that is attached to a saved document path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PaletteNoteDocumentSource {
    /// The row belongs to the currently browsed workspace scope.
    Workspace {
        /// User-visible workspace label.
        workspace_name: String,
        /// Workspace folder used for Markdown context and document-note actions.
        workspace_folder: PathBuf,
    },
    /// The row comes from a saved open tab outside the current scope.
    OpenTab(PaletteOpenTabSource),
}

impl PaletteOpenTabSource {
    /// User-facing source label for rows that come from a saved open tab.
    #[must_use]
    pub fn row_label(&self) -> String {
        match (&self.workspace_name, &self.workspace_folder) {
            (Some(workspace_name), Some(folder)) => {
                format!("Open tab · {workspace_name} · {}", folder.display())
            }
            (Some(workspace_name), None) => format!("Open tab · {workspace_name}"),
            (None, _) => "Open tab · Outside workspace".to_string(),
        }
    }
}

impl PaletteNoteDocumentSource {
    /// User-facing source label shown in row subtitles and preview metadata.
    #[must_use]
    fn row_label(&self) -> String {
        match self {
            Self::Workspace {
                workspace_name,
                workspace_folder,
            } => format!("{workspace_name} · {}", workspace_folder.display()),
            Self::OpenTab(source) => source.row_label(),
        }
    }

    /// Return whether this row belongs to the supplemental open-tab section.
    #[must_use]
    fn is_open_tab(&self) -> bool {
        matches!(self, Self::OpenTab(_))
    }

    /// Real workspace folders available for Markdown rendering and note actions.
    #[must_use]
    fn workspace_folders(&self) -> Vec<PathBuf> {
        match self {
            Self::Workspace {
                workspace_folder, ..
            } => vec![workspace_folder.clone()],
            Self::OpenTab(source) => source.workspace_folder.iter().cloned().collect(),
        }
    }
}

fn bookmark_entry(
    source: &PaletteNoteDocumentSource,
    path: PathBuf,
    line: u32,
    label: Option<&str>,
) -> PaletteNoteEntry {
    let category = if source.is_open_tab() {
        PaletteNoteCategory::OpenTabs
    } else {
        PaletteNoteCategory::Bookmarks
    };
    PaletteNoteEntry {
        category,
        title: format!("Bookmark · {}", bookmark_display_label(label, line)),
        subtitle: format!(
            "{} · {} · {}",
            source.row_label(),
            path.display(),
            format_line_label(line)
        ),
        detail: None,
        note_text: None,
        target: PaletteNoteTarget::Bookmark {
            path,
            line,
            workspace_folders: source.workspace_folders(),
        },
    }
}

fn folder_note_entry(
    workspace_name: String,
    folder: PathBuf,
    note: RichNoteBody,
) -> PaletteNoteEntry {
    PaletteNoteEntry {
        category: PaletteNoteCategory::FolderNotes,
        title: format!("Folder Note · {workspace_name}"),
        subtitle: format!("{workspace_name} · {}", folder.display()),
        detail: note_detail(&note.text),
        note_text: Some(note.text),
        target: PaletteNoteTarget::FolderNote {
            workspace_name,
            folder,
        },
    }
}

fn document_note_entry(
    source: &PaletteNoteDocumentSource,
    path: PathBuf,
    note: RichNoteBody,
) -> PaletteNoteEntry {
    let category = if source.is_open_tab() {
        PaletteNoteCategory::OpenTabs
    } else {
        PaletteNoteCategory::DocumentNotes
    };
    let file_name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let workspace_folders = source.workspace_folders();
    PaletteNoteEntry {
        category,
        title: format!("Document Note · {file_name}"),
        subtitle: format!("{} · {}", source.row_label(), path.display()),
        detail: note_detail(&note.text),
        note_text: Some(note.text),
        target: PaletteNoteTarget::DocumentNote {
            path,
            workspace_folders,
        },
    }
}

fn note_detail(text: &str) -> Option<String> {
    let preview = note_preview_line(text);
    (!preview.is_empty()).then_some(preview)
}

/// Add a resolved document identity to the defensive dedupe set when possible.
fn remember_document_identity(document_ids: &mut HashSet<String>, path: &Path) {
    if let Ok(identity) = bookmark_service::resolve_document_identity(path) {
        document_ids.insert(identity.sidecar_id);
    }
}

/// Build supplemental rows for saved open tabs outside the current workspace scope.
fn build_open_tab_note_entries(
    data_dir: &Path,
    open_editor_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
    scoped_document_ids: &HashSet<String>,
) -> Vec<PaletteNoteEntry> {
    let mut entries = Vec::new();
    for snapshot in open_editor_snapshots {
        let Some(open_tab_source) = snapshot.open_tab_source else {
            continue;
        };
        if bookmark_service::resolve_document_identity(&snapshot.path)
            .is_ok_and(|identity| scoped_document_ids.contains(&identity.sidecar_id))
        {
            continue;
        }

        let source = PaletteNoteDocumentSource::OpenTab(open_tab_source);
        entries.extend(snapshot.bookmarks.into_iter().map(|bookmark| {
            bookmark_entry(
                &source,
                snapshot.path.clone(),
                bookmark.line,
                bookmark.label.as_deref(),
            )
        }));

        if let Ok(Some(document)) = document_note_service::load_for_path(data_dir, &snapshot.path) {
            entries.push(document_note_entry(&source, snapshot.path, document.note));
        }
    }
    entries
}

/// Overlay sidecar bookmark rows with current open-editor rows for the same file.
fn merge_live_bookmark_snapshots(
    persisted: Vec<bookmark_service::WorkspaceBookmark>,
    live_snapshots: Vec<PaletteOpenEditorNoteSnapshot>,
) -> Vec<bookmark_service::WorkspaceBookmark> {
    if live_snapshots.is_empty() {
        return persisted;
    }

    let mut live_document_ids = HashSet::new();
    let mut live_rows = Vec::new();
    for snapshot in live_snapshots {
        let Ok(identity) = bookmark_service::resolve_document_identity(&snapshot.path) else {
            continue;
        };
        live_document_ids.insert(identity.sidecar_id);
        live_rows.extend(snapshot.bookmarks.into_iter().map(|bookmark| {
            bookmark_service::WorkspaceBookmark {
                path: snapshot.path.clone(),
                bookmark_id: bookmark.id,
                line: bookmark.line,
                label: bookmark.label,
            }
        }));
    }

    if live_document_ids.is_empty() {
        return persisted;
    }

    let mut live_path_cache = HashMap::new();
    let mut merged: Vec<_> = persisted
        .into_iter()
        .filter(|bookmark| {
            let is_live_document =
                *live_path_cache
                    .entry(bookmark.path.clone())
                    .or_insert_with(|| {
                        bookmark_service::resolve_document_identity(&bookmark.path)
                            .is_ok_and(|identity| live_document_ids.contains(&identity.sidecar_id))
                    });
            !is_live_document
        })
        .collect();
    merged.extend(live_rows);
    merged.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.bookmark_id.0.cmp(&right.bookmark_id.0))
    });
    merged
}

/// Keep non-folder note rows in their familiar title/subtitle order.
fn sort_note_entries_by_label(entries: &mut [PaletteNoteEntry]) {
    entries.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then_with(|| left.subtitle.cmp(&right.subtitle))
    });
}

/// Find the first configured workspace that owns one saved path.
fn workspace_for_path<'a>(
    workspaces: &'a [WorkspaceConfig],
    path: &Path,
) -> Option<&'a WorkspaceConfig> {
    workspaces
        .iter()
        .find(|workspace| workspace_folder_for_path(workspace, path).is_some())
}

/// Find the first configured folder in one workspace that owns a path.
fn workspace_folder_for_path(workspace: &WorkspaceConfig, path: &Path) -> Option<PathBuf> {
    workspace
        .folders
        .iter()
        .find(|folder| path.starts_with(folder.path()))
        .map(|folder| folder.path.clone())
}

/// Display one zero-based bookmark line in the 1-based form users expect.
#[must_use]
pub fn format_line_label(line: u32) -> String {
    format!("Line {}", line.saturating_add(1))
}

/// Return the bookmark's explicit label or its stable line fallback.
#[must_use]
pub fn bookmark_display_label(label: Option<&str>, line: u32) -> String {
    label
        .filter(|label| !label.trim().is_empty())
        .map_or_else(|| format_line_label(line), ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::bookmark::BookmarkRecord;
    use crate::model::workspace::{WorkspaceFolder, WorkspaceId, WorkspaceScope, WorkspacesFile};
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fixture::create_dir_all(parent);
        }
        fixture::write_text(path, contents);
    }

    fn workspace(id: &str, name: &str, folders: Vec<PathBuf>) -> WorkspaceConfig {
        WorkspaceConfig::with_folders(
            WorkspaceId::new(id),
            name,
            folders.into_iter().map(WorkspaceFolder::new).collect(),
        )
    }

    fn categories(entries: &[PaletteNoteEntry]) -> Vec<PaletteNoteCategory> {
        entries.iter().map(|entry| entry.category).collect()
    }

    fn test_note_entry(category: PaletteNoteCategory, title: &str, body: &str) -> PaletteNoteEntry {
        PaletteNoteEntry {
            category,
            title: title.to_string(),
            subtitle: "Core · /workspace".to_string(),
            detail: None,
            note_text: Some(body.to_string()),
            target: PaletteNoteTarget::FolderNote {
                workspace_name: "Core".to_string(),
                folder: PathBuf::from("/workspace"),
            },
        }
    }

    #[test]
    fn build_note_entries_returns_empty_for_empty_sources() {
        let dir = TempDir::new().expect("tempdir");

        let entries = build_note_entries(
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            dir.path(),
        );

        assert!(entries.is_empty());
    }

    #[test]
    fn build_note_entries_preserves_note_category_order() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("src/main.rs");
        write_file(&file, "fn main() {}\n");
        fixture::create_dir_all(&root);
        let workspaces = vec![workspace("ws", "Core", vec![root.clone()])];

        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 6,
                label: Some("Important bookmark".to_string()),
            }],
            vec![folder_note_service::ListedFolderNote {
                workspace_name: "Core".to_string(),
                folder: root,
                note: RichNoteBody::new("Folder mission"),
            }],
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Document rationale"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert_eq!(
            categories(&entries),
            vec![
                PaletteNoteCategory::Bookmarks,
                PaletteNoteCategory::FolderNotes,
                PaletteNoteCategory::DocumentNotes,
            ]
        );
        assert_eq!(entries[0].title, "Bookmark · Important bookmark");
        assert_eq!(entries[1].detail.as_deref(), Some("Folder mission"));
        assert_eq!(entries[2].detail.as_deref(), Some("Document rationale"));
    }

    #[test]
    fn search_note_entries_matches_metadata_and_note_bodies() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let folder = root.join("docs");
        let file = folder.join("guide.md");
        write_file(&file, "visible source text\n");
        let workspaces = vec![workspace("ws", "Product Docs", vec![root])];
        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 12,
                label: Some("Launch checklist".to_string()),
            }],
            vec![folder_note_service::ListedFolderNote {
                workspace_name: "Product Docs".to_string(),
                folder,
                note: RichNoteBody::new("Folder body has migration plan"),
            }],
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Document body has launch narrative"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert_eq!(
            search_note_entries(&entries, "launch checklist", 10).len(),
            1
        );
        assert_eq!(search_note_entries(&entries, "Line 13", 10).len(), 1);
        assert_eq!(search_note_entries(&entries, "Product Docs", 10).len(), 3);
        assert_eq!(search_note_entries(&entries, "docs/guide.md", 10).len(), 2);
        assert_eq!(search_note_entries(&entries, "migration plan", 10).len(), 1);
        assert_eq!(
            search_note_entries(&entries, "launch narrative", 10).len(),
            1
        );
    }

    #[test]
    fn palette_note_text_query_preserves_trimmed_query_and_prefix_table() {
        let query = PaletteNoteTextQuery::new("  Launch Plan  ").expect("query");
        let overlap_query = PaletteNoteTextQuery::new("ababaca").expect("overlap query");

        assert_eq!(query.as_str(), "Launch Plan");
        assert!(query.matches("the launch plan is ready"));
        assert!(overlap_query.matches("prefix abababaca suffix"));
        assert!(!overlap_query.matches("prefix ababaxyca suffix"));
        assert_eq!(
            PaletteNoteTextQuery::prefix_table(&"ababaca".chars().collect::<Vec<_>>()),
            vec![0, 0, 1, 2, 3, 0, 1]
        );
        assert_eq!(
            PaletteNoteTextQuery::prefix_table(&"ababb".chars().collect::<Vec<_>>()),
            vec![0, 0, 1, 2, 0]
        );
    }

    #[test]
    fn search_note_entries_in_category_limits_matches_to_that_category() {
        let entries = vec![
            test_note_entry(
                PaletteNoteCategory::FolderNotes,
                "Folder launch note",
                "shared body",
            ),
            test_note_entry(
                PaletteNoteCategory::DocumentNotes,
                "Document launch note",
                "shared body",
            ),
        ];

        let document_hits = search_note_entries_in_category(
            &entries,
            PaletteNoteCategory::DocumentNotes,
            "shared body",
            10,
        );
        assert_eq!(document_hits.len(), 1);
        assert_eq!(document_hits[0].title, "Document launch note");

        let default_folder_hits =
            search_note_entries_in_category(&entries, PaletteNoteCategory::FolderNotes, "", 10);
        assert_eq!(default_folder_hits.len(), 1);
        assert_eq!(default_folder_hits[0].title, "Folder launch note");
    }

    #[test]
    fn search_note_entries_does_not_match_bookmark_source_excerpt_text() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("source.md");
        write_file(&file, "needle-from-source-only\n");
        let workspaces = vec![workspace("ws", "Core", vec![root])];
        let entries = build_note_entries(
            &workspaces,
            vec![bookmark_service::WorkspaceBookmark {
                path: file,
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-a".to_string()),
                line: 0,
                label: Some("Bookmark label".to_string()),
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            dir.path(),
        );

        assert!(search_note_entries(&entries, "needle-from-source-only", 10).is_empty());
    }

    #[test]
    fn note_path_scope_and_open_tab_source_labels_are_exact() {
        let folder = PathBuf::from("/workspace/root");
        assert!(path_is_in_folders(
            Path::new("/workspace/root/docs/file.md"),
            std::slice::from_ref(&folder)
        ));
        assert!(!path_is_in_folders(
            Path::new("/workspace/rootish/docs/file.md"),
            std::slice::from_ref(&folder)
        ));

        let in_workspace = PaletteOpenTabSource {
            workspace_name: Some("Docs".to_string()),
            workspace_folder: Some(folder.clone()),
        };
        assert_eq!(
            in_workspace.row_label(),
            format!("Open tab · Docs · {}", folder.display())
        );
        assert_eq!(
            PaletteOpenTabSource {
                workspace_name: Some("Scratch".to_string()),
                workspace_folder: None,
            }
            .row_label(),
            "Open tab · Scratch"
        );
        assert_eq!(
            PaletteOpenTabSource {
                workspace_name: None,
                workspace_folder: None,
            }
            .row_label(),
            "Open tab · Outside workspace"
        );
    }

    #[test]
    fn document_identity_dedupe_and_label_sorting_helpers_preserve_note_rows() {
        let dir = TempDir::new().expect("tempdir");
        let file = dir.path().join("doc.md");
        write_file(&file, "# doc\n");
        let mut document_ids = HashSet::new();

        remember_document_identity(&mut document_ids, &file);
        remember_document_identity(&mut document_ids, &dir.path().join("missing.md"));

        assert_eq!(document_ids.len(), 1);

        let mut entries = vec![
            test_note_entry(PaletteNoteCategory::DocumentNotes, "Zeta", "body"),
            test_note_entry(PaletteNoteCategory::DocumentNotes, "Alpha", "body"),
            PaletteNoteEntry {
                subtitle: "A subtitle".to_string(),
                ..test_note_entry(PaletteNoteCategory::DocumentNotes, "Alpha", "body")
            },
        ];

        sort_note_entries_by_label(&mut entries);

        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.title.as_str(), entry.subtitle.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("Alpha", "A subtitle"),
                ("Alpha", "Core · /workspace"),
                ("Zeta", "Core · /workspace"),
            ]
        );
    }

    #[test]
    fn overlapping_workspace_folders_keep_user_order_for_note_context() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let nested = root.join("src");
        let file = nested.join("main.rs");
        write_file(&file, "fn main() {}\n");

        let parent_first = build_note_entries(
            &[workspace("ws", "Core", vec![root.clone(), nested.clone()])],
            Vec::new(),
            Vec::new(),
            vec![document_note_service::WorkspaceDocumentNote {
                path: file.clone(),
                note: RichNoteBody::new("Parent context"),
            }],
            Vec::new(),
            dir.path(),
        );
        let nested_first = build_note_entries(
            &[workspace("ws", "Core", vec![nested.clone(), root.clone()])],
            Vec::new(),
            Vec::new(),
            vec![document_note_service::WorkspaceDocumentNote {
                path: file,
                note: RichNoteBody::new("Nested context"),
            }],
            Vec::new(),
            dir.path(),
        );

        assert!(matches!(
            &parent_first[0].target,
            PaletteNoteTarget::DocumentNote { workspace_folders, .. }
                if workspace_folders.as_slice() == std::slice::from_ref(&root)
        ));
        assert!(matches!(
            &nested_first[0].target,
            PaletteNoteTarget::DocumentNote { workspace_folders, .. }
                if workspace_folders == &vec![nested]
        ));
    }

    #[test]
    fn open_tab_notes_are_supplemental_and_deduplicated_from_scoped_documents() {
        let dir = TempDir::new().expect("tempdir");
        let scoped_root = dir.path().join("scoped");
        let outside_root = dir.path().join("outside");
        let scoped_file = scoped_root.join("main.rs");
        let outside_file = outside_root.join("outside.md");
        write_file(&scoped_file, "scoped\n");
        write_file(&outside_file, "outside\n");
        let visible = vec![workspace("scoped", "Scoped", vec![scoped_root])];
        let all = vec![
            visible[0].clone(),
            workspace("outside", "Outside", vec![outside_root]),
        ];
        document_note_service::save_for_path(
            dir.path(),
            &outside_file,
            &RichNoteBody::new("Open tab document note"),
        )
        .expect("save outside note");

        let entries = build_note_entries(
            &visible,
            vec![bookmark_service::WorkspaceBookmark {
                path: scoped_file.clone(),
                bookmark_id: crate::model::bookmark::BookmarkId("bookmark-scoped".to_string()),
                line: 0,
                label: Some("Scoped bookmark".to_string()),
            }],
            Vec::new(),
            Vec::new(),
            vec![
                PaletteOpenEditorNoteSnapshot {
                    path: scoped_file,
                    bookmarks: vec![BookmarkRecord::new(1, Some("Live scoped".to_string()))],
                    open_tab_source: None,
                },
                PaletteOpenEditorNoteSnapshot {
                    path: outside_file.clone(),
                    bookmarks: vec![BookmarkRecord::new(3, Some("Outside tab".to_string()))],
                    open_tab_source: Some(open_tab_source_for_path(&all, &outside_file)),
                },
            ],
            dir.path(),
        );

        assert_eq!(
            categories(&entries),
            vec![
                PaletteNoteCategory::Bookmarks,
                PaletteNoteCategory::OpenTabs,
                PaletteNoteCategory::OpenTabs,
            ]
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Scoped bookmark")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Outside tab")
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.title == "Document Note · outside.md")
        );
    }

    #[test]
    fn load_note_entries_overlays_live_bookmarks_for_open_scoped_documents() {
        let dir = TempDir::new().expect("tempdir");
        let root = dir.path().join("workspace");
        let file = root.join("main.rs");
        write_file(&file, "fn main() {}\n");
        bookmark_service::save_for_path(
            dir.path(),
            &file,
            &[BookmarkRecord::new(
                0,
                Some("Persisted bookmark".to_string()),
            )],
        )
        .expect("save bookmark sidecar");
        let workspaces_file = WorkspacesFile {
            current_scope: WorkspaceScope::All,
            workspaces: vec![workspace("ws", "Core", vec![root])],
        };
        let scope_snapshot = workspaces_file.current_scope_snapshot();

        let load = load_note_entries_for_scope(
            dir.path(),
            &scope_snapshot,
            vec![PaletteOpenEditorNoteSnapshot {
                path: file,
                bookmarks: vec![BookmarkRecord::new(3, Some("Live bookmark".to_string()))],
                open_tab_source: None,
            }],
        )
        .expect("load notes");

        assert!(
            load.entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Live bookmark")
        );
        assert!(
            !load
                .entries
                .iter()
                .any(|entry| entry.title == "Bookmark · Persisted bookmark")
        );
    }
}
