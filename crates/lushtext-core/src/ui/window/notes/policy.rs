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

use crate::model::list_budget::grouped_thousands;
use crate::model::palette::PaletteNoteCategory;
use crate::model::workspace::WorkspaceConfig;
use crate::services::bookmark_excerpt;
use crate::services::palette as palette_service;
use crate::services::palette::NotesBrowserMode;
use crate::ui::editor_page::BookmarkEditError;

/// Maximum aggregate searchable UTF-8 bytes retained by Browse Notes.
///
/// The browser's only text bound: it has no row count
/// (`bound-notes-browser-by-budget` D3).
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const NOTES_BROWSER_SOURCE_TEXT_LIMIT: usize = 64 * 1024 * 1024;
/// Maximum recovery diagnostics retained by one Browse Notes load.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
pub const NOTES_BROWSER_DIAGNOSTIC_LIMIT: usize = 1_024;
/// Maximum retained live-editor metadata cloned before note-source admission.
///
/// Admitted by `model::list_budget::RetainedBudget` in the browser's sliced
/// capture, with no count (`bound-notes-browser-by-budget` D4); the palette's
/// one-turn capture applies it beside its own row count.
///
/// Kind: presentation cap; a power of two, never rounded down (`power-of-two-thresholds` D12).
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
///
/// Kind: retry budget; 3 rounded up to a power of two
/// (`power-of-two-thresholds` D6).
pub const MAX_BOOKMARK_SAVE_ATTEMPTS: u32 = 4;
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

/// Bookmark records of one editor captured in one unit of the sliced
/// open-editor capture, which then checks its turn clock.
///
/// Kind: cap; a power of two, rounded down (`bound-notes-browser-by-budget`
/// D4; the unit cost measured in task 8.1).
pub const OPEN_EDITOR_SNAPSHOT_SLICE_ROWS: usize = 256;

/// Browser-owned source policy passed into the shared admission engine.
///
/// No row count and no sidecar count: every byte ceiling stays, and a refused
/// row is skipped and counted rather than ending construction
/// (`bound-notes-browser-by-budget` D3).
pub const NOTES_BROWSER_SOURCE_LIMITS: palette_service::NoteSourceLimits =
    palette_service::NoteSourceLimits {
        entries: None,
        searchable_text_bytes: NOTES_BROWSER_SOURCE_TEXT_LIMIT,
        retained_bytes: palette_service::MAX_PALETTE_NOTE_RETAINED_BYTES,
        sidecar_entries: None,
        sidecar_path_bytes: palette_service::MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES,
        construction_bytes: palette_service::MAX_PALETTE_NOTE_CONSTRUCTION_BYTES,
        diagnostics: NOTES_BROWSER_DIAGNOSTIC_LIMIT,
        on_byte_refusal: palette_service::ByteRefusal::SkipAndCount,
    };

/// Narrow the browser's searchable-text ceiling (a test override, and the
/// `smoke-list-budgets` lane's `LUSHTEXT_NOTES_SOURCE_TEXT_BYTES`).
///
/// Only ever narrows: a request above the production ceiling keeps it.
#[must_use]
pub fn notes_browser_source_limits_for_text_bytes(
    text_bytes: usize,
) -> palette_service::NoteSourceLimits {
    palette_service::NoteSourceLimits {
        searchable_text_bytes: text_bytes.min(NOTES_BROWSER_SOURCE_TEXT_LIMIT),
        ..NOTES_BROWSER_SOURCE_LIMITS
    }
}

/// Where the sections of one grouped browser projection start.
///
/// `starts[i]` is the first row of the `i`-th `PaletteNoteCategory::ALL`
/// section and `starts[4]` is the row count, so section `i` is
/// `starts[i]..starts[i + 1]`, possibly empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoteSectionBounds {
    starts: [u32; 5],
}

impl NoteSectionBounds {
    /// Rows the projection lists.
    #[must_use]
    pub const fn len(&self) -> u32 {
        self.starts[4]
    }

    /// Whether the projection lists nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.starts[4] == 0
    }

    /// The `[start, end)` of the non-empty section holding `position`, or
    /// `None` past the end. Five comparisons at most, whatever the row count.
    #[must_use]
    pub fn section_at(&self, position: u32) -> Option<(u32, u32)> {
        let index = self.section_index_at(position)?;
        Some((self.starts[index], self.starts[index + 1]))
    }

    /// The same sections cut to the first `len` rows (a section past `len`
    /// becomes empty).
    #[must_use]
    pub fn truncated(&self, len: u32) -> Self {
        let mut starts = self.starts;
        for start in &mut starts {
            *start = (*start).min(len);
        }
        Self { starts }
    }

    /// The same sections with `count` rows from `start` removed (a section
    /// wholly inside them becomes empty), for a retirement that removes the
    /// rows nobody holds (`lighten-notes-browser-rows` D4).
    #[must_use]
    pub fn without_range(&self, start: u32, count: u32) -> Self {
        let end = start.saturating_add(count);
        let mut starts = self.starts;
        for bound in &mut starts {
            *bound = if *bound >= end {
                *bound - count
            } else if *bound > start {
                start
            } else {
                *bound
            };
        }
        Self { starts }
    }

    /// The category of the section holding `position`.
    #[must_use]
    pub fn category_at(&self, position: u32) -> Option<PaletteNoteCategory> {
        self.section_index_at(position)
            .map(|index| PaletteNoteCategory::ALL[index])
    }

    fn section_index_at(&self, position: u32) -> Option<usize> {
        if position >= self.len() {
            return None;
        }
        // The first section whose end is past `position`; an empty section's
        // end equals its start, so it is never chosen.
        (0..PaletteNoteCategory::ALL.len()).find(|&index| position < self.starts[index + 1])
    }
}

/// The browser's rows in grouped visual order plus their section bounds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupedNoteRows {
    /// Source indexes, grouped by `PaletteNoteCategory::ALL`, source order kept
    /// within a group.
    pub rows: Vec<u32>,
    /// Where each group starts.
    pub sections: NoteSectionBounds,
}

/// Group matching source indexes by category, keeping source order inside a
/// category (`bound-notes-browser-by-budget` D2).
///
/// The bounded source is already assembled in category order, so this is
/// usually the identity; a stable counting sort keeps it right even if a
/// future source order changes, in two linear passes on the worker. A match
/// whose category is unknown (`category_of` returns `None`) is dropped, as the
/// retired four-pass GTK grouping dropped an index past the source.
#[must_use]
pub fn group_note_rows(
    matches: &[u32],
    category_of: impl Fn(u32) -> Option<PaletteNoteCategory>,
) -> GroupedNoteRows {
    let category_index = |category: PaletteNoteCategory| {
        PaletteNoteCategory::ALL
            .iter()
            .position(|candidate| *candidate == category)
    };
    let mut counts = [0u32; 4];
    for &index in matches {
        if let Some(slot) = category_of(index).and_then(category_index) {
            counts[slot] = counts[slot].saturating_add(1);
        }
    }
    let mut starts = [0u32; 5];
    for slot in 0..4 {
        starts[slot + 1] = starts[slot].saturating_add(counts[slot]);
    }
    let mut cursor = [starts[0], starts[1], starts[2], starts[3]];
    let mut rows = vec![0u32; usize::try_from(starts[4]).unwrap_or(0)];
    for &index in matches {
        if let Some(slot) = category_of(index).and_then(category_index)
            && let Some(row) = rows.get_mut(usize::try_from(cursor[slot]).unwrap_or(usize::MAX))
        {
            *row = index;
            cursor[slot] = cursor[slot].saturating_add(1);
        }
    }
    GroupedNoteRows {
        rows,
        sections: NoteSectionBounds { starts },
    }
}

/// How many rows the browser lists against how many its source found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotesBrowserListedCount {
    /// Rows the source retained (X).
    pub listed: usize,
    /// Rows construction found, retained or not (Y).
    pub found: usize,
    /// Whether `found` is only a floor ("of at least").
    pub found_is_lower_bound: bool,
}

impl NotesBrowserListedCount {
    /// Whether a byte budget kept some rows out of the list.
    #[must_use]
    pub const fn is_limited(&self) -> bool {
        self.found_is_lower_bound || self.found > self.listed
    }
}

/// The visible and spoken limit notice, or `None` when nothing was cut
/// (`bound-notes-browser-by-budget` D5).
///
/// While a query is active the browser matches the retained rows only, and
/// the notice says so.
#[must_use]
pub fn notes_browser_limit_message(
    mode: NotesBrowserMode,
    count: NotesBrowserListedCount,
    searching: bool,
) -> Option<String> {
    if !count.is_limited() {
        return None;
    }
    let noun = match mode {
        NotesBrowserMode::AllNotes => "notes and bookmarks",
        NotesBrowserMode::Bookmarks => "bookmarks",
    };
    let of = if count.found_is_lower_bound {
        "of at least"
    } else {
        "of"
    };
    let (listed, found) = (
        grouped_thousands(count.listed),
        grouped_thousands(count.found.max(count.listed)),
    );
    Some(if searching {
        format!("Searching {listed} {of} {found} {noun}.")
    } else {
        format!("Showing {listed} {of} {found} {noun}. The rest exceed the memory budget.")
    })
}

/// The empty-state copy of a query that lists nothing.
///
/// An empty source with no query says there is nothing yet, unless a byte
/// budget kept every row out, which it then says instead of "nothing yet";
/// otherwise the query matched nothing.
#[must_use]
pub fn notes_browser_empty_label(
    mode: NotesBrowserMode,
    source_is_empty: bool,
    searching: bool,
    limited: bool,
) -> &'static str {
    match (source_is_empty && !searching, limited) {
        (true, true) => match mode {
            NotesBrowserMode::AllNotes => "No notes fit the memory budget",
            NotesBrowserMode::Bookmarks => "No bookmarks fit the memory budget",
        },
        (true, false) => mode.empty_source_label(),
        (false, _) => mode.no_matches_label(),
    }
}

/// Whether publishing a source generation should raise the one polite
/// limit announcement: once per limited generation, never again for its
/// queries, selections or re-presents.
#[must_use]
pub fn should_announce_limit(
    announced_generation: Option<u64>,
    generation: u64,
    limited: bool,
) -> bool {
    limited && announced_generation != Some(generation)
}

/// What the sliced capture does with one editor before its next slice
/// (`bound-notes-browser-by-budget` D4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSliceDecision {
    /// The editor's bookmarks are unchanged since its first slice.
    Proceed,
    /// They changed: drop what was captured from this editor, return its
    /// charge, and capture it again from its first slice.
    RestartEditor,
}

/// Compare the bookmark generation recorded at an editor's first slice with
/// the current one. An editor is never captured in one unbounded unit: a
/// change restarts it, sliced like the first attempt.
#[must_use]
pub fn capture_slice_decision(recorded: Option<u64>, current: u64) -> CaptureSliceDecision {
    match recorded {
        Some(generation) if generation != current => CaptureSliceDecision::RestartEditor,
        _ => CaptureSliceDecision::Proceed,
    }
}

/// Bookmark records taken in the next unit, from `remaining` uncaptured ones.
#[must_use]
pub const fn capture_slice_rows(remaining: usize) -> usize {
    if remaining < OPEN_EDITOR_SNAPSHOT_SLICE_ROWS {
        remaining
    } else {
        OPEN_EDITOR_SNAPSHOT_SLICE_ROWS
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

    use proptest::prelude::*;

    const CATEGORIES: [PaletteNoteCategory; 4] = PaletteNoteCategory::ALL;

    /// The retired GTK-side grouping (`append_notes_sidebar_sections` on
    /// `main`): one pass over the matches per category, in category order.
    fn four_pass_reference(
        matches: &[u32],
        categories: &[PaletteNoteCategory],
    ) -> (Vec<u32>, [u32; 4]) {
        let mut rows = Vec::new();
        let mut lengths = [0u32; 4];
        for (slot, category) in CATEGORIES.into_iter().enumerate() {
            for &index in matches {
                if categories.get(index as usize) == Some(&category) {
                    rows.push(index);
                    lengths[slot] += 1;
                }
            }
        }
        (rows, lengths)
    }

    fn category_strategy() -> impl Strategy<Value = PaletteNoteCategory> {
        prop::sample::select(CATEGORIES.to_vec())
    }

    proptest! {
        #[test]
        fn grouping_matches_the_retired_four_pass_grouping(
            categories in prop::collection::vec(category_strategy(), 0..200),
            keep in prop::collection::vec(any::<bool>(), 0..200),
        ) {
            let matches: Vec<u32> = (0..categories.len())
                .filter(|index| keep.get(*index).copied().unwrap_or(true))
                .map(|index| u32::try_from(index).expect("small"))
                .collect();
            let grouped = group_note_rows(&matches, |index| categories.get(index as usize).copied());
            let (rows, lengths) = four_pass_reference(&matches, &categories);
            prop_assert_eq!(&grouped.rows, &rows);
            let mut start = 0u32;
            for (slot, length) in lengths.into_iter().enumerate() {
                for position in start..start + length {
                    prop_assert_eq!(grouped.sections.section_at(position), Some((start, start + length)));
                    prop_assert_eq!(grouped.sections.category_at(position), Some(CATEGORIES[slot]));
                }
                start += length;
            }
            prop_assert_eq!(grouped.sections.len(), start);
            prop_assert_eq!(grouped.sections.section_at(start), None);
            prop_assert_eq!(grouped.sections.category_at(u32::MAX), None);
        }
    }

    #[test]
    fn grouping_of_an_ordered_source_is_the_identity() {
        let categories = [
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::DocumentNotes,
            PaletteNoteCategory::OpenTabs,
        ];
        let grouped = group_note_rows(&[0, 1, 2, 3], |index| {
            categories.get(index as usize).copied()
        });
        assert_eq!(grouped.rows, vec![0, 1, 2, 3]);
        // Folder notes are empty: their section has zero length and no position.
        assert_eq!(grouped.sections.section_at(0), Some((0, 2)));
        assert_eq!(grouped.sections.section_at(1), Some((0, 2)));
        assert_eq!(grouped.sections.section_at(2), Some((2, 3)));
        assert_eq!(
            grouped.sections.category_at(2),
            Some(PaletteNoteCategory::DocumentNotes)
        );
        assert_eq!(grouped.sections.section_at(3), Some((3, 4)));
        assert_eq!(grouped.sections.section_at(4), None);
    }

    #[test]
    fn truncated_sections_keep_their_bounds_up_to_the_cut() {
        let categories = [
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::FolderNotes,
            PaletteNoteCategory::DocumentNotes,
        ];
        let grouped = group_note_rows(&[0, 1, 2, 3], |index| {
            categories.get(index as usize).copied()
        });
        let cut = grouped.sections.truncated(3);
        assert_eq!(cut.len(), 3);
        assert_eq!(cut.section_at(1), Some((0, 2)));
        assert_eq!(cut.section_at(2), Some((2, 3)));
        assert_eq!(cut.section_at(3), None);
        assert_eq!(grouped.sections.truncated(9), grouped.sections);
        assert!(grouped.sections.truncated(0).is_empty());
    }

    #[test]
    fn sections_without_a_range_shift_down_and_drop_emptied_sections() {
        let categories = [
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::FolderNotes,
            PaletteNoteCategory::DocumentNotes,
        ];
        let grouped = group_note_rows(&[0, 1, 2, 3], |index| {
            categories.get(index as usize).copied()
        });
        // The head.
        let rest = grouped.sections.without_range(0, 1);
        assert_eq!(rest.len(), 3);
        assert_eq!(rest.section_at(0), Some((0, 1)));
        assert_eq!(
            rest.category_at(2),
            Some(PaletteNoteCategory::DocumentNotes)
        );
        // The middle: the folder-note section empties.
        let rest = grouped.sections.without_range(1, 2);
        assert_eq!(rest.len(), 2);
        assert_eq!(rest.category_at(0), Some(PaletteNoteCategory::Bookmarks));
        assert_eq!(
            rest.category_at(1),
            Some(PaletteNoteCategory::DocumentNotes)
        );
        // The tail, everything, nothing.
        assert_eq!(
            grouped.sections.without_range(3, 1),
            grouped.sections.truncated(3)
        );
        assert!(grouped.sections.without_range(0, 9).is_empty());
        assert_eq!(grouped.sections.without_range(2, 0), grouped.sections);
    }

    #[test]
    fn grouping_regroups_an_unordered_source_and_drops_unknown_indexes() {
        let categories = [
            PaletteNoteCategory::OpenTabs,
            PaletteNoteCategory::Bookmarks,
            PaletteNoteCategory::FolderNotes,
            PaletteNoteCategory::Bookmarks,
        ];
        let grouped = group_note_rows(&[0, 1, 2, 3, 9], |index| {
            categories.get(index as usize).copied()
        });
        assert_eq!(grouped.rows, vec![1, 3, 2, 0]);
        assert_eq!(grouped.sections.len(), 4);
        assert!(group_note_rows(&[], |_| None).sections.is_empty());
    }

    /// `main`'s source admission: rows in order while the count and every
    /// byte ceiling fit; the first refusal ends construction.
    fn main_prefix_latch(charges: &[u64], count: usize, bytes: u64) -> Vec<usize> {
        let mut admitted = Vec::new();
        let mut used = 0u64;
        for (index, &charge) in charges.iter().enumerate() {
            if admitted.len() == count || used + charge > bytes {
                break;
            }
            used += charge;
            admitted.push(index);
        }
        admitted
    }

    /// The browser's rule: no count, a refused row is skipped and counted.
    fn browser_skip_and_count(charges: &[u64], bytes: u64) -> (Vec<usize>, usize) {
        let mut admitted = Vec::new();
        let mut used = 0u64;
        for (index, &charge) in charges.iter().enumerate() {
            if used + charge <= bytes {
                used += charge;
                admitted.push(index);
            }
        }
        (admitted, charges.len())
    }

    proptest! {
        /// D3: the count-free skip-and-continue rule admits a superset of
        /// `main`'s count-and-byte prefix latch, and counts every row. The
        /// command palette's limits take the same rule since
        /// `drill-down-palette-categories` D7, so `main`'s palette (16,384
        /// rows, `Stop`) is this `main_prefix_latch` too.
        #[test]
        fn skip_and_count_admits_a_superset_of_mains_prefix_latch(
            charges in prop::collection::vec(0u64..1_000, 0..120),
            count in 0usize..150,
            bytes in 0u64..40_000,
        ) {
            let main = main_prefix_latch(&charges, count, bytes);
            let (browser, found) = browser_skip_and_count(&charges, bytes);
            for index in &main {
                prop_assert!(browser.contains(index), "{index} admitted on main, lost here");
            }
            prop_assert_eq!(&browser[..main.len()], &main[..], "same prefix up to main's refusal");
            prop_assert_eq!(found, charges.len());
            prop_assert!(browser.iter().map(|i| charges[*i]).sum::<u64>() <= bytes);
        }
    }

    #[test]
    fn a_capture_restarts_an_editor_on_any_generation_change() {
        assert_eq!(
            capture_slice_decision(None, 7),
            CaptureSliceDecision::Proceed
        );
        assert_eq!(
            capture_slice_decision(Some(7), 7),
            CaptureSliceDecision::Proceed
        );
        assert_eq!(
            capture_slice_decision(Some(7), 8),
            CaptureSliceDecision::RestartEditor
        );
        assert_eq!(
            capture_slice_decision(Some(u64::MAX), 0),
            CaptureSliceDecision::RestartEditor,
            "a wrapped generation is a change"
        );
    }

    proptest! {
        /// D4: no unit captures more than one slice, however many bookmarks an
        /// editor holds, and every unit makes progress.
        #[test]
        fn a_capture_unit_is_bounded_and_makes_progress(remaining in 0usize..usize::MAX) {
            let rows = capture_slice_rows(remaining);
            prop_assert!(rows <= 256);
            prop_assert_eq!(rows == 0, remaining == 0);
            prop_assert!(rows <= remaining);
        }
    }

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
        assert_eq!(NOTES_BROWSER_SOURCE_TEXT_LIMIT, 0x0400_0000, "64 MiB");
        assert_eq!(NOTES_BROWSER_DIAGNOSTIC_LIMIT, 1_024);
        assert_eq!(OPEN_EDITOR_SNAPSHOT_SLICE_ROWS, 256);
        assert_eq!(
            NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT, 0x0040_0000,
            "4 MiB"
        );
        assert_eq!(NOTES_SAVE_DEBOUNCE_MS, 200);
        assert_eq!(MAX_BOOKMARK_SAVE_ATTEMPTS, 4);
        assert_eq!(NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS, 80);
        assert_eq!(COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS, 150);
        assert_eq!(NOTES_BROWSER_SEARCH_DEBOUNCE_MS, 150);
    }

    #[test]
    fn source_limits_carry_the_workflow_budgets_into_the_shared_engine() {
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.entries, None, "no row count");
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.sidecar_entries, None);
        assert_eq!(
            NOTES_BROWSER_SOURCE_LIMITS.searchable_text_bytes,
            0x0400_0000
        );
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.retained_bytes, 0x0400_0000);
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.sidecar_path_bytes, 0x0080_0000);
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.construction_bytes, 0x0600_0000);
        assert_eq!(NOTES_BROWSER_SOURCE_LIMITS.diagnostics, 1_024);
        assert_eq!(
            NOTES_BROWSER_SOURCE_LIMITS.on_byte_refusal,
            palette_service::ByteRefusal::SkipAndCount
        );
        assert_eq!(
            palette_service::PALETTE_NOTE_SOURCE_LIMITS.on_byte_refusal,
            palette_service::ByteRefusal::SkipAndCount,
            "both consumers skip and count (drill-down-palette-categories D7)"
        );
        assert_eq!(palette_service::PALETTE_NOTE_SOURCE_LIMITS.entries, None);
        assert_eq!(
            palette_service::PALETTE_NOTE_SOURCE_LIMITS.sidecar_entries,
            None
        );
    }

    #[test]
    fn the_text_override_only_narrows() {
        assert_eq!(
            notes_browser_source_limits_for_text_bytes(3).searchable_text_bytes,
            3
        );
        assert_eq!(
            notes_browser_source_limits_for_text_bytes(usize::MAX).searchable_text_bytes,
            0x0400_0000
        );
        let narrowed = notes_browser_source_limits_for_text_bytes(3);
        assert_eq!(narrowed.entries, None);
        assert_eq!(
            narrowed.on_byte_refusal,
            palette_service::ByteRefusal::SkipAndCount
        );
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
        let projections: [ModeProjection; 17] = [
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
    fn the_empty_state_names_why_nothing_is_listed() {
        let all = NotesBrowserMode::AllNotes;
        let bookmarks = NotesBrowserMode::Bookmarks;
        assert_eq!(
            notes_browser_empty_label(all, true, false, false),
            "No notes yet"
        );
        assert_eq!(
            notes_browser_empty_label(bookmarks, true, false, false),
            "No bookmarks exist in the current workspace"
        );
        assert_eq!(
            notes_browser_empty_label(all, true, false, true),
            "No notes fit the memory budget"
        );
        assert_eq!(
            notes_browser_empty_label(bookmarks, true, false, true),
            "No bookmarks fit the memory budget"
        );
        assert_eq!(
            notes_browser_empty_label(all, false, true, true),
            "No notes match that search"
        );
        assert_eq!(
            notes_browser_empty_label(all, true, true, false),
            "No notes match that search"
        );
    }

    #[test]
    fn an_uncut_source_shows_no_limit_notice() {
        for mode in [NotesBrowserMode::AllNotes, NotesBrowserMode::Bookmarks] {
            for searching in [false, true] {
                let count = NotesBrowserListedCount {
                    listed: 9,
                    found: 9,
                    found_is_lower_bound: false,
                };
                assert!(!count.is_limited());
                assert_eq!(notes_browser_limit_message(mode, count, searching), None);
            }
        }
    }

    #[test]
    fn a_byte_cut_says_showing_x_of_y_in_both_modes() {
        let count = NotesBrowserListedCount {
            listed: 3,
            found: 7,
            found_is_lower_bound: false,
        };
        assert!(count.is_limited());
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::AllNotes, count, false).as_deref(),
            Some("Showing 3 of 7 notes and bookmarks. The rest exceed the memory budget.")
        );
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::Bookmarks, count, false).as_deref(),
            Some("Showing 3 of 7 bookmarks. The rest exceed the memory budget.")
        );
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::AllNotes, count, true).as_deref(),
            Some("Searching 3 of 7 notes and bookmarks.")
        );
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::Bookmarks, count, true).as_deref(),
            Some("Searching 3 of 7 bookmarks.")
        );
    }

    #[test]
    fn large_counts_are_grouped_like_the_other_list_budget_notices() {
        let count = NotesBrowserListedCount {
            listed: 90_687,
            found: 128 * 1024,
            found_is_lower_bound: false,
        };
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::AllNotes, count, false).as_deref(),
            Some(
                "Showing 90,687 of 131,072 notes and bookmarks. The rest exceed the memory budget."
            )
        );
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::Bookmarks, count, true).as_deref(),
            Some("Searching 90,687 of 131,072 bookmarks.")
        );
    }

    #[test]
    fn an_unparsed_remainder_says_at_least() {
        let count = NotesBrowserListedCount {
            listed: 3,
            found: 3,
            found_is_lower_bound: true,
        };
        assert!(count.is_limited());
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::AllNotes, count, false).as_deref(),
            Some("Showing 3 of at least 3 notes and bookmarks. The rest exceed the memory budget.")
        );
        assert_eq!(
            notes_browser_limit_message(NotesBrowserMode::Bookmarks, count, true).as_deref(),
            Some("Searching 3 of at least 3 bookmarks.")
        );
    }

    #[test]
    fn the_limit_is_announced_once_per_limited_source_generation() {
        assert!(should_announce_limit(None, 4, true));
        assert!(
            !should_announce_limit(Some(4), 4, true),
            "not again for queries"
        );
        assert!(
            should_announce_limit(Some(4), 5, true),
            "a mode switch may announce"
        );
        assert!(!should_announce_limit(None, 4, false));
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
