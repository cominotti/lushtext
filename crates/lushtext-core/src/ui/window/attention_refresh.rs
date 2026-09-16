// SPDX-License-Identifier: GPL-3.0-or-later

//! Refresh the workspace surfaces when the user's attention returns to them.
//!
//! **Cross-cutting coordination with two owning workflows**, `WFR-COMMAND-PALETTE`
//! and `WFR-WORKSPACE-TREE`; it carries no role name deliberately, in the shape
//! `editor_focus.rs` already uses. Its pure half is
//! [`crate::model::attention_refresh`], shared for the same reason.
//!
//! # Why this exists
//!
//! The palette's file index has no filesystem input at all. Its only update
//! paths are a full rebuild on workspace-membership or visibility change and
//! three incremental mutations issued from the sidebar's own context-menu file
//! operations, so a file created by `git checkout`, by a build, or by a
//! terminal never became searchable. The sidebar's watcher could not close the
//! gap: it watches only materialized rows, non-recursively, while the index is
//! built recursively over the whole workspace.
//!
//! # Why a refresh and not a staleness sweep
//!
//! Detecting *which* directories changed is genuinely cheaper than rebuilding —
//! measured at roughly sixty times cheaper on a warm cache. It still loses,
//! because above `MAX_PENDING_INDEX_UPDATES` the incremental queue escalates to
//! a full rebuild anyway, and a branch switch is far above it. A sweep would
//! pay full detection cost for the motivating scenario and then discard the
//! result. `design.md` for `refresh-workspace-index-on-attention` records the
//! rest of the evidence.
//!
//! # The two moments, and what each one refreshes
//!
//! Window activation is causally correlated with the change being detected:
//! external edits happen while the application is *not* focused, by
//! construction. Palette open covers the remainder, where the user never left —
//! and refreshes **only the index**, because the palette does not read the
//! sidebar tree and a full materialized-tree rescan per `Ctrl+Shift+P` is real
//! filesystem work for nobody.
//!
//! Both delegate to refresh paths that already exist, are already debounced and
//! generation-guarded, and are already covered by the `command-palette-index`
//! and `workspace-refresh-complete` readiness blockers — which is why this
//! change adds no readiness predicate of its own.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use glib::subclass::prelude::ObjectSubclassIsExt;

use crate::model::attention_refresh::{
    AttentionRefreshAdmission, AttentionRefreshFacts, admit_attention_refresh,
};

use super::LushtextWindow;

/// Which surfaces one attention moment refreshes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionSurfaces {
    /// The palette file index only, for a moment the sidebar cannot have missed.
    IndexOnly,
    /// The index and the workspace tree, for a return from outside the app.
    IndexAndTree,
}

/// Refresh history for one workspace folder set.
///
/// Exactly one is retained, and it carries the folder set it describes. A
/// second record is never useful: both readers ask about the *current* scope,
/// and keeping past scopes cost a cap, an eviction rule, and — because an
/// eviction rule must not discard a live measurement — a record that could
/// strand `started_at` forever and wedge the eviction it was meant to bound.
struct AttentionRefreshRecord {
    /// The workspace folder set this record describes.
    folders: Vec<PathBuf>,
    /// When the in-flight refresh started, if one has not settled yet.
    started_at: Option<Instant>,
    /// When the previous refresh settled.
    settled_at: Option<Instant>,
    /// How long the previous refresh took, measured from the attention moment.
    ///
    /// This spans the palette rebuild's 300 ms debounce and any queue wait, not
    /// only the traversal, because that is the latency the throttle is trying
    /// to keep proportionate. One consequence is worth stating: the effective
    /// gap after a trivial refresh is the debounce times the multiplier rather
    /// than the bare floor, so `ATTENTION_REFRESH_MIN_INTERVAL` is a lower
    /// bound the shipped behaviour stays above rather than one it sits on.
    previous_duration: Option<Duration>,
}

impl AttentionRefreshRecord {
    /// A record for `folders` with no history yet.
    const fn new(folders: Vec<PathBuf>) -> Self {
        Self {
            folders,
            started_at: None,
            settled_at: None,
            previous_duration: None,
        }
    }
}

thread_local! {
    /// The one retained refresh record, shared by every window in the process.
    ///
    /// Process-wide rather than per-window so several windows over one
    /// workspace refresh it once between them. GTK confines this to the main
    /// thread, which is what makes a `thread_local` that scope.
    static ATTENTION_REFRESH: RefCell<Option<AttentionRefreshRecord>> = const { RefCell::new(None) };

    /// App-owned modal surfaces currently on screen anywhere in the process.
    static OPEN_MODAL_SURFACES: Cell<usize> = const { Cell::new(0) };
}

/// Suppresses attention refresh while an app-owned modal surface is on screen.
///
/// # Why this is not derived from how long the window was inactive
///
/// A tempting generalisation is to treat a short absence as "not really away"
/// and refuse on that alone, with no call-site cooperation. It does not work
/// for the case this guard exists for: during a Save As the user can browse the
/// chooser for many seconds, so the absence is long by any threshold, and the
/// window still reactivates *before* the resulting save has been queued — the
/// exact instant at which no editor is saving yet and a refresh would be
/// admitted into the shared worker pool immediately ahead of the save.
///
/// # Why a guard rather than a pair of calls
///
/// The obligation is per-call-site and unenforced either way, but a value that
/// releases on drop survives an early `return` in a dialog callback, which a
/// trailing `note_closed()` does not.
#[must_use = "the guard must be held for the surface's lifetime"]
pub struct ModalSurfaceGuard(());

impl ModalSurfaceGuard {
    /// Note that an app-owned modal surface has been shown.
    ///
    /// Hold the returned guard until the surface's result has been handled —
    /// including the cancelled path, which is what the drop covers.
    ///
    /// Named `acquire` rather than `new` because acquiring is the point: the
    /// value has no meaning apart from the suppression it holds, so a `Default`
    /// that produced one silently would be wrong.
    pub fn acquire() -> Self {
        OPEN_MODAL_SURFACES.with(|open| open.set(open.get().saturating_add(1)));
        Self(())
    }
}

impl Drop for ModalSurfaceGuard {
    fn drop(&mut self) {
        OPEN_MODAL_SURFACES.with(|open| open.set(open.get().saturating_sub(1)));
    }
}

impl LushtextWindow {
    /// Refresh the workspace surfaces because the user's attention returned.
    ///
    /// Returns the admission so callers and tests can see *why* a moment was
    /// refused. The two production callers are trigger sites with nothing to
    /// decide and discard it explicitly.
    #[must_use]
    pub fn refresh_workspace_surfaces_on_attention(
        &self,
        surfaces: AttentionSurfaces,
    ) -> AttentionRefreshAdmission {
        let folders = self.current_workspace_folder_paths();
        if folders.is_empty() {
            return AttentionRefreshAdmission::RefuseNoWorkspace;
        }
        let now = Instant::now();
        let user_work_in_flight = self.user_work_blocks_attention_refresh();
        let admission = ATTENTION_REFRESH.with(|state| {
            let mut state = state.borrow_mut();
            // A different folder set is a different question, so the record is
            // replaced rather than looked up. That is also what makes a refresh
            // left in flight by a scope change unable to outlive its scope.
            // Written as insert-then-compare so the lookup has no panic path:
            // `folders` is non-empty here, so a freshly inserted placeholder is
            // always replaced on this same pass.
            let record = state.get_or_insert_with(|| AttentionRefreshRecord::new(Vec::new()));
            if record.folders != folders {
                *record = AttentionRefreshRecord::new(folders);
            }
            let admission = admit_attention_refresh(AttentionRefreshFacts {
                in_flight: record.started_at.is_some(),
                since_previous: record
                    .settled_at
                    .map(|settled| now.saturating_duration_since(settled)),
                previous_duration: record.previous_duration,
                user_work_in_flight,
            });
            if admission == AttentionRefreshAdmission::Start {
                record.started_at = Some(now);
            }
            admission
        });

        if admission == AttentionRefreshAdmission::Start {
            self.rebuild_file_index();
            if surfaces == AttentionSurfaces::IndexAndTree {
                self.imp().sidebar.refresh_folder_trees_silently();
            }
        }
        admission
    }

    /// Whether an operation that protects unsaved work is in flight.
    ///
    /// A refresh would queue behind these on `spawn_blocking_then`'s shared
    /// worker slots. The modal-surface count is part of it because
    /// `notify::is-active` also flips around a native or portal dialog, and a
    /// save-in-flight check alone is too late there: the window reactivates
    /// before the save it is about to race has been queued.
    ///
    /// This composite is the fourth place in the shell to ask "is user work in
    /// flight", after `close_request` and the close-safety fingerprint check,
    /// and the three memberships differ. Converging them is recorded as a
    /// deferral in the change's `design.md` rather than done here.
    fn user_work_blocks_attention_refresh(&self) -> bool {
        let imp = self.imp();
        OPEN_MODAL_SURFACES.with(Cell::get) > 0
            || self.has_saving_editors()
            || imp.session.close_safety_inflight.get()
            || imp.drafts.autosave_pending.get()
            || imp.drafts.first_dirty_autosave_pending.get()
    }
}

/// Record that an attention-triggered refresh has settled.
///
/// Called from the file-index build terminal, which is the dominant cost of
/// the pair: the sidebar's half is bounded by its materialized rows, so
/// timing the index is what makes the adaptive interval track the thing
/// that can actually be slow.
///
/// It deliberately does **not** re-resolve the current folder set. The one
/// retained record carries its own key, and there is at most one refresh in
/// flight, so taking `started_at` from that record settles exactly the
/// refresh that set it. Re-resolving would mean a scope change between
/// start and terminal settles a different record than the one that started,
/// leaving the first permanently in flight.
pub(super) fn note_attention_refresh_settled() {
    ATTENTION_REFRESH.with(|state| {
        let mut state = state.borrow_mut();
        let Some(record) = state.as_mut() else {
            return;
        };
        let Some(started_at) = record.started_at.take() else {
            // A rebuild this module did not start — a workspace-scope or
            // visibility change — must not be credited as an attention
            // refresh, or it would throttle the next real one.
            return;
        };
        let now = Instant::now();
        record.settled_at = Some(now);
        record.previous_duration = Some(now.saturating_duration_since(started_at));
    });
}
