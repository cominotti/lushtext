// SPDX-License-Identifier: GPL-3.0-or-later

//! Compact command-palette search requests and test instrumentation.

use std::sync::Arc;

use crate::model::palette::{PaletteFileEntry, PaletteNoteEntry, PaletteSearchRow, SearchMode};
use crate::services::palette::{
    FileIndex, GroupedSearchInput, PaletteSearchCancellation, PaletteSearchOutcome, grouped_search,
};
use crate::ui::plain_disposal::DisposalOwned;

/// One compact query plus shared source snapshots retained by the latest slot.
pub(super) struct CommandPaletteSearchRequest {
    pub query: Arc<str>,
    pub mode: SearchMode,
    pub index: Arc<DisposalOwned<FileIndex>>,
    pub open_tabs: Arc<[PaletteFileEntry]>,
    pub note_entries: Arc<DisposalOwned<Box<[PaletteNoteEntry]>>>,
    pub workspace_group_label: Arc<str>,
}

pub(super) fn execute_search(
    request: &CommandPaletteSearchRequest,
    cancellation: &PaletteSearchCancellation,
    max_per_source: usize,
) -> PaletteSearchOutcome<Vec<PaletteSearchRow>> {
    delay_search_for_test();
    grouped_search(
        GroupedSearchInput {
            index: request.index.as_ref(),
            open_tabs: &request.open_tabs,
            note_entries: request.note_entries.as_ref(),
            workspace_group_label: &request.workspace_group_label,
            query: &request.query,
            mode: request.mode,
            max_per_source,
        },
        cancellation,
    )
}

#[cfg(feature = "test-utils")]
static SEARCH_DELAY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(feature = "test-utils")]
pub fn set_search_delay_for_test(delay_ms: u64) {
    SEARCH_DELAY_MS.store(delay_ms, std::sync::atomic::Ordering::Release);
}

fn delay_search_for_test() {
    #[cfg(feature = "test-utils")]
    {
        let delay_ms = SEARCH_DELAY_MS.load(std::sync::atomic::Ordering::Acquire);
        if delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        }
    }
}
