// SPDX-License-Identifier: GPL-3.0-or-later

//! Command palette service — fuzzy matching, file indexing, and command registry.
//!
//! This module stays GTK-free and fully unit-testable. The implementation is
//! split by workflow so file indexing, command registry maintenance, and fuzzy
//! scoring can evolve independently without one giant service file.

mod commands;
mod fuzzy;
mod grouped;
mod index;
mod notes;
mod runtime;

pub use commands::{
    NoteCommandSection, all_commands, is_note_command, note_command_section, search_all,
    search_commands, search_non_note_commands, search_note_commands,
    search_note_commands_for_section, search_open_files,
};
#[cfg(feature = "property-tests")]
pub use commands::{
    OpenFileSelectionEvidence, merge_sorted_for_property_test,
    open_file_selection_equivalence_for_property_test,
};
pub use fuzzy::{PALETTE_CANCEL_CHECK_INTERVAL, compare_palette_rank, fuzzy_score};
pub use grouped::{GroupedSearchInput, grouped_search};
pub use index::{
    FileIndex, FileIndexBuildCoordinator, FileIndexBuildCoordinatorSnapshot, FileIndexBuildMetrics,
    FileIndexBuildOutcome, FileIndexBuildRequest, FileIndexBuildStart, FileIndexTruncationReason,
    MAX_INDEXED_FILES,
};
pub use notes::{
    MAX_PALETTE_NOTE_ENTRIES, MAX_PALETTE_NOTE_TEXT_BYTES, NoteSourceLimits, NoteSourceMetrics,
    NoteSourceRefreshCoordinator, NoteSourceRefreshCoordinatorSnapshot, NoteSourceRefreshRequest,
    NoteSourceRefreshStart, NoteSourceTruncationReason, NotesBrowserQueryCoordinator,
    NotesBrowserQueryRequest, NotesBrowserQueryResult, PALETTE_NOTE_SOURCE_LIMITS,
    PaletteNoteSourceLoad, PaletteNoteSourceOutcome, admit_synthetic_note_bodies_for_benchmark,
    bookmark_display_label, build_note_entries, format_line_label,
    load_note_entries_bounded_for_scope, load_note_entries_for_scope,
    load_palette_note_entries_for_scope, open_tab_source_for_path, path_is_in_folders,
    query_notes_browser_source, search_note_entries, search_note_entries_in_category,
};
#[cfg(feature = "test-utils")]
pub use notes::{set_note_source_delay_for_test, set_notes_browser_query_delay_for_test};
pub use runtime::{
    PaletteSearchCancellation, PaletteSearchCoordinator, PaletteSearchCoordinatorSnapshot,
    PaletteSearchMetrics, PaletteSearchOutcome, PaletteSearchStart,
};

#[cfg(test)]
mod tests;
