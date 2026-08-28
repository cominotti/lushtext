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

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::model::workspace::{WorkspaceScope, WorkspacesFile};

/// Attempts `create_unique` makes before giving up on a free name.
///
/// 1,000: far past any plausible number of `New File N` siblings a user would
/// accumulate, and small enough that the loop cannot become a visible stall on a
/// slow filesystem.
pub const MAX_UNIQUE_NAME_ATTEMPTS: u32 = 1_000;

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
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

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
    fn unique_name_candidates_produce_the_documented_sequence() {
        assert_eq!(unique_name_candidate("New File", 0), "New File");
        assert_eq!(unique_name_candidate("New File", 1), "New File");
        assert_eq!(unique_name_candidate("New File", 2), "New File 2");
        assert_eq!(unique_name_candidate("New File", 17), "New File 17");
        assert_eq!(unique_name_candidate("New Folder", 3), "New Folder 3");
    }

    #[test]
    fn unique_name_attempt_ceiling_is_pinned() {
        assert_eq!(MAX_UNIQUE_NAME_ATTEMPTS, 1_000);
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
}
