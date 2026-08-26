// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure policy for the document-load workflow.
//!
//! Every decision the load workflow makes that does not need a widget lives
//! here: whether decoded text is installed in one turn or in bounded slices, how
//! wide a clear slice may be and where it must stop, which install phase a
//! scheduled slice may run, what an abort disposition means for a session in a
//! given phase, and whether a dispatched request still describes its editor.
//!
//! This module imports no `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`.
//! That purity is not decoration: `.cargo/mutants.toml` reaches pure policy
//! through `ui/**/policy.rs`, and `make check-workflow-boundaries` fails on a
//! single such import. Keep the GTK iterators, buffers, and marks on the
//! coordination side and pass this module numbers and enums.
//!
//! ## The paragraph-boundary contract
//!
//! Bounded install and clear slices MUST end on a paragraph boundary.
//! `GtkTextBuffer` validates layout a whole paragraph at a time, so a slice that
//! stops mid-paragraph re-lays-out everything already installed in that
//! paragraph on every later slice — quadratic work that once froze crash
//! recovery of a 33 MB single-line draft for minutes. A paragraph larger than
//! the slice budget therefore installs or clears in **one** turn; that single
//! long turn costs no more than the first render of that paragraph pays anyway.
//!
//! The install side of that rule is
//! [`next_install_boundary`](crate::model::file_load::next_install_boundary),
//! which stays in `model/` because `services/editor_io.rs` depends on the same
//! module. The clear side is `clear_slice_char_count` plus
//! `clear_slice_extends_to_paragraph_end`, which together say "take at most
//! one budget of characters, then keep going to the next line start".

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::file_load::SYNCHRONOUS_INSTALL_THRESHOLD_BYTES;
use crate::ui::editor_page::EditorLoadState;

/// Bound deletion work by characters so even four-byte Unicode stays within the
/// byte-oriented installation slice policy.
pub(crate) const CLEAR_SLICE_CHARS: i32 = 64 * 1024;

/// Worst-case bytes one buffer character can occupy in UTF-8.
const MAX_UTF8_BYTES_PER_CHAR: u64 = 4;

/// Which step of one bounded installation a scheduled slice is running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadInstallPhase {
    /// Deleting whatever the buffer held before this load.
    ClearingExisting,
    /// Inserting decoded text at the installation mark.
    Installing,
    /// Deleting the partial content a cancelled installation left behind.
    ClearingCancelled,
    /// Running the final projection; no further slice may start.
    Finalizing,
}

/// Why one bounded installation is being stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbortDisposition {
    /// The user or a newer request cancelled it; the partial buffer is cleared.
    Cancel,
    /// The page is going away; nothing is published and everything is retired.
    Dispose,
}

/// What one scheduled installation slice may do.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallSliceAction {
    /// The session already reached a terminal; the slice does nothing.
    Ignore,
    /// The owning page is gone; retire the payload without user feedback.
    AbortDisposed,
    /// The request is no longer current; clear the partial buffer.
    AbortCancelled,
    /// Run the slice body for this phase.
    RunPhase(LoadInstallPhase),
}

/// What one abort request means for a session in its current phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AbortAction {
    /// Already terminal, already finalizing, or already clearing; do nothing.
    Ignore,
    /// Retire the payload and the admission charge immediately.
    Dispose,
    /// Move to bounded cleanup of the partially installed buffer.
    BeginCancelledClear,
}

/// How the newest load for one editor ended.
///
/// [`RefusedAsStale`](Self::RefusedAsStale) is deliberately distinct from both
/// [`Cancelled`](Self::Cancelled) and [`Failed`](Self::Failed): a completion the
/// workflow refuses to publish is not a user-visible failure and not a user
/// cancellation, and conflating them hides the freshness seam this workflow
/// exists to protect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LoadOutcome {
    /// No load has reached a terminal for this editor yet.
    #[default]
    None,
    /// A load is in flight.
    InFlight,
    /// Decoded content was installed and published.
    Loaded,
    /// The read, decode, or admission failed and the user was told.
    Failed,
    /// The user cancelled the load and the partial content was cleared.
    Cancelled,
    /// A completion arrived for a request the editor no longer owns.
    RefusedAsStale,
}

/// Freshness of one dispatched load request, checked before its result applies.
///
/// The pair `{load_generation, cancel_token}` used to travel as two loose
/// parameters through the planning dispatch, the admission queue, and the worker
/// completion, and to be compared clause by clause at the end. Constructing it
/// once at the workflow entry point and validating it as a unit is what makes a
/// mismatched call a type error rather than an invisible one.
///
/// The ticket carries **dispatch-time expectation**; the editor supplies the
/// live values, which is why the predicate takes the editor rather than a
/// `*Facts` companion.
#[derive(Clone, Debug)]
pub(crate) struct LoadRequestTicket {
    /// The load generation this request was dispatched under.
    pub(crate) load_generation: u64,
    /// The cancellation token this request was dispatched with.
    pub(crate) cancel_token: Arc<AtomicBool>,
}

impl LoadRequestTicket {
    /// Build the ticket one load request is dispatched and validated under.
    pub(crate) fn new(load_generation: u64, cancel_token: Arc<AtomicBool>) -> Self {
        Self {
            load_generation,
            cancel_token,
        }
    }

    /// Whether the user or a newer request has asked this load to stop.
    pub(crate) fn cancel_requested(&self) -> bool {
        self.cancel_token.load(Ordering::Acquire)
    }
}

/// Whether a dispatched request still describes the editor's live load state.
///
/// All three clauses must hold: the generation must still be the one dispatched,
/// the editor must still hold the very token this request carries (a newer load
/// installs a fresh `Arc`, so pointer identity is what stops an older worker
/// from being un-cancelled), and that token must not be set.
pub(crate) fn load_request_is_current(
    ticket: &LoadRequestTicket,
    live_generation: u64,
    live_cancel_token: &Arc<AtomicBool>,
) -> bool {
    ticket.load_generation == live_generation
        && Arc::ptr_eq(&ticket.cancel_token, live_cancel_token)
        && !ticket.cancel_requested()
}

/// Whether a bounded installation's slices may still publish into the buffer.
///
/// Deliberately weaker than [`load_request_is_current`]: an installation is
/// already the newest request's own work, so it re-reads the editor's *current*
/// token rather than comparing token identity. Keeping the two predicates
/// distinct preserves the pre-migration behavior exactly.
pub(crate) const fn installation_is_current(
    live_generation: u64,
    session_generation: u64,
    cancel_requested: bool,
) -> bool {
    live_generation == session_generation && !cancel_requested
}

/// Whether decoded text must be installed in bounded main-loop slices.
///
/// Either side of the swap can be large: installing a big payload in one turn
/// blocks the main loop, and so does deleting a big existing buffer.
pub(crate) fn requires_chunked_install(incoming_bytes: usize, existing_chars: i32) -> bool {
    let existing_bytes = u64::try_from(existing_chars)
        .unwrap_or(u64::MAX)
        .saturating_mul(MAX_UTF8_BYTES_PER_CHAR);
    incoming_bytes > SYNCHRONOUS_INSTALL_THRESHOLD_BYTES
        || existing_bytes > u64::try_from(SYNCHRONOUS_INSTALL_THRESHOLD_BYTES).unwrap_or(u64::MAX)
}

/// How many characters one clear slice may delete.
pub(crate) fn clear_slice_char_count(remaining_chars: i32) -> i32 {
    remaining_chars.min(CLEAR_SLICE_CHARS)
}

/// Whether a clear slice must keep going to the next line start.
///
/// This is the clear-side half of the paragraph-boundary contract in the module
/// documentation. A deletion that stops inside a line would re-lay-out the
/// shrinking remainder on every turn; extending to the next line start deletes
/// each paragraph exactly once.
pub(crate) const fn clear_slice_extends_to_paragraph_end(
    at_buffer_end: bool,
    at_line_start: bool,
) -> bool {
    !at_buffer_end && !at_line_start
}

/// What one scheduled installation slice may do, given what it observed.
pub(crate) const fn install_slice_action(
    terminal: bool,
    editor_alive: bool,
    phase: LoadInstallPhase,
    installation_current: bool,
) -> InstallSliceAction {
    if terminal {
        return InstallSliceAction::Ignore;
    }
    if !editor_alive {
        return InstallSliceAction::AbortDisposed;
    }
    // Bounded cleanup of a cancelled install is itself the response to a stale
    // request, so it must not be re-aborted for being stale.
    if !matches!(phase, LoadInstallPhase::ClearingCancelled) && !installation_current {
        return InstallSliceAction::AbortCancelled;
    }
    InstallSliceAction::RunPhase(phase)
}

/// What one abort request means for a session in its current phase.
pub(crate) const fn abort_action(
    disposition: AbortDisposition,
    phase: LoadInstallPhase,
    terminal: bool,
) -> AbortAction {
    if terminal {
        return AbortAction::Ignore;
    }
    // Final projection owns the main thread and holds no cancellable payload.
    if matches!(phase, LoadInstallPhase::Finalizing) {
        return AbortAction::Ignore;
    }
    if matches!(disposition, AbortDisposition::Cancel)
        && matches!(phase, LoadInstallPhase::ClearingCancelled)
    {
        return AbortAction::Ignore;
    }
    match disposition {
        AbortDisposition::Dispose => AbortAction::Dispose,
        AbortDisposition::Cancel => AbortAction::BeginCancelledClear,
    }
}

/// Which load state a failed read must publish.
///
/// A reload that fails over an already-loaded buffer keeps `Loaded`: the visible
/// text is still the last content that really came off disk, and demoting the
/// tab to `Failed` would claim otherwise.
pub(crate) const fn load_failure_state(previous: EditorLoadState) -> EditorLoadState {
    match previous {
        EditorLoadState::Loaded => EditorLoadState::Loaded,
        _ => EditorLoadState::Failed,
    }
}

/// Whether bounded cleanup must publish the user-cancelled terminal state.
///
/// Only a cancellation that interrupted a load the user could see is worth a
/// notification; disposal and idle cancellation are silent.
pub(crate) const fn publishes_user_cancellation(
    was_loading: bool,
    installation_active: bool,
) -> bool {
    was_loading && installation_active
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    #[test]
    fn ticket_is_current_only_when_generation_token_and_cancellation_all_agree() {
        let cancel = token();
        let ticket = LoadRequestTicket::new(7, Arc::clone(&cancel));

        assert!(load_request_is_current(&ticket, 7, &cancel));
        assert!(!load_request_is_current(&ticket, 8, &cancel));
        assert!(!load_request_is_current(&ticket, 7, &token()));

        cancel.store(true, Ordering::Release);
        assert!(!load_request_is_current(&ticket, 7, &cancel));
    }

    #[test]
    fn ticket_reports_cancellation() {
        let cancel = token();
        let ticket = LoadRequestTicket::new(1, Arc::clone(&cancel));
        assert!(!ticket.cancel_requested());

        cancel.store(true, Ordering::Release);
        assert!(ticket.cancel_requested());
    }

    #[test]
    fn installation_freshness_ignores_token_identity() {
        assert!(installation_is_current(4, 4, false));
        assert!(!installation_is_current(5, 4, false));
        assert!(!installation_is_current(4, 4, true));
    }

    #[test]
    fn chunked_install_triggers_on_either_side_of_the_swap() {
        assert!(!requires_chunked_install(1, 1));
        assert!(requires_chunked_install(
            SYNCHRONOUS_INSTALL_THRESHOLD_BYTES + 1,
            0
        ));
        assert!(!requires_chunked_install(
            SYNCHRONOUS_INSTALL_THRESHOLD_BYTES,
            0
        ));

        let threshold_chars =
            i32::try_from(SYNCHRONOUS_INSTALL_THRESHOLD_BYTES / 4).expect("threshold fits i32");
        assert!(!requires_chunked_install(0, threshold_chars));
        assert!(requires_chunked_install(0, threshold_chars + 1));
        // A negative character count cannot be trusted into the multiply.
        assert!(requires_chunked_install(0, -1));
    }

    #[test]
    fn the_clear_slice_budget_matches_the_shared_replacement_budget() {
        // Pinned against the shared constant rather than restated: the load
        // workflow's clear budget is deliberately the same 64 KiB the bounded
        // buffer-replacement workflow uses, so a slice of either kind costs the
        // same one paragraph-aligned pass. Asserting it relatively (as the
        // bounding test below does) cannot catch the value drifting.
        assert_eq!(
            CLEAR_SLICE_CHARS,
            crate::model::buffer_replacement::REPLACEMENT_CLEAR_SLICE_CHARS
        );
    }

    #[test]
    fn clear_slices_are_bounded_and_stop_on_paragraph_boundaries() {
        assert_eq!(clear_slice_char_count(10), 10);
        assert_eq!(clear_slice_char_count(CLEAR_SLICE_CHARS), CLEAR_SLICE_CHARS);
        assert_eq!(
            clear_slice_char_count(CLEAR_SLICE_CHARS + 1),
            CLEAR_SLICE_CHARS
        );

        assert!(clear_slice_extends_to_paragraph_end(false, false));
        assert!(!clear_slice_extends_to_paragraph_end(true, false));
        assert!(!clear_slice_extends_to_paragraph_end(false, true));
        assert!(!clear_slice_extends_to_paragraph_end(true, true));
    }

    #[test]
    fn slice_action_orders_terminal_disposal_and_staleness() {
        assert_eq!(
            install_slice_action(true, true, LoadInstallPhase::Installing, true),
            InstallSliceAction::Ignore
        );
        assert_eq!(
            install_slice_action(false, false, LoadInstallPhase::Installing, true),
            InstallSliceAction::AbortDisposed
        );
        assert_eq!(
            install_slice_action(false, true, LoadInstallPhase::Installing, false),
            InstallSliceAction::AbortCancelled
        );
        assert_eq!(
            install_slice_action(false, true, LoadInstallPhase::ClearingCancelled, false),
            InstallSliceAction::RunPhase(LoadInstallPhase::ClearingCancelled)
        );
        assert_eq!(
            install_slice_action(false, true, LoadInstallPhase::ClearingExisting, true),
            InstallSliceAction::RunPhase(LoadInstallPhase::ClearingExisting)
        );
    }

    #[test]
    fn abort_action_protects_terminal_finalizing_and_repeated_cancellation() {
        assert_eq!(
            abort_action(
                AbortDisposition::Dispose,
                LoadInstallPhase::Installing,
                true
            ),
            AbortAction::Ignore
        );
        assert_eq!(
            abort_action(
                AbortDisposition::Dispose,
                LoadInstallPhase::Finalizing,
                false
            ),
            AbortAction::Ignore
        );
        assert_eq!(
            abort_action(
                AbortDisposition::Cancel,
                LoadInstallPhase::ClearingCancelled,
                false
            ),
            AbortAction::Ignore
        );
        // Disposal still retires a session that is already clearing.
        assert_eq!(
            abort_action(
                AbortDisposition::Dispose,
                LoadInstallPhase::ClearingCancelled,
                false
            ),
            AbortAction::Dispose
        );
        assert_eq!(
            abort_action(
                AbortDisposition::Cancel,
                LoadInstallPhase::Installing,
                false
            ),
            AbortAction::BeginCancelledClear
        );
        assert_eq!(
            abort_action(
                AbortDisposition::Dispose,
                LoadInstallPhase::ClearingExisting,
                false
            ),
            AbortAction::Dispose
        );
    }

    #[test]
    fn a_failed_reload_over_loaded_content_keeps_the_loaded_state() {
        assert_eq!(
            load_failure_state(EditorLoadState::Loaded),
            EditorLoadState::Loaded
        );
        assert_eq!(
            load_failure_state(EditorLoadState::Loading),
            EditorLoadState::Failed
        );
        assert_eq!(
            load_failure_state(EditorLoadState::Untitled),
            EditorLoadState::Failed
        );
        assert_eq!(
            load_failure_state(EditorLoadState::Failed),
            EditorLoadState::Failed
        );
    }

    #[test]
    fn user_cancellation_is_published_only_for_a_visible_interrupted_load() {
        assert!(publishes_user_cancellation(true, true));
        assert!(!publishes_user_cancellation(true, false));
        assert!(!publishes_user_cancellation(false, true));
        assert!(!publishes_user_cancellation(false, false));
    }

    #[test]
    fn load_outcome_defaults_to_none() {
        assert_eq!(LoadOutcome::default(), LoadOutcome::None);
    }
}
