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

use std::path::PathBuf;
use std::sync::Arc;

use crate::services::palette::{FileIndex, FileIndexMutationLedger, MAX_INDEXED_FILES};

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
pub const MAX_RESULTS_PER_SOURCE: usize = 50;

/// Maximum incremental mutations retained before overflow escalates to a rebuild.
pub const MAX_PENDING_INDEX_UPDATES: usize = 1_024;

/// Maximum exact retained bytes the pending mutation queue may own.
pub const MAX_PENDING_INDEX_UPDATE_BYTES: u64 = 4 * 1024 * 1024;

/// Smallest capacity the pending queue's growth estimate assumes.
///
/// `Vec` grows by doubling from a small floor, so the estimate is
/// `max(capacity, this) * 2`. The admission decision must charge for that
/// reallocation *before* it happens, or a push that passes the ceiling check can
/// still leave the queue owning more than
/// [`MAX_PENDING_INDEX_UPDATE_BYTES`].
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
        assert_eq!(MAX_RESULTS_PER_SOURCE, 50);
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
}
