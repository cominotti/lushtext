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
//! recovery of a 33 MB single-line draft for minutes. A **visible** paragraph
//! larger than the slice budget therefore installs or clears in **one** turn;
//! that single long turn costs no more than the first render of that paragraph
//! pays anyway.
//!
//! **The one exception: a hidden tail** (`sliced-protected-install`, C3). A
//! protected body (`protected_install_is_sliced`) is installed through
//! `ui::protected_fold::FoldedInstall`, whose slices may end inside a folded
//! paragraph, but only inside its hidden tail, which GTK never lays out
//! (ledger A24, A65, A77); the head goes in once, after the tail. The clear
//! side deletes the tail ranges the install recorded, from each paragraph's start,
//! before the paragraph-aligned clear below runs (`delete_buffer_slice`).
//!
//! The install side of that rule is
//! [`next_install_boundary`](crate::model::file_load::next_install_boundary),
//! which stays in `model/` because `services/editor_io.rs` depends on the same
//! module. The clear side is `clear_slice_char_count` plus
//! `clear_slice_extends_to_paragraph_end`, which together say "take at most
//! one budget of characters, then keep going to the next line start".

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(kani)]
mod kani_proofs;

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
/// blocks the main loop, and so does deleting a big existing buffer. A dense
/// payload ([`batch_exceeds_slice`]) is sliced at any size.
pub(crate) fn requires_chunked_install(
    incoming_bytes: usize,
    existing_chars: i32,
    incoming_dense: bool,
) -> bool {
    let existing_bytes = u64::try_from(existing_chars)
        .unwrap_or(u64::MAX)
        .saturating_mul(MAX_UTF8_BYTES_PER_CHAR);
    // A dense document is paced by visible characters at any size, so each
    // slice's validation stays bounded (`bound-post-open-settle`, D6.1).
    incoming_dense
        || incoming_bytes > SYNCHRONOUS_INSTALL_THRESHOLD_BYTES
        || existing_bytes > u64::try_from(SYNCHRONOUS_INSTALL_THRESHOLD_BYTES).unwrap_or(u64::MAX)
}

/// Whether a body's fold plan makes its install sliced through the protected
/// executor: a hidden tail that could exceed one tail slice
/// (`model::protected_install::hidden_tail_requires_slicing`). Such a body is
/// installed in bounded turns at any size.
pub(crate) fn protected_install_is_sliced(
    plan: Option<&crate::model::protected_fold::FoldPlan>,
) -> bool {
    plan.is_some_and(|plan| {
        crate::model::protected_install::hidden_tail_requires_slicing(plan.hidden_chars())
    })
}

/// How the next install or replacement slice is queued.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallSlicePacing {
    /// From an idle **below** GtkTextView's validation (`PACED_SLICE_PRIORITY`):
    /// the view lays out each slice before the next goes in (ledger A70).
    BehindValidation,
    /// From an idle one step **below** that (`background_slice_priority`):
    /// nothing lays this body out, and it waits for the visible tab's paced
    /// slices and validation to finish first.
    BehindVisibleWork,
    /// From a 1 ms timeout: an ordinary body.
    Timeout,
}

/// How the next slice is queued: a dense body's slices
/// (`bound-post-open-settle`, D6) and a protected body's (C3), whose tail
/// slices then follow each other at main-loop speed instead of one timeout
/// apart, are paced; an ordinary body's follow a timeout.
///
/// A paced body going into a **background** tab — one whose view shows an
/// empty placeholder (`defer-background-tab-layout`, ledger A83) and which is
/// not its window's selected tab (`laid_out` is false) — waits one priority
/// step lower still, so the visible tab's install and layout take the CPU
/// first (above validation, a long background install starved the visible
/// view's layout; measured, design D4). Each slice is still bounded by its
/// budget.
pub(crate) const fn install_slice_pacing(
    dense: bool,
    laid_out: bool,
    protected: bool,
) -> InstallSlicePacing {
    match (dense || protected, laid_out) {
        (true, true) => InstallSlicePacing::BehindValidation,
        (true, false) => InstallSlicePacing::BehindVisibleWork,
        (false, _) => InstallSlicePacing::Timeout,
    }
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

// ─── Post-open settle (`bound-post-open-settle`, design D1, D2, D9) ─────

/// How long highlighting and validation must stay quiet before an opened
/// document counts as settled (design D1): longer than one 2000 px batch
/// interval and than the gap between context-engine slices (ledger A69: the
/// background worker reschedules through an idle, so there is no timer gap),
/// shorter than the debounce windows. The only time-based settle clause; it
/// says "quiet", not "complete", because neither GtkSourceView nor
/// GtkTextView exposes a completion signal.
///
/// Kind: wall-clock interval, µs. Rounding: 100 ms, not a power of two;
/// deferred to `power-of-two-thresholds`.
pub const SETTLE_QUIET_US: i64 = 100_000;

/// How long the window must have seen no key press, button press, scroll
/// start or touch begin before a deferred language attach may run (design
/// D2's fallback): two reference frames.
///
/// Kind: wall-clock interval, µs, derived (2 × `REFERENCE_FRAME_US`, 16,666);
/// no rounding of its own.
pub const INPUT_QUIET_US: i64 = 2 * crate::model::responsiveness_budget::REFERENCE_FRAME_US as i64;

/// The ceiling on that wait, from the editor's first frame: after it the
/// attach runs at the next low-priority dispatch whatever the input (review R6).
///
/// Kind: wall-clock ceiling, µs. Value: 2,048 ms, a power of two in
/// milliseconds (rounded up from the 2 s first chosen).
pub const ATTACH_DEADLINE_US: i64 = 2_048_000;

/// The facts the settle predicate reads, all live state of one editor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SettleFacts {
    /// The editor's load published (or failed) and the open completed.
    pub open_complete: bool,
    /// The language attach stage ran (or no attach was requested).
    pub language_attached: bool,
    /// A load slice, publish, attach or post-open debounce is still pending.
    pub post_open_work_pending: bool,
    /// When the last highlight-updated or adjustment-`upper` change was seen,
    /// monotonic µs, or `None`.
    pub last_mark_us: Option<i64>,
    /// When the settle sentinel last dispatched, monotonic µs, or `None`.
    pub last_sentinel_us: Option<i64>,
}

/// Which stage an opened document is in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettleStage {
    /// The load is still installing or publishing.
    Opening,
    /// Installed; the language attach stage has not run yet.
    AttachPending,
    /// Highlighting or validation work is still arriving.
    Quieting,
    /// Every clause holds.
    Settled,
}

impl SettleStage {
    /// The stable lower-case name automation reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::AttachPending => "attach-pending",
            Self::Quieting => "quieting",
            Self::Settled => "settled",
        }
    }
}

/// The settle stage at `now_us` (design D1): open complete, attach run, no
/// post-open work pending, `SETTLE_QUIET_US` since the last mark, and a
/// sentinel dispatched after it.
#[must_use]
pub const fn settle_stage(facts: SettleFacts, now_us: i64) -> SettleStage {
    if !facts.open_complete || facts.post_open_work_pending {
        return SettleStage::Opening;
    }
    if !facts.language_attached {
        return SettleStage::AttachPending;
    }
    let quiet = match facts.last_mark_us {
        None => true,
        Some(mark) => now_us.saturating_sub(mark) >= SETTLE_QUIET_US,
    };
    let sentinel_after_mark = match (facts.last_sentinel_us, facts.last_mark_us) {
        (Some(sentinel), Some(mark)) => sentinel >= mark,
        (Some(_), None) => true,
        (None, _) => false,
    };
    if quiet && sentinel_after_mark {
        SettleStage::Settled
    } else {
        SettleStage::Quieting
    }
}

/// Whether a deferred language attach may run now (design D2 fallback):
/// after the editor's first frame, and either input-quiet or past the
/// deadline.
#[must_use]
pub const fn deferred_attach_may_run(
    now_us: i64,
    first_frame_us: Option<i64>,
    last_input_us: Option<i64>,
) -> bool {
    let Some(first_frame) = first_frame_us else {
        return false;
    };
    if now_us.saturating_sub(first_frame) >= ATTACH_DEADLINE_US {
        return true;
    }
    match last_input_us {
        None => true,
        Some(input) => now_us.saturating_sub(input) >= INPUT_QUIET_US,
    }
}

// ─── Highlight attach (`bound-post-open-settle`, D3) ─────────────────────

/// In which order an install meets the highlighting engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightAttachPlan {
    /// Highlighting stays on through the install: inserts into a buffer with
    /// an attached language only invalidate (ledger A68), and the document is
    /// within its language's total-size threshold, so the engine's analysis of
    /// it is bounded. Publishing then toggles nothing.
    KeepAttached,
    /// Highlighting goes off for the install and comes back at publish: the
    /// scale keeps syntax off for this document, and turning highlighting on
    /// over a whole buffer costs O(document) in one dispatch (A68), so the
    /// install must not leave a language attached to a body it may not
    /// highlight.
    OffUntilPublish,
}

/// The highlight order for an incoming body the scale does or does not let
/// highlight (design D3).
#[must_use]
pub const fn highlight_attach_plan(syntax_allowed: bool) -> HighlightAttachPlan {
    if syntax_allowed {
        HighlightAttachPlan::KeepAttached
    } else {
        HighlightAttachPlan::OffUntilPublish
    }
}

// ─── Paced restyle of dense documents (`bound-post-open-settle`, D9) ─────

pub use crate::model::buffer_replacement::batch_exceeds_slice;

/// A half-open run of buffer paragraphs (GTK lines), `first..end`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParagraphSpan {
    /// The first paragraph.
    pub first: u32,
    /// One past the last paragraph.
    pub end: u32,
}

impl ParagraphSpan {
    /// How many paragraphs the span holds.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.end.saturating_sub(self.first)
    }

    /// Whether the span holds no paragraph.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.end <= self.first
    }
}

/// Which side of the visible paragraphs the next outward range comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RestyleSide {
    Below,
    Above,
}

/// Where a paced restyle is: everything before `above` and from `below` on
/// is still to do, and the visible paragraphs come first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestyleCursor {
    /// Paragraphs `0..above` are still to restyle.
    above: u32,
    /// Paragraphs `below..count` are still to restyle.
    below: u32,
    /// The end of the visible paragraphs, clamped to `count`.
    visible_end: u32,
    count: u32,
    next: RestyleSide,
}

impl RestyleCursor {
    /// A restyle of a `count`-paragraph document whose visible paragraphs
    /// are `visible` (clamped to the document).
    #[must_use]
    pub const fn new(visible: ParagraphSpan, count: u32) -> Self {
        let first = if visible.first < count {
            visible.first
        } else {
            count
        };
        let end = if visible.end < first {
            first
        } else if visible.end > count {
            count
        } else {
            visible.end
        };
        Self {
            above: first,
            below: first,
            visible_end: end,
            count,
            next: RestyleSide::Below,
        }
    }

    /// Whether every paragraph has been handed out.
    #[must_use]
    pub const fn is_done(self) -> bool {
        self.above == 0 && self.below >= self.count
    }
}

/// The next paragraph-aligned range of a paced restyle, and the cursor after
/// it; `None` once every paragraph has been handed out (design D9, review R21).
///
/// The visible paragraphs go first, forwards, in ranges of at most `v_slice`
/// visible characters; then ranges alternate below and above them, outwards,
/// one side continuing alone once the other is exhausted. A range takes whole
/// paragraphs while their sum stays within `v_slice`, and always takes at
/// least one, so a single paragraph over the budget goes alone (C2's
/// exception). `paragraph_chars(i)` is paragraph `i`'s visible characters.
/// Allocation-free; Kani checks it over a six-paragraph model
/// (`policy/kani_proofs.rs`).
pub fn next_restyle_range(
    cursor: RestyleCursor,
    v_slice: u64,
    paragraph_chars: impl Fn(u32) -> u64,
) -> Option<(RestyleCursor, ParagraphSpan)> {
    if cursor.is_done() {
        return None;
    }
    let mut next = cursor;
    let in_visible = cursor.below < cursor.visible_end;
    let side = if in_visible || cursor.above == 0 {
        RestyleSide::Below
    } else if cursor.below >= cursor.count {
        RestyleSide::Above
    } else {
        cursor.next
    };
    let span = match side {
        RestyleSide::Below => {
            let limit = if in_visible {
                cursor.visible_end
            } else {
                cursor.count
            };
            let first = cursor.below;
            let mut end = first;
            let mut chars = 0u64;
            while end < limit {
                let total = chars.saturating_add(paragraph_chars(end));
                if end > first && total > v_slice {
                    break;
                }
                chars = total;
                end += 1;
            }
            next.below = end;
            ParagraphSpan { first, end }
        }
        RestyleSide::Above => {
            let end = cursor.above;
            let mut first = end;
            let mut chars = 0u64;
            while first > 0 {
                let total = chars.saturating_add(paragraph_chars(first - 1));
                if first < end && total > v_slice {
                    break;
                }
                chars = total;
                first -= 1;
            }
            next.above = first;
            ParagraphSpan { first, end }
        }
    };
    if !in_visible {
        next.next = match side {
            RestyleSide::Below => RestyleSide::Above,
            RestyleSide::Above => RestyleSide::Below,
        };
    }
    Some((next, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_background_paced_body_waits_behind_the_visible_tabs_work() {
        use InstallSlicePacing::{BehindValidation, BehindVisibleWork, Timeout};
        for protected in [false, true] {
            for dense in [false, true] {
                for laid_out in [false, true] {
                    let expected = match (dense || protected, laid_out) {
                        (false, _) => Timeout,
                        (true, true) => BehindValidation,
                        (true, false) => BehindVisibleWork,
                    };
                    assert_eq!(install_slice_pacing(dense, laid_out, protected), expected);
                }
            }
        }
        assert_eq!(install_slice_pacing(true, false, false), BehindVisibleWork);
        assert_eq!(install_slice_pacing(false, false, false), Timeout);
    }

    fn ranges(visible: ParagraphSpan, lengths: &[u64], v_slice: u64) -> Vec<(u32, u32)> {
        let count = u32::try_from(lengths.len()).expect("fits");
        let mut cursor = RestyleCursor::new(visible, count);
        let mut out = Vec::new();
        while let Some((next, range)) = next_restyle_range(cursor, v_slice, |i| lengths[i as usize])
        {
            out.push((range.first, range.end));
            cursor = next;
        }
        assert!(cursor.is_done());
        out
    }

    #[test]
    fn a_restyle_starts_with_the_visible_paragraphs_and_alternates_outwards() {
        let visible = ParagraphSpan { first: 3, end: 5 };
        assert_eq!(
            ranges(visible, &[10; 8], 10),
            vec![
                (3, 4),
                (4, 5),
                (5, 6),
                (2, 3),
                (6, 7),
                (1, 2),
                (7, 8),
                (0, 1)
            ]
        );
        // Ranges grow to the budget, and the exhausted side stops alternating.
        assert_eq!(
            ranges(visible, &[10; 8], 25),
            vec![(3, 5), (5, 7), (1, 3), (7, 8), (0, 1)]
        );
        // A paragraph over the budget goes alone.
        assert_eq!(
            ranges(ParagraphSpan { first: 0, end: 1 }, &[1, 99, 1, 1], 10),
            vec![(0, 1), (1, 2), (2, 4)]
        );
        assert!(ranges(ParagraphSpan::default(), &[], 10).is_empty());
        // A visible span past the end is clamped.
        assert_eq!(
            ranges(ParagraphSpan { first: 9, end: 12 }, &[1, 1], 1),
            vec![(1, 2), (0, 1)]
        );
    }

    #[test]
    fn settle_needs_open_attach_quiet_and_a_later_sentinel() {
        let settled = SettleFacts {
            open_complete: true,
            language_attached: true,
            post_open_work_pending: false,
            last_mark_us: Some(1_000),
            last_sentinel_us: Some(2_000),
        };
        let quiet = 1_000 + SETTLE_QUIET_US;
        assert_eq!(settle_stage(settled, quiet), SettleStage::Settled);
        assert_eq!(settle_stage(settled, quiet - 1), SettleStage::Quieting);
        let early_sentinel = SettleFacts {
            last_sentinel_us: Some(500),
            ..settled
        };
        assert_eq!(settle_stage(early_sentinel, quiet), SettleStage::Quieting);
        let no_sentinel = SettleFacts {
            last_sentinel_us: None,
            ..settled
        };
        assert_eq!(settle_stage(no_sentinel, quiet), SettleStage::Quieting);
        let unattached = SettleFacts {
            language_attached: false,
            ..settled
        };
        assert_eq!(settle_stage(unattached, quiet), SettleStage::AttachPending);
        let pending = SettleFacts {
            post_open_work_pending: true,
            ..settled
        };
        assert_eq!(settle_stage(pending, quiet), SettleStage::Opening);
        let no_mark = SettleFacts {
            last_mark_us: None,
            ..settled
        };
        assert_eq!(settle_stage(no_mark, 0), SettleStage::Settled);
        assert_eq!(SettleStage::AttachPending.as_str(), "attach-pending");
    }

    #[test]
    fn a_deferred_attach_waits_for_its_frame_and_input_quiet_until_the_deadline() {
        assert!(!deferred_attach_may_run(10_000_000, None, None));
        assert!(deferred_attach_may_run(1_000, Some(0), None));
        assert!(!deferred_attach_may_run(1_000, Some(0), Some(900)));
        assert!(deferred_attach_may_run(
            900 + INPUT_QUIET_US,
            Some(0),
            Some(900)
        ));
        assert!(deferred_attach_may_run(
            ATTACH_DEADLINE_US,
            Some(0),
            Some(ATTACH_DEADLINE_US)
        ));
        assert!(!deferred_attach_may_run(
            ATTACH_DEADLINE_US - 1,
            Some(0),
            Some(ATTACH_DEADLINE_US - 1)
        ));
    }

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
        assert!(!requires_chunked_install(1, 1, false));
        assert!(requires_chunked_install(
            SYNCHRONOUS_INSTALL_THRESHOLD_BYTES + 1,
            0,
            false
        ));
        assert!(!requires_chunked_install(
            SYNCHRONOUS_INSTALL_THRESHOLD_BYTES,
            0,
            false
        ));

        let threshold_chars =
            i32::try_from(SYNCHRONOUS_INSTALL_THRESHOLD_BYTES / 4).expect("threshold fits i32");
        assert!(!requires_chunked_install(0, threshold_chars, false));
        assert!(requires_chunked_install(0, threshold_chars + 1, false));
        // A negative character count cannot be trusted into the multiply.
        assert!(requires_chunked_install(0, -1, false));
    }

    #[test]
    fn highlighting_stays_attached_only_for_a_body_the_scale_lets_highlight() {
        assert_eq!(
            highlight_attach_plan(true),
            HighlightAttachPlan::KeepAttached
        );
        assert_eq!(
            highlight_attach_plan(false),
            HighlightAttachPlan::OffUntilPublish
        );
    }

    #[test]
    fn a_dense_payload_is_chunked_at_any_size() {
        // Review R13: density, not size, decides; a small dense document is
        // sliced, a small short-line one stays direct.
        assert!(requires_chunked_install(1, 0, true));
        assert!(!requires_chunked_install(1, 0, false));
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
