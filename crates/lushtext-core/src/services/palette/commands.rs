// SPDX-License-Identifier: GPL-3.0-or-later

//! Built-in command registry and unified command-palette search entry points.
//!
//! This slice owns static command definitions, Notes workflow classification for
//! command rows, command subset searches, and the merge logic that combines
//! command results with file-index matches.

use std::collections::HashSet;
#[cfg(feature = "property-tests")]
use std::path::PathBuf;

use crate::model::palette::{
    CommandCategory, CommandDef, PaletteFileEntry, ScoredResult, SearchMode, SearchResultItem,
};

#[cfg(feature = "property-tests")]
use super::fuzzy::search_items_full_sort_reference;
use super::fuzzy::{search_items, search_items_cancellable};
use super::index::FileIndex;
use super::runtime::{PaletteSearchCancellation, PaletteSearchOutcome};

/// All built-in commands available in the palette.
#[must_use]
pub fn all_commands() -> &'static [CommandDef] {
    // Static registry used by every palette search; keeping it static avoids
    // rebuilding command metadata on each query.
    static COMMANDS: &[CommandDef] = &[
        CommandDef {
            id: "win.new-tab",
            label: "New File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+N"),
        },
        CommandDef {
            id: "win.open-file",
            label: "Open File",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+O"),
        },
        CommandDef {
            id: "win.open-recent",
            label: "Open Recent Documents",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+K"),
        },
        CommandDef {
            id: "win.open-folder",
            label: "Open Folder",
            category: CommandCategory::File,
            shortcut: None,
        },
        CommandDef {
            id: "win.save",
            label: "Save",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+S"),
        },
        CommandDef {
            id: "win.save-as",
            label: "Save As",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+Shift+S"),
        },
        CommandDef {
            id: "win.show-local-history",
            label: "Local History",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Alt+L"),
        },
        CommandDef {
            id: "win.print",
            label: "Print",
            category: CommandCategory::File,
            shortcut: Some("Ctrl+P"),
        },
        CommandDef {
            id: "win.begin-search",
            label: "Find and Replace",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+F"),
        },
        CommandDef {
            id: "win.toggle-bookmark",
            label: "Toggle Bookmark",
            category: CommandCategory::Notes,
            shortcut: Some("Ctrl+F2"),
        },
        CommandDef {
            id: "win.edit-bookmark-label",
            label: "Edit Bookmark",
            category: CommandCategory::Notes,
            shortcut: Some("Ctrl+Shift+F2"),
        },
        CommandDef {
            id: "win.next-bookmark",
            label: "Next Bookmark",
            category: CommandCategory::Notes,
            shortcut: Some("F2"),
        },
        CommandDef {
            id: "win.prev-bookmark",
            label: "Previous Bookmark",
            category: CommandCategory::Notes,
            shortcut: Some("Shift+F2"),
        },
        CommandDef {
            id: "win.open-document-note",
            label: "Open Document Note",
            category: CommandCategory::Notes,
            shortcut: None,
        },
        CommandDef {
            id: "win.open-folder-note",
            label: "Open Folder Note",
            category: CommandCategory::Notes,
            shortcut: None,
        },
        CommandDef {
            id: "win.close-tab",
            label: "Close Tab",
            category: CommandCategory::Edit,
            shortcut: Some("Ctrl+W"),
        },
        CommandDef {
            id: "win.show-bookmarks",
            label: "Browse Bookmarks",
            category: CommandCategory::Notes,
            shortcut: Some("Ctrl+Alt+B"),
        },
        CommandDef {
            id: "win.show-notes",
            label: "Browse Notes",
            category: CommandCategory::Notes,
            shortcut: Some("Ctrl+Alt+A"),
        },
        CommandDef {
            id: "win.toggle-sidebar",
            label: "Toggle Sidebar",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "win.toggle-properties",
            label: "Document Properties",
            category: CommandCategory::View,
            shortcut: Some("F9"),
        },
        CommandDef {
            id: "win.toggle-fullscreen",
            label: "Fullscreen",
            category: CommandCategory::View,
            shortcut: Some("F11"),
        },
        CommandDef {
            id: "win.toggle-focus-mode",
            label: "Focus Mode",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+Shift+F11"),
        },
        CommandDef {
            id: "win.zoom-in",
            label: "Zoom In",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+="),
        },
        CommandDef {
            id: "win.zoom-out",
            label: "Zoom Out",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+-"),
        },
        CommandDef {
            id: "win.zoom-reset",
            label: "Reset Zoom",
            category: CommandCategory::View,
            shortcut: Some("Ctrl+0"),
        },
        CommandDef {
            id: "win.show-help-overlay",
            label: "Keyboard Shortcuts",
            category: CommandCategory::View,
            shortcut: None,
        },
        CommandDef {
            id: "app.preferences",
            label: "Preferences",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.about",
            label: "About LushText",
            category: CommandCategory::App,
            shortcut: None,
        },
        CommandDef {
            id: "app.quit",
            label: "Quit",
            category: CommandCategory::App,
            shortcut: Some("Ctrl+Q"),
        },
    ];
    COMMANDS
}

/// Workflow sections for note and bookmark launcher commands.
///
/// This policy lives beside the static command registry so every palette
/// surface shares one source of truth for which actions are Notes actions and
/// how they are grouped. The GTK adapter renders these sections instead of
/// duplicating command-id policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteCommandSection {
    Browse,
    CurrentDocument,
    BookmarkNavigation,
    Workspace,
}

impl NoteCommandSection {
    /// Notes-mode section order.
    pub const ALL: [Self; 4] = [
        Self::Browse,
        Self::CurrentDocument,
        Self::BookmarkNavigation,
        Self::Workspace,
    ];

    /// Display label for the section header shown in Notes mode.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Browse => "Browse",
            Self::CurrentDocument => "Current Document",
            Self::BookmarkNavigation => "Bookmark Navigation",
            Self::Workspace => "Workspace",
        }
    }
}

/// Return the Notes-mode intent section for a command.
#[must_use]
pub fn note_command_section(command: &CommandDef) -> Option<NoteCommandSection> {
    // Classify by stable action id instead of label text so copy changes cannot
    // silently move commands between Notes sections.
    match command.id {
        "win.show-notes" | "win.show-bookmarks" => Some(NoteCommandSection::Browse),
        "win.toggle-bookmark" | "win.edit-bookmark-label" | "win.open-document-note" => {
            Some(NoteCommandSection::CurrentDocument)
        }
        "win.next-bookmark" | "win.prev-bookmark" => Some(NoteCommandSection::BookmarkNavigation),
        "win.open-folder-note" => Some(NoteCommandSection::Workspace),
        _ => None,
    }
}

/// Return whether the command belongs to the Notes command-palette surface.
#[must_use]
pub fn is_note_command(command: &CommandDef) -> bool {
    note_command_section(command).is_some()
}

/// Search the command registry with a fuzzy query.
pub fn search_commands(query: &str, max: usize) -> Vec<ScoredResult<'static>> {
    search_items(
        all_commands().iter(),
        |command| command.label,
        SearchResultItem::Command,
        query,
        max,
    )
}

/// Search only note and bookmark workflow commands.
pub fn search_note_commands(query: &str, max: usize) -> Vec<ScoredResult<'static>> {
    search_command_subset(query, max, is_note_command)
}

/// Search note and bookmark workflow commands for one Notes-mode intent section.
#[must_use]
pub fn search_note_commands_for_section(
    section: NoteCommandSection,
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'static>> {
    search_command_subset(query, max, move |command| {
        note_command_section(command) == Some(section)
    })
}

/// Search commands outside the Notes workflow surface.
#[must_use]
pub fn search_non_note_commands(query: &str, max: usize) -> Vec<ScoredResult<'static>> {
    search_command_subset(query, max, |command| !is_note_command(command))
}

/// Run one command-search path for filtered command surfaces.
///
/// Keeping filtering here means Notes, non-Notes, and full command searches all
/// use the same fuzzy scoring and max-result behavior.
fn search_command_subset<F>(query: &str, max: usize, predicate: F) -> Vec<ScoredResult<'static>>
where
    F: Fn(&CommandDef) -> bool,
{
    let cancellation = PaletteSearchCancellation::default();
    match search_command_subset_cancellable(query, max, predicate, &cancellation) {
        PaletteSearchOutcome::Complete { value, .. } => value,
        PaletteSearchOutcome::Cancelled { .. } => unreachable!("fresh token cannot cancel"),
    }
}

pub(super) fn search_commands_cancellable(
    query: &str,
    max: usize,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<ScoredResult<'static>>> {
    search_command_subset_cancellable(query, max, |_| true, cancellation)
}

fn search_command_subset_cancellable<F>(
    query: &str,
    max: usize,
    predicate: F,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<ScoredResult<'static>>>
where
    F: Fn(&CommandDef) -> bool,
{
    search_items_cancellable(
        all_commands().iter(),
        predicate,
        |command, fuzzy_query| fuzzy_query.score(command.label),
        SearchResultItem::Command,
        query,
        max,
        cancellation,
    )
}

/// Search open file-backed tab entries with the same fuzzy matcher as indexed files.
pub fn search_open_files<'a>(
    files: &'a [PaletteFileEntry],
    query: &str,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    search_items(
        files.iter(),
        |file| file.display_name.as_str(),
        SearchResultItem::OpenFile,
        query,
        max,
    )
}

pub(super) fn search_open_files_cancellable<'a>(
    files: &'a [PaletteFileEntry],
    query: &str,
    max: usize,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<ScoredResult<'a>>> {
    let mut canonical_paths = HashSet::new();
    let mut unresolved_paths = HashSet::new();
    search_items_cancellable(
        files.iter(),
        |file| match file.identity.canonical_path() {
            Some(path) => canonical_paths.insert(path.to_path_buf()),
            None => unresolved_paths.insert(file.path.clone()),
        },
        |file, fuzzy_query| fuzzy_query.score(&file.display_name),
        SearchResultItem::OpenFile,
        query,
        max,
        cancellation,
    )
}

/// Bounded/reference projection plus retention metrics for property tests.
#[cfg(feature = "property-tests")]
pub type OpenFileSelectionEvidence = (
    Vec<(PathBuf, u32, usize)>,
    Vec<(PathBuf, u32, usize)>,
    super::runtime::PaletteSearchMetrics,
);

/// Compare bounded and full-sort open-file selection in the property-test lane.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn open_file_selection_equivalence_for_property_test(
    files: &[PaletteFileEntry],
    query: &str,
    max: usize,
) -> OpenFileSelectionEvidence {
    let cancellation = PaletteSearchCancellation::default();
    let bounded = search_open_files_cancellable(files, query, max, &cancellation);
    let metrics = bounded.metrics();
    let PaletteSearchOutcome::Complete { value: bounded, .. } = bounded else {
        unreachable!("fresh token cannot cancel");
    };
    let mut reference_canonical_paths = HashSet::new();
    let mut reference_unresolved_paths = HashSet::new();
    let reference = search_items_full_sort_reference(
        files.iter(),
        |file| match file.identity.canonical_path() {
            Some(path) => reference_canonical_paths.insert(path.to_path_buf()),
            None => reference_unresolved_paths.insert(file.path.clone()),
        },
        |file, fuzzy_query| fuzzy_query.score(&file.display_name),
        SearchResultItem::OpenFile,
        query,
        max,
    );
    let project = |results: Vec<ScoredResult<'_>>| {
        results
            .into_iter()
            .filter_map(|result| match result.item {
                SearchResultItem::OpenFile(file) => {
                    Some((file.path.clone(), result.score, result.source_ordinal))
                }
                SearchResultItem::File(_)
                | SearchResultItem::Command(_)
                | SearchResultItem::Note(_) => None,
            })
            .collect()
    };
    (project(bounded), project(reference), metrics)
}

/// Search the palette's file and command launcher sources according to mode.
///
/// Note records are supplied by the cached note source in `services::palette::notes`.
/// This helper therefore returns no rows for `Notes` mode instead of falling
/// back to note workflow commands.
#[must_use]
pub fn search_all<'a>(
    index: &'a FileIndex,
    query: &str,
    mode: SearchMode,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    match mode {
        SearchMode::Files => index.search(query, max),
        SearchMode::Notes => Vec::new(),
        SearchMode::Commands => search_commands(query, max),
        SearchMode::All => {
            let files = index.search(query, max);
            let commands = search_commands(query, max);
            merge_sorted(files, commands, max)
        }
    }
}

/// Merge sorted result streams by fuzzy score, preferring files on ties.
///
/// `All` mode uses this to preserve stable source priority without losing the
/// score ordering produced by each source-specific search.
fn merge_sorted<'a>(
    a: Vec<ScoredResult<'a>>,
    b: Vec<ScoredResult<'a>>,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    let mut result = Vec::with_capacity(max.min(a.len() + b.len()));
    let mut a = a.into_iter().peekable();
    let mut b = b.into_iter().peekable();
    while result.len() < max {
        match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => {
                if x.score >= y.score {
                    result.push(
                        a.next()
                            .expect("peeked iterator entry should still be available"),
                    );
                } else {
                    result.push(
                        b.next()
                            .expect("peeked iterator entry should still be available"),
                    );
                }
            }
            (Some(_), None) => result.push(
                a.next()
                    .expect("peeked iterator entry should still be available"),
            ),
            (None, Some(_)) => result.push(
                b.next()
                    .expect("peeked iterator entry should still be available"),
            ),
            (None, None) => break,
        }
    }
    result
}

/// Merge two already-sorted result streams through the production palette policy.
///
/// This feature-only hook gives property tests direct access to the stable tie
/// behavior without exposing the helper in ordinary builds.
#[cfg(feature = "property-tests")]
#[must_use]
pub fn merge_sorted_for_property_test<'a>(
    a: Vec<ScoredResult<'a>>,
    b: Vec<ScoredResult<'a>>,
    max: usize,
) -> Vec<ScoredResult<'a>> {
    merge_sorted(a, b, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    static LEFT_COMMAND: CommandDef = CommandDef {
        id: "test.left",
        label: "Left",
        category: CommandCategory::App,
        shortcut: None,
    };

    static RIGHT_COMMAND: CommandDef = CommandDef {
        id: "test.right",
        label: "Right",
        category: CommandCategory::App,
        shortcut: None,
    };

    fn command_result(command: &'static CommandDef, score: u32) -> ScoredResult<'static> {
        ScoredResult {
            item: SearchResultItem::Command(command),
            score,
            source_ordinal: 0,
        }
    }

    #[test]
    fn merge_sorted_honors_zero_max_even_with_available_results() {
        let results = merge_sorted(
            vec![command_result(&LEFT_COMMAND, 10)],
            vec![command_result(&RIGHT_COMMAND, 9)],
            0,
        );

        assert!(results.is_empty());
    }

    #[test]
    fn merge_sorted_keeps_descending_scores_and_prefers_left_ties() {
        let results = merge_sorted(
            vec![
                command_result(&LEFT_COMMAND, 10),
                command_result(&LEFT_COMMAND, 8),
            ],
            vec![
                command_result(&RIGHT_COMMAND, 9),
                command_result(&RIGHT_COMMAND, 8),
            ],
            4,
        );
        let scores: Vec<u32> = results.iter().map(|result| result.score).collect();

        assert_eq!(scores, vec![10, 9, 8, 8]);
        match results[2].item {
            SearchResultItem::Command(command) => assert_eq!(command.id, "test.left"),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => {
                panic!("expected command result");
            }
        }
    }
}
