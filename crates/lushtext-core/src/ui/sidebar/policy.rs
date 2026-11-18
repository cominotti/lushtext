// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions owned by the workspace tree workflow.
//!
//! This is the workflow's one pure policy module, at its canonical role home. It
//! imports no GTK-family crate, which is what keeps it inside the default
//! `cargo-mutants` `ui/**/policy.rs` scope — and this workflow's decisions are
//! exactly the ones that most need that coverage, because they **rename and
//! delete the user's own documents**.
//!
//! Policy constants are pinned to concrete literals in the units a reader would
//! sanity-check, and the tests assert against those literals rather than against
//! the constants they came from.
//!
//! Every decision here is integer-only: the collapse step policy is a budget
//! policy of the whole-pixel table (`workflow-convention.md`), and nothing else
//! here needs a fraction.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::model::list_budget::{for_each_unheld_range, grouped_thousands};
use crate::model::workspace::{WorkspaceId, WorkspaceScope, WorkspacesFile};
use crate::model::workspace_visibility::WorkspaceEntryVisibility;
use crate::services::file_tree::{SidebarScanBudget, compare_directory_entries};

/// Attempts `create_unique` makes before giving up on a free name.
///
/// 1,000: far past any plausible number of `New File N` siblings a user would
/// accumulate, and small enough that the loop cannot become a visible stall on a
/// slow filesystem.
///
/// Kind: retry budget; 1,000 rounded up to a power of two
/// (`power-of-two-thresholds` D6).
pub const MAX_UNIQUE_NAME_ATTEMPTS: u32 = 1_024;

/// Path copies a sidebar row retains: the item, the store mirror, the sibling
/// cache, the location key and its parent, the visible-count key, and the
/// watch contribution, rounded up (`bound-lists-by-budget-not-count` D3).
///
/// Kind: coverage window; a power of two, rounded up.
pub const SIDEBAR_ROW_PATH_COPIES: u64 = 8;
/// Bytes a sidebar row retains beyond its path copies: its `FileTreeItem`,
/// tree row, mirror row and cache entries.
///
/// Kind: coverage window; rounded up to a power of two from the measured
/// per-row growth (`bound-lists-by-budget-not-count` task 2.4).
pub const SIDEBAR_ROW_FIXED_CHARGE: u64 = 1_024;
/// The per-path graph overhead the scanner charges beyond a path's bytes
/// (`directory_path_graph_bytes`: two `usize` words).
///
/// Kind: external fact; `2 * size_of::<usize>()` on the 64-bit targets.
pub const SIDEBAR_PATH_GRAPH_OVERHEAD: u64 = 16;
/// The entry count `main` listed before this budget (`MAX_DIR_ENTRIES`).
///
/// Kind: external fact; the retired cap the budget must not lose against.
pub const MAIN_SIDEBAR_CAP_ENTRIES: u64 = 10_000;
/// `main`'s worst case: 10,000 rows whose paths are near Linux `PATH_MAX`
/// (4,096 bytes: 255-byte names under a deep parent).
///
/// Kind: derived; computed from the constants it names, not rounded.
pub const MAIN_SIDEBAR_WORST_RETENTION: u64 = MAIN_SIDEBAR_CAP_ENTRIES
    * ((4_096 + SIDEBAR_PATH_GRAPH_OVERHEAD) * SIDEBAR_ROW_PATH_COPIES + SIDEBAR_ROW_FIXED_CHARGE);
/// The user-experience target: 131,072 rows whose names are 32 bytes under a
/// 256-byte parent path (a separator between them).
///
/// Kind: derived; computed from the constants it names, not rounded.
pub const SIDEBAR_TYPICAL_FLOOR_RETENTION: u64 = 128
    * 1024
    * ((256 + 1 + 32 + SIDEBAR_PATH_GRAPH_OVERHEAD) * SIDEBAR_ROW_PATH_COPIES
        + SIDEBAR_ROW_FIXED_CHARGE);
/// The retained-byte budget of one expanded sidebar directory.
///
/// The power of two at or above the larger of
/// [`MAIN_SIDEBAR_WORST_RETENTION`] and [`SIDEBAR_TYPICAL_FLOOR_RETENTION`], so
/// a directory never lists less than `main` listed and 131,072 typical entries
/// are listed in full. Past it the listing ends with a budget row.
///
/// Kind: presentation cap; up from `SIDEBAR_TYPICAL_FLOOR_RETENTION`
/// (`bound-lists-by-budget-not-count` D3).
pub const SIDEBAR_DIRECTORY_RETAINED_BYTES: u64 = 512 * 1024 * 1024;
const _: () = assert!(SIDEBAR_DIRECTORY_RETAINED_BYTES >= MAIN_SIDEBAR_WORST_RETENTION);
const _: () = assert!(SIDEBAR_DIRECTORY_RETAINED_BYTES >= SIDEBAR_TYPICAL_FLOOR_RETENTION);

/// The scan budget of one sidebar directory under `retained_bytes`.
#[must_use]
pub const fn sidebar_scan_budget(retained_bytes: u64) -> SidebarScanBudget {
    SidebarScanBudget {
        retained_bytes,
        path_copies: SIDEBAR_ROW_PATH_COPIES,
        row_fixed_charge: SIDEBAR_ROW_FIXED_CHARGE,
    }
}

/// The label of the row that ends a directory the budget cut.
#[must_use]
pub fn budget_row_label(shown: usize, total: usize) -> String {
    format!(
        "Showing {} of {} entries",
        grouped_thousands(shown),
        grouped_thousands(total)
    )
}

/// The accessible description of the budget row: why, and how to reach the rest.
#[must_use]
pub fn budget_row_description() -> &'static str {
    "This folder has more entries than the sidebar keeps in memory; \
     open files past the list with Open (Ctrl+O)"
}

/// The label of the row that ends a directory still loading.
#[must_use]
pub fn loading_row_label(listed: usize, total: usize) -> String {
    format!(
        "Loading entries\u{2026} {} of {}",
        grouped_thousands(listed),
        grouped_thousands(total)
    )
}

/// The accessible description of the loading row.
#[must_use]
pub fn loading_row_description() -> &'static str {
    "Entries are still being added to this folder"
}

/// Debounce interval for persisting workspace changes to disk, in milliseconds.
///
/// 150 ms: long enough that a burst of folder reorders or a rename coalesces into
/// one write, and short enough that a user who changes their workspace list and
/// immediately closes the window is covered by the close-time flush rather than by
/// luck. Relocated out of the facade, because a cap the workflow owns is a policy
/// value, not narration.
pub const PERSIST_DEBOUNCE_MS: u64 = 150;

/// What an inline rename commit should actually do.
///
/// Naming the decision keeps the three outcomes distinguishable at the call
/// site. Before this existed, "cancel" and "rename" were an `if` chain and the
/// **collision case did not exist at all**: the only validation was
/// empty-or-unchanged, and `rename(2)` silently replaced whatever the typed name
/// already referred to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameIntent {
    /// The typed name is empty, unchanged, or not a plain sibling name: restore
    /// the row and do nothing.
    Cancel,
    /// The typed name is a new sibling name in the same directory.
    Rename { new_path: PathBuf, new_name: String },
}

/// Decide what an inline rename commit means, without touching the filesystem.
///
/// Whether the destination already exists is deliberately **not** decided here:
/// it is a live filesystem fact that must be checked inside the worker while the
/// write guard is held — and the rename itself uses `RENAME_NOREPLACE` so the
/// check and the rename are one kernel operation. A decision taken on the GTK
/// thread would be stale by the time the rename runs.
///
/// A typed name containing a path separator is **refused**, not silently
/// reinterpreted. `Path::with_file_name("sub/x")` would move the file into a
/// different directory, which is not what an inline rename in a tree row means:
/// the user typed into a cell that shows one name, and the visible affordance
/// promises a rename within that directory. `..` is refused for the same reason.
/// Refusing by cancelling restores the row, which is the same thing an empty name
/// does — the user sees their edit not take rather than a file appear somewhere
/// they did not look.
///
/// Case-folding collisions are **not** refused here: on a case-insensitive
/// filesystem `notes.md` -> `Notes.md` is a legitimate rename whose destination
/// "exists" only in the sense that it is the same file. The kernel's
/// `RENAME_NOREPLACE` answers that correctly for the platform actually in use,
/// which a pure function cannot.
#[must_use]
pub fn rename_intent(old_path: &Path, typed_name: &str) -> RenameIntent {
    let new_name = typed_name.trim();
    let old_name = old_path
        .file_name()
        .map_or_default(|name| name.to_string_lossy().into_owned());

    if new_name.is_empty() || new_name == old_name {
        return RenameIntent::Cancel;
    }
    if name_is_not_a_plain_sibling(new_name) {
        return RenameIntent::Cancel;
    }

    RenameIntent::Rename {
        new_path: old_path.with_file_name(new_name),
        new_name: new_name.to_string(),
    }
}

/// Return whether a typed name would leave the row's own directory.
fn name_is_not_a_plain_sibling(name: &str) -> bool {
    name == "."
        || name == ".."
        || name.contains('/')
        || std::path::MAIN_SEPARATOR != '/' && name.contains(std::path::MAIN_SEPARATOR)
}

/// Why a rename could not be performed, in the workflow's own vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRenameRefusal {
    /// Something already exists at the typed name.
    ///
    /// The rename is refused rather than performed, because the platform rename
    /// silently replaces a regular destination and the replaced file's contents
    /// are unrecoverable.
    DestinationExists { name: String },
}

impl WorkspaceRenameRefusal {
    /// Return the user-facing explanation for this refusal.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::DestinationExists { name } => {
                format!("A file named '{name}' already exists in this folder")
            }
        }
    }
}

/// Whether a completed workspace rename is known to be durable.
///
/// A rename that took effect but whose parent-directory sync failed is still a
/// completed rename: the entry has its new name, so the row, the cache, the
/// watch targets, and open tabs must follow it. Only its survival across a
/// power loss is unconfirmed (`rust-1-100-adoption` A8).
#[derive(Debug)]
pub enum RenameDurability {
    /// Every directory the rename mutated was synced.
    Confirmed,
    /// The rename took effect, but a directory sync failed with this error.
    Unconfirmed(std::io::Error),
}

/// The warning a completed rename owes the user, if any.
///
/// A confirmed rename owes none. An unconfirmed one names the new name and says
/// the change is not yet confirmed on disk, in the wording an unconfirmed save
/// uses; it is a warning, never an error, because the rename happened. The
/// caller shows it at [`RENAME_DURABILITY_WARNING_SEVERITY`].
#[must_use]
pub fn rename_durability_warning(durability: &RenameDurability, new_name: &str) -> Option<String> {
    match durability {
        RenameDurability::Confirmed => None,
        RenameDurability::Unconfirmed(_) => Some(format!(
            "Renamed to '{new_name}', but the change is not yet confirmed on disk"
        )),
    }
}

/// The severity of [`rename_durability_warning`]'s message.
pub const RENAME_DURABILITY_WARNING_SEVERITY: crate::services::notifications::NotificationSeverity =
    crate::services::notifications::NotificationSeverity::Warning;

/// Build the candidate name for one `create_unique` attempt.
///
/// Attempt 1 uses the bare base name; later attempts append the attempt number,
/// which is what produces `New File`, `New File 2`, `New File 3`.
#[must_use]
pub fn unique_name_candidate(base: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return base.to_string();
    }
    format!("{base} {attempt}")
}

/// Stable serialized name for one workspace scope kind.
///
/// Extracted out of the automation adapter so the evidence surface and the exported
/// snapshot cannot drift: `scope_kind` is a documented contract value, and two
/// independent `match`es over the same enum is exactly how a contract string changes
/// on one side only. Returns `&'static str` because these are protocol tokens, not
/// user-facing text, and must never be localized.
#[must_use]
pub fn workspace_scope_kind_name(scope: &WorkspaceScope) -> &'static str {
    match scope {
        WorkspaceScope::All => "all",
        WorkspaceScope::Workspace(_) => "workspace",
    }
}

/// What a completed workspace load may do when a mutation superseded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupersededLoadAction {
    /// Memory already holds the full list: keep it and let the pending write stand.
    KeepMemory,
    /// Memory is only a mutation on top of the empty initial file: merge, then persist.
    MergeAndPersist,
}

/// Decide what a superseded workspace load may do.
///
/// # Why one bit decides it
///
/// When a mutation supersedes a load, neither side may simply win, and which side is
/// *safe* depends entirely on what "absent from memory" means:
///
/// * **A load has already been adopted** — memory holds the full list, so a workspace
///   absent from it is one the user **deleted**. Merging would resurrect it.
/// * **No load has been adopted yet** — `workspaces_file` is still the empty initial
///   value, so memory is one mutation on top of nothing. Discarding the load lets the
///   already-scheduled write commit that empty-plus-one state over every workspace on
///   disk, which is worse than the revert the guard exists to prevent.
#[must_use]
pub const fn superseded_load_action(any_load_adopted: bool) -> SupersededLoadAction {
    if any_load_adopted {
        SupersededLoadAction::KeepMemory
    } else {
        SupersededLoadAction::MergeAndPersist
    }
}

/// Merge a completed load with the in-memory state that superseded it.
///
/// The loaded file is the **base**, because it is the only source carrying every
/// workspace already on disk. In-memory workspaces are layered on top and win on id
/// collision, because a mutation that bumped the persistence generation is newer than
/// the snapshot the worker read.
///
/// Only ever called under [`SupersededLoadAction::MergeAndPersist`]: this function
/// cannot express a deletion, so applying it when memory is authoritative would
/// resurrect workspaces the user removed.
#[must_use]
pub fn merge_superseded_workspace_load(
    loaded: WorkspacesFile,
    in_memory: WorkspacesFile,
) -> WorkspacesFile {
    let mut merged = loaded;
    for workspace in in_memory.workspaces {
        if let Some(slot) = merged
            .workspaces
            .iter_mut()
            .find(|candidate| candidate.id == workspace.id)
        {
            *slot = workspace;
        } else {
            merged.workspaces.push(workspace);
        }
    }
    merged
}

/// Whether a directory operation on `changed` affects an open tab at `open`.
///
/// Prefix matching, not equality: renaming or deleting a directory must reach
/// every open tab beneath it.
#[must_use]
pub fn directory_operation_affects_open_path(changed: &Path, open: &Path) -> bool {
    open.starts_with(changed)
}

/// Whether a user-confirmed delete may act on the object the dialog named.
///
/// Naming the two outcomes keeps a **safety refusal** distinguishable from a
/// **failure** at the call site: a refusal destroyed nothing and must leave the
/// tree row alone, while a failure attempted the delete and could not finish it.
/// Collapsing them into one `bool` or one `Result` loses that difference, and the
/// difference is what decides whether the delete callback fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmedDeleteVerdict {
    /// The name still refers to the confirmed object: the delete may proceed.
    Proceed,
    /// The confirmed object is already gone: destroy nothing, reconcile the row.
    ///
    /// Distinct from a refusal because the user's intent was *satisfied*, not
    /// declined. Collapsing it into `RefuseIdentityChanged` regressed the behavior
    /// this workflow had before the identity recheck existed: `remove_*_if_exists`
    /// returned `Ok` for a vanished target and the row was reconciled, whereas the
    /// refusal leaves a stale row behind and tells the user their delete did not
    /// happen — for an object that is not there.
    ReconcileAlreadyGone,
    /// The name refers to a **different** object, or the confirmed object's identity
    /// was never readable: delete nothing and leave the row alone.
    RefuseIdentityChanged,
}

/// Decide whether a confirmed delete still describes the object the user saw.
///
/// # Why identity rather than the path
///
/// A delete confirmation is **user-paced**, so an unbounded amount of time passes
/// between the dialog naming an object and the answer arriving. The path is only a
/// *name*, and over that window the sidebar's own inline rename, an editor
/// Save As, or an external `mv` can make the name refer to a different object.
/// Acting on the name would then destroy whatever now answers to it — and the
/// confirmed-directory branch is deliberately recursive, so the blast radius is a
/// whole unrelated subtree with no undo.
///
/// Kind substitution alone fails safe, because `remove_dir_all` on a regular file
/// is `ENOTDIR` and `remove_file` on a directory is `EISDIR`. The dangerous case is
/// **same-kind** substitution, and comparing identity is what catches it.
///
/// A missing `expected` is refused rather than treated as "nothing to do": if the
/// object's identity could not be read when the user was asked, there is nothing to
/// prove the name still means what the dialog said, and `remove_*_if_exists` would
/// otherwise delete whatever appeared there afterwards.
///
/// A missing `current` under a known `expected` is a **third** outcome, not a refusal.
/// The confirmed object is gone, so there is nothing to destroy and nothing to protect;
/// refusing there would leave a stale tree row and report a failed delete for an object
/// that is not there, which is what `remove_*_if_exists` correctly avoided before this
/// recheck existed.
///
/// This mirrors the rule the repository already states for the analogous draft
/// orphan-body case, and which the placeholder-cleanup path already follows:
/// record the candidate inode, take the stable write guard, recheck the inode,
/// then delete. Never delete by path alone.
#[must_use]
pub fn confirmed_delete_verdict(
    expected_inode: Option<u64>,
    current_inode: Option<u64>,
) -> ConfirmedDeleteVerdict {
    match (expected_inode, current_inode) {
        (Some(expected), Some(current)) if expected == current => ConfirmedDeleteVerdict::Proceed,
        (Some(_), None) => ConfirmedDeleteVerdict::ReconcileAlreadyGone,
        _ => ConfirmedDeleteVerdict::RefuseIdentityChanged,
    }
}

// ---------------------------------------------------------------------------
// Workspace JSON persistence: plain latest-generation policy.
//
// Relocated from `model/workspace_persistence.rs` by the workspace-tree
// migration. Its only consumers were this workflow — `ui/sidebar/imp.rs` and the
// former `ui/sidebar/workspaces.rs`, which the same migration dissolved into the
// four `execution` roles, so the persistence consumer is now
// `ui/sidebar/persist_execution.rs`. Single-workflow pure policy, and it belongs
// beside the workflow it serves. It lands in `policy.rs` specifically: the
// default mutation scope reaches pure policy through the literal
// `ui/**/policy.rs` convention, so any other file name under `ui/sidebar/`
// would leave the scope, which the mutation-testing capability classifies as a
// coverage regression that blocks the relocation.
//
// This is NOT a journal: no generation is written to the file, there is no
// stale-record cleanup, a failed write leaves the previous file intact, and the
// read-back is an ordinary next-launch load rather than recovery from failure.
// ---------------------------------------------------------------------------

const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];

/// Typed identity for one requested workspace snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkspacePersistenceGeneration(u64);

impl WorkspacePersistenceGeneration {
    /// Return the scalar generation for diagnostics and deterministic tests.
    #[must_use]
    pub fn value(self) -> u64 {
        self.0
    }

    fn next(self) -> Self {
        let next = self.0.wrapping_add(1);
        Self(if next == 0 { 1 } else { next })
    }
}

/// User-safe failed terminal retained until a newer durable success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePersistenceFailure {
    /// Generation whose write failed.
    pub generation: WorkspacePersistenceGeneration,
    /// Consecutive failures for the current requested snapshot.
    pub attempts: usize,
    /// Sanitized summary suitable for application feedback.
    pub summary: String,
}

/// Reason a pending generation is allowed to enter the one worker slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceStartReason {
    /// The normal mutation debounce elapsed.
    Debounce,
    /// A bounded automatic retry delay elapsed.
    RetryWakeup,
    /// Window close is flushing the newest state without debounce.
    CloseFlush,
}

/// Effect of applying one matching worker terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceTerminalEffect {
    /// The terminal did not match the current in-flight generation.
    IgnoredStale,
    /// No pending generation remains.
    Settled,
    /// A newer requested generation is ready to start immediately.
    StartNewest,
    /// Retry the current generation after the bounded delay.
    RetryAfter(Duration),
    /// Automatic retries are exhausted; wait for a new mutation or close flush.
    AwaitExplicitRetry,
}

/// Decision used by asynchronous close coordination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspacePersistenceCloseDecision {
    /// The newest requested generation is already durable.
    Durable,
    /// One matching worker must terminate before close can decide again.
    WaitForInFlight(WorkspacePersistenceGeneration),
    /// Start the newest requested generation immediately, bypassing debounce.
    StartNow(WorkspacePersistenceGeneration),
}

/// Requested, in-flight, durable, and failed workspace-persistence state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspacePersistenceState {
    requested: WorkspacePersistenceGeneration,
    durable: WorkspacePersistenceGeneration,
    in_flight: Option<WorkspacePersistenceGeneration>,
    failed: Option<WorkspacePersistenceFailure>,
}

impl WorkspacePersistenceState {
    /// Advance the newest requested snapshot and wake it independently of old failure state.
    pub fn request_mutation(&mut self) -> WorkspacePersistenceGeneration {
        self.requested = self.requested.next();
        if self
            .failed
            .as_ref()
            .is_some_and(|failure| failure.generation != self.requested)
        {
            self.failed = None;
        }
        self.requested
    }

    /// Start the newest pending generation while preserving non-durable state.
    pub fn start(
        &mut self,
        reason: WorkspacePersistenceStartReason,
    ) -> Option<WorkspacePersistenceGeneration> {
        if self.in_flight.is_some() || self.requested == self.durable {
            return None;
        }
        if self.failed.is_some() && reason == WorkspacePersistenceStartReason::Debounce {
            return None;
        }

        let generation = self.requested;
        self.in_flight = Some(generation);
        Some(generation)
    }

    /// Apply one successful terminal only when it owns the current worker slot.
    pub fn apply_success(
        &mut self,
        generation: WorkspacePersistenceGeneration,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        self.durable = generation;
        self.failed = None;
        if self.requested == self.durable {
            WorkspacePersistenceTerminalEffect::Settled
        } else {
            WorkspacePersistenceTerminalEffect::StartNewest
        }
    }

    /// Apply one failed terminal and choose bounded retry or newest-state progress.
    pub fn apply_failure(
        &mut self,
        generation: WorkspacePersistenceGeneration,
        summary: impl Into<String>,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        if self.requested != generation {
            self.failed = None;
            return WorkspacePersistenceTerminalEffect::StartNewest;
        }

        let attempts = self
            .failed
            .as_ref()
            .filter(|failure| failure.generation == generation)
            .map_or(1, |failure| failure.attempts.saturating_add(1));
        self.failed = Some(WorkspacePersistenceFailure {
            generation,
            attempts,
            summary: summary.into(),
        });
        RETRY_DELAYS
            .get(attempts.saturating_sub(1))
            .copied()
            .map_or(
                WorkspacePersistenceTerminalEffect::AwaitExplicitRetry,
                WorkspacePersistenceTerminalEffect::RetryAfter,
            )
    }

    /// Return whether dirty, in-flight, failed, or retry-waiting work remains.
    #[must_use]
    pub fn has_pending_work(&self) -> bool {
        self.requested != self.durable || self.in_flight.is_some() || self.failed.is_some()
    }

    /// Return whether the current requested generation retains a failed terminal.
    #[must_use]
    pub fn is_failed(&self) -> bool {
        self.failed.is_some()
    }

    /// Return the newest requested generation.
    #[must_use]
    pub fn requested_generation(&self) -> WorkspacePersistenceGeneration {
        self.requested
    }

    /// Return the newest durably accepted generation.
    #[must_use]
    pub fn durable_generation(&self) -> WorkspacePersistenceGeneration {
        self.durable
    }

    /// Return the generation currently occupying the worker slot.
    #[must_use]
    pub fn in_flight_generation(&self) -> Option<WorkspacePersistenceGeneration> {
        self.in_flight
    }

    /// Return the retained current failure, if any.
    #[must_use]
    pub fn failure(&self) -> Option<&WorkspacePersistenceFailure> {
        self.failed.as_ref()
    }

    /// Decide how close should flush the newest requested snapshot.
    #[must_use]
    pub fn close_decision(&self) -> WorkspacePersistenceCloseDecision {
        if let Some(generation) = self.in_flight {
            WorkspacePersistenceCloseDecision::WaitForInFlight(generation)
        } else if self.requested == self.durable {
            WorkspacePersistenceCloseDecision::Durable
        } else {
            WorkspacePersistenceCloseDecision::StartNow(self.requested)
        }
    }
}

// ---------------------------------------------------------------------------
// Directory-scan flight: plain ownership policy for one materialized workspace
// directory scan.
//
// Relocated from `model/workspace_scan.rs` by the workspace-tree migration.
// Single owning workflow, so it belongs beside the workflow it serves, and it
// lands in `policy.rs` for the same mutation-scope reason as the persistence
// policy above.
//
// GTK payloads remain in the sidebar adapter. This policy only decides which
// scalar ticket may be active, which latest ticket may wait, and which
// completion is current enough to advance or terminate the flight.
//
// `WorkspaceScanFlight` and `WorkspaceScanFlightMetrics` stay `pub` because
// `crates/lushtext-core/benches/benchmarks.rs` measures scan pressure through
// them. That is a deliberate public path, re-pointed by this move rather than
// preserved behind a compatibility alias.
// ---------------------------------------------------------------------------

/// Identity carried by one admitted or pending directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceScanTicket {
    /// Section lifetime that admitted the request.
    pub lifetime: u64,
    /// Latest target generation for this store at submission time.
    pub target_generation: u64,
    /// Unique scan generation within this store flight.
    pub scan_generation: u64,
}

/// Result of submitting one new compact request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScanSubmission {
    /// No scan was active, so this ticket receives worker admission now.
    Start(WorkspaceScanTicket),
    /// One scan remains active while this ticket replaces the pending request.
    QueueLatest {
        /// Latest ticket retained as the sole pending request.
        ticket: WorkspaceScanTicket,
        /// Active ticket whose cooperative cancellation should be requested.
        cancel_active: WorkspaceScanTicket,
        /// Whether this submission displaced an older pending ticket.
        replaced_pending: bool,
    },
}

/// Result of releasing one active ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScanFinish {
    /// The completion was stale or already released.
    Stale,
    /// The latest pending ticket now receives worker admission.
    StartLatest(WorkspaceScanTicket),
    /// The current flight reached terminal idle state.
    Terminal,
}

/// Direct scalar ownership evidence for one store flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceScanFlightMetrics {
    /// Maximum simultaneous active tickets; always at most one.
    pub active_high_water: usize,
    /// Maximum simultaneous pending tickets; always at most one.
    pub pending_high_water: usize,
    /// Tickets that received worker admission.
    pub starts: u64,
    /// Requests that superseded an active ticket.
    pub cancellation_requests: u64,
    /// Pending tickets replaced before admission.
    pub pending_replacements: u64,
    /// Current flights that reached terminal idle state.
    pub terminals: u64,
    /// Late or duplicate completions rejected by ticket identity.
    pub stale_completions: u64,
}

/// One-active plus one-latest policy for a materialized child store.
#[derive(Debug, Default)]
pub struct WorkspaceScanFlight {
    next_scan_generation: u64,
    next_target_generation: u64,
    active: Option<WorkspaceScanTicket>,
    pending: Option<WorkspaceScanTicket>,
    metrics: WorkspaceScanFlightMetrics,
}

impl WorkspaceScanFlight {
    /// Submit a request and either admit it or replace the sole pending ticket.
    pub fn submit(&mut self, lifetime: u64) -> WorkspaceScanSubmission {
        self.next_scan_generation = self.next_scan_generation.wrapping_add(1);
        self.next_target_generation = self.next_target_generation.wrapping_add(1);
        let ticket = WorkspaceScanTicket {
            lifetime,
            target_generation: self.next_target_generation,
            scan_generation: self.next_scan_generation,
        };

        let Some(active) = self.active else {
            self.active = Some(ticket);
            self.metrics.starts = self.metrics.starts.saturating_add(1);
            self.metrics.active_high_water = 1;
            return WorkspaceScanSubmission::Start(ticket);
        };

        let replaced_pending = self.pending.replace(ticket).is_some();
        self.metrics.cancellation_requests = self.metrics.cancellation_requests.saturating_add(1);
        self.metrics.pending_high_water = 1;
        if replaced_pending {
            self.metrics.pending_replacements = self.metrics.pending_replacements.saturating_add(1);
        }
        WorkspaceScanSubmission::QueueLatest {
            ticket,
            cancel_active: active,
            replaced_pending,
        }
    }

    /// Return whether `ticket` is the active and latest request for this lifetime.
    #[must_use]
    pub fn is_current(&self, ticket: WorkspaceScanTicket, lifetime: u64) -> bool {
        self.active == Some(ticket)
            && ticket.lifetime == lifetime
            && ticket.target_generation == self.next_target_generation
    }

    /// Release the matching active ticket and hand off only the latest pending one.
    pub fn finish(&mut self, ticket: WorkspaceScanTicket) -> WorkspaceScanFinish {
        if self.active != Some(ticket) {
            self.metrics.stale_completions = self.metrics.stale_completions.saturating_add(1);
            return WorkspaceScanFinish::Stale;
        }

        self.active = None;
        if let Some(latest) = self.pending.take() {
            self.active = Some(latest);
            self.metrics.starts = self.metrics.starts.saturating_add(1);
            WorkspaceScanFinish::StartLatest(latest)
        } else {
            self.metrics.terminals = self.metrics.terminals.saturating_add(1);
            WorkspaceScanFinish::Terminal
        }
    }

    /// Invalidate all ownership when a store or section lifetime ends.
    pub fn cancel_all(&mut self) {
        if self.active.take().is_some() {
            self.metrics.cancellation_requests =
                self.metrics.cancellation_requests.saturating_add(1);
        }
        self.pending = None;
        self.next_target_generation = self.next_target_generation.wrapping_add(1);
    }

    /// Return the active ticket without exposing any adapter payload.
    #[must_use]
    pub fn active(&self) -> Option<WorkspaceScanTicket> {
        self.active
    }

    /// Return the sole latest pending ticket, if one exists.
    #[must_use]
    pub fn pending(&self) -> Option<WorkspaceScanTicket> {
        self.pending
    }

    /// Return direct scalar ownership and terminal evidence.
    #[must_use]
    pub fn metrics(&self) -> WorkspaceScanFlightMetrics {
        self.metrics
    }
}

// --- workspace path reveal ---

/// Frame ticks a reveal waits for its row to come to rest before it ends
/// `unsettled`.
///
/// 60: one second at a 60 Hz frame clock. The bin's forwarded request lands one
/// idle after the frame that recorded it and the re-slice one frame after that,
/// so a healthy reveal settles within a handful of ticks; the rest is headroom
/// for a loaded compositor. Reaching the budget is a real finding, never
/// retried.
///
/// Kind: coverage window; 60 frames rounded up to a power of two
/// (`power-of-two-thresholds` D5).
pub const REVEAL_SETTLE_FRAME_BUDGET: u32 = 64;

/// The latest workspace path reveal's observable status.
///
/// Serialized through `as_str` into `window.workspace.reveal_status`; the
/// strings are a documented automation contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WorkspaceRevealStatus {
    /// No reveal has been requested in this window.
    #[default]
    None,
    /// A reveal is expanding ancestors, awaiting a scan, or settling.
    Pending,
    /// The row is selected, drawn inside the sidebar viewport, and at rest.
    Revealed,
    /// The path lies under no configured workspace folder.
    NotInWorkspace,
    /// The path's workspace is hidden by the current workspace scope.
    OutsideScope,
    /// A path component does not exist.
    NotFound,
    /// A path component is excluded by the workspace entry visibility rule.
    HiddenByVisibility,
    /// A path component lies past the per-directory entry cap.
    BeyondEntryCap,
    /// A newer reveal replaced this one, or the sidebar was disposed.
    Superseded,
    /// The row did not come to rest within `REVEAL_SETTLE_FRAME_BUDGET`.
    Unsettled,
}

impl WorkspaceRevealStatus {
    /// Stable snapshot and documentation name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Revealed => "revealed",
            Self::NotInWorkspace => "not-in-workspace",
            Self::OutsideScope => "outside-scope",
            Self::NotFound => "not-found",
            Self::HiddenByVisibility => "hidden-by-visibility",
            Self::BeyondEntryCap => "beyond-entry-cap",
            Self::Superseded => "superseded",
            Self::Unsettled => "unsettled",
        }
    }

    /// Whether this status is one of the typed failures.
    #[must_use]
    pub const fn is_failure(self) -> bool {
        !matches!(self, Self::None | Self::Pending | Self::Revealed)
    }
}

/// Why a reveal was refused before any directory was expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceRevealRefusal {
    /// The path is relative, not normal, or under no configured folder.
    NotInWorkspace,
    /// Every folder containing the path belongs to a workspace outside the scope.
    OutsideScope,
    /// A component below the folder is not admitted by the visibility rule.
    HiddenByVisibility,
}

impl From<WorkspaceRevealRefusal> for WorkspaceRevealStatus {
    fn from(refusal: WorkspaceRevealRefusal) -> Self {
        match refusal {
            WorkspaceRevealRefusal::NotInWorkspace => Self::NotInWorkspace,
            WorkspaceRevealRefusal::OutsideScope => Self::OutsideScope,
            WorkspaceRevealRefusal::HiddenByVisibility => Self::HiddenByVisibility,
        }
    }
}

/// Where a reveal goes: the workspace and folder that own the target, and the
/// directories to expand on the way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRevealPlan {
    /// Workspace whose section shows the target.
    pub workspace_id: WorkspaceId,
    /// Configured folder the target lies under (a top-level row).
    pub folder: PathBuf,
    /// Directories to expand, in order: the folder itself, then each directory
    /// down to the target's parent. Empty when the target is the folder.
    pub ancestors: Vec<PathBuf>,
    /// The path to select.
    pub target: PathBuf,
}

impl WorkspaceRevealPlan {
    /// Tree depth of the target row: `0` for a folder row, one more per
    /// directory below it.
    #[must_use]
    pub fn depth(&self) -> u32 {
        u32::try_from(self.ancestors.len()).unwrap_or(u32::MAX)
    }
}

/// Resolve a reveal target to its workspace, folder, and ancestor chain.
///
/// Pure: it reads no filesystem state, so whether each component exists is
/// learned later from the section's own bounded scans. The folder is the
/// deepest configured folder containing the target among workspaces the scope
/// covers (the first such workspace on a tie), so nested folder memberships
/// reveal in the section that shows the row with the shortest chain.
///
/// # Errors
///
/// `NotInWorkspace` for a relative or non-normal path or one under no folder;
/// `OutsideScope` when only out-of-scope workspaces contain it;
/// `HiddenByVisibility` when a component below the folder is not admitted.
pub fn resolve_workspace_reveal(
    workspaces: &WorkspacesFile,
    scope: &WorkspaceScope,
    visibility: &WorkspaceEntryVisibility,
    target: &Path,
) -> Result<WorkspaceRevealPlan, WorkspaceRevealRefusal> {
    let normal = target.is_absolute()
        && target
            .components()
            .all(|component| !matches!(component, Component::CurDir | Component::ParentDir));
    if !normal {
        return Err(WorkspaceRevealRefusal::NotInWorkspace);
    }

    let mut contained = false;
    let mut best: Option<(&WorkspaceId, &Path)> = None;
    for workspace in &workspaces.workspaces {
        let in_scope = match scope {
            WorkspaceScope::All => true,
            WorkspaceScope::Workspace(id) => *id == workspace.id,
        };
        for folder in &workspace.folders {
            if !target.starts_with(&folder.path) {
                continue;
            }
            contained = true;
            let deeper = best.is_none_or(|(_, current)| {
                folder.path.components().count() > current.components().count()
            });
            if in_scope && deeper {
                best = Some((&workspace.id, folder.path.as_path()));
            }
        }
    }
    let Some((workspace_id, folder)) = best else {
        return Err(if contained {
            WorkspaceRevealRefusal::OutsideScope
        } else {
            WorkspaceRevealRefusal::NotInWorkspace
        });
    };

    let relative = target
        .strip_prefix(folder)
        .map_err(|_| WorkspaceRevealRefusal::NotInWorkspace)?;
    let mut ancestors = Vec::new();
    let mut current = folder.to_path_buf();
    for component in relative.components() {
        let name = component.as_os_str();
        if !visibility.admits(name) {
            return Err(WorkspaceRevealRefusal::HiddenByVisibility);
        }
        ancestors.push(current.clone());
        current.push(name);
    }
    Ok(WorkspaceRevealPlan {
        workspace_id: workspace_id.clone(),
        folder: folder.to_path_buf(),
        ancestors,
        target: target.to_path_buf(),
    })
}

/// Why a scanned directory did not contain the next component of a reveal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceRevealMiss {
    /// The component does not exist (or would have been kept had it existed).
    NotFound,
    /// The directory was truncated and the component may lie past the cap.
    BeyondEntryCap,
}

impl From<WorkspaceRevealMiss> for WorkspaceRevealStatus {
    fn from(miss: WorkspaceRevealMiss) -> Self {
        match miss {
            WorkspaceRevealMiss::NotFound => Self::NotFound,
            WorkspaceRevealMiss::BeyondEntryCap => Self::BeyondEntryCap,
        }
    }
}

/// Classify a component a completed directory scan did not list.
///
/// `last_kept` is the last entry the scan kept when it truncated the directory,
/// `None` when it listed everything. The bounded scan keeps the first entries in
/// `compare_directory_entries` order, so a missing component that sorts at or
/// before the last kept entry would have been listed had it existed: that is
/// `NotFound` even in a truncated directory. Only one that could sort after it
/// is `BeyondEntryCap`. A component not known to be a directory (the target
/// itself) is checked as either kind.
#[must_use]
pub fn classify_missing_reveal_component(
    missing: &Path,
    known_directory: bool,
    last_kept: Option<(&Path, bool)>,
) -> WorkspaceRevealMiss {
    let Some(last_kept) = last_kept else {
        return WorkspaceRevealMiss::NotFound;
    };
    let kinds: &[bool] = if known_directory {
        &[true]
    } else {
        &[true, false]
    };
    let could_lie_past_cap = kinds.iter().any(|&is_dir| {
        compare_directory_entries((missing, is_dir), last_kept) == Ordering::Greater
    });
    if could_lie_past_cap {
        WorkspaceRevealMiss::BeyondEntryCap
    } else {
        WorkspaceRevealMiss::NotFound
    }
}

// ---------------------------------------------------------------------------
// The owned, sliced directory collapse (`slice-large-directory-collapse` D3,
// D4, D9). `collapse_execution.rs` reads these facts from GTK once per step
// and applies the one removal `next_collapse_step` picks.
// ---------------------------------------------------------------------------

/// Flattened rows at or below which a directory, or a drained descendant,
/// collapses through GTK's direct `set_expanded(false)` in one call
/// (design D3). It is at most [`SIDEBAR_REALIZED_ROWS_PER_FRAME`], so the
/// final collapse never replaces more row widgets than one step.
///
/// Kind: cap; a power of two, rounded down.
pub const DIRECT_COLLAPSE_ROWS: u32 = 32;

/// Rows bound to a row widget that one collapse step removes, and so the most
/// row widgets one frame creates and styles in their place (design D3; ledger
/// A118). It is named for the sidebar rather than the collapse because the
/// load's band cap (design D10) may use it too.
///
/// Kind: cap; a power of two, rounded down.
pub const SIDEBAR_REALIZED_ROWS_PER_FRAME: u32 = 32;

/// Rows bound to a row widget that one collapse step removes when no row
/// would enter the realized band in their place (nothing listed after the
/// band): such a removal only destroys row widgets, at about a third of a
/// replacement's cost (design D18, R2), so it may take more rows than
/// [`SIDEBAR_REALIZED_ROWS_PER_FRAME`] within the same frame budget.
///
/// Kind: cap; a power of two, rounded down.
pub const COLLAPSE_DESTROY_ONLY_ROWS: u32 = 64;

/// Rows no widget is bound to that one removal takes: their removal destroys
/// and creates no widget (A110, A118), and costs LushText's mirror
/// bookkeeping plus GTK's rb-tree removal (design D3, Prototype A: 4.9–6.6 ms
/// per 16,384 rows under load, so half fits the turn slice).
///
/// Kind: cap; a power of two, rounded down.
pub const COLLAPSE_UNBOUND_CHUNK_ROWS: u32 = 8_192;

/// Milliseconds a mapped list may go without a frame tick before the next
/// collapse step runs from the low-priority fallback source (design D4): a
/// minimized or occluded window keeps its widgets mapped and its frame clock
/// may stop. About 15 reference frames at 120 Hz.
///
/// Kind: out: time; a power of two, rounded up.
pub const COLLAPSE_FRAME_STARVATION_MS: u64 = 128;

const _: () = assert!(DIRECT_COLLAPSE_ROWS <= SIDEBAR_REALIZED_ROWS_PER_FRAME);
const _: () = assert!(SIDEBAR_REALIZED_ROWS_PER_FRAME <= COLLAPSE_DESTROY_ONLY_ROWS);

/// One expanded store of a collapsing subtree, as GTK lists it now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollapseStore {
    /// Depth below the collapsing directory: 0 for its own store.
    pub depth: u32,
    /// Flattened position of the store's first row.
    pub first_row: u32,
    /// Rows the store itself holds.
    pub length: u32,
    /// Flattened rows under the store, its expanded descendants' included.
    pub span: u32,
}

/// What one collapse step decides from.
#[derive(Clone, Copy, Debug)]
pub struct CollapseFacts<'a> {
    /// The subtree's expanded stores; `stores[0]` is the collapsing
    /// directory's own store, and every other one lies under it.
    pub stores: &'a [CollapseStore],
    /// The flattened positions GTK holds a row object for (its realized band,
    /// its focus row, the selection), strictly ascending; see
    /// [`collapse_band`].
    pub band: &'a [u32],
    /// Flattened rows the model lists after the last realized row: the rows
    /// a bound removal would bring into the band from below.
    pub rows_after: u32,
    /// Flattened rows the model lists before the first realized row: the
    /// rows a bound removal could bring into the band from above, when the
    /// removal shortens a view scrolled to its end and the view clamps.
    /// A bound removal is destroy-only (design D18) when `rows_after` is zero
    /// and at most one unit of rows lies before the band, so at most a unit
    /// can enter whatever the view does.
    pub rows_before: u32,
}

/// The one removal a collapse step applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollapseStep {
    /// Remove `count` rows from `stores[store]`, store-relative from `start`,
    /// none of them bound to a row widget.
    RemoveUnbound {
        store: usize,
        start: u32,
        count: u32,
    },
    /// Remove `count` rows from `stores[store]`, store-relative from `start`,
    /// some of them bound: at most one frame's unit.
    RemoveBound {
        store: usize,
        start: u32,
        count: u32,
    },
    /// Collapse the drained descendant directory that `stores[store]` lists
    /// through GTK's direct call.
    CollapseDescendant(usize),
    /// Collapse the directory itself through GTK's direct call.
    CollapseRow,
}

impl CollapseStep {
    /// Whether the step may replace row widgets, so it needs the window's
    /// one-unit-per-frame token (design D4).
    #[must_use]
    pub const fn takes_unit(self) -> bool {
        !matches!(self, Self::RemoveUnbound { .. })
    }
}

/// The held positions as [`CollapseFacts::band`] takes them: sorted and
/// without repeats. The allocation-free [`next_collapse_step`] relies on it.
#[must_use]
pub fn collapse_band(mut positions: Vec<u32>) -> Vec<u32> {
    positions.sort_unstable();
    positions.dedup();
    positions
}

/// The window-wide frame-interval token of one frame clock (design D4, D25,
/// R6.2, R6.3): the counter of the frame whose interval the latest bound rows
/// were removed in, and how many bound rows that interval has removed.
///
/// Under D25 every bound step runs between frames, from the step source or
/// the starved fallback, and reads the counter there: it equals the counter
/// of the frame just painted (ledger A125), so the rows charged against one
/// counter are the ones the next frame lays out.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameUnitToken {
    /// The frame counter of the interval the latest bound rows left in.
    pub taken_frame: Option<i64>,
    /// Bound rows that interval removed.
    pub rows_taken: u32,
}

/// Bound rows the interval after frame `frame` may still remove, of
/// `budget` (design D25, R13's S1). An interval keyed by another counter,
/// earlier or **later** (another clock, or one whose counter restarted,
/// design R6.3), is stale and leaves the whole budget.
#[must_use]
pub const fn interval_rows_left(token: FrameUnitToken, frame: i64, budget: u32) -> u32 {
    match token.taken_frame {
        Some(taken) if taken == frame => budget.saturating_sub(token.rows_taken),
        _ => budget,
    }
}

/// The token after `rows` bound rows left in the interval after frame
/// `frame` (design D25): added to that interval's rows, or starting it.
#[must_use]
pub const fn record_interval_rows(token: FrameUnitToken, frame: i64, rows: u32) -> FrameUnitToken {
    match token.taken_frame {
        Some(taken) if taken == frame => FrameUnitToken {
            taken_frame: Some(frame),
            rows_taken: token.rows_taken.saturating_add(rows),
        },
        _ => FrameUnitToken {
            taken_frame: Some(frame),
            rows_taken: rows,
        },
    }
}

/// The bound rows one step may remove, given the policy's `step` and the
/// interval's `rows_left` (design D25, R13's S1): `None` when the step must
/// wait for the next frame interval, else the rows to charge and, for a
/// [`CollapseStep::RemoveBound`], the count to remove.
///
/// - A destroy-only removal ([`collapse_removal_is_destroy_only`]) is one
///   whole interval: it runs only when the interval has removed nothing, and
///   charges the whole `budget` (design D18: it creates no row widget, and
///   its unit is [`COLLAPSE_DESTROY_ONLY_ROWS`]).
/// - Any other bound removal takes at most `step_rows` and at most what the
///   interval has left.
/// - A direct collapse of the directory or of a drained descendant (`rows`
///   left in it, at most the direct cap) runs only when they fit what is
///   left.
/// - An unbound removal is not this function's: it never waits for an
///   interval.
#[must_use]
pub const fn bound_step_rows(
    step: CollapseStep,
    rows: u32,
    destroy_only: bool,
    rows_left: u32,
    budget: u32,
    step_rows: u32,
) -> Option<(u32, u32)> {
    match step {
        CollapseStep::RemoveUnbound { count, .. } => Some((0, count)),
        CollapseStep::RemoveBound { count, .. } if destroy_only => {
            if rows_left == budget {
                Some((budget, count))
            } else {
                None
            }
        }
        CollapseStep::RemoveBound { count, .. } => {
            let mut take = count;
            if take > step_rows {
                take = step_rows;
            }
            if take > rows_left {
                take = rows_left;
            }
            if take == 0 { None } else { Some((take, take)) }
        }
        CollapseStep::CollapseDescendant(_) | CollapseStep::CollapseRow => {
            if rows <= rows_left && rows_left > 0 {
                Some((rows, rows))
            } else {
                None
            }
        }
    }
}

/// ATTRIBUTION PATCH (not committed; design R13, task 11.3): the bound rows
/// one step-source dispatch may remove, from `SLC_STEP_ROWS`, default the
/// unit, clamped to at least 1 and at most the unit. Removed by task 10.11.
#[must_use]
pub fn slc_attribution_step_rows() -> u32 {
    // Kani proves the production policy: no environment is read under it
    // (an `env::var` reaches `memchr` and `io::Error`, which CBMC cannot
    // unwind).
    #[cfg(kani)]
    {
        SIDEBAR_REALIZED_ROWS_PER_FRAME
    }
    #[cfg(not(kani))]
    {
        static ROWS: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *ROWS.get_or_init(|| {
            std::env::var("SLC_STEP_ROWS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(slc_attribution_unit_rows)
                .clamp(1, slc_attribution_unit_rows().max(1))
        })
    }
}

/// The next removal of a collapse (design D4, D9):
///
/// 1. the directory's whole remaining subtree is at most
///    [`DIRECT_COLLAPSE_ROWS`] rows → [`CollapseStep::CollapseRow`];
/// 2. otherwise the deepest store (the last of the deepest, so earlier stores'
///    positions hold), which has no expanded store under it:
///    - at most [`DIRECT_COLLAPSE_ROWS`] rows, and not the directory's own →
///      [`CollapseStep::CollapseDescendant`];
///    - else its last range of rows nobody holds, its tail
///      [`COLLAPSE_UNBOUND_CHUNK_ROWS`] at most, through
///      [`for_each_unheld_range`]
///      → [`CollapseStep::RemoveUnbound`];
///    - else, every row of it held, its head rows down to
///      [`DIRECT_COLLAPSE_ROWS`] left, [`SIDEBAR_REALIZED_ROWS_PER_FRAME`] at
///      most, or [`COLLAPSE_DESTROY_ONLY_ROWS`] when the removal can only
///      destroy row widgets ([`collapse_removal_is_destroy_only`], design
///      D18) → [`CollapseStep::RemoveBound`].
///
/// Allocation-free and integer-only, so Kani proves it directly (C1–C5,
/// `policy/kani_proofs.rs`). With no store it collapses the row.
/// ATTRIBUTION PATCH (not committed): the unit from `SLC_UNIT`, default the
/// constant, and the direct cap at most the unit.
#[must_use]
pub fn slc_attribution_unit_rows() -> u32 {
    // Kani proves the production policy: no environment is read under it
    // (an `env::var` reaches `memchr` and `io::Error`, which CBMC cannot
    // unwind).
    #[cfg(kani)]
    {
        SIDEBAR_REALIZED_ROWS_PER_FRAME
    }
    #[cfg(not(kani))]
    {
        static UNIT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *UNIT.get_or_init(|| {
            std::env::var("SLC_UNIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(SIDEBAR_REALIZED_ROWS_PER_FRAME)
        })
    }
}

/// ATTRIBUTION PATCH (not committed).
#[must_use]
pub fn slc_attribution_direct_rows() -> u32 {
    DIRECT_COLLAPSE_ROWS
        .min(slc_attribution_unit_rows())
        .min(slc_attribution_step_rows())
}

/// ATTRIBUTION PATCH (not committed): the destroy-only unit from
/// `SLC_DESTROY`, default the constant.
#[must_use]
pub fn slc_attribution_destroy_only_rows() -> u32 {
    // Kani proves the production policy: no environment is read under it
    // (an `env::var` reaches `memchr` and `io::Error`, which CBMC cannot
    // unwind).
    #[cfg(kani)]
    {
        COLLAPSE_DESTROY_ONLY_ROWS.max(SIDEBAR_REALIZED_ROWS_PER_FRAME)
    }
    #[cfg(not(kani))]
    {
        static UNIT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *UNIT.get_or_init(|| {
            std::env::var("SLC_DESTROY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(COLLAPSE_DESTROY_ONLY_ROWS)
                .max(slc_attribution_unit_rows())
        })
    }
}

/// ATTRIBUTION PATCH (not committed): the unbound chunk from `SLC_CHUNK`,
/// default the constant.
#[must_use]
pub fn slc_attribution_chunk_rows() -> u32 {
    // Kani proves the production policy: no environment is read under it
    // (an `env::var` reaches `memchr` and `io::Error`, which CBMC cannot
    // unwind).
    #[cfg(kani)]
    {
        COLLAPSE_UNBOUND_CHUNK_ROWS
    }
    #[cfg(not(kani))]
    {
        static UNIT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
        *UNIT.get_or_init(|| {
            std::env::var("SLC_CHUNK")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(COLLAPSE_UNBOUND_CHUNK_ROWS)
        })
    }
}

/// Whether a bound removal can only destroy row widgets (design D18): no
/// row is listed after the band, and at most [`SIDEBAR_REALIZED_ROWS_PER_FRAME`]
/// rows before it, which bounds what a view clamped at its end can bring in
/// from above.
#[must_use]
pub const fn collapse_removal_is_destroy_only(facts: &CollapseFacts<'_>) -> bool {
    facts.rows_after == 0 && facts.rows_before <= SIDEBAR_REALIZED_ROWS_PER_FRAME
}

#[must_use]
pub fn next_collapse_step(facts: &CollapseFacts<'_>) -> CollapseStep {
    let Some(root) = facts.stores.first() else {
        return CollapseStep::CollapseRow;
    };
    if root.span <= slc_attribution_direct_rows() {
        return CollapseStep::CollapseRow;
    }
    let mut chosen = 0;
    for (index, store) in facts.stores.iter().enumerate() {
        if store.depth >= facts.stores[chosen].depth {
            chosen = index;
        }
    }
    let store = facts.stores[chosen];
    if chosen != 0 && store.length <= slc_attribution_direct_rows() {
        return CollapseStep::CollapseDescendant(chosen);
    }
    let first = store.first_row;
    let end = first.saturating_add(store.length);
    let lowest = facts.band.partition_point(|position| *position < first);
    let highest = facts.band.partition_point(|position| *position < end);
    let held = &facts.band[lowest..highest];
    let mut last_unheld: Option<(u32, u32)> = None;
    for_each_unheld_range(end, held, |start, count| {
        let clipped_start = start.max(first);
        let clipped_end = start + count;
        if last_unheld.is_none() && clipped_start < clipped_end {
            last_unheld = Some((clipped_start, clipped_end - clipped_start));
        }
    });
    if let Some((start, count)) = last_unheld {
        let chunk = count.min(slc_attribution_chunk_rows());
        return CollapseStep::RemoveUnbound {
            store: chosen,
            start: start + count - chunk - first,
            count: chunk,
        };
    }
    let surplus = store.length.saturating_sub(slc_attribution_direct_rows());
    if surplus == 0 {
        // The directory's own store, all held and small, under a span that
        // nothing above collapsed: only reachable with inconsistent facts
        // (a span larger than the stores account for). Collapse it.
        return CollapseStep::CollapseRow;
    }
    let unit = if collapse_removal_is_destroy_only(facts) {
        slc_attribution_destroy_only_rows()
    } else {
        slc_attribution_unit_rows()
    };
    CollapseStep::RemoveBound {
        store: chosen,
        start: 0,
        count: surplus.min(unit),
    }
}

/// `next_collapse_step` (`slice-large-directory-collapse` task 3.1): the
/// reference cases, and a property test driving the policy against a model of
/// GTK's realized band (ledger A118) until the directory collapses.
#[cfg(test)]
mod collapse_tests {
    use proptest::prelude::*;

    use super::{
        COLLAPSE_UNBOUND_CHUNK_ROWS, CollapseFacts, CollapseStep, CollapseStore,
        DIRECT_COLLAPSE_ROWS, SIDEBAR_REALIZED_ROWS_PER_FRAME, collapse_band, next_collapse_step,
    };

    /// A directory at flattened position 10 whose own store lists `length`
    /// rows and nothing expanded under it.
    fn flat(length: u32) -> [CollapseStore; 1] {
        [CollapseStore {
            depth: 0,
            first_row: 11,
            length,
            span: length,
        }]
    }

    fn step(stores: &[CollapseStore], band: &[u32]) -> CollapseStep {
        next_collapse_step(&CollapseFacts {
            stores,
            band,
            rows_after: 1,
            rows_before: 0,
        })
    }

    fn step_with(
        stores: &[CollapseStore],
        band: &[u32],
        rows_before: u32,
        rows_after: u32,
    ) -> CollapseStep {
        next_collapse_step(&CollapseFacts {
            stores,
            band,
            rows_after,
            rows_before,
        })
    }

    /// Design D18: with every row of the store held, a removal with nothing
    /// listed after the band and at most a unit before it takes the larger
    /// destroy-only unit; a row after the band, or more than a unit before
    /// it (a view clamped at its end refills from above), keeps the unit.
    #[test]
    fn a_removal_with_nothing_to_bring_in_takes_the_destroy_only_unit() {
        let band: Vec<u32> = (10..311).collect();
        let held = flat(300);
        let destroy_only = CollapseStep::RemoveBound {
            store: 0,
            start: 0,
            count: super::COLLAPSE_DESTROY_ONLY_ROWS,
        };
        let unit = CollapseStep::RemoveBound {
            store: 0,
            start: 0,
            count: SIDEBAR_REALIZED_ROWS_PER_FRAME,
        };
        assert_eq!(step_with(&held, &band, 10, 0), destroy_only);
        assert_eq!(
            step_with(&held, &band, SIDEBAR_REALIZED_ROWS_PER_FRAME, 0),
            destroy_only
        );
        assert_eq!(step_with(&held, &band, 10, 1), unit);
        assert_eq!(
            step_with(&held, &band, SIDEBAR_REALIZED_ROWS_PER_FRAME + 1, 0),
            unit
        );
        // Never below the direct rows.
        assert_eq!(
            step_with(&flat(80), &(10..91).collect::<Vec<_>>(), 0, 0),
            CollapseStep::RemoveBound {
                store: 0,
                start: 0,
                count: 80 - DIRECT_COLLAPSE_ROWS,
            }
        );
    }

    /// Design D25, R6.2, R6.3: an interval of one clock admits at most its
    /// budget of bound rows; a counter that moved on (or back: another clock,
    /// a restarted one) starts a new interval with the whole budget.
    #[test]
    fn the_frame_interval_token_admits_one_budget_per_interval() {
        use super::{FrameUnitToken, interval_rows_left, record_interval_rows};
        let fresh = FrameUnitToken::default();
        assert_eq!(interval_rows_left(fresh, 7, 32), 32);
        let taken = record_interval_rows(fresh, 7, 16);
        assert_eq!(interval_rows_left(taken, 7, 32), 16, "16 of 32 left");
        let full = record_interval_rows(taken, 7, 16);
        assert_eq!(full.rows_taken, 32);
        assert_eq!(interval_rows_left(full, 7, 32), 0, "the interval is spent");
        assert_eq!(
            interval_rows_left(full, 8, 32),
            32,
            "the next frame frees it"
        );
        assert_eq!(
            interval_rows_left(full, 3, 32),
            32,
            "a stored counter above the clock's is stale and frees the interval"
        );
        let next = record_interval_rows(full, 8, 8);
        assert_eq!(
            next,
            FrameUnitToken {
                taken_frame: Some(8),
                rows_taken: 8
            },
            "a new interval starts from its own rows"
        );
        assert_eq!(
            interval_rows_left(record_interval_rows(fresh, 7, 40), 7, 32),
            0,
            "an over-budget charge saturates at zero left"
        );
    }

    /// Design D25, R13's S1: what one step may take from the interval.
    #[test]
    fn a_bound_step_takes_at_most_the_step_rows_and_what_the_interval_has_left() {
        use super::{CollapseStep, bound_step_rows};
        let bound = |count| CollapseStep::RemoveBound {
            store: 0,
            start: 0,
            count,
        };
        // Unit 32, steps of 16: two steps per interval.
        assert_eq!(
            bound_step_rows(bound(32), 0, false, 32, 32, 16),
            Some((16, 16))
        );
        assert_eq!(
            bound_step_rows(bound(32), 0, false, 16, 32, 16),
            Some((16, 16))
        );
        assert_eq!(bound_step_rows(bound(32), 0, false, 0, 32, 16), None);
        assert_eq!(
            bound_step_rows(bound(10), 0, false, 16, 32, 16),
            Some((10, 10))
        );
        assert_eq!(
            bound_step_rows(bound(32), 0, false, 8, 32, 16),
            Some((8, 8))
        );
        // Steps of the unit: one step per interval.
        assert_eq!(
            bound_step_rows(bound(32), 0, false, 32, 32, 32),
            Some((32, 32))
        );
        // A destroy-only removal is a whole interval.
        assert_eq!(
            bound_step_rows(bound(64), 0, true, 32, 32, 16),
            Some((32, 64))
        );
        assert_eq!(bound_step_rows(bound(64), 0, true, 16, 32, 16), None);
        // Direct collapses run when their rows fit what is left.
        assert_eq!(
            bound_step_rows(CollapseStep::CollapseRow, 12, false, 16, 32, 16),
            Some((12, 12))
        );
        assert_eq!(
            bound_step_rows(CollapseStep::CollapseRow, 20, false, 16, 32, 16),
            None
        );
        assert_eq!(
            bound_step_rows(CollapseStep::CollapseDescendant(1), 0, false, 0, 32, 16),
            None,
            "a spent interval admits no collapse, even of an empty store"
        );
        // Unbound removals never wait.
        let unbound = CollapseStep::RemoveUnbound {
            store: 0,
            start: 5,
            count: 100,
        };
        assert_eq!(
            bound_step_rows(unbound, 0, false, 0, 32, 16),
            Some((0, 100))
        );
    }

    #[test]
    fn the_constants_are_the_measured_values() {
        assert_eq!(DIRECT_COLLAPSE_ROWS, 32);
        assert_eq!(SIDEBAR_REALIZED_ROWS_PER_FRAME, 32);
        assert_eq!(COLLAPSE_UNBOUND_CHUNK_ROWS, 8_192);
        assert_eq!(super::COLLAPSE_FRAME_STARVATION_MS, 128);
    }

    #[test]
    fn a_small_subtree_collapses_directly() {
        assert_eq!(step(&flat(32), &[]), CollapseStep::CollapseRow);
        assert_eq!(step(&flat(0), &[10, 11]), CollapseStep::CollapseRow);
        assert_eq!(step(&[], &[]), CollapseStep::CollapseRow);
        assert_ne!(step(&flat(33), &[]), CollapseStep::CollapseRow);
    }

    #[test]
    fn an_empty_band_removes_the_tail_chunk_first() {
        assert_eq!(
            step(&flat(20_000), &[]),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 20_000 - 8_192,
                count: 8_192,
            }
        );
        assert_eq!(
            step(&flat(100), &[]),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 0,
                count: 100,
            }
        );
    }

    #[test]
    fn a_band_at_the_head_removes_the_rows_after_it_first() {
        // The band holds the directory row and its first 50 children.
        let band: Vec<u32> = (10..61).collect();
        assert_eq!(
            step(&flat(300), &band),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 50,
                count: 250,
            }
        );
    }

    #[test]
    fn a_band_in_the_middle_removes_after_it_then_before_it() {
        // Children 1,000..1,207 held (flattened 1,011..1,218).
        let band: Vec<u32> = (1_011..1_218).collect();
        let stores = flat(5_000);
        assert_eq!(
            step(&stores, &band),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 1_207,
                count: 3_793,
            }
        );
        let after = [CollapseStore {
            length: 1_207,
            span: 1_207,
            ..stores[0]
        }];
        assert_eq!(
            step(&after, &band),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 0,
                count: 1_000,
            }
        );
    }

    #[test]
    fn a_band_past_the_end_removes_every_child_unbound() {
        let band: Vec<u32> = (400..607).collect();
        assert_eq!(
            step(&flat(200), &band),
            CollapseStep::RemoveUnbound {
                store: 0,
                start: 0,
                count: 200,
            }
        );
    }

    #[test]
    fn a_held_store_removes_one_unit_from_its_head_down_to_the_direct_rows() {
        let band: Vec<u32> = (10..300).collect();
        assert_eq!(
            step(&flat(200), &band),
            CollapseStep::RemoveBound {
                store: 0,
                start: 0,
                count: 32,
            }
        );
        // Fewer than a unit above the direct rows: only the surplus.
        assert_eq!(
            step(&flat(40), &band),
            CollapseStep::RemoveBound {
                store: 0,
                start: 0,
                count: 8,
            }
        );
        // Exactly the direct rows left: the directory collapses.
        assert_eq!(step(&flat(32), &band), CollapseStep::CollapseRow);
    }

    #[test]
    fn a_nested_store_is_drained_and_collapsed_before_its_parent() {
        // `outer/` at 10 lists 100 rows; its row 3 (flattened 14) is `inner/`
        // with 5,000 rows from 15.
        let stores = [
            CollapseStore {
                depth: 0,
                first_row: 11,
                length: 100,
                span: 5_100,
            },
            CollapseStore {
                depth: 1,
                first_row: 15,
                length: 5_000,
                span: 5_000,
            },
        ];
        let band: Vec<u32> = (10..217).collect();
        assert_eq!(
            step(&stores, &band),
            CollapseStep::RemoveUnbound {
                store: 1,
                start: 202,
                count: 4_798,
            }
        );
        let drained = [
            CollapseStore {
                span: 132,
                ..stores[0]
            },
            CollapseStore {
                length: 32,
                span: 32,
                ..stores[1]
            },
        ];
        assert_eq!(step(&drained, &band), CollapseStep::CollapseDescendant(1));
    }

    #[test]
    fn a_store_shorter_than_one_unit_collapses_as_a_descendant() {
        let stores = [
            CollapseStore {
                depth: 0,
                first_row: 11,
                length: 100,
                span: 110,
            },
            CollapseStore {
                depth: 1,
                first_row: 12,
                length: 10,
                span: 10,
            },
        ];
        assert_eq!(step(&stores, &[]), CollapseStep::CollapseDescendant(1));
    }

    #[test]
    fn the_band_wrapper_sorts_and_drops_repeats() {
        assert_eq!(collapse_band(vec![5, 1, 5, 3, 1]), vec![1, 3, 5]);
        assert!(collapse_band(Vec::new()).is_empty());
    }

    #[test]
    fn only_removing_unbound_rows_takes_no_unit() {
        assert!(
            !CollapseStep::RemoveUnbound {
                store: 0,
                start: 0,
                count: 1,
            }
            .takes_unit()
        );
        assert!(
            CollapseStep::RemoveBound {
                store: 0,
                start: 0,
                count: 1,
            }
            .takes_unit()
        );
        assert!(CollapseStep::CollapseDescendant(1).takes_unit());
        assert!(CollapseStep::CollapseRow.takes_unit());
    }

    /// GTK's realized band as A118 pins it, on a flattened list of `total`
    /// rows: `band` rows from `top`, kept on the same row across a removal
    /// before it and moved to the removal's start when its row goes, and kept
    /// full at the end of the list.
    #[derive(Clone, Copy, Debug)]
    struct BandModel {
        top: u32,
        band: u32,
        total: u32,
    }

    impl BandModel {
        fn positions(self, directory: u32) -> Vec<u32> {
            let end = self.top.saturating_add(self.band).min(self.total);
            // The selection and focus sit on the directory row (design D7).
            let mut held: Vec<u32> = (self.top..end).collect();
            held.push(directory);
            collapse_band(held)
        }

        fn remove(&mut self, start: u32, count: u32) {
            if start + count <= self.top {
                self.top -= count;
            } else if start <= self.top {
                self.top = start;
            }
            self.total -= count;
            self.top = self.top.min(self.total.saturating_sub(self.band));
        }
    }

    /// The subtree as the policy sees it: the directory's own store and at
    /// most one expanded child store, listed as its row `child_at`.
    #[derive(Clone, Copy, Debug)]
    struct Subtree {
        directory: u32,
        length: u32,
        child: Option<(u32, u32)>,
    }

    impl Subtree {
        fn stores(self) -> Vec<CollapseStore> {
            let child_rows = self.child.map_or(0, |(_, rows)| rows);
            let mut stores = vec![CollapseStore {
                depth: 0,
                first_row: self.directory + 1,
                length: self.length,
                span: self.length + child_rows,
            }];
            if let Some((at, rows)) = self.child {
                stores.push(CollapseStore {
                    depth: 1,
                    first_row: self.directory + 1 + at + 1,
                    length: rows,
                    span: rows,
                });
            }
            stores
        }
    }

    proptest! {
        /// Driven to the end, every step removes rows from the subtree only,
        /// never more bound rows than a unit, none for an unbound removal,
        /// the descendant before its parent, and the directory only once at
        /// most `DIRECT_COLLAPSE_ROWS` rows are left; the number of steps
        /// stays within design D2's bound.
        #[test]
        fn a_collapse_driven_against_the_band_model_keeps_every_bound(
            above in 0u32..300,
            length in 0u32..40_000,
            child in prop::option::of((0u32..400, 0u32..20_000)),
            after in 0u32..600,
            top_fraction in 0u32..=100,
            band in 1u32..260,
        ) {
            let child = child.map(|(at, rows)| (at.min(length.saturating_sub(1)), rows))
                .filter(|_| length > 0);
            let mut subtree = Subtree { directory: above, length, child };
            let child_rows = child.map_or(0, |(_, rows)| rows);
            let total = above + 1 + length + child_rows + after;
            let mut model = BandModel {
                top: (u64::from(total) * u64::from(top_fraction) / 100)
                    .try_into()
                    .expect("within the list"),
                band,
                total,
            };
            model.top = model.top.min(total.saturating_sub(band));
            let initial_subtree = length + child_rows;
            let stores_count = 1 + u32::from(child.is_some());
            let unit_cap = stores_count
                * ((band + 1).div_ceil(SIDEBAR_REALIZED_ROWS_PER_FRAME) + 2);
            let unbound_cap = stores_count
                * (3 + initial_subtree.div_ceil(COLLAPSE_UNBOUND_CHUNK_ROWS));
            let (mut unit_steps, mut unbound_steps) = (0u32, 0u32);
            loop {
                let stores = subtree.stores();
                let held = model.positions(subtree.directory);
                let band_end = (model.top + model.band).min(model.total);
                let rows_before = model.top;
                let rows_after = model.total - band_end;
                let next = step_with(&stores, &held, rows_before, rows_after);
                let bound_in = |start: u32, count: u32| -> u32 {
                    u32::try_from(
                        held.iter().filter(|p| **p >= start && **p < start + count).count(),
                    )
                    .expect("few held rows")
                };
                match next {
                    CollapseStep::CollapseRow => {
                        prop_assert!(stores[0].span <= DIRECT_COLLAPSE_ROWS);
                        break;
                    }
                    CollapseStep::CollapseDescendant(index) => {
                        prop_assert_eq!(index, 1);
                        let store = stores[1];
                        prop_assert!(store.span <= DIRECT_COLLAPSE_ROWS);
                        model.remove(store.first_row, store.length);
                        subtree.child = None;
                        unit_steps += 1;
                    }
                    CollapseStep::RemoveUnbound { store, start, count }
                    | CollapseStep::RemoveBound { store, start, count } => {
                        prop_assert!(count > 0);
                        let facts = stores[store];
                        prop_assert!(start + count <= facts.length);
                        // A store is drained only when nothing deeper is expanded.
                        prop_assert!(store == 1 || subtree.child.is_none());
                        let flat_start = facts.first_row + start;
                        let bound = bound_in(flat_start, count);
                        if matches!(next, CollapseStep::RemoveUnbound { .. }) {
                            prop_assert_eq!(bound, 0);
                            prop_assert!(count <= COLLAPSE_UNBOUND_CHUNK_ROWS);
                            unbound_steps += 1;
                        } else {
                            let destroy_only = rows_after == 0
                                && rows_before <= SIDEBAR_REALIZED_ROWS_PER_FRAME;
                            let cap = if destroy_only {
                                super::COLLAPSE_DESTROY_ONLY_ROWS
                            } else {
                                SIDEBAR_REALIZED_ROWS_PER_FRAME
                            };
                            prop_assert!(count <= cap);
                            prop_assert!(facts.length - count >= DIRECT_COLLAPSE_ROWS);
                            unit_steps += 1;
                        }
                        // Rows of the old band that stay, against the new
                        // band: the rest entered it (design D2, D18).
                        let removed_in_band = band_end
                            .min(flat_start + count)
                            .saturating_sub(model.top.max(flat_start));
                        let surviving = band_end - model.top - removed_in_band;
                        model.remove(flat_start, count);
                        let new_band = (model.top + model.band).min(model.total) - model.top;
                        let entering = new_band.saturating_sub(surviving);
                        prop_assert!(
                            entering <= SIDEBAR_REALIZED_ROWS_PER_FRAME,
                            "{entering} rows entered the band in one step"
                        );
                        if store == 1 {
                            let (at, rows) = subtree.child.expect("the child store");
                            subtree.child = Some((at, rows - count));
                        } else {
                            subtree.length -= count;
                            if let Some((at, rows)) = subtree.child {
                                // Rows removed before the expanded row shift it.
                                prop_assert!(start + count <= at || start > at);
                                let at = if start + count <= at { at - count } else { at };
                                subtree.child = Some((at, rows));
                            }
                        }
                    }
                }
                prop_assert!(unit_steps <= unit_cap, "unit steps {unit_steps} over {unit_cap}");
                prop_assert!(
                    unbound_steps <= unbound_cap,
                    "unbound steps {unbound_steps} over {unbound_cap}"
                );
            }
        }
    }
}

#[cfg(test)]
mod directory_budget_tests {
    use super::*;

    #[test]
    fn the_budget_is_the_power_of_two_above_the_larger_floor() {
        assert_eq!(MAIN_SIDEBAR_WORST_RETENTION, 10_000 * (4_112 * 8 + 1_024));
        assert_eq!(
            SIDEBAR_TYPICAL_FLOOR_RETENTION,
            128 * 1024 * (305 * 8 + 1_024)
        );
        let floor = MAIN_SIDEBAR_WORST_RETENTION.max(SIDEBAR_TYPICAL_FLOOR_RETENTION);
        let budget = SIDEBAR_DIRECTORY_RETAINED_BYTES;
        assert!(budget.is_power_of_two() && budget >= floor && budget / 2 < floor);
    }

    #[test]
    fn the_rows_state_counts_and_how_to_reach_the_rest() {
        assert_eq!(
            budget_row_label(12_345, 200_000),
            "Showing 12,345 of 200,000 entries"
        );
        assert_eq!(
            loading_row_label(256, 128 * 1024),
            "Loading entries\u{2026} 256 of 131,072"
        );
        assert!(budget_row_description().contains("Ctrl+O"));
        assert!(!loading_row_description().is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_intent_cancels_an_empty_or_unchanged_name() {
        let path = Path::new("/w/notes.md");
        assert_eq!(rename_intent(path, ""), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "   "), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "notes.md"), RenameIntent::Cancel);
        // Trimming happens before the unchanged comparison.
        assert_eq!(rename_intent(path, "  notes.md  "), RenameIntent::Cancel);
    }

    #[test]
    fn rename_intent_keeps_the_new_name_in_the_same_directory() {
        assert_eq!(
            rename_intent(Path::new("/w/sub/notes.md"), " final.md "),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/sub/final.md"),
                new_name: "final.md".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_refuses_a_name_that_would_leave_the_directory() {
        let path = Path::new("/w/notes.md");
        // A separator would turn a rename into a move.
        assert_eq!(rename_intent(path, "sub/final.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "/absolute.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "../escape.md"), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, ".."), RenameIntent::Cancel);
        assert_eq!(rename_intent(path, "."), RenameIntent::Cancel);
        // Trailing separators are refused too, not silently trimmed.
        assert_eq!(rename_intent(path, "final.md/"), RenameIntent::Cancel);
        // A plain sibling name with dots in it is still fine.
        assert_eq!(
            rename_intent(path, "final.tar.gz"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/final.tar.gz"),
                new_name: "final.tar.gz".to_string(),
            }
        );
        // A leading dot is a hidden file, not an escape.
        assert_eq!(
            rename_intent(path, ".hidden"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/.hidden"),
                new_name: ".hidden".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_allows_a_case_only_change() {
        // Deliberately not refused: on a case-insensitive filesystem this is a
        // real rename whose destination "exists" only as the same file, and the
        // kernel's RENAME_NOREPLACE answers that for the platform in use.
        assert_eq!(
            rename_intent(Path::new("/w/notes.md"), "Notes.md"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/w/Notes.md"),
                new_name: "Notes.md".to_string(),
            }
        );
    }

    #[test]
    fn rename_intent_of_a_root_like_path_does_not_panic() {
        // `file_name()` is `None` for `/`, so the old name is empty and any typed
        // name is a change.
        assert_eq!(
            rename_intent(Path::new("/"), "anything"),
            RenameIntent::Rename {
                new_path: PathBuf::from("/anything"),
                new_name: "anything".to_string(),
            }
        );
        assert_eq!(rename_intent(Path::new("/"), ""), RenameIntent::Cancel);
    }

    #[test]
    fn destination_collision_names_the_file_the_user_typed() {
        let refusal = WorkspaceRenameRefusal::DestinationExists {
            name: "final.md".to_string(),
        };
        assert_eq!(
            refusal.message(),
            "A file named 'final.md' already exists in this folder"
        );
    }

    #[test]
    fn a_confirmed_rename_owes_no_warning() {
        assert_eq!(
            rename_durability_warning(&RenameDurability::Confirmed, "final.md"),
            None
        );
    }

    #[test]
    fn an_unconfirmed_rename_warns_with_the_new_name() {
        let durability =
            RenameDurability::Unconfirmed(std::io::Error::from(std::io::ErrorKind::Unsupported));
        assert_eq!(
            rename_durability_warning(&durability, "final.md").as_deref(),
            Some("Renamed to 'final.md', but the change is not yet confirmed on disk")
        );
        assert_eq!(
            RENAME_DURABILITY_WARNING_SEVERITY,
            crate::services::notifications::NotificationSeverity::Warning
        );
    }

    #[test]
    fn unique_name_candidates_produce_the_documented_sequence() {
        assert_eq!(unique_name_candidate("New File", 0), "New File");
        assert_eq!(unique_name_candidate("New File", 1), "New File");
        assert_eq!(unique_name_candidate("New File", 2), "New File 2");
        assert_eq!(unique_name_candidate("New File", 17), "New File 17");
        assert_eq!(unique_name_candidate("New Folder", 3), "New Folder 3");
    }

    #[test]
    fn unique_name_attempt_ceiling_is_pinned() {
        assert_eq!(MAX_UNIQUE_NAME_ATTEMPTS, 1_024);
    }

    #[test]
    fn directory_operations_match_open_tabs_by_prefix_not_equality() {
        let dir = Path::new("/w/sub");
        assert!(directory_operation_affects_open_path(dir, dir));
        assert!(directory_operation_affects_open_path(
            dir,
            Path::new("/w/sub/deep/notes.md")
        ));
        assert!(!directory_operation_affects_open_path(
            dir,
            Path::new("/w/other/notes.md")
        ));
        // A sibling whose name merely starts with the same characters must not
        // match: `starts_with` is component-wise, not byte-wise.
        assert!(!directory_operation_affects_open_path(
            dir,
            Path::new("/w/subtle/notes.md")
        ));
    }

    #[test]
    fn a_confirmed_delete_proceeds_only_against_the_identity_the_user_was_shown() {
        assert_eq!(
            confirmed_delete_verdict(Some(42), Some(42)),
            ConfirmedDeleteVerdict::Proceed
        );
    }

    #[test]
    fn a_same_name_different_object_is_refused() {
        // The defect this exists for: between the dialog and the answer, an
        // inline rename, a Save As, or an external `mv` put a different object
        // under the same name. Acting on the name destroys the wrong file, and
        // recursively so for a directory.
        assert_eq!(
            confirmed_delete_verdict(Some(42), Some(43)),
            ConfirmedDeleteVerdict::RefuseIdentityChanged
        );
    }

    #[test]
    fn a_vanished_target_reconciles_the_row_instead_of_reporting_a_refusal() {
        // Not a refusal: the confirmed object is gone, so there is nothing to
        // destroy and nothing to protect. Refusing here regressed the behavior the
        // workflow had before the identity recheck — `remove_*_if_exists` returned
        // `Ok` and the row was reconciled — and told the user their delete failed
        // for an object that is not there.
        assert_eq!(
            confirmed_delete_verdict(Some(42), None),
            ConfirmedDeleteVerdict::ReconcileAlreadyGone
        );
    }

    #[test]
    fn a_vanished_target_is_still_distinguished_from_a_substituted_one() {
        // The pair that must never collapse: both have "the identity we recorded is
        // not there now", and only one of them has something else standing in its
        // place. Only the second may leave the row alone and warn.
        assert_ne!(
            confirmed_delete_verdict(Some(42), None),
            confirmed_delete_verdict(Some(42), Some(43))
        );
    }

    #[test]
    fn an_unreadable_original_identity_is_refused() {
        // Without a recorded identity there is nothing to prove the name still
        // means what the dialog said, so `remove_*_if_exists` must not run and
        // delete whatever appeared there afterwards.
        assert_eq!(
            confirmed_delete_verdict(None, Some(42)),
            ConfirmedDeleteVerdict::RefuseIdentityChanged
        );
        assert_eq!(
            confirmed_delete_verdict(None, None),
            ConfirmedDeleteVerdict::RefuseIdentityChanged
        );
    }

    // --- Workspace JSON persistence (relocated with the module) ---

    #[test]
    fn starting_a_write_does_not_make_it_durable() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(generation)
        );
        assert!(state.has_pending_work());
        assert_eq!(
            state.durable_generation(),
            WorkspacePersistenceGeneration::default()
        );
    }

    #[test]
    fn older_success_schedules_the_newest_requested_generation() {
        let mut state = WorkspacePersistenceState::default();
        let older = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(older)
        );
        let newer = state.request_mutation();
        assert_eq!(
            state.apply_success(older),
            WorkspacePersistenceTerminalEffect::StartNewest
        );
        assert_eq!(state.requested_generation(), newer);
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(newer)
        );
    }

    #[test]
    fn current_failure_stays_pending_and_uses_bounded_backoff() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        for expected_delay in RETRY_DELAYS {
            assert_eq!(
                state.start(if state.failure().is_some() {
                    WorkspacePersistenceStartReason::RetryWakeup
                } else {
                    WorkspacePersistenceStartReason::Debounce
                }),
                Some(generation)
            );
            assert_eq!(
                state.apply_failure(generation, "Workspace changes could not be saved."),
                WorkspacePersistenceTerminalEffect::RetryAfter(expected_delay)
            );
            assert!(state.has_pending_work());
            assert!(state.is_failed());
            assert_eq!(state.start(WorkspacePersistenceStartReason::Debounce), None);
        }

        assert_eq!(
            state.start(WorkspacePersistenceStartReason::RetryWakeup),
            Some(generation)
        );
        assert_eq!(
            state.apply_failure(generation, "Workspace changes could not be saved."),
            WorkspacePersistenceTerminalEffect::AwaitExplicitRetry
        );
        assert_eq!(state.failure().map(|failure| failure.attempts), Some(5));
    }

    #[test]
    fn newer_mutation_wakes_progress_after_an_older_failure() {
        let mut state = WorkspacePersistenceState::default();
        let failed = state.request_mutation();
        state.start(WorkspacePersistenceStartReason::Debounce);
        state.apply_failure(failed, "failed");
        let newest = state.request_mutation();
        assert!(!state.is_failed());
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(newest)
        );
    }

    #[test]
    fn close_bypasses_debounce_and_waits_for_inflight_work() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::StartNow(generation)
        );
        state.start(WorkspacePersistenceStartReason::CloseFlush);
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::WaitForInFlight(generation)
        );
        state.apply_success(generation);
        assert_eq!(
            state.close_decision(),
            WorkspacePersistenceCloseDecision::Durable
        );
    }

    #[test]
    fn stale_terminals_cannot_mutate_current_state() {
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        state.start(WorkspacePersistenceStartReason::Debounce);
        let stale = WorkspacePersistenceGeneration(generation.value().saturating_add(1));
        assert_eq!(
            state.apply_success(stale),
            WorkspacePersistenceTerminalEffect::IgnoredStale
        );
        assert_eq!(state.in_flight_generation(), Some(generation));
    }

    // --- Directory-scan flight (relocated with the module) ---

    #[test]
    fn rapid_submissions_keep_one_active_and_only_the_latest_pending_ticket() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(first) = flight.submit(7) else {
            panic!("first request should start");
        };
        let WorkspaceScanSubmission::QueueLatest { ticket: second, .. } = flight.submit(7) else {
            panic!("second request should wait");
        };
        let WorkspaceScanSubmission::QueueLatest {
            ticket: latest,
            cancel_active,
            replaced_pending,
        } = flight.submit(7)
        else {
            panic!("latest request should replace pending work");
        };

        assert_eq!(cancel_active, first);
        assert!(replaced_pending);
        assert_eq!(flight.active(), Some(first));
        assert_eq!(flight.pending(), Some(latest));
        assert_ne!(second, latest);
        assert_eq!(flight.metrics().active_high_water, 1);
        assert_eq!(flight.metrics().pending_high_water, 1);
        assert_eq!(flight.metrics().pending_replacements, 1);
    }

    #[test]
    fn active_completion_starts_latest_and_only_current_ticket_reaches_terminal() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(first) = flight.submit(3) else {
            panic!("first request should start");
        };
        let WorkspaceScanSubmission::QueueLatest { ticket: latest, .. } = flight.submit(3) else {
            panic!("latest request should wait");
        };

        assert!(!flight.is_current(first, 3));
        assert_eq!(
            flight.finish(first),
            WorkspaceScanFinish::StartLatest(latest)
        );
        assert!(flight.is_current(latest, 3));
        assert_eq!(flight.finish(first), WorkspaceScanFinish::Stale);
        assert_eq!(flight.finish(latest), WorkspaceScanFinish::Terminal);
        assert_eq!(flight.metrics().starts, 2);
        assert_eq!(flight.metrics().terminals, 1);
        assert_eq!(flight.metrics().stale_completions, 1);
    }

    #[test]
    fn lifetime_change_and_cancel_all_fail_closed() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(ticket) = flight.submit(11) else {
            panic!("first request should start");
        };

        assert!(!flight.is_current(ticket, 12));
        flight.cancel_all();

        assert_eq!(flight.active(), None);
        assert_eq!(flight.pending(), None);
        assert_eq!(flight.finish(ticket), WorkspaceScanFinish::Stale);
    }

    // --- Triage of the seven inherited persistence survivors (slot 5b) ---
    //
    // These seven field/operator mutants survived at the module's previous home in
    // `model/workspace_persistence.rs` and were carried here unchanged by the
    // relocation, so they are baseline rather than regressions. The relocation put
    // them in a file this change owns, and the mutation-testing capability requires
    // the owning change to triage rather than pass them on again. Each survived
    // because an existing assertion was satisfied by the mutant's own value — the
    // classic weak-assertion shape — so the fix is step two of the documented
    // order: tighten the tests, no production change and no exclusion.

    #[test]
    fn a_generation_reports_its_own_ordinal_rather_than_a_constant() {
        // `value()` was only ever asserted against another `value()` or against a
        // freshly defaulted generation, both of which a constant satisfies.
        let mut state = WorkspacePersistenceState::default();
        assert_eq!(state.request_mutation().value(), 1);
        assert_eq!(state.request_mutation().value(), 2);
        assert_eq!(state.request_mutation().value(), 3);
        assert_eq!(
            WorkspacePersistenceGeneration::default().value(),
            0,
            "a default generation is the zeroth, not the first"
        );
    }

    #[test]
    fn a_busy_worker_refuses_a_second_start_even_with_newer_work_pending() {
        // Kills `in_flight.is_some() || requested == durable` -> `&&`: with the
        // conjunction, a state that is BOTH busy and newly dirty would start a
        // second concurrent write of the same file.
        let mut state = WorkspacePersistenceState::default();
        let first = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(first)
        );
        state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            None,
            "a write already occupies the worker slot"
        );
    }

    #[test]
    fn a_settled_state_reports_no_pending_work() {
        // Kills `has_pending_work -> true` and the `!=` -> `==` inversion: a freshly
        // defaulted state has requested == durable and no in-flight or failed work.
        assert!(!WorkspacePersistenceState::default().has_pending_work());
    }

    #[test]
    fn dirty_work_alone_is_pending_work_without_a_failure_or_a_worker() {
        // Kills both `||` -> `&&` mutations in `has_pending_work`: with either
        // conjunction, a merely-dirty state reports settled and the close flush
        // would let the window close over an unwritten workspace list.
        let mut state = WorkspacePersistenceState::default();
        state.request_mutation();
        assert!(state.in_flight_generation().is_none());
        assert!(!state.is_failed());
        assert!(
            state.has_pending_work(),
            "a requested generation that is not durable is pending work on its own"
        );
    }

    #[test]
    fn the_durable_generation_advances_past_the_default_on_success() {
        // Kills `durable_generation -> Default::default()`: the only prior
        // assertion compared it against a defaulted generation, which the mutant
        // returns verbatim.
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(generation)
        );
        state.apply_success(generation);
        assert_eq!(state.durable_generation().value(), 1);
        assert_ne!(
            state.durable_generation(),
            WorkspacePersistenceGeneration::default()
        );
    }

    #[test]
    fn an_in_flight_write_and_a_recorded_failure_are_mutually_exclusive_and_both_imply_dirt() {
        // Documents the invariant that makes the last `||` in `has_pending_work`
        // defensive rather than live, and which is why the mutation replacing it
        // with `&&` is *equivalent* rather than a coverage gap (see the narrow
        // `exclude_re` entry in `.cargo/mutants.toml`).
        //
        // No test can kill that mutant, because `a || b || c` and `a || (b && c)`
        // differ only when `a` is false and exactly one of `b`/`c` is true — and
        // both of those states are unreachable. This test fails if a future change
        // makes either reachable, which is the protection the exclusion needs.
        let mut state = WorkspacePersistenceState::default();
        let generation = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::Debounce),
            Some(generation)
        );

        // In flight: `failed` is clear, and the state is dirty.
        assert!(state.in_flight_generation().is_some());
        assert!(
            !state.is_failed(),
            "a started write clears any prior failure"
        );
        assert_ne!(
            state.requested_generation(),
            state.durable_generation(),
            "an in-flight write always implies requested != durable"
        );

        // Failed: `in_flight` is cleared before `failed` is recorded, and the
        // state is still dirty because the write never became durable.
        state.apply_failure(generation, "disk full");
        assert!(
            state.in_flight_generation().is_none(),
            "a terminal clears the worker slot before recording the failure"
        );
        assert!(state.is_failed());
        assert_ne!(
            state.requested_generation(),
            state.durable_generation(),
            "a recorded failure always implies requested != durable"
        );

        // Success is the only path that advances `durable`, and it clears both.
        let retry = state.request_mutation();
        assert_eq!(
            state.start(WorkspacePersistenceStartReason::RetryWakeup),
            Some(retry)
        );
        state.apply_success(retry);
        assert!(state.in_flight_generation().is_none());
        assert!(!state.is_failed());
        assert_eq!(state.requested_generation(), state.durable_generation());
        assert!(!state.has_pending_work());
    }

    #[test]
    fn scope_kind_names_are_the_documented_protocol_tokens() {
        // These two strings are a documented automation contract (`workspace.scope_kind`),
        // not user-facing text. Extracting this out of the automation adapter is what
        // stopped the evidence surface and the exported snapshot from matching over the
        // same enum twice; asserting the literals is what stops the surviving value from
        // drifting on both sides at once.
        use crate::model::workspace::{WorkspaceId, WorkspaceScope};

        assert_eq!(
            workspace_scope_kind_name(&WorkspaceScope::All),
            "all",
            "the unscoped kind is the literal `all`"
        );
        assert_eq!(
            workspace_scope_kind_name(&WorkspaceScope::Workspace(WorkspaceId::new("ws-1"))),
            "workspace",
            "a scoped kind is the literal `workspace`, independent of which workspace"
        );
        // Different workspaces share one kind token: the id travels in its own field.
        assert_eq!(
            workspace_scope_kind_name(&WorkspaceScope::Workspace(WorkspaceId::new("other"))),
            workspace_scope_kind_name(&WorkspaceScope::Workspace(WorkspaceId::new("ws-1")))
        );
    }

    // --- Superseded workspace load ---

    fn workspaces(names: &[&str]) -> WorkspacesFile {
        use crate::model::workspace::{WorkspaceConfig, WorkspaceId, WorkspacesFile};
        let mut file = WorkspacesFile::default();
        for name in names {
            file.workspaces.push(WorkspaceConfig::with_one_folder(
                WorkspaceId::new(*name),
                *name,
                PathBuf::from(format!("/w/{name}")),
            ));
        }
        file
    }

    fn names(file: &WorkspacesFile) -> Vec<String> {
        let mut names: Vec<String> = file
            .workspaces
            .iter()
            .map(|workspace| workspace.name.clone())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_superseded_load_merges_only_before_the_first_adoption() {
        assert_eq!(
            superseded_load_action(false),
            SupersededLoadAction::MergeAndPersist
        );
        assert_eq!(
            superseded_load_action(true),
            SupersededLoadAction::KeepMemory
        );
    }

    #[test]
    fn merging_before_the_first_load_keeps_both_the_stored_list_and_the_new_workspace() {
        // The catastrophic case: memory is one mutation on top of the *empty* initial
        // file, and the already-scheduled write would otherwise commit that alone,
        // destroying every stored workspace.
        let merged = merge_superseded_workspace_load(
            workspaces(&["one", "two", "three"]),
            workspaces(&["created-during-load"]),
        );
        assert_eq!(
            names(&merged),
            vec!["created-during-load", "one", "three", "two"]
        );
    }

    #[test]
    fn a_newer_in_memory_workspace_wins_on_id_collision() {
        use crate::model::workspace::{WorkspaceConfig, WorkspaceId};
        let mut in_memory = workspaces(&["one"]);
        in_memory.workspaces[0] = WorkspaceConfig::with_one_folder(
            WorkspaceId::new("one"),
            "renamed",
            PathBuf::from("/w/one"),
        );

        let merged = merge_superseded_workspace_load(workspaces(&["one", "two"]), in_memory);

        assert_eq!(names(&merged), vec!["renamed", "two"]);
        assert_eq!(
            merged.workspaces.len(),
            2,
            "a collision replaces rather than duplicates"
        );
    }

    #[test]
    fn merging_cannot_express_a_deletion_which_is_why_it_is_gated() {
        // This is the bug the pass-2 audit's fix introduced and this test pins: merge
        // has no way to represent "the user removed this", so applying it once memory
        // is authoritative resurrects a deleted workspace. `superseded_load_action`
        // returning `KeepMemory` after the first adoption is what prevents that, and
        // this asserts the hazard is real rather than theoretical.
        let merged =
            merge_superseded_workspace_load(workspaces(&["one", "two"]), workspaces(&["one"]));
        assert_eq!(
            names(&merged),
            vec!["one", "two"],
            "the merge resurrects `two`, which is correct only before the first adoption"
        );
    }

    // --- workspace path reveal ---

    fn reveal_workspaces() -> WorkspacesFile {
        use crate::model::workspace::WorkspaceConfig;
        let mut file = WorkspacesFile::default();
        file.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new("a"),
            "a",
            PathBuf::from("/w/a"),
        ));
        file.workspaces.push(WorkspaceConfig::with_one_folder(
            WorkspaceId::new("b"),
            "b",
            PathBuf::from("/w/b"),
        ));
        file
    }

    fn hidden_off() -> WorkspaceEntryVisibility {
        WorkspaceEntryVisibility::new(false, [".git"])
    }

    #[test]
    fn reveal_resolves_the_folder_and_every_ancestor_in_order() {
        let plan = resolve_workspace_reveal(
            &reveal_workspaces(),
            &WorkspaceScope::All,
            &hidden_off(),
            Path::new("/w/b/x/y/file.txt"),
        )
        .expect("resolves");
        assert_eq!(plan.workspace_id, WorkspaceId::new("b"));
        assert_eq!(plan.folder, PathBuf::from("/w/b"));
        assert_eq!(
            plan.ancestors,
            vec![
                PathBuf::from("/w/b"),
                PathBuf::from("/w/b/x"),
                PathBuf::from("/w/b/x/y")
            ]
        );
        assert_eq!(plan.depth(), 3);
    }

    #[test]
    fn revealing_a_folder_itself_needs_no_expansion() {
        let plan = resolve_workspace_reveal(
            &reveal_workspaces(),
            &WorkspaceScope::All,
            &hidden_off(),
            Path::new("/w/a"),
        )
        .expect("resolves");
        assert!(plan.ancestors.is_empty());
        assert_eq!(plan.depth(), 0);
    }

    #[test]
    fn reveal_refuses_paths_outside_every_folder() {
        for target in [
            "/elsewhere/file",
            "/w/ab/file",
            "relative/file",
            "/w/a/../b/file",
        ] {
            assert_eq!(
                resolve_workspace_reveal(
                    &reveal_workspaces(),
                    &WorkspaceScope::All,
                    &hidden_off(),
                    Path::new(target),
                ),
                Err(WorkspaceRevealRefusal::NotInWorkspace),
                "{target}"
            );
        }
    }

    #[test]
    fn reveal_refuses_a_workspace_the_scope_hides() {
        assert_eq!(
            resolve_workspace_reveal(
                &reveal_workspaces(),
                &WorkspaceScope::Workspace(WorkspaceId::new("a")),
                &hidden_off(),
                Path::new("/w/b/file"),
            ),
            Err(WorkspaceRevealRefusal::OutsideScope)
        );
    }

    #[test]
    fn reveal_refuses_hidden_and_excluded_components_but_not_the_folder_name() {
        let mut file = WorkspacesFile::default();
        file.workspaces
            .push(crate::model::workspace::WorkspaceConfig::with_one_folder(
                WorkspaceId::new("dot"),
                "dot",
                PathBuf::from("/w/.config"),
            ));
        // A configured folder is always visible as a root, dot-named or not.
        assert!(
            resolve_workspace_reveal(
                &file,
                &WorkspaceScope::All,
                &hidden_off(),
                Path::new("/w/.config/app.toml")
            )
            .is_ok()
        );
        for target in ["/w/a/.secret/file", "/w/a/.git", "/w/a/sub/.env"] {
            assert_eq!(
                resolve_workspace_reveal(
                    &reveal_workspaces(),
                    &WorkspaceScope::All,
                    &hidden_off(),
                    Path::new(target),
                ),
                Err(WorkspaceRevealRefusal::HiddenByVisibility),
                "{target}"
            );
        }
        // Hidden files on admits dot-names but still not excluded names.
        let shown = WorkspaceEntryVisibility::new(true, [".git"]);
        assert!(
            resolve_workspace_reveal(
                &reveal_workspaces(),
                &WorkspaceScope::All,
                &shown,
                Path::new("/w/a/.secret/file")
            )
            .is_ok()
        );
        assert_eq!(
            resolve_workspace_reveal(
                &reveal_workspaces(),
                &WorkspaceScope::All,
                &shown,
                Path::new("/w/a/.git/config")
            ),
            Err(WorkspaceRevealRefusal::HiddenByVisibility)
        );
    }

    #[test]
    fn reveal_prefers_the_deepest_in_scope_folder() {
        let mut file = reveal_workspaces();
        file.workspaces
            .push(crate::model::workspace::WorkspaceConfig::with_one_folder(
                WorkspaceId::new("nested"),
                "nested",
                PathBuf::from("/w/a/deep"),
            ));
        let plan = resolve_workspace_reveal(
            &file,
            &WorkspaceScope::All,
            &hidden_off(),
            Path::new("/w/a/deep/file"),
        )
        .expect("resolves");
        assert_eq!(plan.workspace_id, WorkspaceId::new("nested"));
        assert_eq!(plan.ancestors, vec![PathBuf::from("/w/a/deep")]);
        // Scoped to the outer workspace, the outer folder wins.
        let plan = resolve_workspace_reveal(
            &file,
            &WorkspaceScope::Workspace(WorkspaceId::new("a")),
            &hidden_off(),
            Path::new("/w/a/deep/file"),
        )
        .expect("resolves");
        assert_eq!(plan.workspace_id, WorkspaceId::new("a"));
        assert_eq!(plan.depth(), 2);
    }

    #[test]
    fn reveal_status_names_are_the_documented_contract() {
        let names: Vec<&str> = [
            WorkspaceRevealStatus::None,
            WorkspaceRevealStatus::Pending,
            WorkspaceRevealStatus::Revealed,
            WorkspaceRevealStatus::NotInWorkspace,
            WorkspaceRevealStatus::OutsideScope,
            WorkspaceRevealStatus::NotFound,
            WorkspaceRevealStatus::HiddenByVisibility,
            WorkspaceRevealStatus::BeyondEntryCap,
            WorkspaceRevealStatus::Superseded,
            WorkspaceRevealStatus::Unsettled,
        ]
        .into_iter()
        .map(WorkspaceRevealStatus::as_str)
        .collect();
        assert_eq!(
            names,
            [
                "none",
                "pending",
                "revealed",
                "not-in-workspace",
                "outside-scope",
                "not-found",
                "hidden-by-visibility",
                "beyond-entry-cap",
                "superseded",
                "unsettled"
            ]
        );
        assert!(!WorkspaceRevealStatus::None.is_failure());
        assert!(!WorkspaceRevealStatus::Pending.is_failure());
        assert!(!WorkspaceRevealStatus::Revealed.is_failure());
        assert!(WorkspaceRevealStatus::Unsettled.is_failure());
        assert!(WorkspaceRevealStatus::Superseded.is_failure());
        assert_eq!(REVEAL_SETTLE_FRAME_BUDGET, 64);
    }

    #[test]
    fn a_missing_component_in_a_complete_listing_is_not_found() {
        assert_eq!(
            classify_missing_reveal_component(Path::new("/d/zzz"), false, None),
            WorkspaceRevealMiss::NotFound
        );
    }

    #[test]
    fn a_missing_component_sorting_past_the_last_kept_entry_is_beyond_the_cap() {
        let last = (Path::new("/d/row-09999.txt"), false);
        assert_eq!(
            classify_missing_reveal_component(Path::new("/d/row-10040.txt"), false, Some(last)),
            WorkspaceRevealMiss::BeyondEntryCap
        );
        // It would have been kept had it existed.
        assert_eq!(
            classify_missing_reveal_component(Path::new("/d/row-00001.txt"), false, Some(last)),
            WorkspaceRevealMiss::NotFound
        );
        // A directory sorts before every file, so a missing directory never
        // lies past a file.
        assert_eq!(
            classify_missing_reveal_component(Path::new("/d/zzz"), true, Some(last)),
            WorkspaceRevealMiss::NotFound
        );
        // Unknown kind: as a file it could lie past the cap.
        assert_eq!(
            classify_missing_reveal_component(Path::new("/d/zzz"), false, Some(last)),
            WorkspaceRevealMiss::BeyondEntryCap
        );
    }

    proptest::proptest! {
        #[test]
        fn reveal_ancestor_chain_always_lies_inside_the_chosen_folder(
            folder in proptest::collection::vec("[a-z]{1,6}", 1..4),
            relative in proptest::collection::vec("[a-z][a-z0-9.]{0,6}", 0..6),
        ) {
            let folder_path: PathBuf = std::iter::once("/".to_string()).chain(folder).collect();
            let target: PathBuf = relative.iter().fold(folder_path.clone(), |path, part| path.join(part));
            let mut file = WorkspacesFile::default();
            file.workspaces.push(crate::model::workspace::WorkspaceConfig::with_one_folder(
                WorkspaceId::new("p"),
                "p",
                folder_path.clone(),
            ));
            let plan = resolve_workspace_reveal(
                &file,
                &WorkspaceScope::All,
                &WorkspaceEntryVisibility::new(true, Vec::<String>::new()),
                &target,
            ).expect("every generated target lies under the folder");
            proptest::prop_assert_eq!(&plan.folder, &folder_path);
            proptest::prop_assert_eq!(plan.ancestors.len(), relative.len());
            for (index, ancestor) in plan.ancestors.iter().enumerate() {
                proptest::prop_assert!(ancestor.starts_with(&plan.folder));
                proptest::prop_assert!(target.starts_with(ancestor) && ancestor != &target);
                proptest::prop_assert_eq!(ancestor.components().count(), folder_path.components().count() + index);
            }
        }
    }
}
