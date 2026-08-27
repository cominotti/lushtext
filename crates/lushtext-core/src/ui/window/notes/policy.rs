// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions owned by the notes and bookmarks workflow.
//!
//! This is the workflow's one pure policy module. It imports no GTK-family
//! crate, which is what keeps it inside the default `cargo-mutants`
//! `ui/**/policy.rs` scope. Every value here is a decision a reader can check
//! without a display server: which folder a folder-note action can target, how
//! many live-editor snapshots one note-source request may retain, what the
//! browser calls each inventory mode, and what the user is told when a bookmark
//! edit or preview fails.
//!
//! Policy constants are pinned to concrete literals in the units a reader would
//! sanity-check, and the tests assert against those literals rather than against
//! the constants they came from: an assertion comparing a value to the constant
//! it was computed from cannot detect the constant changing.

use std::path::PathBuf;

use crate::model::workspace::WorkspaceConfig;
use crate::services::bookmark_excerpt;
use crate::services::palette as palette_service;
use crate::services::palette::NotesBrowserMode;
use crate::ui::editor_page::BookmarkEditError;

/// Maximum note rows materialized into a browser at once.
///
/// 500 rows: the point past which an Adwaita sidebar rebuild stops being
/// imperceptible on a mid-range laptop. Beyond it the browser shows a
/// refine-your-search notice rather than growing the list.
pub const NOTES_BROWSER_RENDER_LIMIT: usize = 500;
/// Maximum rows admitted into one Browse Notes source.
pub const NOTES_BROWSER_SOURCE_ENTRY_LIMIT: usize = 10_000;
/// Maximum aggregate searchable UTF-8 bytes retained by Browse Notes.
pub const NOTES_BROWSER_SOURCE_TEXT_LIMIT: usize = 64 * 1024 * 1024;
/// Maximum sidecar candidates retained by each Browse Notes directory scan.
pub const NOTES_BROWSER_SIDECAR_SCAN_LIMIT: usize = 10_000;
/// Maximum recovery diagnostics retained by one Browse Notes load.
pub const NOTES_BROWSER_DIAGNOSTIC_LIMIT: usize = 1_024;
/// Maximum open-editor snapshots plus bookmark rows captured on GTK.
pub const NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT: usize = 10_000;
/// Maximum retained live-editor metadata cloned before note-source admission.
pub const NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT: u64 = 4 * 1024 * 1024;
/// Quiet window after a bookmark burst before one sidecar write is issued (ms).
pub const NOTES_SAVE_DEBOUNCE_MS: u64 = 200;
/// Consecutive failed bookmark sidecar writes before retry stops on its own.
///
/// 3: enough to ride out a transient `EINTR`, a brief lock contention, or one
/// full disk that the user immediately clears, and small enough that a genuinely
/// unwritable sidecar cannot pulse the status bar or churn the worker pool. Past
/// the cap the write stays outstanding — the dirty flag is left set — so the
/// user's next bookmark edit retries.
pub const MAX_BOOKMARK_SAVE_ATTEMPTS: u32 = 3;
/// Quiet window before a note dialog re-evaluates its Save sensitivity (ms).
pub const NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS: u64 = 80;
/// Quiet window coalescing command-palette note-source reloads (ms).
pub const COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS: u64 = 150;
/// Quiet window after a keystroke before the browser re-queries its source (ms).
///
/// Deliberately its own decision rather than a reuse of the palette's window:
/// the browser queries an already-published in-memory source, so this number can
/// move without touching palette indexing.
pub const NOTES_BROWSER_SEARCH_DEBOUNCE_MS: u64 = 150;

/// Browser-owned source policy passed into the shared admission engine.
pub const NOTES_BROWSER_SOURCE_LIMITS: palette_service::NoteSourceLimits =
    palette_service::NoteSourceLimits {
        entries: NOTES_BROWSER_SOURCE_ENTRY_LIMIT,
        searchable_text_bytes: NOTES_BROWSER_SOURCE_TEXT_LIMIT,
        retained_bytes: palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES,
        sidecar_entries: NOTES_BROWSER_SIDECAR_SCAN_LIMIT,
        sidecar_path_bytes: palette_service::MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES,
        construction_bytes: palette_service::MAX_PALETTE_NOTE_CONSTRUCTION_BYTES,
        diagnostics: NOTES_BROWSER_DIAGNOSTIC_LIMIT,
    };

/// Narrow the browser source policy to a smaller admitted entry count.
///
/// The sidecar scan ceiling follows the entry ceiling down, because scanning more
/// sidecars than the source can admit only wastes I/O.
///
/// Production calls this with [`NOTES_BROWSER_SOURCE_ENTRY_LIMIT`], where the
/// narrowing is a no-op, so both feature configurations take the same code path.
/// That is deliberate: gating it to `test-utils` would leave production compiling
/// a different branch from the one the tests exercise.
#[must_use]
pub fn notes_browser_source_limits_for_entries(
    entries: usize,
) -> palette_service::NoteSourceLimits {
    palette_service::NoteSourceLimits {
        entries,
        sidecar_entries: NOTES_BROWSER_SOURCE_LIMITS.sidecar_entries.min(entries),
        ..NOTES_BROWSER_SOURCE_LIMITS
    }
}

/// Decision for `Open Folder Note...` when the caller has not supplied an exact folder row.
///
/// Folder notes are attached to folders, not workspaces. Naming this decision
/// keeps the zero/one/many rules explicit so command actions and workspace
/// header actions cannot quietly fall back to the first configured folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderNoteOpenTarget {
    /// The current shared scope is `All workspaces`, so no single folder can be inferred.
    AggregateScope,
    /// A concrete workspace ID was requested but no restored workspace matched it.
    WorkspaceMissing,
    /// The concrete workspace exists but has no folders to attach a note to.
    EmptyWorkspace { workspace_name: String },
    /// The concrete workspace has exactly one folder and can open directly.
    SingleFolder {
        workspace_name: String,
        folder: PathBuf,
    },
    /// The concrete workspace has multiple folders and needs a visible choice.
    ChooseFolder {
        workspace_name: String,
        folders: Vec<PathBuf>,
    },
}

/// Decide what a folder-note action can do for one concrete restored workspace.
#[must_use]
pub fn folder_note_target_for_workspace(workspace: WorkspaceConfig) -> FolderNoteOpenTarget {
    let workspace_name = workspace.name;
    let folders = workspace
        .folders
        .into_iter()
        .map(|folder| folder.path().to_path_buf())
        .collect::<Vec<_>>();

    match folders.as_slice() {
        [] => FolderNoteOpenTarget::EmptyWorkspace { workspace_name },
        [folder] => FolderNoteOpenTarget::SingleFolder {
            workspace_name,
            folder: folder.clone(),
        },
        _ => FolderNoteOpenTarget::ChooseFolder {
            workspace_name,
            folders,
        },
    }
}

/// Return whether a folder-note action can start immediately from this target.
#[must_use]
pub fn folder_note_action_available(target: &FolderNoteOpenTarget) -> bool {
    matches!(
        target,
        FolderNoteOpenTarget::SingleFolder { .. } | FolderNoteOpenTarget::ChooseFolder { .. }
    )
}

/// Return the Notes-menu bookmark row label for the active cursor context.
#[must_use]
pub fn bookmark_menu_label(cursor_line_has_bookmark: bool) -> &'static str {
    if cursor_line_has_bookmark {
        "Remove Bookmark"
    } else {
        "Add Bookmark"
    }
}

/// Heap bytes one live-editor note snapshot retains beyond its fixed struct size.
#[must_use]
pub fn open_editor_snapshot_heap_bytes(
    path_capacity: usize,
    workspace_name_capacity: Option<usize>,
    workspace_folder_capacity: Option<usize>,
) -> u64 {
    let bytes = path_capacity.saturating_add(
        workspace_name_capacity
            .unwrap_or(0)
            .saturating_add(workspace_folder_capacity.unwrap_or(0)),
    );
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// Snapshot vector capacity admitted for one bounded live-editor capture.
///
/// Three independent ceilings apply and the smallest wins: the caller's combined
/// snapshot-and-bookmark row limit, the number of open tabs that actually exist,
/// and how many fixed-size snapshots fit inside the retained-byte budget.
#[must_use]
pub fn open_editor_snapshot_capacity(
    max_snapshots_and_bookmarks: usize,
    open_page_count: usize,
    max_retained_bytes: u64,
    snapshot_size: usize,
) -> usize {
    let byte_limited = usize::try_from(
        max_retained_bytes / u64::try_from(snapshot_size.max(1)).unwrap_or(u64::MAX),
    )
    .unwrap_or(usize::MAX);
    max_snapshots_and_bookmarks
        .min(open_page_count)
        .min(byte_limited)
}

/// Bytes already reserved once a capture has admitted `capacity` fixed-size snapshots.
#[must_use]
pub fn open_editor_snapshot_reserved_bytes(capacity: usize, snapshot_size: usize) -> u64 {
    u64::try_from(capacity.saturating_mul(snapshot_size)).unwrap_or(u64::MAX)
}

/// User-facing vocabulary for one notes-browser inventory mode.
///
/// Fourteen strings, one decision each. They live here rather than beside the
/// dialog because the browser and the command palette must agree on them, and
/// because a mode that gained a third variant would otherwise silently reuse
/// another mode's copy.
pub trait NotesBrowserModeExt {
    fn title(self) -> &'static str;
    fn loading_label(self) -> &'static str;
    fn deferred_label(self) -> &'static str;
    fn search_placeholder(self) -> &'static str;
    fn search_accessible_label(self) -> &'static str;
    fn search_description(self) -> &'static str;
    fn results_accessible_label(self) -> &'static str;
    fn results_description(self) -> &'static str;
    fn empty_source_label(self) -> &'static str;
    fn no_matches_label(self) -> &'static str;
    fn unselected_title(self) -> &'static str;
    fn unselected_meta(self) -> &'static str;
    fn unselected_placeholder(self) -> &'static str;
    fn source_limit_message(self) -> &'static str;
    fn source_limit_status_message(self) -> &'static str;
    fn source_recovery_status_message(self) -> &'static str;
    fn source_failure_message(self) -> &'static str;
    fn open_action_label(self) -> &'static str;
    fn unselected_value_text(self) -> &'static str;
}

impl NotesBrowserModeExt for NotesBrowserMode {
    fn title(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes",
            Self::Bookmarks => "Bookmarks",
        }
    }

    fn loading_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Loading notes…",
            Self::Bookmarks => "Loading bookmarks…",
        }
    }

    fn deferred_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes deferred by memory pressure",
            Self::Bookmarks => "Bookmarks deferred by memory pressure",
        }
    }

    fn search_placeholder(self) -> &'static str {
        match self {
            Self::AllNotes => "Search Notes…",
            Self::Bookmarks => "Search Bookmarks…",
        }
    }

    fn search_accessible_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Search notes",
            Self::Bookmarks => "Search bookmarks",
        }
    }

    fn search_description(self) -> &'static str {
        match self {
            Self::AllNotes => "Filter bookmarks, document notes, and folder notes",
            Self::Bookmarks => "Filter bookmarks in the current workspace",
        }
    }

    fn results_accessible_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes results",
            Self::Bookmarks => "Bookmark results",
        }
    }

    fn results_description(self) -> &'static str {
        match self {
            Self::AllNotes => "Choose a bookmark, document note, or folder note",
            Self::Bookmarks => "Choose a bookmark to preview or open",
        }
    }

    fn empty_source_label(self) -> &'static str {
        match self {
            Self::AllNotes => "No notes yet",
            Self::Bookmarks => "No bookmarks exist in the current workspace",
        }
    }

    fn no_matches_label(self) -> &'static str {
        match self {
            Self::AllNotes => "No notes match that search",
            Self::Bookmarks => "No bookmarks match that search",
        }
    }

    fn unselected_title(self) -> &'static str {
        match self {
            Self::AllNotes => "Select a note",
            Self::Bookmarks => "Select a bookmark",
        }
    }

    fn unselected_meta(self) -> &'static str {
        match self {
            Self::AllNotes => {
                "Choose a bookmark, folder note, or document note to preview it here."
            }
            Self::Bookmarks => "Choose a bookmark to preview its source context here.",
        }
    }

    fn unselected_placeholder(self) -> &'static str {
        match self {
            Self::AllNotes => "Select a note to preview its details.",
            Self::Bookmarks => "Select a bookmark to preview its source context.",
        }
    }

    fn source_limit_message(self) -> &'static str {
        match self {
            Self::AllNotes => {
                "Some later notes were omitted because the source reached its safety limits."
            }
            Self::Bookmarks => {
                "Some later bookmarks were omitted because the source reached its safety limits."
            }
        }
    }

    fn source_limit_status_message(self) -> &'static str {
        match self {
            Self::AllNotes => "The Notes source was limited to stay responsive",
            Self::Bookmarks => "The bookmark source was limited to stay responsive",
        }
    }

    fn source_recovery_status_message(self) -> &'static str {
        match self {
            Self::AllNotes => "Some note data could not be loaded",
            Self::Bookmarks => "Some bookmark data could not be loaded",
        }
    }

    fn source_failure_message(self) -> &'static str {
        match self {
            Self::AllNotes => "Notes could not be listed",
            Self::Bookmarks => "Bookmarks could not be listed",
        }
    }

    fn open_action_label(self) -> &'static str {
        match self {
            Self::AllNotes => "Open selected note",
            Self::Bookmarks => "Open selected bookmark",
        }
    }

    fn unselected_value_text(self) -> &'static str {
        match self {
            Self::AllNotes => "No note selected",
            Self::Bookmarks => "No bookmark selected",
        }
    }
}

/// Compose the browser's truncation notice from its two independent causes.
///
/// Source omission and render capping are different facts and can both hold, so
/// the notice is built from a list rather than from nested conditionals; an empty
/// list means the label is hidden.
#[must_use]
pub fn notes_browser_limit_messages(
    mode: NotesBrowserMode,
    source_truncated: bool,
    render_truncated: bool,
    render_limit: usize,
) -> Vec<String> {
    let mut messages = Vec::new();
    if source_truncated {
        messages.push(mode.source_limit_message().to_string());
    }
    if render_truncated {
        messages.push(format!(
            "Showing first {render_limit} matches. Refine search to narrow results."
        ));
    }
    messages
}

/// Parse only the syntax of a user-facing 1-based bookmark line.
///
/// Range and collision checks stay in the editor layer so failed edits leave the
/// live bookmark projection unchanged.
pub fn parse_bookmark_target_line(text: &str) -> Result<u32, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Enter a line number.".to_string());
    }

    trimmed
        .parse::<u32>()
        .map_err(|_| "Line must be a whole number.".to_string())
}

/// Convert editor validation failures into dialog feedback.
#[must_use]
pub fn bookmark_edit_error_message(error: &BookmarkEditError) -> String {
    match error {
        BookmarkEditError::NotFound => "That bookmark is no longer available.".to_string(),
        BookmarkEditError::LineOutOfRange {
            requested_line,
            max_line,
        } => format!("Line {requested_line} is outside this document. Use 1 through {max_line}."),
        BookmarkEditError::LineOccupied { line } => {
            format!("Line {line} already has another bookmark.")
        }
    }
}

/// Explain why a closed-file bookmark excerpt could not be shown.
#[must_use]
pub fn bookmark_unavailable_description(
    reason: bookmark_excerpt::BookmarkExcerptUnavailableReason,
) -> &'static str {
    match reason {
        bookmark_excerpt::BookmarkExcerptUnavailableReason::MissingOrUnreadable => {
            "Bookmark preview unavailable: the file is missing or cannot be read."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::BinaryOrUnsupported => {
            "Bookmark preview unavailable: this file is not UTF-8 text."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::TooLargeToPreview => {
            "Bookmark preview unavailable: this file is too large to preview."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineBeyondPreviewBudget => {
            "Bookmark preview unavailable: the bookmarked line is beyond the preview budget."
        }
        bookmark_excerpt::BookmarkExcerptUnavailableReason::LineOutOfRange => {
            "Bookmark preview unavailable: the bookmarked line is no longer in this file."
        }
    }
}

/// Formatted raw bookmark body plus text-buffer offsets for target emphasis.
pub struct RawBookmarkExcerptText {
    /// Text inserted into the raw preview buffer.
    pub text: String,
    /// Character offset where the target line starts.
    pub target_start: i32,
    /// Character offset immediately after the target line.
    pub target_end: i32,
}

/// Render raw source context with line numbers and a target-line marker.
#[must_use]
pub fn format_raw_bookmark_excerpt(
    excerpt: &bookmark_excerpt::BookmarkExcerpt,
) -> RawBookmarkExcerptText {
    let line_number_width = excerpt
        .lines
        .last()
        .map_or(1, |line| line.number.saturating_add(1).to_string().len())
        .max(2);
    let mut text = String::new();
    let mut target_start = 0;
    let mut target_end = 0;

    if excerpt.window.truncation.before {
        push_raw_preview_line(&mut text, "... earlier bookmark context omitted ...");
    }

    for (index, line) in excerpt.lines.iter().enumerate() {
        if index == excerpt.window.target_line_index {
            target_start = raw_preview_offset(&text);
        }

        let marker = if index == excerpt.window.target_line_index {
            ">"
        } else {
            " "
        };
        let line_number = line.number.saturating_add(1);
        push_raw_preview_line(
            &mut text,
            &format!("{marker} {line_number:>line_number_width$} | {}", line.text),
        );

        if index == excerpt.window.target_line_index {
            target_end = raw_preview_offset(&text).saturating_sub(1);
        }
    }

    if excerpt.window.truncation.after {
        push_raw_preview_line(&mut text, "... later bookmark context omitted ...");
    }

    RawBookmarkExcerptText {
        text,
        target_start,
        target_end,
    }
}

fn push_raw_preview_line(text: &mut String, line: &str) {
    text.push_str(line);
    text.push('\n');
}

fn raw_preview_offset(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::workspace::{WorkspaceFolder, WorkspaceId};

    /// Fixed struct size used by the arithmetic tests, pinned rather than
    /// computed from the production type so a layout change cannot silently
    /// move the expected numbers.
    const SNAPSHOT_SIZE_BYTES: usize = 64;

    fn workspace(name: &str, folders: &[&str]) -> WorkspaceConfig {
        WorkspaceConfig {
            id: WorkspaceId::new(name),
            name: name.to_string(),
            folders: folders
                .iter()
                .map(|path| WorkspaceFolder::new(PathBuf::from(path)))
                .collect(),
        }
    }

    #[test]
    fn workflow_budgets_are_pinned_to_their_reviewed_literals() {
        assert_eq!(NOTES_BROWSER_RENDER_LIMIT, 500);
        assert_eq!(NOTES_BROWSER_SOURCE_ENTRY_LIMIT, 10_000);
        assert_eq!(NOTES_BROWSER_SOURCE_TEXT_LIMIT, 0x0400_0000, "64 MiB");
        assert_eq!(NOTES_BROWSER_SIDECAR_SCAN_LIMIT, 10_000);
        assert_eq!(NOTES_BROWSER_DIAGNOSTIC_LIMIT, 1_024);
        assert_eq!(NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT, 10_000);
        assert_eq!(
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT, 0x0040_0000,
            "4 MiB"
        );
        assert_eq!(NOTES_SAVE_DEBOUNCE_MS, 200);
        assert_eq!(MAX_BOOKMARK_SAVE_ATTEMPTS, 3);
        assert_eq!(NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS, 80);
        assert_eq!(COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS, 150);
        assert_eq!(NOTES_BROWSER_SEARCH_DEBOUNCE_MS, 150);
    }

    #[test]
    fn source_limits_carry_the_workflow_budgets_into_the_shared_engine() {
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.entries, 10_000);
        assert_eq!(
            NOTES_BROWSER_SOURCE_LIMITS.searchable_text_bytes,
            0x0400_0000
        );
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.sidecar_entries, 10_000);
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.diagnostics, 1_024);
    }

    #[test]
    fn narrowed_source_limits_pull_the_sidecar_ceiling_down_with_the_entry_ceiling() {
        let narrowed = notes_browser_source_limits_for_entries(3);
        assert_eq!(narrowed.entries, 3);
        assert_eq!(narrowed.sidecar_entries, 3);
        // Every other budget is untouched.
        assert_eq!(
            narrowed.searchable_text_bytes,
            NOTES_BROWSER_SOURCE_LIMITS.searchable_text_bytes
        );
        assert_eq!(
            narrowed.diagnostics,
            NOTES_BROWSER_SOURCE_LIMITS.diagnostics
        );
    }

    #[test]
    fn narrowing_above_the_sidecar_ceiling_leaves_it_alone() {
        let widened = notes_browser_source_limits_for_entries(50_000);
        assert_eq!(widened.entries, 50_000);
        assert_eq!(widened.sidecar_entries, 10_000);
    }

    #[test]
    fn folder_note_target_names_the_zero_one_and_many_cases() {
        assert_eq!(
            folder_note_target_for_workspace(workspace("Empty", &[])),
            FolderNoteOpenTarget::EmptyWorkspace {
                workspace_name: "Empty".to_string()
            }
        );
        assert_eq!(
            folder_note_target_for_workspace(workspace("One", &["/a"])),
            FolderNoteOpenTarget::SingleFolder {
                workspace_name: "One".to_string(),
                folder: PathBuf::from("/a"),
            }
        );
        assert_eq!(
            folder_note_target_for_workspace(workspace("Many", &["/a", "/b"])),
            FolderNoteOpenTarget::ChooseFolder {
                workspace_name: "Many".to_string(),
                folders: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            }
        );
    }

    #[test]
    fn folder_note_choice_preserves_the_stored_folder_order() {
        // Stored order is the user's ordering, and the chooser presents it as
        // given: nothing here may sort, reverse, or deduplicate it. The
        // single-folder case cannot express this invariant — one element reads
        // the same either way — so it is asserted on the multi-folder arm.
        assert_eq!(
            folder_note_target_for_workspace(workspace("Many", &["/second", "/first"])),
            FolderNoteOpenTarget::ChooseFolder {
                workspace_name: "Many".to_string(),
                folders: vec![PathBuf::from("/second"), PathBuf::from("/first")],
            }
        );
    }

    #[test]
    fn folder_note_action_is_available_only_for_actionable_targets() {
        assert!(!folder_note_action_available(
            &FolderNoteOpenTarget::AggregateScope
        ));
        assert!(!folder_note_action_available(
            &FolderNoteOpenTarget::WorkspaceMissing
        ));
        assert!(!folder_note_action_available(
            &FolderNoteOpenTarget::EmptyWorkspace {
                workspace_name: "w".to_string()
            }
        ));
        assert!(folder_note_action_available(
            &FolderNoteOpenTarget::SingleFolder {
                workspace_name: "w".to_string(),
                folder: PathBuf::from("/a"),
            }
        ));
        assert!(folder_note_action_available(
            &FolderNoteOpenTarget::ChooseFolder {
                workspace_name: "w".to_string(),
                folders: vec![PathBuf::from("/a"), PathBuf::from("/b")],
            }
        ));
    }

    #[test]
    fn bookmark_menu_label_follows_the_cursor_line() {
        assert_eq!(bookmark_menu_label(true), "Remove Bookmark");
        assert_eq!(bookmark_menu_label(false), "Add Bookmark");
    }

    #[test]
    fn open_editor_snapshot_capacity_takes_the_smallest_of_three_ceilings() {
        // Row limit wins.
        assert_eq!(
            open_editor_snapshot_capacity(3, 10, 4 * 1024 * 1024, SNAPSHOT_SIZE_BYTES),
            3
        );
        // Open-tab count wins.
        assert_eq!(
            open_editor_snapshot_capacity(100, 7, 4 * 1024 * 1024, SNAPSHOT_SIZE_BYTES),
            7
        );
        // Byte budget wins: 640 bytes / 64 bytes per snapshot = 10.
        assert_eq!(
            open_editor_snapshot_capacity(100, 50, 640, SNAPSHOT_SIZE_BYTES),
            10
        );
        // Zero open tabs is honest rather than saturating.
        assert_eq!(
            open_editor_snapshot_capacity(100, 0, 4 * 1024 * 1024, SNAPSHOT_SIZE_BYTES),
            0
        );
    }

    #[test]
    fn open_editor_snapshot_capacity_survives_a_zero_snapshot_size() {
        // `snapshot_size.max(1)` must prevent a division by zero rather than
        // panicking on a hypothetical zero-sized snapshot type.
        assert_eq!(open_editor_snapshot_capacity(5, 5, 64, 0), 5);
    }

    #[test]
    fn open_editor_snapshot_reserved_bytes_multiplies_without_overflowing() {
        assert_eq!(open_editor_snapshot_reserved_bytes(10, 64), 640);
        assert_eq!(
            open_editor_snapshot_reserved_bytes(usize::MAX, 64),
            u64::try_from(usize::MAX.saturating_mul(64)).unwrap_or(u64::MAX)
        );
    }

    #[test]
    fn open_editor_snapshot_heap_bytes_sums_path_and_scope_capacities() {
        assert_eq!(open_editor_snapshot_heap_bytes(10, None, None), 10);
        assert_eq!(open_editor_snapshot_heap_bytes(10, Some(4), None), 14);
        assert_eq!(open_editor_snapshot_heap_bytes(10, Some(4), Some(6)), 20);
        assert_eq!(open_editor_snapshot_heap_bytes(10, None, Some(6)), 16);
        assert_eq!(
            open_editor_snapshot_heap_bytes(usize::MAX, Some(usize::MAX), None),
            u64::try_from(usize::MAX).unwrap_or(u64::MAX)
        );
    }

    #[test]
    fn every_mode_string_differs_between_the_two_inventory_modes() {
        let all = NotesBrowserMode::AllNotes;
        let bookmarks = NotesBrowserMode::Bookmarks;
        type ModeProjection = (&'static str, fn(NotesBrowserMode) -> &'static str);
        let projections: [ModeProjection; 19] = [
            ("title", NotesBrowserMode::title),
            ("loading_label", NotesBrowserMode::loading_label),
            ("deferred_label", NotesBrowserMode::deferred_label),
            ("search_placeholder", NotesBrowserMode::search_placeholder),
            (
                "search_accessible_label",
                NotesBrowserMode::search_accessible_label,
            ),
            ("search_description", NotesBrowserMode::search_description),
            (
                "results_accessible_label",
                NotesBrowserMode::results_accessible_label,
            ),
            ("results_description", NotesBrowserMode::results_description),
            ("empty_source_label", NotesBrowserMode::empty_source_label),
            ("no_matches_label", NotesBrowserMode::no_matches_label),
            ("unselected_title", NotesBrowserMode::unselected_title),
            ("unselected_meta", NotesBrowserMode::unselected_meta),
            (
                "unselected_placeholder",
                NotesBrowserMode::unselected_placeholder,
            ),
            (
                "source_limit_message",
                NotesBrowserMode::source_limit_message,
            ),
            (
                "source_limit_status_message",
                NotesBrowserMode::source_limit_status_message,
            ),
            (
                "source_recovery_status_message",
                NotesBrowserMode::source_recovery_status_message,
            ),
            (
                "source_failure_message",
                NotesBrowserMode::source_failure_message,
            ),
            ("open_action_label", NotesBrowserMode::open_action_label),
            (
                "unselected_value_text",
                NotesBrowserMode::unselected_value_text,
            ),
        ];
        for (name, projection) in projections {
            assert_ne!(
                projection(all),
                projection(bookmarks),
                "{name} must not reuse the other mode's copy"
            );
            assert!(!projection(all).is_empty(), "{name} for AllNotes is empty");
            assert!(
                !projection(bookmarks).is_empty(),
                "{name} for Bookmarks is empty"
            );
        }
    }

    #[test]
    fn mode_titles_are_pinned_to_their_user_visible_literals() {
        assert_eq!(NotesBrowserMode::AllNotes.title(), "Notes");
        assert_eq!(NotesBrowserMode::Bookmarks.title(), "Bookmarks");
        assert_eq!(
            NotesBrowserMode::Bookmarks.empty_source_label(),
            "No bookmarks exist in the current workspace"
        );
        assert_eq!(
            NotesBrowserMode::AllNotes.empty_source_label(),
            "No notes yet"
        );
    }

    #[test]
    fn limit_messages_report_both_causes_independently() {
        assert!(
            notes_browser_limit_messages(NotesBrowserMode::AllNotes, false, false, 500).is_empty()
        );
        assert_eq!(
            notes_browser_limit_messages(NotesBrowserMode::AllNotes, true, false, 500),
            vec![
                NotesBrowserMode::AllNotes
                    .source_limit_message()
                    .to_string()
            ]
        );
        assert_eq!(
            notes_browser_limit_messages(NotesBrowserMode::AllNotes, false, true, 500),
            vec!["Showing first 500 matches. Refine search to narrow results.".to_string()]
        );
        let both = notes_browser_limit_messages(NotesBrowserMode::Bookmarks, true, true, 500);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0], NotesBrowserMode::Bookmarks.source_limit_message());
        assert!(both[1].contains("500"));
    }

    #[test]
    fn limit_message_names_the_caller_supplied_render_limit() {
        // Pinned to a literal that is not the production 500, so the assertion
        // cannot be satisfied by the production constant changing.
        assert_eq!(
            notes_browser_limit_messages(NotesBrowserMode::AllNotes, false, true, 7),
            vec!["Showing first 7 matches. Refine search to narrow results.".to_string()]
        );
    }

    #[test]
    fn bookmark_target_line_parses_only_whole_numbers() {
        assert_eq!(parse_bookmark_target_line("12"), Ok(12));
        assert_eq!(parse_bookmark_target_line("  12  "), Ok(12));
        assert_eq!(
            parse_bookmark_target_line("   "),
            Err("Enter a line number.".to_string())
        );
        assert_eq!(
            parse_bookmark_target_line(""),
            Err("Enter a line number.".to_string())
        );
        assert_eq!(
            parse_bookmark_target_line("12.5"),
            Err("Line must be a whole number.".to_string())
        );
        assert_eq!(
            parse_bookmark_target_line("-1"),
            Err("Line must be a whole number.".to_string())
        );
        assert_eq!(
            parse_bookmark_target_line("abc"),
            Err("Line must be a whole number.".to_string())
        );
    }

    #[test]
    fn bookmark_edit_errors_name_the_offending_lines() {
        assert_eq!(
            bookmark_edit_error_message(&BookmarkEditError::NotFound),
            "That bookmark is no longer available."
        );
        assert_eq!(
            bookmark_edit_error_message(&BookmarkEditError::LineOutOfRange {
                requested_line: 99,
                max_line: 10,
            }),
            "Line 99 is outside this document. Use 1 through 10."
        );
        assert_eq!(
            bookmark_edit_error_message(&BookmarkEditError::LineOccupied { line: 4 }),
            "Line 4 already has another bookmark."
        );
    }

    #[test]
    fn every_unavailable_reason_has_its_own_explanation() {
        use bookmark_excerpt::BookmarkExcerptUnavailableReason as Reason;
        let reasons = [
            Reason::MissingOrUnreadable,
            Reason::BinaryOrUnsupported,
            Reason::TooLargeToPreview,
            Reason::LineBeyondPreviewBudget,
            Reason::LineOutOfRange,
        ];
        let mut seen = Vec::new();
        for reason in reasons {
            let text = bookmark_unavailable_description(reason);
            assert!(text.starts_with("Bookmark preview unavailable: "));
            assert!(!seen.contains(&text), "{text} is reused by two reasons");
            seen.push(text);
        }
        assert_eq!(seen.len(), 5);
    }

    fn excerpt(
        target_line_index: usize,
        lines: &[(u32, &str)],
        before: bool,
        after: bool,
    ) -> bookmark_excerpt::BookmarkExcerpt {
        bookmark_excerpt::BookmarkExcerpt {
            presentation: bookmark_excerpt::BookmarkExcerptPresentation::PlainText,
            window: bookmark_excerpt::BookmarkExcerptLineWindow {
                first_line: lines.first().map_or(0, |(number, _)| *number),
                target_line: lines
                    .get(target_line_index)
                    .map_or(0, |(number, _)| *number),
                target_line_index,
                truncation: bookmark_excerpt::BookmarkExcerptTruncation {
                    before,
                    after,
                    within_line: false,
                },
            },
            lines: lines
                .iter()
                .map(|(number, text)| bookmark_excerpt::BookmarkExcerptLine {
                    number: *number,
                    text: (*text).to_string(),
                    truncated: false,
                })
                .collect(),
        }
    }

    #[test]
    fn raw_excerpt_marks_the_target_line_and_pads_line_numbers() {
        let rendered = format_raw_bookmark_excerpt(&excerpt(
            1,
            &[(0, "one"), (1, "two"), (2, "three")],
            false,
            false,
        ));
        assert_eq!(
            rendered.text, "   1 | one\n>  2 | two\n   3 | three\n",
            "line-number column is padded to the minimum width of 2"
        );
        // The target line body ">  2 | two" occupies character offsets 11..21;
        // the trailing newline is excluded by `saturating_sub(1)`.
        assert_eq!(rendered.target_start, 11);
        assert_eq!(rendered.target_end, 21);
        let start = usize::try_from(rendered.target_start).expect("non-negative offset");
        let end = usize::try_from(rendered.target_end).expect("non-negative offset");
        assert_eq!(&rendered.text[start..end], ">  2 | two");
    }

    #[test]
    fn raw_excerpt_widens_the_number_column_for_large_line_numbers() {
        let rendered =
            format_raw_bookmark_excerpt(&excerpt(0, &[(0, "a"), (998, "b")], false, false));
        // Highest displayed number is 999, so the column is 3 wide, not 2.
        assert!(
            rendered.text.starts_with(">   1 | a\n"),
            "{}",
            rendered.text
        );
        assert!(rendered.text.contains("  999 | b\n"), "{}", rendered.text);
    }

    #[test]
    fn raw_excerpt_reports_omitted_context_on_both_sides() {
        let rendered = format_raw_bookmark_excerpt(&excerpt(0, &[(5, "x")], true, true));
        assert!(
            rendered
                .text
                .starts_with("... earlier bookmark context omitted ...\n")
        );
        assert!(
            rendered
                .text
                .ends_with("... later bookmark context omitted ...\n")
        );
        // The leading omission notice shifts the target offsets past it.
        assert!(rendered.target_start > 0);
        assert!(rendered.target_end > rendered.target_start);
    }

    #[test]
    fn raw_excerpt_of_no_lines_is_empty_rather_than_panicking() {
        let rendered = format_raw_bookmark_excerpt(&excerpt(0, &[], false, false));
        assert_eq!(rendered.text, "");
        assert_eq!(rendered.target_start, 0);
        assert_eq!(rendered.target_end, 0);
    }
}
