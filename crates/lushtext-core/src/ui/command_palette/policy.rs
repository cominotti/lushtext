// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decision logic for the command palette workflow.
//!
//! Everything here is free of GTK-family imports so the default mutation scope
//! reaches it through the `ui/**/policy.rs` convention. The palette owns two
//! ordered stage orders and this module holds the decisions both of them make:
//!
//! * [`FileIndexUpdate`] and [`IndexUpdateAdmission`] — bounded retention of
//!   queued index mutations, including the `Vec` capacity-growth byte estimate
//!   and the escalate-to-full-rebuild decision on overflow.
//! * [`FileIndexUpdateBatchKind`] plus [`index_flush_is_blocked`] — which batch
//!   one flush turn builds, and when a flush must not start at all.
//! * [`FileIndexMutationTicket`] plus [`FileIndexMutationFacts`] — the mutation
//!   seam's freshness identity, validated as one unit by
//!   [`FileIndexMutationTicket::is_current`], with the replay-on-loss decision
//!   expressed by [`FileIndexMutationTicket::arbitrate`].
//! * [`classify_index_retirement`] — the last-owned-at-cap retirement predicate
//!   the three retirement call sites share.
//! * [`first_activatable`] and [`next_activatable`] — header-skipping result
//!   navigation over an activatable-flag sequence.
//! * [`no_results_visible`], [`result_count_text`], and
//!   [`accessible_value_text`] — the palette's presentation decisions.
//! * The category view (`drill-down-palette-categories`): the "More" row's
//!   wording ([`more_row_text`], [`more_row_accessible_name`]), the category
//!   view's row limit ([`category_row_limit`] over
//!   [`PALETTE_CATEGORY_RETAINED_BYTES`]), its navigation without a walk
//!   ([`next_category_row`], [`page_activatable`]), the selection on leaving
//!   ([`leave_selection`]), when Backspace leaves
//!   ([`backspace_leaves_category`]), and its title, notices and
//!   announcement.

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::list_budget::{grouped_thousands, rows_within};
use crate::model::palette::PaletteCategory;
use crate::services::palette::{
    FileIndex, FileIndexMutationLedger, MAX_INDEXED_FILES, PALETTE_CATEGORY_CANDIDATE_BYTES,
};

/// Debounce interval for handing incremental index updates to the worker.
///
/// Seventy-five milliseconds coalesces rapid sidebar mutations while keeping
/// file creation, deletion, and rename projections responsive.
pub const INDEX_UPDATE_DEBOUNCE_MS: u64 = 75;

/// Debounce interval for non-empty palette query text.
///
/// Empty queries deliberately bypass this so clearing the entry repaints the
/// default result list immediately.
pub const SEARCH_DEBOUNCE_MS: u64 = 150;

/// Maximum fuzzy matches to show from any one source group.
///
/// The palette already caps visible results at a small, scannable list; keeping
/// the same cap per source prevents one group from monopolizing mixed results
/// while staying cheap for list-model replacement and keyboard navigation.
///
/// Kind: presentation cap; 50 rounded up to a power of two
/// (`power-of-two-thresholds` D12; its bound measured in task 9.5 M1).
pub const MAX_RESULTS_PER_SOURCE: usize = 64;

/// Rows Page Up and Page Down move the selection, in both views.
///
/// Kind: out: presentation; about one viewport of 400 px at the rows' height.
pub const RESULT_PAGE_ROWS: u32 = 16;

/// Bytes a category view's ranking may hold at its peak, on the worker
/// (`drill-down-palette-categories` D3).
///
/// Kind: presentation cap; up from `PALETTE_CATEGORY_FLOOR_RETENTION`, the
/// power of two at or above it.
pub const PALETTE_CATEGORY_RETAINED_BYTES: u64 = 8 * 1024 * 1024;

/// The ranking peak of the file index's largest admitted size, so the
/// workspace category view of a full index is never cut.
///
/// Kind: derived; `MAX_INDEXED_FILES * PALETTE_CATEGORY_CANDIDATE_BYTES`.
pub const PALETTE_CATEGORY_FLOOR_RETENTION: u64 =
    MAX_INDEXED_FILES as u64 * PALETTE_CATEGORY_CANDIDATE_BYTES;

// No loss at today's ceiling: the whole index fits the budget.
const _: () = assert!(PALETTE_CATEGORY_FLOOR_RETENTION <= PALETTE_CATEGORY_RETAINED_BYTES);
// And the budget is the power of two at or above the floor, not more: a change
// of the candidate's size that leaves it further above (or below) fails here.
const _: () = assert!(PALETTE_CATEGORY_RETAINED_BYTES / 2 < PALETTE_CATEGORY_FLOOR_RETENTION);

/// Rows a category view lists at most: the budget's equal-charge prefix.
#[must_use]
pub fn category_row_limit() -> usize {
    rows_within(
        PALETTE_CATEGORY_RETAINED_BYTES,
        PALETTE_CATEGORY_CANDIDATE_BYTES,
    )
}

/// The "More" row's visible text: `"{N} more…"`, N grouped in thousands.
#[must_use]
pub fn more_row_text(hidden: usize) -> String {
    format!("{} more…", grouped_thousands(hidden))
}

/// The "More" row's accessible name.
#[must_use]
pub fn more_row_accessible_name(hidden: usize, category_label: &str) -> String {
    format!(
        "Show {} more results in {category_label}",
        grouped_thousands(hidden)
    )
}

/// The "More" row's accessible description.
#[must_use]
pub fn more_row_description(category_label: &str) -> String {
    format!("Opens a list of every result in {category_label}")
}

/// The category view's title: `"{category} · {N} results"`.
#[must_use]
pub fn category_title(category_label: &str, matching: usize) -> String {
    let noun = if matching == 1 { "result" } else { "results" };
    format!("{category_label} · {} {noun}", grouped_thousands(matching))
}

/// The category list's accessible label.
#[must_use]
pub fn category_list_label(category_label: &str) -> String {
    format!("Command palette results in {category_label}")
}

/// The polite announcement raised once when a category view opens.
#[must_use]
pub fn category_entry_announcement(category_label: &str) -> String {
    format!("{category_label}: showing all results. Escape returns to all results.")
}

/// The budget notice when the row limit cut a category: `"Showing X of Y
/// results"`, or `None` when every match is listed.
#[must_use]
pub fn category_budget_notice(listed: usize, matching: usize) -> Option<String> {
    (matching > listed).then(|| {
        format!(
            "Showing {} of {} results",
            grouped_thousands(listed),
            grouped_thousands(matching)
        )
    })
}

/// The notice of a note category whose source a byte ceiling cut.
#[must_use]
pub fn note_source_notice(held: usize, found: usize, found_is_lower_bound: bool) -> String {
    let at_least = if found_is_lower_bound {
        "at least "
    } else {
        ""
    };
    format!(
        "Some notes are not searchable: the note source holds {} of {at_least}{} notes",
        grouped_thousands(held),
        grouped_thousands(found)
    )
}

/// Next selection in a category view, which has no headers: one step or a
/// page in `delta`'s direction, clamped to `[0, len)`. O(1).
#[must_use]
pub fn next_category_row(current: Option<u32>, len: u32, delta: i32) -> Option<u32> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let Some(current) = current.filter(|current| *current < len) else {
        return Some(if delta < 0 { last } else { 0 });
    };
    let step = delta.unsigned_abs();
    Some(if delta < 0 {
        current.saturating_sub(step)
    } else {
        current.saturating_add(step).min(last)
    })
}

/// Where a category view's selection goes when a ranking is published: a
/// republish of the **same query** (a source changed: the index replaced, a
/// file created by a build, the note source refreshed) keeps the selected
/// row, clamped to the new length, so background churn never sends a user
/// browsing deep in a long category back to the top; the first publish after
/// entering and a new query select the first row (review finding R2 of
/// `drill-down-palette-categories`). Returns the row and whether to scroll to
/// it (only when it moved), or `None` for an empty list. O(1).
#[must_use]
pub fn category_publish_selection(
    same_query: bool,
    selected: Option<u32>,
    len: u32,
) -> Option<(u32, bool)> {
    let last = len.checked_sub(1)?;
    match selected {
        Some(position) if same_query => {
            let kept = position.min(last);
            Some((kept, kept != position))
        }
        _ => Some((0, true)),
    }
}

/// A page move in the grouped view: `rows` rows in `delta`'s sign, landing on
/// the nearest activatable row in the move's direction (else the nearest one
/// back toward the start), so headers are skipped.
#[must_use]
pub fn page_activatable(activatable: &[bool], current: u32, delta: i32) -> Option<u32> {
    let len = u32::try_from(activatable.len()).unwrap_or(u32::MAX);
    if len == 0 {
        return None;
    }
    if current >= len {
        return first_activatable(activatable);
    }
    let step = delta.unsigned_abs();
    let target = if delta < 0 {
        current.saturating_sub(step)
    } else {
        current.saturating_add(step).min(len - 1)
    };
    let is_activatable =
        |position: u32| usize::try_from(position).is_ok_and(|index| activatable[index]);
    if is_activatable(target) {
        return Some(target);
    }
    let direction = if delta < 0 { -1 } else { 1 };
    match next_activatable(activatable, target, direction) {
        Some(position) if position != target => Some(position),
        _ => next_activatable(activatable, target, -direction)
            .filter(|position| is_activatable(*position)),
    }
}

/// What one grouped row is, for [`leave_selection`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupedRowKind {
    /// A group header (not activatable).
    Header,
    /// A result row of a category.
    Result(PaletteCategory),
    /// A category's "More" row.
    More(PaletteCategory),
}

/// Where the selection goes when the grouped rows arrive after leaving
/// `category`: its "More" row if it is still cut, else its last row, else the
/// first activatable row.
#[must_use]
pub fn leave_selection(rows: &[GroupedRowKind], category: PaletteCategory) -> Option<u32> {
    let position = |index: usize| u32::try_from(index).ok();
    if let Some(index) = rows
        .iter()
        .position(|row| *row == GroupedRowKind::More(category))
    {
        return position(index);
    }
    if let Some(index) = rows
        .iter()
        .rposition(|row| *row == GroupedRowKind::Result(category))
    {
        return position(index);
    }
    rows.iter()
        .position(|row| *row != GroupedRowKind::Header)
        .and_then(position)
}

/// Whether a Backspace press leaves the category view: only on a query that
/// was already empty, with a key release seen since it became empty, so the
/// auto-repeat of the Backspace that emptied it (presses without releases)
/// does not leave (`drill-down-palette-categories` D5).
#[must_use]
pub const fn backspace_leaves_category(
    query_is_empty: bool,
    release_seen_since_empty: bool,
) -> bool {
    query_is_empty && release_seen_since_empty
}

/// Tracks [`backspace_leaves_category`]'s second input from the key events
/// the capture controller sees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BackspaceLeaveTracker {
    release_seen_since_empty: bool,
}

impl BackspaceLeaveTracker {
    /// The query changed: a non-empty query forgets any release, so the
    /// count starts again when it next becomes empty.
    pub fn query_changed(&mut self, query_is_empty: bool) {
        if !query_is_empty {
            self.release_seen_since_empty = false;
        }
    }

    /// A key was released while the query was `query_is_empty`.
    pub fn key_released(&mut self, query_is_empty: bool) {
        if query_is_empty {
            self.release_seen_since_empty = true;
        }
    }

    /// A Backspace press: whether it leaves the category view.
    #[must_use]
    pub const fn backspace_pressed(&self, query_is_empty: bool) -> bool {
        backspace_leaves_category(query_is_empty, self.release_seen_since_empty)
    }

    /// Forget everything (entering or leaving a category view). An empty
    /// query on entry counts as released: the key that activated the More row
    /// is not a Backspace.
    pub fn reset(&mut self, query_is_empty: bool) {
        self.release_seen_since_empty = query_is_empty;
    }
}

/// Maximum incremental mutations retained before overflow escalates to a rebuild.
///
/// Kind: cap; a power of two, rounded down.
pub const MAX_PENDING_INDEX_UPDATES: usize = 1_024;

/// Maximum exact retained bytes the pending mutation queue may own.
///
/// Kind: cap; a power of two, rounded down.
pub const MAX_PENDING_INDEX_UPDATE_BYTES: u64 = 4 * 1024 * 1024;

/// Smallest capacity the pending queue's growth estimate assumes.
///
/// `Vec` grows by doubling from a small floor, so the estimate is
/// `max(capacity, this) * 2`. The admission decision must charge for that
/// reallocation *before* it happens, or a push that passes the ceiling check can
/// still leave the queue owning more than
/// [`MAX_PENDING_INDEX_UPDATE_BYTES`].
///
/// Kind: cap; a power of two, rounded down.
const PENDING_QUEUE_MIN_CAPACITY: usize = 4;

/// A pending incremental mutation to the palette's file index.
///
/// Sidebar file operations queue these and a short main-loop debounce coalesces
/// bursts before a serialized background worker applies them to the index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileIndexUpdate {
    /// A newly created file that belongs to a known workspace folder.
    Create {
        /// Absolute path of the created file.
        path: PathBuf,
        /// Workspace folder the created file is indexed under.
        workspace_folder: Arc<PathBuf>,
    },
    /// A deleted file, or a directory prefix whose files are all gone.
    Delete(PathBuf),
    /// A renamed file, or a renamed directory prefix.
    Rename {
        /// Path the file had before the rename.
        old_path: PathBuf,
        /// Path the file has after the rename.
        new_path: PathBuf,
    },
}

impl FileIndexUpdate {
    /// Apply this mutation to a worker-owned index clone under its ledger.
    pub(crate) fn apply(&self, index: &mut FileIndex, ledger: &mut FileIndexMutationLedger) {
        match self {
            Self::Create {
                path,
                workspace_folder,
            } => {
                index.add_path_for_bounded_batch(
                    path.clone(),
                    Arc::clone(workspace_folder),
                    ledger,
                );
            }
            Self::Delete(path) => index.remove_path_for_bounded_batch(path, ledger),
            Self::Rename { old_path, new_path } => {
                index.rename_path_for_bounded_batch(old_path, new_path, ledger);
            }
        }
    }

    /// Exact conservative bytes this queued mutation retains.
    #[must_use]
    pub fn retained_byte_weight(&self) -> u64 {
        let path_bytes = |path: &PathBuf| u64::try_from(path.capacity()).unwrap_or(u64::MAX);
        u64::try_from(std::mem::size_of::<Self>())
            .unwrap_or(u64::MAX)
            .saturating_add(match self {
                Self::Create { path, .. } | Self::Delete(path) => path_bytes(path),
                Self::Rename { old_path, new_path } => {
                    path_bytes(old_path).saturating_add(path_bytes(new_path))
                }
            })
    }
}

/// Live queue state the admission decision reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexUpdateQueueState {
    /// Whether bounded overflow has already demoted this queue to a rebuild.
    pub rebuild_pending: bool,
    /// Mutations currently retained.
    pub len: usize,
    /// Allocated slots currently retained.
    pub capacity: usize,
    /// Exact bytes the queue currently owns.
    pub retained_bytes: u64,
}

/// What the workflow does with one newly arrived index mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexUpdateAdmission {
    /// A rebuild is already pending, so individual mutations no longer matter.
    AlreadyRebuilding,
    /// The count or byte ceiling would be exceeded; escalate to a full rebuild.
    EscalateToRebuild,
    /// Retain the mutation, reserving this many extra slots first.
    Retain {
        /// Extra slots to reserve before pushing, or zero when capacity suffices.
        reserve_additional: usize,
        /// Exact bytes the queue owns after the push, including shell growth.
        retained_bytes: u64,
    },
}

/// Decide whether one arriving mutation fits the bounded queue.
///
/// The byte arithmetic charges for `Vec` shell growth *before* the push, because
/// a reallocation that happens after the ceiling check would let the queue own
/// more than [`MAX_PENDING_INDEX_UPDATE_BYTES`]. Overflow escalates to a full
/// filesystem rebuild rather than dropping the mutation, so no filesystem change
/// can be silently lost.
#[must_use]
pub fn admit_index_update(state: IndexUpdateQueueState, update_bytes: u64) -> IndexUpdateAdmission {
    if state.rebuild_pending {
        return IndexUpdateAdmission::AlreadyRebuilding;
    }
    let reserve_additional = pending_queue_growth_slots(state.len, state.capacity);
    let shell_growth =
        u64::try_from(reserve_additional.saturating_mul(std::mem::size_of::<FileIndexUpdate>()))
            .unwrap_or(u64::MAX);
    // An arithmetic overflow is treated exactly like exceeding the ceiling, so
    // the decision never needs to unwrap and never panics.
    let Some(next_bytes) = state
        .retained_bytes
        .checked_add(shell_growth)
        .and_then(|bytes| bytes.checked_add(update_bytes))
    else {
        return IndexUpdateAdmission::EscalateToRebuild;
    };
    if state.len >= MAX_PENDING_INDEX_UPDATES || next_bytes > MAX_PENDING_INDEX_UPDATE_BYTES {
        return IndexUpdateAdmission::EscalateToRebuild;
    }
    IndexUpdateAdmission::Retain {
        reserve_additional,
        retained_bytes: next_bytes,
    }
}

/// Extra slots the next push needs, or zero while spare capacity remains.
fn pending_queue_growth_slots(len: usize, capacity: usize) -> usize {
    if len < capacity {
        return 0;
    }
    capacity
        .max(PENDING_QUEUE_MIN_CAPACITY)
        .saturating_mul(2)
        .saturating_sub(capacity)
}

/// Which kind of batch one flush turn hands to the worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileIndexUpdateBatchKind {
    /// Clone the live index and apply the queued mutations to the clone.
    Incremental,
    /// Discard the queue and rescan the current workspace folders.
    Rebuild,
}

/// Choose the batch kind from the queue's overflow state.
#[must_use]
pub const fn select_batch_kind(rebuild_pending: bool) -> FileIndexUpdateBatchKind {
    if rebuild_pending {
        FileIndexUpdateBatchKind::Rebuild
    } else {
        FileIndexUpdateBatchKind::Incremental
    }
}

/// Whether a flush turn must not start.
///
/// A flush is blocked while the serialized worker still owns the index, and it
/// is pointless when there is neither a queued mutation nor a pending rebuild.
#[must_use]
pub const fn index_flush_is_blocked(
    worker_running: bool,
    queue_is_empty: bool,
    rebuild_pending: bool,
) -> bool {
    worker_running || (queue_is_empty && !rebuild_pending)
}

/// Identity of one index-mutation batch, captured when it is dispatched.
///
/// The mutation seam is inverted at the worker: the completion resumes on GTK
/// and must decide whether the index it is about to install still descends from
/// the index it cloned. Reifying the base generation here keeps that one
/// comparison a validated value instead of a `==` clause repeated at each
/// arbitration site.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIndexMutationTicket {
    /// Generation of the index this batch cloned from.
    pub base_generation: u64,
    /// Batch kind the worker was dispatched with.
    pub kind: FileIndexUpdateBatchKind,
}

/// Live index state observed on GTK when a mutation worker completion resumes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIndexMutationFacts {
    /// Generation the installed index currently carries.
    pub live_generation: u64,
}

/// What a resuming mutation completion is allowed to do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileIndexMutationArbitration {
    /// The batch still descends from the live index; install it.
    Accept {
        /// Generation the installed index takes.
        next_generation: u64,
    },
    /// A full replacement won the race; discard and replay through a rebuild.
    RejectAndReplay,
}

impl FileIndexMutationTicket {
    /// Open one batch against the generation it cloned from.
    #[must_use]
    pub const fn new(base_generation: u64, kind: FileIndexUpdateBatchKind) -> Self {
        Self {
            base_generation,
            kind,
        }
    }

    /// Whether the live index is still the one this batch cloned from.
    #[must_use]
    pub const fn is_current(&self, facts: FileIndexMutationFacts) -> bool {
        self.base_generation == facts.live_generation
    }

    /// Decide install-or-replay for one resuming worker completion.
    ///
    /// Rejection requests a rebuild rather than dropping the batch, because the
    /// mutations this worker applied are not recorded anywhere else: the queue
    /// was already drained when the batch was built.
    #[must_use]
    pub const fn arbitrate(&self, facts: FileIndexMutationFacts) -> FileIndexMutationArbitration {
        if self.is_current(facts) {
            FileIndexMutationArbitration::Accept {
                next_generation: self.base_generation.wrapping_add(1),
            }
        } else {
            FileIndexMutationArbitration::RejectAndReplay
        }
    }
}

/// Which retirement lane a released file index belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileIndexRetirementKind {
    /// A whole index replaced by a workspace-folder change.
    FullReplacement,
    /// The base index a winning incremental batch superseded.
    AcceptedIncremental,
    /// The output of an incremental batch that lost its generation race.
    RejectedIncremental,
}

/// Whether one released index reached the bounded worker lane at the policy cap.
///
/// Only a *last-owned* index at [`MAX_INDEXED_FILES`] is worth recording: a
/// shared index is still alive, and an index below the cap is small enough that
/// the lane is not what a test is proving. Returning the kind rather than a bool
/// keeps the three call sites from re-deriving the conjunction.
#[must_use]
pub const fn classify_index_retirement(
    kind: FileIndexRetirementKind,
    last_owned: bool,
    released_len: usize,
) -> Option<FileIndexRetirementKind> {
    if last_owned && released_len == MAX_INDEXED_FILES {
        Some(kind)
    } else {
        None
    }
}

/// First row that can actually be opened or executed.
///
/// Expressed over an activatable-flag sequence rather than a `gio::ListStore`
/// so result navigation is decidable without GTK.
#[must_use]
pub fn first_activatable(activatable: &[bool]) -> Option<u32> {
    activatable
        .iter()
        .position(|flag| *flag)
        .and_then(|index| u32::try_from(index).ok())
}

/// Next activatable row in `delta`'s direction, skipping source headers.
///
/// A scan that runs off the end falls back to the current row when that row is
/// itself activatable, so a keypress at a boundary holds selection instead of
/// clearing it. A selection index at or past the end restarts from the first
/// activatable row, which is how GTK's unset selection is handled.
#[must_use]
pub fn next_activatable(activatable: &[bool], current: u32, delta: i32) -> Option<u32> {
    let len = u32::try_from(activatable.len()).unwrap_or(u32::MAX);
    if len == 0 {
        return None;
    }
    if current >= len {
        return first_activatable(activatable);
    }
    let is_activatable =
        |position: u32| usize::try_from(position).is_ok_and(|index| activatable[index]);

    if delta > 0 {
        let mut position = current.saturating_add(1);
        while position < len {
            if is_activatable(position) {
                return Some(position);
            }
            position = position.saturating_add(1);
        }
    } else {
        let mut position = current.saturating_sub(1);
        loop {
            if is_activatable(position) {
                return Some(position);
            }
            if position == 0 {
                break;
            }
            position = position.saturating_sub(1);
        }
    }

    Some(current).filter(|position| is_activatable(*position))
}

/// Whether the "No results" status line should be shown.
///
/// It appears only for a non-empty query that produced no activatable row: an
/// empty query showing default rows is not a no-results state.
#[must_use]
pub const fn no_results_visible(has_activatable_results: bool, query_is_empty: bool) -> bool {
    !has_activatable_results && !query_is_empty
}

/// Pluralized result count for the results list's accessible value.
#[must_use]
pub fn result_count_text(activatable_count: usize) -> String {
    match activatable_count {
        0 => "No command palette results".to_string(),
        1 => "1 command palette result".to_string(),
        count => format!("{count} command palette results"),
    }
}

/// Accessible value text for the results list.
///
/// Precedence is selection first, then in-progress search, then the result
/// count: a screen-reader user who has moved the selection wants to hear where
/// they are, not how many rows exist.
#[must_use]
pub fn accessible_value_text(
    selected_display_name: Option<&str>,
    searching: bool,
    activatable_count: usize,
) -> String {
    if let Some(name) = selected_display_name {
        format!("Selected {name}")
    } else if searching {
        "Searching command palette".to_string()
    } else {
        result_count_text(activatable_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files shown for one query when the workspace index holds `count`
    /// matching files and the palette caps each category at
    /// [`MAX_RESULTS_PER_SOURCE`].
    fn file_rows_shown(count: usize) -> usize {
        use crate::model::palette::{
            IndexedFile, PaletteFileIdentity, PaletteSearchRow, SearchMode,
        };
        use crate::services::palette::{
            GroupedSearchInput, PaletteSearchCancellation, PaletteSearchOutcome, grouped_search,
        };
        let folder = Arc::new(PathBuf::from("/workspace"));
        let index = FileIndex::from(
            (0..count)
                .map(|n| {
                    let path = folder.join(format!("match_{n:05}.rs"));
                    IndexedFile::new(
                        path.clone(),
                        PaletteFileIdentity::canonical(path),
                        Arc::clone(&folder),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let PaletteSearchOutcome::Complete { value: rows, .. } = grouped_search(
            GroupedSearchInput {
                index: &index,
                open_tabs: &[],
                note_entries: &[],
                workspace_group_label: "Workspace",
                query: "match",
                mode: SearchMode::Files,
                max_per_source: MAX_RESULTS_PER_SOURCE,
            },
            &PaletteSearchCancellation::default(),
        ) else {
            panic!("a fresh grouped search completes");
        };
        rows.iter()
            .filter(|row| matches!(row, PaletteSearchRow::File { .. }))
            .count()
    }

    /// No loss against `main` (`power-of-two-thresholds` D12): `main` showed 50
    /// matches per category; the presentation cap rounds up to 64.
    #[test]
    fn no_loss_a_palette_category_shows_every_match_main_showed() {
        assert_eq!(file_rows_shown(50), 50);
        assert_eq!(file_rows_shown(64), 64);
        assert_eq!(file_rows_shown(65), 64);
    }

    fn queue_state(len: usize, capacity: usize, retained_bytes: u64) -> IndexUpdateQueueState {
        IndexUpdateQueueState {
            rebuild_pending: false,
            len,
            capacity,
            retained_bytes,
        }
    }

    fn create_update() -> FileIndexUpdate {
        FileIndexUpdate::Create {
            path: PathBuf::from("/workspace/notes/todo.md"),
            workspace_folder: Arc::new(PathBuf::from("/workspace")),
        }
    }

    #[test]
    fn declared_queue_ceilings_are_the_documented_values() {
        // The byte ceiling is spelled `4 * 1024 * 1024`, so pin the product
        // rather than the expression: an arithmetic slip there silently changes
        // how much sidebar churn the queue absorbs before escalating.
        assert_eq!(MAX_PENDING_INDEX_UPDATE_BYTES, 0x0040_0000);
        assert_eq!(MAX_PENDING_INDEX_UPDATES, 1_024);
        assert_eq!(INDEX_UPDATE_DEBOUNCE_MS, 75);
        assert_eq!(SEARCH_DEBOUNCE_MS, 150);
        assert_eq!(MAX_RESULTS_PER_SOURCE, 64);
    }

    #[test]
    fn each_update_kind_actually_mutates_the_index() {
        use crate::model::palette::{IndexedFile, PaletteFileIdentity};

        let folder = Arc::new(PathBuf::from("/workspace"));
        let indexed = |name: &str| {
            let path = folder.join(name);
            IndexedFile::new(
                path.clone(),
                PaletteFileIdentity::canonical(path),
                Arc::clone(&folder),
            )
        };
        let base = FileIndex::from(vec![indexed("one.rs"), indexed("two.rs")]);

        let mut created = base.clone();
        let mut ledger = created.incremental_mutation_ledger();
        FileIndexUpdate::Create {
            path: folder.join("three.rs"),
            workspace_folder: Arc::clone(&folder),
        }
        .apply(&mut created, &mut ledger);
        assert_eq!(created.len(), 3, "Create must add a file");

        let mut deleted = base.clone();
        let mut ledger = deleted.incremental_mutation_ledger();
        FileIndexUpdate::Delete(folder.join("one.rs")).apply(&mut deleted, &mut ledger);
        assert_eq!(deleted.len(), 1, "Delete must remove a file");

        let mut renamed = base;
        let mut ledger = renamed.incremental_mutation_ledger();
        FileIndexUpdate::Rename {
            old_path: folder.join("one.rs"),
            new_path: folder.join("renamed.rs"),
        }
        .apply(&mut renamed, &mut ledger);
        assert_eq!(renamed.len(), 2, "Rename keeps the count");
        assert!(
            renamed
                .files()
                .iter()
                .any(|file| file.path == folder.join("renamed.rs")),
            "Rename must move the path"
        );
        assert!(
            !renamed
                .files()
                .iter()
                .any(|file| file.path == folder.join("one.rs")),
            "Rename must retire the old path"
        );
    }

    #[test]
    fn rebuild_pending_queue_ignores_further_updates() {
        let mut state = queue_state(0, 0, 0);
        state.rebuild_pending = true;
        assert_eq!(
            admit_index_update(state, 64),
            IndexUpdateAdmission::AlreadyRebuilding
        );
    }

    #[test]
    fn empty_queue_reserves_twice_the_capacity_floor() {
        let IndexUpdateAdmission::Retain {
            reserve_additional,
            retained_bytes,
        } = admit_index_update(queue_state(0, 0, 0), 64)
        else {
            panic!("an empty queue must retain");
        };
        // `Vec` growth is `max(floor) * 2`, so an unallocated queue reserves 8.
        let expected_slots = PENDING_QUEUE_MIN_CAPACITY * 2;
        assert_eq!(reserve_additional, expected_slots);
        let expected_shell =
            u64::try_from(expected_slots * size_of::<FileIndexUpdate>()).expect("fits u64");
        assert_eq!(retained_bytes, expected_shell + 64);
    }

    #[test]
    fn spare_capacity_charges_no_shell_growth() {
        assert_eq!(
            admit_index_update(queue_state(1, 4, 500), 64),
            IndexUpdateAdmission::Retain {
                reserve_additional: 0,
                retained_bytes: 564,
            }
        );
    }

    #[test]
    fn full_capacity_doubles_and_charges_the_difference() {
        let IndexUpdateAdmission::Retain {
            reserve_additional, ..
        } = admit_index_update(queue_state(8, 8, 500), 64)
        else {
            panic!("a full-but-small queue must retain");
        };
        assert_eq!(reserve_additional, 8);
    }

    #[test]
    fn count_cap_boundary_at_under_and_over() {
        let under = MAX_PENDING_INDEX_UPDATES - 1;
        assert!(matches!(
            admit_index_update(queue_state(under, MAX_PENDING_INDEX_UPDATES, 0), 64),
            IndexUpdateAdmission::Retain { .. }
        ));
        assert_eq!(
            admit_index_update(
                queue_state(MAX_PENDING_INDEX_UPDATES, MAX_PENDING_INDEX_UPDATES, 0),
                64
            ),
            IndexUpdateAdmission::EscalateToRebuild
        );
        assert_eq!(
            admit_index_update(
                queue_state(
                    MAX_PENDING_INDEX_UPDATES + 1,
                    MAX_PENDING_INDEX_UPDATES + 1,
                    0
                ),
                64
            ),
            IndexUpdateAdmission::EscalateToRebuild
        );
    }

    #[test]
    fn byte_cap_boundary_at_under_and_over() {
        let capacity = 64;
        let at = admit_index_update(
            queue_state(1, capacity, MAX_PENDING_INDEX_UPDATE_BYTES - 64),
            64,
        );
        assert_eq!(
            at,
            IndexUpdateAdmission::Retain {
                reserve_additional: 0,
                retained_bytes: MAX_PENDING_INDEX_UPDATE_BYTES,
            },
            "exactly at the ceiling is still admitted"
        );
        assert_eq!(
            admit_index_update(
                queue_state(1, capacity, MAX_PENDING_INDEX_UPDATE_BYTES - 63),
                64
            ),
            IndexUpdateAdmission::EscalateToRebuild,
            "one byte over the ceiling escalates"
        );
        assert!(matches!(
            admit_index_update(
                queue_state(1, capacity, MAX_PENDING_INDEX_UPDATE_BYTES - 65),
                64
            ),
            IndexUpdateAdmission::Retain { .. }
        ));
    }

    #[test]
    fn byte_overflow_escalates_instead_of_wrapping() {
        assert_eq!(
            admit_index_update(queue_state(1, 64, u64::MAX), 64),
            IndexUpdateAdmission::EscalateToRebuild
        );
    }

    #[test]
    fn shell_growth_alone_can_cross_the_byte_ceiling() {
        // Spare capacity is exhausted, so the doubling charge is what tips it.
        let state = queue_state(4, 4, MAX_PENDING_INDEX_UPDATE_BYTES - 1);
        assert_eq!(
            admit_index_update(state, 0),
            IndexUpdateAdmission::EscalateToRebuild
        );
    }

    #[test]
    fn retained_byte_weight_covers_both_rename_paths() {
        let rename = FileIndexUpdate::Rename {
            old_path: PathBuf::from("/workspace/a.md"),
            new_path: PathBuf::from("/workspace/deeply/nested/b.md"),
        };
        let delete = FileIndexUpdate::Delete(PathBuf::from("/workspace/a.md"));
        assert!(
            rename.retained_byte_weight() > delete.retained_byte_weight(),
            "a rename retains two paths and must weigh more than one"
        );
        assert!(create_update().retained_byte_weight() > 0);
    }

    #[test]
    fn batch_kind_follows_rebuild_pending() {
        assert_eq!(
            select_batch_kind(false),
            FileIndexUpdateBatchKind::Incremental
        );
        assert_eq!(select_batch_kind(true), FileIndexUpdateBatchKind::Rebuild);
    }

    #[test]
    fn flush_is_blocked_by_the_worker_or_by_having_nothing_to_do() {
        assert!(index_flush_is_blocked(true, false, true), "worker owns it");
        assert!(index_flush_is_blocked(true, true, false));
        assert!(
            index_flush_is_blocked(false, true, false),
            "nothing queued and no rebuild"
        );
        assert!(
            !index_flush_is_blocked(false, true, true),
            "an empty queue still flushes a pending rebuild"
        );
        assert!(
            !index_flush_is_blocked(false, false, false),
            "a queued mutation flushes"
        );
    }

    #[test]
    fn mutation_ticket_accepts_only_its_own_base_generation() {
        let ticket = FileIndexMutationTicket::new(7, FileIndexUpdateBatchKind::Incremental);
        assert!(ticket.is_current(FileIndexMutationFacts { live_generation: 7 }));
        assert_eq!(
            ticket.arbitrate(FileIndexMutationFacts { live_generation: 7 }),
            FileIndexMutationArbitration::Accept { next_generation: 8 }
        );
    }

    #[test]
    fn mutation_ticket_rejects_every_other_generation() {
        let ticket = FileIndexMutationTicket::new(7, FileIndexUpdateBatchKind::Incremental);
        for live in [0u64, 6, 8, 9, u64::MAX] {
            let facts = FileIndexMutationFacts {
                live_generation: live,
            };
            assert!(!ticket.is_current(facts), "generation {live} is not 7");
            assert_eq!(
                ticket.arbitrate(facts),
                FileIndexMutationArbitration::RejectAndReplay,
                "generation {live} must replay"
            );
        }
    }

    #[test]
    fn mutation_ticket_next_generation_wraps() {
        let ticket = FileIndexMutationTicket::new(u64::MAX, FileIndexUpdateBatchKind::Rebuild);
        assert_eq!(
            ticket.arbitrate(FileIndexMutationFacts {
                live_generation: u64::MAX
            }),
            FileIndexMutationArbitration::Accept { next_generation: 0 }
        );
    }

    #[test]
    fn retirement_classification_needs_last_owned_and_the_cap() {
        let kind = FileIndexRetirementKind::AcceptedIncremental;
        assert_eq!(
            classify_index_retirement(kind, true, MAX_INDEXED_FILES),
            Some(kind)
        );
        assert_eq!(
            classify_index_retirement(kind, false, MAX_INDEXED_FILES),
            None,
            "a shared index is still alive"
        );
        assert_eq!(
            classify_index_retirement(kind, true, MAX_INDEXED_FILES - 1),
            None,
            "below the cap is not the lane under test"
        );
        assert_eq!(
            classify_index_retirement(kind, true, MAX_INDEXED_FILES + 1),
            None,
            "the predicate is equality, not a threshold"
        );
        assert_eq!(classify_index_retirement(kind, false, 0), None);
    }

    #[test]
    fn retirement_classification_preserves_the_lane() {
        for kind in [
            FileIndexRetirementKind::FullReplacement,
            FileIndexRetirementKind::AcceptedIncremental,
            FileIndexRetirementKind::RejectedIncremental,
        ] {
            assert_eq!(
                classify_index_retirement(kind, true, MAX_INDEXED_FILES),
                Some(kind)
            );
        }
    }

    #[test]
    fn first_activatable_skips_leading_headers() {
        assert_eq!(first_activatable(&[]), None);
        assert_eq!(first_activatable(&[false, false]), None);
        assert_eq!(first_activatable(&[false, true, true]), Some(1));
        assert_eq!(first_activatable(&[true]), Some(0));
    }

    #[test]
    fn next_activatable_moves_forward_over_headers() {
        let rows = [false, true, false, false, true];
        assert_eq!(next_activatable(&rows, 1, 1), Some(4));
        assert_eq!(next_activatable(&rows, 0, 1), Some(1));
    }

    #[test]
    fn next_activatable_moves_backward_over_headers() {
        let rows = [false, true, false, false, true];
        assert_eq!(next_activatable(&rows, 4, -1), Some(1));
        assert_eq!(
            next_activatable(&rows, 1, -1),
            Some(1),
            "only headers below, so the activatable current row holds selection"
        );
        assert_eq!(
            next_activatable(&[false, false, true], 1, -1),
            None,
            "only headers below and the current row is itself a header"
        );
    }

    #[test]
    fn next_activatable_holds_the_current_row_at_a_boundary() {
        let rows = [false, true];
        assert_eq!(
            next_activatable(&rows, 1, 1),
            Some(1),
            "no next row, but the current one is activatable"
        );
        let header_last = [true, false];
        assert_eq!(
            next_activatable(&header_last, 1, 1),
            None,
            "no next row and the current one is a header"
        );
    }

    #[test]
    fn next_activatable_treats_only_a_positive_delta_as_forward() {
        // Callers pass -1 or +1, but the direction test is `delta > 0`, so a
        // zero delta scans backward. Pinning that keeps the boundary explicit:
        // a `>=` there would silently make 0 mean "forward".
        let rows = [true, false, true, false, true];
        assert_eq!(
            next_activatable(&rows, 2, 1),
            Some(4),
            "positive is forward"
        );
        assert_eq!(
            next_activatable(&rows, 2, 0),
            Some(0),
            "zero is not forward, so it scans backward"
        );
        assert_eq!(next_activatable(&rows, 2, -1), Some(0));
    }

    #[test]
    fn next_activatable_restarts_from_an_out_of_range_selection() {
        let rows = [false, true, true];
        assert_eq!(next_activatable(&rows, 99, 1), Some(1));
        assert_eq!(next_activatable(&rows, 99, -1), Some(1));
        assert_eq!(next_activatable(&[], 0, 1), None);
        assert_eq!(next_activatable(&[false], 5, -1), None);
    }

    #[test]
    fn no_results_needs_a_non_empty_query() {
        assert!(no_results_visible(false, false));
        assert!(
            !no_results_visible(false, true),
            "empty query shows defaults"
        );
        assert!(!no_results_visible(true, false));
        assert!(!no_results_visible(true, true));
    }

    #[test]
    fn result_count_text_pluralizes() {
        assert_eq!(result_count_text(0), "No command palette results");
        assert_eq!(result_count_text(1), "1 command palette result");
        assert_eq!(result_count_text(2), "2 command palette results");
    }

    #[test]
    fn accessible_value_text_prefers_selection_then_searching_then_count() {
        assert_eq!(
            accessible_value_text(Some("todo.md"), true, 5),
            "Selected todo.md"
        );
        assert_eq!(
            accessible_value_text(None, true, 5),
            "Searching command palette"
        );
        assert_eq!(
            accessible_value_text(None, false, 5),
            "5 command palette results"
        );
        assert_eq!(
            accessible_value_text(None, false, 0),
            "No command palette results"
        );
    }

    // --- drill-down-palette-categories ---

    #[test]
    fn more_row_wording_groups_thousands() {
        assert_eq!(more_row_text(39_936), "39,936 more…");
        assert_eq!(more_row_text(1), "1 more…");
        assert_eq!(
            more_row_accessible_name(39_936, "All Workspaces"),
            "Show 39,936 more results in All Workspaces"
        );
        assert_eq!(
            more_row_description("Commands"),
            "Opens a list of every result in Commands"
        );
        assert_eq!(category_title("Bookmarks", 1), "Bookmarks · 1 result");
        assert_eq!(
            category_title("All Workspaces", 131_040),
            "All Workspaces · 131,040 results"
        );
        assert_eq!(
            category_list_label("Bookmarks"),
            "Command palette results in Bookmarks"
        );
        assert_eq!(
            category_entry_announcement("Open Tabs"),
            "Open Tabs: showing all results. Escape returns to all results."
        );
    }

    #[test]
    fn the_budget_notice_shows_only_a_cut() {
        assert_eq!(category_budget_notice(10, 10), None);
        assert_eq!(
            category_budget_notice(233_016, 300_000).as_deref(),
            Some("Showing 233,016 of 300,000 results")
        );
        assert_eq!(
            note_source_notice(16 * 1024, 20_000, false),
            "Some notes are not searchable: the note source holds 16,384 of 20,000 notes"
        );
        assert_eq!(
            note_source_notice(5, 9, true),
            "Some notes are not searchable: the note source holds 5 of at least 9 notes"
        );
    }

    #[test]
    fn the_category_budget_holds_the_whole_index() {
        assert_eq!(PALETTE_CATEGORY_RETAINED_BYTES, 0x0080_0000, "8 MiB");
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            PALETTE_CATEGORY_FLOOR_RETENTION, 4_718_592,
            "4.5 MiB at 36 bytes"
        );
        assert!(category_row_limit() >= MAX_INDEXED_FILES);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(category_row_limit(), 233_016);
        assert_eq!(RESULT_PAGE_ROWS, 16);
    }

    #[test]
    fn category_rows_step_and_page_with_clamping() {
        assert_eq!(next_category_row(Some(0), 0, 1), None);
        assert_eq!(next_category_row(Some(0), 10, 1), Some(1));
        assert_eq!(next_category_row(Some(0), 10, -1), Some(0));
        assert_eq!(next_category_row(Some(9), 10, 1), Some(9));
        assert_eq!(next_category_row(Some(3), 10, 16), Some(9));
        assert_eq!(next_category_row(Some(12), 128 * 1024, -16), Some(0));
        assert_eq!(next_category_row(Some(20), 128 * 1024, -16), Some(4));
        assert_eq!(next_category_row(None, 10, 1), Some(0));
        assert_eq!(next_category_row(None, 10, -1), Some(9));
        assert_eq!(
            next_category_row(Some(99), 10, 1),
            Some(0),
            "stale selection"
        );
        assert_eq!(
            next_category_row(Some(u32::MAX - 1), u32::MAX, i32::MAX),
            Some(u32::MAX - 1)
        );
    }

    #[test]
    fn grouped_pages_skip_headers_in_the_move_direction() {
        // header, 3 rows, header, 3 rows
        let rows = [false, true, true, true, false, true, true, true];
        assert_eq!(
            page_activatable(&rows, 1, 3),
            Some(5),
            "lands on a header, goes on"
        );
        assert_eq!(
            page_activatable(&rows, 1, 16),
            Some(7),
            "clamps to the last row"
        );
        assert_eq!(
            page_activatable(&rows, 7, -3),
            Some(3),
            "header skipped going up"
        );
        assert_eq!(
            page_activatable(&rows, 7, -16),
            Some(1),
            "row 0 is a header"
        );
        assert_eq!(page_activatable(&rows, 99, 16), Some(1));
        assert_eq!(page_activatable(&[], 0, 16), None);
        assert_eq!(page_activatable(&[false], 0, 16), None);
    }

    #[test]
    fn leaving_selects_the_more_row_then_the_last_row_then_the_first() {
        use crate::model::palette::PaletteNoteCategory;
        use GroupedRowKind::{Header, More, Result};
        let files = PaletteCategory::WorkspaceFiles;
        let notes = PaletteCategory::Note(PaletteNoteCategory::Bookmarks);
        let rows = [
            Header,
            Result(files),
            Result(files),
            More(files),
            Header,
            Result(notes),
        ];
        assert_eq!(leave_selection(&rows, files), Some(3));
        assert_eq!(leave_selection(&rows, notes), Some(5));
        let rows = [Header, Result(files), Result(files), Header, Result(notes)];
        assert_eq!(leave_selection(&rows, files), Some(2), "no longer cut");
        assert_eq!(
            leave_selection(&rows, PaletteCategory::Commands),
            Some(1),
            "category gone"
        );
        assert_eq!(leave_selection(&[Header], files), None);
    }

    #[test]
    fn backspace_leaves_only_after_a_release_on_an_empty_query() {
        assert!(!backspace_leaves_category(false, true));
        assert!(!backspace_leaves_category(true, false));
        assert!(backspace_leaves_category(true, true));
        // Holding Backspace over "rs": press, press (query now empty), press…
        let mut tracker = BackspaceLeaveTracker::default();
        tracker.reset(false);
        assert!(!tracker.backspace_pressed(false));
        tracker.query_changed(false);
        assert!(!tracker.backspace_pressed(false));
        tracker.query_changed(true);
        assert!(!tracker.backspace_pressed(true), "auto-repeat stays");
        assert!(!tracker.backspace_pressed(true), "and stays");
        // Release, then press: leaves.
        tracker.key_released(true);
        assert!(tracker.backspace_pressed(true));
        // Typing again forgets the release.
        tracker.query_changed(false);
        tracker.key_released(false);
        tracker.query_changed(true);
        assert!(!tracker.backspace_pressed(true));
        // Entering with an empty query: the first Backspace leaves.
        tracker.reset(true);
        assert!(tracker.backspace_pressed(true));
    }

    #[test]
    fn a_same_query_republish_keeps_the_category_selection() {
        assert_eq!(
            category_publish_selection(true, Some(1_500), 2_000),
            Some((1_500, false))
        );
        assert_eq!(
            category_publish_selection(true, Some(1_500), 1_000),
            Some((999, true))
        );
        assert_eq!(
            category_publish_selection(false, Some(1_500), 2_000),
            Some((0, true))
        );
        assert_eq!(
            category_publish_selection(true, None, 2_000),
            Some((0, true))
        );
        assert_eq!(category_publish_selection(true, Some(3), 0), None);
    }

    proptest::proptest! {
        #[test]
        fn category_moves_stay_in_range(
            current in proptest::option::of(0u32..300),
            len in 0u32..300,
            delta in -40i32..40,
        ) {
            match next_category_row(current, len, delta) {
                None => proptest::prop_assert_eq!(len, 0),
                Some(position) => proptest::prop_assert!(position < len),
            }
        }

        #[test]
        fn grouped_pages_land_on_activatable_rows(
            rows in proptest::collection::vec(proptest::bool::ANY, 0..80),
            current in 0u32..90,
            delta in -40i32..40,
        ) {
            if let Some(position) = page_activatable(&rows, current, delta) {
                proptest::prop_assert!(rows[position as usize]);
            } else {
                proptest::prop_assert!(!rows.contains(&true) || rows.is_empty());
            }
        }
    }
}
