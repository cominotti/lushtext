// SPDX-License-Identifier: GPL-3.0-or-later

//! `execution` role for the workspace tree workflow's **workspace path reveal**
//! stage order: `win.reveal-workspace-path`.
//!
//! # Role
//!
//! Coordination, `execution`, qualified by the stage order it serves, at the
//! workflow's canonical role home. It owns the reveal's generation counter, its
//! poll source, and its exactly-once terminal. The pure halves — resolving a path
//! to its workspace, folder, and ancestor chain, and classifying a component a
//! scan did not list — are `policy.rs`'s; the per-section steps it drives (find a
//! row, select and `scroll_to` it, read the slice bin's evidence) are the
//! section's own, in `workspace_section/folder_execution.rs`.
//!
//! # Stage order, and where control resumes
//!
//! resolve → (expand the next ancestor ⇢ **await its admitted scan** → repeat) →
//! select and `scroll_to` ⇢ **await the bin's settle** → terminal.
//!
//! Both waits resume in one 16 ms poll source owned here, never at the call site:
//!
//! 1. **Scan resumption.** An ancestor is expanded with `TreeListRow::set_expanded`,
//!    exactly as a user click does, so the section's bounded, admitted,
//!    generation-guarded child scan runs and the per-directory entry cap applies.
//!    The poll resumes once the section reports no child scan, admission retry, or
//!    batched reconciliation pending, and only then decides `not-found` or
//!    `beyond-entry-cap` from the listing the scan actually produced.
//! 2. **Settle resumption.** After the list's own `scroll_to`, the slice bin
//!    forwards the request to the outer scroller from an idle. The poll ends the
//!    reveal `revealed` on the first frame in which the row is mapped, drawn wholly
//!    inside the outer viewport, and the bin reports no pending request; it counts
//!    only frames the clock actually produced, so a loaded compositor does not
//!    shorten the budget. `REVEAL_SETTLE_FRAME_BUDGET` frames (or the wall budget,
//!    when frames stop) end it `unsettled`, a real finding that is never retried.
//!
//! The reveal never touches the outer adjustment itself: the bin's forwarding is
//! the path under proof.
//!
//! # Terminals
//!
//! Every path reaches exactly one terminal through `terminate_reveal`, which
//! retires the poll source with it. A newer reveal and sidebar disposal end the
//! active one `superseded`; a stale poll for an older generation finds no active
//! reveal and stops. A reveal that fails after expanding (`not-found`, `beyond-entry-cap`)
//! collapses what it expanded, so a failure leaves the tree as it found it.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::sidebar::policy::{
    self, REVEAL_SETTLE_FRAME_BUDGET, WorkspaceRevealPlan, WorkspaceRevealRefusal,
    WorkspaceRevealStatus,
};

use super::{LushtextSidebar, WorkspaceSection};

/// How often the active reveal re-checks its scan or settle condition.
const REVEAL_POLL_INTERVAL: Duration = Duration::from_millis(16);
/// Wall-clock ceiling on the settle stage, for a frame clock that stops ticking
/// (an unmapped sidebar produces no frames, so the frame budget alone would never
/// run out).
const REVEAL_SETTLE_WALL_BUDGET: Duration = Duration::from_secs(10);

/// The window's reveal bookkeeping: the latest status for evidence, and the one
/// active reveal.
#[derive(Default)]
pub(crate) struct RevealState {
    generation: Cell<u64>,
    status: Cell<WorkspaceRevealStatus>,
    depth: Cell<Option<u32>>,
    superseded_count: Cell<u64>,
    active: RefCell<Option<ActiveReveal>>,
    /// The section and target of the latest `revealed` reveal, for geometry.
    revealed: RefCell<Option<(glib::WeakRef<WorkspaceSection>, PathBuf)>>,
}

struct ActiveReveal {
    generation: u64,
    section: glib::WeakRef<WorkspaceSection>,
    plan: WorkspaceRevealPlan,
    next_ancestor: usize,
    expanded_by_reveal: Vec<PathBuf>,
    stage: RevealStage,
    source: Option<glib::SourceId>,
}

#[derive(Clone, Copy)]
enum RevealStage {
    Expanding,
    Settling {
        frames: u32,
        last_frame: Option<i64>,
        started: Instant,
    },
}

/// Who retires the poll source when a reveal terminates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PollSource {
    /// The terminal runs outside the poll, so it removes the source itself.
    Remove,
    /// The terminal runs inside the poll, whose `Break` removes the source.
    RemovedByPoll,
}

/// What one poll decided.
enum RevealStep {
    Wait,
    Finish(WorkspaceRevealStatus),
}

/// The latest reveal as evidence reads it.
pub(crate) struct RevealObservation {
    pub(crate) status: WorkspaceRevealStatus,
    pub(crate) generation: u64,
    pub(crate) depth: Option<u32>,
    pub(crate) superseded_count: u64,
    pub(crate) pending: bool,
}

/// The widgets a completed reveal is drawn with, for the visual-geometry snapshot.
pub(crate) struct RevealGeometry {
    pub(crate) viewport: gtk4::Widget,
    pub(crate) row: gtk4::Widget,
    pub(crate) header: gtk4::Widget,
    pub(crate) neighbours: Vec<gtk4::Widget>,
}

/// Neighbour rows reported on each side of a revealed row (eight in all).
const REVEAL_NEIGHBOURS_PER_SIDE: usize = 4;

impl LushtextSidebar {
    /// Start revealing `target`, superseding any reveal still in flight.
    pub(super) fn begin_workspace_path_reveal(&self, target: &Path) {
        let state = &self.imp().reveal;
        self.supersede_active_reveal();
        let generation = state.generation.get().saturating_add(1);
        state.generation.set(generation);
        state.depth.set(None);
        state.revealed.borrow_mut().take();

        let resolved = {
            let file = self.imp().workspaces_file.borrow();
            let scope = self.imp().current_scope.borrow();
            let visibility =
                crate::ui::workspace_visibility::workspace_entry_visibility(&self.imp().settings);
            policy::resolve_workspace_reveal(&file, &scope, &visibility, target)
        };
        let mut plan = match resolved {
            Ok(plan) => plan,
            Err(refusal) => {
                state.status.set(refusal.into());
                return;
            }
        };
        let Some(section) = self.section_for_workspace(&plan.workspace_id) else {
            // The workspace is configured but its section is not built yet (a
            // reveal issued while the sidebar rebuilds): nothing can come to rest.
            state.status.set(WorkspaceRevealStatus::Unsettled);
            return;
        };
        if let Some(root) = section.focused_root() {
            // A focused-folder drilldown shows only `root` as a top-level row.
            if !target.starts_with(&root) {
                state
                    .status
                    .set(WorkspaceRevealRefusal::OutsideScope.into());
                return;
            }
            plan.ancestors
                .retain(|ancestor| ancestor.starts_with(&root));
        }

        state.status.set(WorkspaceRevealStatus::Pending);
        state.depth.set(Some(plan.depth()));
        let sidebar_weak = self.downgrade();
        let source = glib::timeout_add_local(REVEAL_POLL_INTERVAL, move || {
            let Some(sidebar) = sidebar_weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            sidebar.poll_workspace_path_reveal(generation)
        });
        *state.active.borrow_mut() = Some(ActiveReveal {
            generation,
            section: section.downgrade(),
            plan,
            next_ancestor: 0,
            expanded_by_reveal: Vec::new(),
            stage: RevealStage::Expanding,
            source: Some(source),
        });
    }

    /// End the active reveal, if any, `superseded` (a newer reveal, sidebar
    /// disposal, or unrooting), from outside its poll.
    pub(super) fn supersede_active_reveal(&self) {
        self.terminate_reveal(WorkspaceRevealStatus::Superseded, PollSource::Remove);
    }

    /// Whether a reveal is still expanding, scanning, or settling.
    #[must_use]
    pub fn workspace_path_reveal_pending(&self) -> bool {
        self.imp().reveal.active.borrow().is_some()
    }

    /// The latest reveal's status, generation, depth, and supersession count.
    pub(crate) fn workspace_reveal_observation(&self) -> RevealObservation {
        let state = &self.imp().reveal;
        RevealObservation {
            status: state.status.get(),
            generation: state.generation.get(),
            depth: state.depth.get(),
            superseded_count: state.superseded_count.get(),
            pending: self.workspace_path_reveal_pending(),
        }
    }

    /// The widgets of the latest `revealed` row, or why there are none.
    pub(crate) fn workspace_reveal_geometry(&self) -> Result<RevealGeometry, &'static str> {
        let state = &self.imp().reveal;
        if state.status.get() != WorkspaceRevealStatus::Revealed {
            return Err("no-reveal");
        }
        let (section, target) = {
            let revealed = state.revealed.borrow();
            let Some((section, target)) = revealed.as_ref() else {
                return Err("no-reveal");
            };
            (section.upgrade().ok_or("no-reveal")?, target.clone())
        };
        let row = section
            .mapped_row_widget_for_path(&target)
            .ok_or("row-not-realized")?;
        Ok(RevealGeometry {
            viewport: self.imp().outer_scrolled_window.get().upcast(),
            neighbours: section.mapped_neighbour_rows(&row, REVEAL_NEIGHBOURS_PER_SIDE),
            header: section.header_widget(),
            row,
        })
    }

    /// The sidebar's outer vertical adjustment, which only the slice bins move
    /// during a reveal; exported as the `workspace-sidebar-viewport` scroll anchor.
    pub(crate) fn sidebar_viewport_vadjustment(&self) -> gtk4::Adjustment {
        self.imp().outer_scrolled_window.vadjustment()
    }

    fn section_for_workspace(
        &self,
        id: &crate::model::workspace::WorkspaceId,
    ) -> Option<WorkspaceSection> {
        self.imp()
            .sections
            .borrow()
            .iter()
            .find(|section| &section.workspace_id() == id)
            .cloned()
    }

    /// One poll of the active reveal: the single resumption point for both waits.
    fn poll_workspace_path_reveal(&self, generation: u64) -> glib::ControlFlow {
        let snapshot = {
            let active = self.imp().reveal.active.borrow();
            match active.as_ref() {
                Some(active) if active.generation == generation => {
                    Some((active.section.upgrade(), active.stage))
                }
                _ => None,
            }
        };
        let Some((section, stage)) = snapshot else {
            return glib::ControlFlow::Break;
        };
        let Some(section) = section else {
            self.finish_reveal(WorkspaceRevealStatus::Superseded);
            return glib::ControlFlow::Break;
        };
        let step = match stage {
            RevealStage::Expanding => self.advance_expansion(&section),
            RevealStage::Settling {
                frames,
                last_frame,
                started,
            } => self.check_settle(&section, frames, last_frame, started),
        };
        match step {
            RevealStep::Wait => glib::ControlFlow::Continue,
            RevealStep::Finish(status) => {
                self.finish_reveal(status);
                glib::ControlFlow::Break
            }
        }
    }

    /// Expand the next collapsed ancestor, or select the target once every
    /// ancestor is expanded and scanned.
    fn advance_expansion(&self, section: &WorkspaceSection) -> RevealStep {
        if section.child_scans_pending() {
            return RevealStep::Wait;
        }
        let (ancestors, target, mut next) = {
            let active = self.imp().reveal.active.borrow();
            let Some(active) = active.as_ref() else {
                return RevealStep::Wait;
            };
            (
                active.plan.ancestors.clone(),
                active.plan.target.clone(),
                active.next_ancestor,
            )
        };

        while let Some(directory) = ancestors.get(next) {
            let Some(row) = section.tree_row_for_path(directory) else {
                let parent = next.checked_sub(1).and_then(|parent| ancestors.get(parent));
                return RevealStep::Finish(missing_component(section, parent, directory, true));
            };
            if !row.is_expanded() {
                row.set_expanded(true);
                if !row.is_expanded() {
                    // Known empty, or not a directory: nothing below it to reveal.
                    return RevealStep::Finish(WorkspaceRevealStatus::NotFound);
                }
                self.record_reveal_expansion(next, directory);
                return RevealStep::Wait;
            }
            next += 1;
        }
        self.set_next_ancestor(next);

        let Some((position, _)) = section.flattened_row_for_path(&target) else {
            return RevealStep::Finish(missing_component(
                section,
                ancestors.last(),
                &target,
                false,
            ));
        };
        section.select_and_scroll_to_position(position);
        if let Some(active) = self.imp().reveal.active.borrow_mut().as_mut() {
            active.stage = RevealStage::Settling {
                frames: 0,
                last_frame: None,
                started: Instant::now(),
            };
        }
        RevealStep::Wait
    }

    /// Settle check: revealed once the row is mapped, wholly inside the outer
    /// viewport, and the bin has nothing pending.
    fn check_settle(
        &self,
        section: &WorkspaceSection,
        frames: u32,
        last_frame: Option<i64>,
        started: Instant,
    ) -> RevealStep {
        let target = {
            let active = self.imp().reveal.active.borrow();
            let Some(active) = active.as_ref() else {
                return RevealStep::Wait;
            };
            active.plan.target.clone()
        };
        let outer = self.imp().outer_scrolled_window.get();
        let at_rest = !section.file_tree_slice_bin().outer_request_pending()
            && section
                .mapped_row_widget_for_path(&target)
                .and_then(|row| row.compute_bounds(&outer))
                .is_some_and(|bounds| {
                    bounds.y() >= 0.0
                        && bounds.y() + bounds.height() <= outer.height() as f32
                        && bounds.height() > 0.0
                });
        if at_rest {
            return RevealStep::Finish(WorkspaceRevealStatus::Revealed);
        }
        // Count frames the clock actually produced; an unrealized list has no
        // clock, and every poll then counts as one.
        let frame = section
            .file_tree_slice_bin()
            .frame_clock()
            .map(|clock| clock.frame_counter());
        let advanced = frame.is_none() || frame != last_frame;
        let frames = if advanced {
            frames.saturating_add(1)
        } else {
            frames
        };
        if frames >= REVEAL_SETTLE_FRAME_BUDGET || started.elapsed() >= REVEAL_SETTLE_WALL_BUDGET {
            return RevealStep::Finish(WorkspaceRevealStatus::Unsettled);
        }
        if let Some(active) = self.imp().reveal.active.borrow_mut().as_mut() {
            active.stage = RevealStage::Settling {
                frames,
                last_frame: frame,
                started,
            };
        }
        RevealStep::Wait
    }

    fn record_reveal_expansion(&self, index: usize, directory: &Path) {
        if let Some(active) = self.imp().reveal.active.borrow_mut().as_mut() {
            active.next_ancestor = index;
            active.expanded_by_reveal.push(directory.to_path_buf());
        }
    }

    fn set_next_ancestor(&self, index: usize) {
        if let Some(active) = self.imp().reveal.active.borrow_mut().as_mut() {
            active.next_ancestor = index;
        }
    }

    /// The terminal of a reveal that ended from its own poll.
    fn finish_reveal(&self, status: WorkspaceRevealStatus) {
        // The poll that finishes returns `Break`, which removes its own source;
        // removing it here as well would be a double removal.
        self.terminate_reveal(status, PollSource::RemovedByPoll);
    }

    /// The exactly-once terminal every reveal path reaches.
    fn terminate_reveal(&self, status: WorkspaceRevealStatus, poll_source: PollSource) {
        let Some(mut active) = self.imp().reveal.active.borrow_mut().take() else {
            return;
        };
        if let Some(source) = active.source.take()
            && poll_source == PollSource::Remove
        {
            source.remove();
        }
        let state = &self.imp().reveal;
        state.status.set(status);
        if status == WorkspaceRevealStatus::Superseded {
            state
                .superseded_count
                .set(state.superseded_count.get().saturating_add(1));
        }
        let section = active.section.upgrade();
        match (status, section) {
            (WorkspaceRevealStatus::Revealed, Some(section)) => {
                *state.revealed.borrow_mut() = Some((section.downgrade(), active.plan.target));
            }
            (
                WorkspaceRevealStatus::NotFound | WorkspaceRevealStatus::BeyondEntryCap,
                Some(section),
            ) => {
                // Leave the tree as it was found: collapse, deepest first, only
                // what this reveal expanded.
                for directory in active.expanded_by_reveal.iter().rev() {
                    if let Some(row) = section.tree_row_for_path(directory) {
                        row.set_expanded(false);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Classify `missing`, a component the completed scan of `parent` did not list.
fn missing_component(
    section: &WorkspaceSection,
    parent: Option<&PathBuf>,
    missing: &Path,
    known_directory: bool,
) -> WorkspaceRevealStatus {
    let Some(parent) = parent else {
        // The top-level folder row itself is missing from the section.
        return WorkspaceRevealStatus::NotFound;
    };
    let last_kept = section
        .tree_row_for_path(parent)
        .and_then(|row| WorkspaceSection::truncated_listing_tail(&row));
    policy::classify_missing_reveal_component(
        missing,
        known_directory,
        last_kept
            .as_ref()
            .map(|(path, is_dir)| (path.as_path(), *is_dir)),
    )
    .into()
}
