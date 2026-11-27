// SPDX-License-Identifier: GPL-3.0-or-later

//! Seam value objects for the workspace tree workflow.
//!
//! Reified in the established Ticket/Facts/predicate shape: a `*Ticket` captures
//! the expectation at dispatch, a `*Facts` captures observed live state at
//! completion, and one predicate validates them together.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Identity of the row an inline file operation was issued for.
///
/// # Why this exists
///
/// The rename completion used to re-read the section's **live** `context_target`
/// cell instead of the target the operation was issued for. `context_target` is
/// replaced by any right-click, by a new-item bind, and cleared on row recycling,
/// so when it changed while the rename worker ran, two things went wrong at once:
/// the renamed row's watch mirror kept the old path — silently ending
/// external-change detection for that file, so the next save overwrote another
/// program's edits — and, if the cell pointed at a *different* row, the renamed
/// path was written onto that unrelated row's item and inserted into the
/// directory-row index, so a later right-click Delete targeted the wrong file.
///
/// Capturing the row at dispatch and validating it at completion makes both
/// unrepresentable: the ticket carries the path the operation is *for*, and
/// [`FileOperationFacts`] carries the path the captured row *still has*.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileOperationTicket {
    /// Path the operation was issued against.
    path: PathBuf,
    /// Whether the target was a directory when the operation was issued.
    ///
    /// Captured rather than re-read, because the live cell can describe another
    /// row by the time the worker finishes, and directory renames take a
    /// different index-maintenance path from file renames.
    is_dir: bool,
    /// Whether this operation is committing a freshly created placeholder.
    is_new: bool,
}

/// What kind of target an inline file operation was issued against.
///
/// A named pair rather than two positional `bool` parameters. The production call
/// site derives `is_dir` from a multi-line `is_some_and` closure and then passes
/// `is_new` immediately after it, so a transposition would type-check, read
/// plausibly, and silently send directory renames down the file index-maintenance
/// path — the "a value must not be renamed while crossing a seam" defect this
/// module exists to make unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileOperationTargetKind {
    /// Whether the target was a directory when the operation was issued.
    pub(super) is_dir: bool,
    /// Whether this operation is committing a freshly created placeholder.
    pub(super) is_new: bool,
}

impl FileOperationTicket {
    /// Capture one inline file operation's target at dispatch time.
    pub(super) fn new(path: PathBuf, kind: FileOperationTargetKind) -> Self {
        Self {
            path,
            is_dir: kind.is_dir,
            is_new: kind.is_new,
        }
    }

    /// Return the path this operation was issued against.
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Return whether the captured target was a directory.
    pub(super) const fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// Return whether this operation commits a new placeholder.
    pub(super) const fn is_new(&self) -> bool {
        self.is_new
    }

    /// Return whether the captured row still describes this operation's target.
    ///
    /// A `false` answer means the row was recycled, re-bound, or replaced while
    /// the worker ran. The filesystem operation has still happened — this
    /// predicate gates only the **row projection and index maintenance**, which
    /// must never be applied to a row that now describes a different file.
    pub(super) fn row_is_current(&self, facts: &FileOperationFacts) -> bool {
        facts.row_path.as_deref() == Some(self.path.as_path())
    }
}

/// Live state observed at one inline file operation's completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileOperationFacts {
    /// Path the captured row's item reports **now**, or `None` if the row or its
    /// item is gone.
    pub(super) row_path: Option<PathBuf>,
}

impl FileOperationFacts {
    /// Record the live row identity one completion must be validated against.
    pub(super) const fn new(row_path: Option<PathBuf>) -> Self {
        Self { row_path }
    }
}

/// Window-owned projection of file tabs that sidebar rows may render.
///
/// The sidebar treats these paths as display identities only. The window still
/// owns duplicate detection, active-tab selection, canonical reconciliation,
/// and failed-load handling.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SidebarFileRowStateSnapshot {
    open_identities: HashSet<PathBuf>,
    active_identities: HashSet<PathBuf>,
}

impl SidebarFileRowStateSnapshot {
    /// Build a snapshot from the open and active file identities visible to the window.
    #[must_use]
    pub(crate) fn from_identities(
        open_identities: HashSet<PathBuf>,
        active_identities: HashSet<PathBuf>,
    ) -> Self {
        Self {
            open_identities,
            active_identities,
        }
    }

    /// Return whether a file-tree path is open in any tab.
    #[must_use]
    pub(crate) fn is_open(&self, path: &Path) -> bool {
        self.open_identities.contains(path)
    }

    /// Return whether a file-tree path is the active tab.
    #[must_use]
    pub(crate) fn is_active(&self, path: &Path) -> bool {
        self.active_identities.contains(path)
    }

    /// Owned copy of the open-tab identities, for the evidence surface.
    #[must_use]
    pub(crate) fn open_identities(&self) -> HashSet<PathBuf> {
        self.open_identities.clone()
    }

    /// Owned copy of the active-tab identities, for the evidence surface.
    #[must_use]
    pub(crate) fn active_identities(&self) -> HashSet<PathBuf> {
        self.active_identities.clone()
    }
}

/// Monotonic identity for one effective materialized watch-target set.
///
/// Moved here from `workspace_section/watch_targets.rs` by the workspace-tree
/// migration: that module was not one coordination job, and its two generation
/// newtypes are seam values — they exist to be captured at dispatch and compared
/// at completion, which is this module's whole subject.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct WatchTargetGeneration(u64);

impl WatchTargetGeneration {
    /// Advance to the next target generation.
    ///
    /// The mirror bookkeeping used to increment this by writing the tuple field
    /// directly. Moving the newtype into this module made that a privacy error,
    /// which is the encapsulation the original lacked: a generation may only ever
    /// move forward by one, and now nothing outside this module can set it to an
    /// arbitrary value.
    #[must_use]
    pub(super) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    /// Scalar generation for the evidence surface.
    ///
    /// Ungated on purpose: a monotonic identity is ordinary observable state, not
    /// test-only state, and the evidence surface needs it in a production build.
    #[must_use]
    pub(crate) const fn value_for_evidence(self) -> u64 {
        self.0
    }
}

/// Monotonic identity for one workspace-section object lifetime.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct WatchLifetimeGeneration(u64);

impl WatchLifetimeGeneration {
    #[must_use]
    pub(super) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// Expectation captured when one workspace watch install is dispatched.
///
/// # Why this exists
///
/// The pair used to travel into the watch-install worker as a loose
/// `(section_weak, generation, lifetime)` tuple and was compared clause by clause
/// in the completion closure. Both halves are `u64`-backed monotonic counters, so
/// nothing but argument order distinguished them — and the two mismatches mean
/// **opposite things**: a stale *lifetime* means this section's watching has ended
/// and the worker's result must be retired, while a stale *targets* generation
/// means the target set moved on and the install must be **re-entered**. A single
/// `bool` predicate collapsing "this section is gone" into "this generation is
/// stale" would silently pick one of those two behaviours for both cases.
///
/// Constructing the pair once at dispatch and asking it for a *disposition* keeps
/// the two consequences distinguishable, and makes passing one generation where
/// the other belongs a type error rather than a reordered argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceWatchTicket {
    targets_generation: WatchTargetGeneration,
    lifetime_generation: WatchLifetimeGeneration,
}

/// Live watch state observed when one install worker completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceWatchFacts {
    targets_generation: WatchTargetGeneration,
    lifetime_generation: WatchLifetimeGeneration,
}

/// What a completing watch install is allowed to do.
///
/// Named for the **decision** rather than as an `is_current` question, per the
/// naming lesson the notes migration recorded: the caller is not asking whether
/// something is current, it is asking what it may now do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceWatchDisposition {
    /// Still the current install: adopt the worker's watcher.
    Install,
    /// This section's watch lifetime ended: retire the result and do not re-enter.
    Retire,
    /// Targets moved on while the worker ran: retire this result and restart.
    Restart,
}

impl WorkspaceWatchTicket {
    /// Capture one watch install's expectation at dispatch time.
    pub(super) const fn new(
        targets_generation: WatchTargetGeneration,
        lifetime_generation: WatchLifetimeGeneration,
    ) -> Self {
        Self {
            targets_generation,
            lifetime_generation,
        }
    }

    /// Return the target generation this install was dispatched for.
    pub(super) const fn targets_generation(self) -> WatchTargetGeneration {
        self.targets_generation
    }

    /// Decide what a completing install may do, validating both halves together.
    ///
    /// Lifetime is checked **first** and deliberately: a section whose watching has
    /// stopped must not be restarted just because its target generation also moved,
    /// which is the ordering the clause-by-clause comparison had and which this
    /// preserves exactly.
    pub(super) fn disposition(self, facts: &WorkspaceWatchFacts) -> WorkspaceWatchDisposition {
        if facts.lifetime_generation != self.lifetime_generation {
            return WorkspaceWatchDisposition::Retire;
        }
        if facts.targets_generation != self.targets_generation {
            return WorkspaceWatchDisposition::Restart;
        }
        WorkspaceWatchDisposition::Install
    }
}

impl WorkspaceWatchFacts {
    /// Record the live watch state one completion must be validated against.
    pub(super) const fn new(
        targets_generation: WatchTargetGeneration,
        lifetime_generation: WatchLifetimeGeneration,
    ) -> Self {
        Self {
            targets_generation,
            lifetime_generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ticket() -> FileOperationTicket {
        FileOperationTicket::new(
            PathBuf::from("/w/notes.md"),
            FileOperationTargetKind {
                is_dir: false,
                is_new: false,
            },
        )
    }

    #[test]
    fn row_is_current_when_the_captured_row_still_holds_the_issued_path() {
        assert!(
            ticket().row_is_current(&FileOperationFacts::new(Some(PathBuf::from("/w/notes.md"))))
        );
    }

    #[test]
    fn a_recycled_row_is_not_current() {
        // The regression this seam exists for: the row now describes another
        // file, so its projection and index entries must not be rewritten.
        assert!(
            !ticket().row_is_current(&FileOperationFacts::new(Some(PathBuf::from("/w/other.md"))))
        );
    }

    #[test]
    fn a_vanished_row_is_not_current() {
        assert!(!ticket().row_is_current(&FileOperationFacts::new(None)));
    }

    #[test]
    fn the_ticket_reports_the_captured_target_rather_than_live_state() {
        let directory = FileOperationTicket::new(
            PathBuf::from("/w/sub"),
            FileOperationTargetKind {
                is_dir: true,
                is_new: true,
            },
        );
        assert_eq!(directory.path(), Path::new("/w/sub"));
        assert!(directory.is_dir());
        assert!(directory.is_new());
        let file = ticket();
        assert!(!file.is_dir());
        assert!(!file.is_new());
    }

    // --- Workspace watch install ---

    fn generations(
        targets: u64,
        lifetime: u64,
    ) -> (WatchTargetGeneration, WatchLifetimeGeneration) {
        let mut target_generation = WatchTargetGeneration::default();
        for _ in 0..targets {
            target_generation = target_generation.next();
        }
        let mut lifetime_generation = WatchLifetimeGeneration::default();
        for _ in 0..lifetime {
            lifetime_generation = lifetime_generation.next();
        }
        (target_generation, lifetime_generation)
    }

    #[test]
    fn an_unchanged_install_may_adopt_its_watcher() {
        let (targets, lifetime) = generations(3, 2);
        let ticket = WorkspaceWatchTicket::new(targets, lifetime);
        assert_eq!(
            ticket.disposition(&WorkspaceWatchFacts::new(targets, lifetime)),
            WorkspaceWatchDisposition::Install
        );
    }

    #[test]
    fn a_stale_lifetime_retires_rather_than_restarting() {
        // The distinction this seam exists for: the section stopped watching, so
        // the result must be dropped and the install must NOT be re-entered.
        let (targets, lifetime) = generations(3, 2);
        let facts = WorkspaceWatchFacts::new(targets, lifetime.next());
        assert_eq!(
            WorkspaceWatchTicket::new(targets, lifetime).disposition(&facts),
            WorkspaceWatchDisposition::Retire
        );
    }

    #[test]
    fn a_stale_target_generation_restarts_rather_than_retiring() {
        let (targets, lifetime) = generations(3, 2);
        let (newer_targets, _) = generations(4, 2);
        let facts = WorkspaceWatchFacts::new(newer_targets, lifetime);
        assert_eq!(
            WorkspaceWatchTicket::new(targets, lifetime).disposition(&facts),
            WorkspaceWatchDisposition::Restart
        );
    }

    #[test]
    fn a_stale_lifetime_wins_over_a_stale_target_generation() {
        // Ordering is load-bearing and preserves the clause-by-clause original: a
        // section whose watching has stopped must not be restarted merely because
        // its targets also moved on.
        let (targets, lifetime) = generations(3, 2);
        let (newer_targets, _) = generations(4, 2);
        let facts = WorkspaceWatchFacts::new(newer_targets, lifetime.next());
        assert_eq!(
            WorkspaceWatchTicket::new(targets, lifetime).disposition(&facts),
            WorkspaceWatchDisposition::Retire
        );
    }
}
