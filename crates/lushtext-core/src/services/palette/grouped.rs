// SPDX-License-Identifier: GPL-3.0-or-later

//! GTK-free source grouping, priority, and canonical-file deduplication.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::model::palette::{
    PaletteFileEntry, PaletteNoteCategory, PaletteNoteEntry, PaletteSearchRow, SearchMode,
    SearchResultItem,
};

use super::commands::{search_commands_cancellable, search_open_files_cancellable};
use super::index::FileIndex;
use super::notes::search_note_entries_cancellable;
use super::runtime::{PaletteSearchCancellation, PaletteSearchMetrics, PaletteSearchOutcome};

/// Borrowed source and policy snapshot for one grouped palette search.
#[derive(Clone, Copy)]
pub struct GroupedSearchInput<'a> {
    /// Shared workspace file index snapshot.
    pub index: &'a FileIndex,
    /// Open file-backed tabs in visible source order.
    pub open_tabs: &'a [PaletteFileEntry],
    /// Note and bookmark rows in source order.
    pub note_entries: &'a [PaletteNoteEntry],
    /// Visible label for the workspace file group.
    pub workspace_group_label: &'a str,
    /// Current fuzzy query.
    pub query: &'a str,
    /// Source category selected by the user.
    pub mode: SearchMode,
    /// Maximum retained result rows for each source.
    pub max_per_source: usize,
}

struct FileGroupInput<'a> {
    index: &'a FileIndex,
    open_tabs: &'a [PaletteFileEntry],
    workspace_group_label: &'a str,
    query: &'a str,
    max_per_source: usize,
    cancellation: &'a PaletteSearchCancellation,
}

/// Search and group every source while preserving the existing visible policy.
#[must_use]
pub fn grouped_search(
    input: GroupedSearchInput<'_>,
    cancellation: &PaletteSearchCancellation,
) -> PaletteSearchOutcome<Vec<PaletteSearchRow>> {
    let GroupedSearchInput {
        index,
        open_tabs,
        note_entries,
        workspace_group_label,
        query,
        mode,
        max_per_source,
    } = input;
    let file_input = FileGroupInput {
        index,
        open_tabs,
        workspace_group_label,
        query,
        max_per_source,
        cancellation,
    };
    let mut rows = Vec::new();
    let mut metrics = PaletteSearchMetrics::default();

    macro_rules! completed {
        ($outcome:expr) => {
            match $outcome {
                PaletteSearchOutcome::Complete {
                    value,
                    metrics: source_metrics,
                } => {
                    metrics.merge(source_metrics);
                    if cancellation.is_cancelled() {
                        return PaletteSearchOutcome::Cancelled { metrics };
                    }
                    value
                }
                PaletteSearchOutcome::Cancelled {
                    metrics: source_metrics,
                } => {
                    metrics.merge(source_metrics);
                    return PaletteSearchOutcome::Cancelled { metrics };
                }
            }
        };
    }

    match mode {
        SearchMode::Files => {
            if let Err(cancelled) = append_file_groups(&mut rows, &mut metrics, &file_input) {
                return cancelled;
            }
        }
        SearchMode::Notes => {
            for category in PaletteNoteCategory::ALL {
                let results = completed!(search_note_entries_cancellable(
                    note_entries,
                    Some(category),
                    query,
                    max_per_source,
                    cancellation,
                ));
                append_group(&mut rows, category.label(), note_rows_from_results(results));
            }
        }
        SearchMode::Commands => {
            let results = completed!(search_commands_cancellable(
                query,
                max_per_source,
                cancellation,
            ));
            rows.extend(command_rows_from_results(results));
        }
        SearchMode::All => {
            if let Err(cancelled) = append_file_groups(&mut rows, &mut metrics, &file_input) {
                return cancelled;
            }
            for category in PaletteNoteCategory::ALL {
                let results = completed!(search_note_entries_cancellable(
                    note_entries,
                    Some(category),
                    query,
                    max_per_source,
                    cancellation,
                ));
                append_group(
                    &mut rows,
                    category.all_mode_label(),
                    note_rows_from_results(results),
                );
            }
            let results = completed!(search_commands_cancellable(
                query,
                max_per_source,
                cancellation,
            ));
            append_group(&mut rows, "Commands", command_rows_from_results(results));
        }
    }

    if cancellation.is_cancelled() {
        PaletteSearchOutcome::Cancelled { metrics }
    } else {
        PaletteSearchOutcome::Complete {
            value: rows,
            metrics,
        }
    }
}

fn append_file_groups(
    rows: &mut Vec<PaletteSearchRow>,
    metrics: &mut PaletteSearchMetrics,
    input: &FileGroupInput<'_>,
) -> Result<(), PaletteSearchOutcome<Vec<PaletteSearchRow>>> {
    let open_canonical_paths: HashSet<PathBuf> = input
        .open_tabs
        .iter()
        .filter_map(|file| file.identity.canonical_path().map(PathBuf::from))
        .collect();
    let open_results = match search_open_files_cancellable(
        input.open_tabs,
        input.query,
        input.max_per_source,
        input.cancellation,
    ) {
        PaletteSearchOutcome::Complete {
            value,
            metrics: source_metrics,
        } => {
            metrics.merge(source_metrics);
            if input.cancellation.is_cancelled() {
                return Err(PaletteSearchOutcome::Cancelled { metrics: *metrics });
            }
            value
        }
        PaletteSearchOutcome::Cancelled {
            metrics: source_metrics,
        } => {
            metrics.merge(source_metrics);
            return Err(PaletteSearchOutcome::Cancelled { metrics: *metrics });
        }
    };
    let open_rows = open_results
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::OpenFile(file) => Some(PaletteSearchRow::File {
                display_name: file.display_name.clone(),
                subtitle: file.subtitle.clone(),
                file_path: file.path.clone(),
            }),
            SearchResultItem::File(_)
            | SearchResultItem::Command(_)
            | SearchResultItem::Note(_) => None,
        })
        .collect();
    append_group(rows, "Open Tabs", open_rows);

    let workspace_results = match input.index.search_cancellable_excluding(
        input.query,
        input.max_per_source,
        &open_canonical_paths,
        input.cancellation,
    ) {
        PaletteSearchOutcome::Complete {
            value,
            metrics: source_metrics,
        } => {
            metrics.merge(source_metrics);
            if input.cancellation.is_cancelled() {
                return Err(PaletteSearchOutcome::Cancelled { metrics: *metrics });
            }
            value
        }
        PaletteSearchOutcome::Cancelled {
            metrics: source_metrics,
        } => {
            metrics.merge(source_metrics);
            return Err(PaletteSearchOutcome::Cancelled { metrics: *metrics });
        }
    };
    let workspace_rows = workspace_results
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::File(file) => Some(PaletteSearchRow::File {
                display_name: file.name.clone(),
                subtitle: file.relative_display(),
                file_path: file.path.clone(),
            }),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::Command(_)
            | SearchResultItem::Note(_) => None,
        })
        .collect();
    append_group(rows, input.workspace_group_label, workspace_rows);
    Ok(())
}

fn note_rows_from_results(
    results: Vec<crate::model::palette::ScoredResult<'_>>,
) -> Vec<PaletteSearchRow> {
    results
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Note(note) => Some(PaletteSearchRow::Note {
                display_name: note.title.clone(),
                subtitle: note.display_subtitle(),
                target: note.target.clone(),
            }),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Command(_) => None,
        })
        .collect()
}

fn command_rows_from_results(
    results: Vec<crate::model::palette::ScoredResult<'static>>,
) -> Vec<PaletteSearchRow> {
    results
        .into_iter()
        .filter_map(|result| match result.item {
            SearchResultItem::Command(command) => Some(PaletteSearchRow::Command {
                display_name: command.label.to_string(),
                subtitle: command.display_subtitle(),
                action_id: command.id.to_string(),
            }),
            SearchResultItem::OpenFile(_)
            | SearchResultItem::File(_)
            | SearchResultItem::Note(_) => None,
        })
        .collect()
}

fn append_group(rows: &mut Vec<PaletteSearchRow>, label: &str, group: Vec<PaletteSearchRow>) {
    if group.is_empty() {
        return;
    }
    rows.push(PaletteSearchRow::Header {
        label: label.to_string(),
    });
    rows.extend(group);
}
