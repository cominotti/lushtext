// SPDX-License-Identifier: GPL-3.0-or-later

//! # The window-composition workflow
//!
//! How a window gets from the application's `open` or `activate` to a fully
//! composed shell without any main-loop dispatch exceeding B_settle
//! (`model::responsiveness_budget`), and without the chrome moving once it is
//! drawn (`first-frame-window-shell`, C4b of the long-line programme), with
//! the content filled in one frame after the first (`shorten-window-content-fill-in`, C4c).
//!
//! The window is **built whole and revealed in stages**: one template, every
//! template child constructed in stage C, and the content the first frame does
//! not need marked invisible in the template (the content stack, the workspace
//! sidebar, the document-properties panel) or kept invisible while concealed
//! (the children of the search-panel, palette and Focus Mode revealers). GTK
//! validates CSS for, measures and maps only visible widgets (ledger A47,
//! A49), so invisible content costs the realize, present and first-frame
//! dispatches nothing, while every object exists for every read from C on.
//!
//! ## Role table
//!
//! | Module | Role | Owns |
//! | --- | --- | --- |
//! | this file | narrative facade | this narration and the workflow's entry operations |
//! | [`policy`] | pure policy | the stage order, which stages a window schedules, forced chains, the painted-frame decision, the activation turn size |
//! | `execution` | coordination | the pending request, the idle dispatches, the reveal dispatch after the first frame, forced reveals, activation turns and their shown frame, revealer concealment, teardown |
//! | `seams` | seam value objects | `PendingWindowRequest`, from the handler to stage B1 |
//! | [`evidence`] | evidence | [`evidence::CompositionEvidence`], which the construction evidence and the readiness blocker project |
//! | `test_policy` | test-only overrides | collapse to one dispatch; hold before a stage |
//!
//! ## Stage order
//!
//! 1. **Request.** `open` or `activate` records a `PendingWindowRequest`
//!    (files, an application hold, the request time) and queues stage W; it
//!    does no window work, so the main loop runs first. A second request
//!    before C merges its files into the first. With a window already
//!    composed, `activate` presents it and `open` admits its files there.
//! 2. **W, warm.** One text layout and the first frame's icon lookups, once per
//!    process, so they are not paid inside the first frame's measure.
//! 3. **C, construct.** `LushtextWindow::new`: the whole template, the wiring,
//!    size and maximized restore, the split-view restore plan, and the three
//!    breakpoints, installed before the window is realized (ledger A50: a
//!    breakpoint that matches on the first allocation after map applies before
//!    the child is allocated). The window is added to the application, so the
//!    request's hold is released here.
//! 4. **R, realize** (GTK's one-time first-realize cost, ledger A46), and
//! 5. **P, present** (CSS validation and map of the visible tree), **in one
//!    dispatch**: a realize alone runs a frame on the unmapped window that is
//!    never presented and paces the first visible frame a full 60 Hz interval
//!    behind it (ledger A51).
//! 6. **F1**: the first frame, the first painted on the mapped window, draws
//!    the chrome and the skeleton — the header bar, the tab bar, the status
//!    bar, and the rail and pane edges of the real split views — in their
//!    final geometry.
//! 7. **B1–B3, one reveal dispatch** after F1, in stage order, one span each:
//!    **content** (the content stack, and the activation's files admitted in
//!    bounded turns, `policy::activation_opens_this_turn`, the session
//!    restore's page turn), **workspace** (the sidebar, into a shown or a
//!    hidden rail) and **properties** (the panel, into a shown or a hidden
//!    pane or sheet). A rail the restored layout hides draws nothing either
//!    way; revealing it anyway leaves the composed window identical to an
//!    unstaged one.
//! 8. **Complete** on the first frame painted after the reveal dispatch; the
//!    **activation shown** frame is the first painted after the activation's
//!    queued opens drained: the opened files' tabs and editors on screen (a
//!    file's text may still be loading).
//!
//! ## Where control leaves, and where it comes back
//!
//! - **1 → 2 → 3 → 4+5**: each is a `G_PRIORITY_DEFAULT_IDLE` source, and
//!   control resumes in `execution`'s `run_warm`, `run_construct` and
//!   `run_realize_and_present`.
//! - **4+5 → 6**: `present` maps the window; R and P are *observed* through
//!   the window's `realize` and `map` signals, so a window its caller presents
//!   composes the same way.
//! - **6 → 7 → 8**: the window's frame clock `after-paint` resumes in
//!   `composition_frame_painted` (frames on the unmapped window are ignored),
//!   which asks `policy::frame_painted_decision`; after F1 it queues the one
//!   reveal dispatch, and after the frame that shows it, completes. The same
//!   handler stamps the activation's shown frame and stays connected until
//!   both are known.
//! - **Activation turns**: each turn is an idle that re-arms while files remain;
//!   the last one queues a frame, which stamps the activation shown.
//!
//! ## Forced reveals
//!
//! An operation that needs a staged surface **visible, focusable or
//! allocated** calls [`LushtextWindow::ensure_composed`] first, which completes
//! the stages that surface needs (`policy::CompositionPlan::forced_chain`)
//! synchronously in the current dispatch: the sidebar and properties toggles,
//! and every path that focuses the editor. Reads and projections never force
//! a stage, because every object exists from C. Actions and accelerators exist
//! from C, so none is dropped. Hidden surfaces (search panel, palette, Focus
//! Mode affordance, Markdown preview) are not stages: their child becomes
//! visible as their revealer opens.
//!
//! ## Teardown
//!
//! Every source and handler is removed when the window leaves its application
//! (`window-removed`, ledger A21), so no stage runs on a closed window; a quit
//! before stage C drops the pending request and its hold.

pub mod evidence;
mod execution;
pub mod policy;
mod seams;
mod test_policy;

pub(crate) use execution::CompositionState;
pub(crate) use execution::{drop_pending_window_request, request_window};
pub use policy::StagedSurface;
pub use seams::PendingWindowRequest;
#[cfg(feature = "test-utils")]
pub use test_policy::{set_composition_collapsed_for_test, set_composition_hold_before_for_test};
