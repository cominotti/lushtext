// SPDX-License-Identifier: GPL-3.0-or-later

//! Coordination — **admission**, for the bounded whole-buffer replacement workflow.
//!
//! Owns the editor's active/parked replacement slot, the request/body pairing
//! (`BufferReplacementRequest` / `ReplacementBody`), and the entry and disposal
//! boundaries (`accept_request`, `dispose`) that hand off to [`super::execution`].

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use gtk4::subclass::prelude::ObjectSubclassIsExt;
use lushtext_model::document::paragraph_boundary::DocumentProfile;

use crate::ui::editor_page::LushtextEditorPage;

use super::evidence::BufferReplacementTerminalDiagnostic;
use super::execution::{self, BufferReplacementSession};
use super::policy::{
    BufferReplacementCancelReason, BufferReplacementMetrics, BufferReplacementOutcome,
    BufferReplacementTicket, StartDisposition, start_disposition,
};

pub(super) type FreshnessCheck = Box<dyn Fn(&LushtextEditorPage) -> bool>;
pub(super) type TerminalCallback = Box<dyn FnOnce(BufferReplacementOutcome)>;
/// Callback that receives a guarded (disposal-owned) source body back.
type GuardedBodyCallback = Box<dyn FnOnce(crate::ui::plain_disposal::DisposalOwned<String>)>;
/// Callback that receives a plain source body back (test-only cancel probe).
#[cfg(feature = "test-utils")]
type PlainBodyCallback = Box<dyn FnOnce(String)>;

/// The uninstalled source body paired with the workflow callbacks matched to
/// its kind.
///
/// Keeping each callback *inside* the body variant it belongs to makes a
/// guarded cancellation callback structurally impossible to attach to a plain
/// body (and a plain callback impossible to attach to a guarded body): the only
/// constructors that accept a guarded callback also demand a guarded body. As a
/// result terminal teardown matches every legal pairing exhaustively with no
/// runtime panic arm for an unrepresentable mismatch.
pub(super) enum ReplacementBody {
    Plain {
        body: String,
        /// Test-only probe returning the uninstalled plain body on cancel.
        #[cfg(feature = "test-utils")]
        on_cancel: Option<PlainBodyCallback>,
    },
    Guarded {
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        /// Returns the uninstalled guarded body on cancel/supersede, keeping its
        /// disposal reservation intact.
        on_cancel: Option<GuardedBodyCallback>,
        /// Preserves the installed guarded source for an accepted workflow cache.
        on_complete: Option<GuardedBodyCallback>,
    },
}

impl ReplacementBody {
    /// At a completed install into a protected editor, a guarded body nobody
    /// else keeps becomes the Protected View text source (design D4); a body
    /// with an `on_complete` consumer goes there, which decides (draft
    /// recovery retains it through its local-history seeding).
    pub(super) fn retain_for_protected_view(self, editor: &LushtextEditorPage) -> Self {
        match self {
            Self::Guarded {
                body,
                on_cancel,
                on_complete: None,
            } if editor.is_protected_view() => match editor.retain_protected_source(body) {
                None => Self::default(),
                Some(body) => Self::Guarded {
                    body,
                    on_cancel,
                    on_complete: None,
                },
            },
            other => other,
        }
    }

    /// Return this uninstalled body to its workflow owner on cancel/supersede.
    ///
    /// A guarded completion callback is intentionally dropped uncalled here; a
    /// guarded body with no cancel callback disposes itself through
    /// `DisposalOwned`.
    pub(super) fn return_on_cancel(self) {
        match self {
            #[cfg(feature = "test-utils")]
            Self::Plain { body, on_cancel } => {
                if let Some(callback) = on_cancel {
                    callback(body);
                }
            }
            #[cfg(not(feature = "test-utils"))]
            Self::Plain { body } => drop(body),
            Self::Guarded {
                body,
                on_cancel,
                on_complete: _,
            } => match on_cancel {
                Some(callback) => callback(body),
                None => drop(body),
            },
        }
    }

    /// Finalize an accepted replacement: route a guarded body to its completion
    /// callback and yield the retained plain body for terminal evidence.
    pub(super) fn into_completed_body(self) -> String {
        match self {
            #[cfg(feature = "test-utils")]
            Self::Plain { body, on_cancel: _ } => body,
            #[cfg(not(feature = "test-utils"))]
            Self::Plain { body } => body,
            Self::Guarded {
                body,
                on_cancel: _,
                on_complete,
            } => {
                match on_complete {
                    Some(callback) => callback(body),
                    None => drop(body),
                }
                String::new()
            }
        }
    }
}

impl Default for ReplacementBody {
    fn default() -> Self {
        Self::Plain {
            body: String::new(),
            #[cfg(feature = "test-utils")]
            on_cancel: None,
        }
    }
}

impl Deref for ReplacementBody {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Plain { body, .. } => body,
            Self::Guarded { body, .. } => body,
        }
    }
}

/// One complete replacement request; pending ownership never contains GTK objects.
///
/// The body kind and its cancellation/completion callbacks are paired by
/// construction: every constructor that accepts a guarded callback also demands
/// a guarded body, so a guarded callback cannot reach a plain body (or the
/// reverse). There is no builder that could later cross those kinds.
pub struct BufferReplacementRequest {
    pub(super) ticket: BufferReplacementTicket,
    pub(super) body: ReplacementBody,
    pub(super) is_current: FreshnessCheck,
    pub(super) callback: TerminalCallback,
    /// The body's paragraph profile, computed by the worker that prepared it;
    /// joined into the editor's document scale at a `Complete` terminal.
    pub(super) profile: Option<DocumentProfile>,
}

impl BufferReplacementRequest {
    /// Plain-body replacement with no body-return callback (e.g. eviction).
    pub fn new(
        ticket: BufferReplacementTicket,
        body: String,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Plain {
                body,
                #[cfg(feature = "test-utils")]
                on_cancel: None,
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
            profile: None,
        }
    }

    /// Plain-body replacement that returns the uninstalled body when cancelled.
    #[cfg(feature = "test-utils")]
    pub fn new_returning_body_on_cancel(
        ticket: BufferReplacementTicket,
        body: String,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_cancel: impl FnOnce(String) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Plain {
                body,
                on_cancel: Some(Box::new(on_cancel)),
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
            profile: None,
        }
    }

    /// Guarded-body replacement that returns the uninstalled guarded body on
    /// cancel without releasing its disposal reservation.
    pub(crate) fn new_guarded_returning_body_on_cancel(
        ticket: BufferReplacementTicket,
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_cancel: impl FnOnce(crate::ui::plain_disposal::DisposalOwned<String>) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Guarded {
                body,
                on_cancel: Some(Box::new(on_cancel)),
                on_complete: None,
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
            profile: None,
        }
    }

    /// Guarded-body replacement that preserves the installed guarded source for
    /// an accepted workflow cache when the replacement completes.
    pub(crate) fn new_guarded_returning_body_on_complete(
        ticket: BufferReplacementTicket,
        body: crate::ui::plain_disposal::DisposalOwned<String>,
        is_current: impl Fn(&LushtextEditorPage) -> bool + 'static,
        callback: impl FnOnce(BufferReplacementOutcome) + 'static,
        on_complete: impl FnOnce(crate::ui::plain_disposal::DisposalOwned<String>) + 'static,
    ) -> Self {
        Self {
            ticket,
            body: ReplacementBody::Guarded {
                body,
                on_cancel: None,
                on_complete: Some(Box::new(on_complete)),
            },
            is_current: Box::new(is_current),
            callback: Box::new(callback),
            profile: None,
        }
    }

    /// Attach the body's worker-computed paragraph profile: before the first
    /// byte enters the buffer the editor gates what the body may not meet
    /// (the context engine, a `GtkSourceMap`), and a completed replacement
    /// joins the profile into the document scale.
    #[must_use]
    pub(crate) fn with_profile(mut self, profile: DocumentProfile) -> Self {
        self.profile = Some(profile);
        self
    }
}

/// Active/parked replacement ownership for one editor tab.
#[derive(Default)]
pub(super) enum ReplacementSlot {
    #[default]
    Idle,
    Running {
        active: Rc<RefCell<BufferReplacementSession>>,
        pending: Option<Box<BufferReplacementRequest>>,
    },
}

/// Editor-owned active/latest state for all whole-buffer replacement workflows.
#[derive(Default)]
pub struct BufferReplacementState {
    pub(super) slot: RefCell<ReplacementSlot>,
    pub projection_suspended: std::cell::Cell<bool>,
    /// Whether the newest session was cancelled after it started mutating, so
    /// its bounded clear emptied the buffer and nothing published a whole
    /// document since: the buffer holds neither the old text nor the new.
    /// Cleared by a completed replacement or a published load. While it holds
    /// and the user has not typed since, no save may write the buffer.
    pub left_partial: std::cell::Cell<bool>,
    pub(super) slice_count: std::cell::Cell<u64>,
    /// The newest session's sliced protected install counts (C3).
    pub(super) protected_install: std::cell::Cell<crate::ui::protected_fold::FoldedInstallMetrics>,
    /// Ledger clear turns the newest session ran (C3).
    pub(super) protected_clear_turns: std::cell::Cell<u64>,
    #[cfg(feature = "test-utils")]
    pub(super) stale_after_slices: std::cell::Cell<Option<u64>>,
    /// Last terminal reached, for the evidence surface to report.
    ///
    /// Deliberately **not** `#[cfg(feature = "test-utils")]`, unlike its
    /// `stale_after_slices` sibling above. That one is a test-only actuation
    /// seam and must not compile into a release build; this one is read by
    /// `evidence::BufferReplacementEvidence`, which is a production observation
    /// surface that widget tests happen to be the main consumer of. Gating it
    /// would make the evidence surface's shape depend on the feature set.
    pub(super) last_terminal: RefCell<Option<BufferReplacementTerminalDiagnostic>>,
}

impl BufferReplacementState {
    pub(super) fn active_session(&self) -> Option<Rc<RefCell<BufferReplacementSession>>> {
        match &*self.slot.borrow() {
            ReplacementSlot::Idle => None,
            ReplacementSlot::Running { active, .. } => Some(Rc::clone(active)),
        }
    }

    pub(super) fn has_active(&self) -> bool {
        matches!(&*self.slot.borrow(), ReplacementSlot::Running { .. })
    }

    pub(super) fn has_pending(&self) -> bool {
        matches!(
            &*self.slot.borrow(),
            ReplacementSlot::Running {
                pending: Some(_),
                ..
            }
        )
    }
}

/// Take ownership of the editor for `request`, superseding any live session.
pub(super) fn accept_request(editor: &LushtextEditorPage, request: BufferReplacementRequest) {
    let Some(active) = editor.imp().replacement.active_session() else {
        execution::begin(editor, request);
        return;
    };

    execution::cancel_session(&active, BufferReplacementCancelReason::Superseded, false);
    // Read ownership into a local **before** the match: a `match` scrutinee's
    // temporaries live for the whole match, so borrowing inline would hold a
    // shared `Ref` on `slot` while the `Immediately` arm's `begin` takes
    // `borrow_mut()` on the same cell.
    let still_owned = editor.imp().replacement.has_active();
    match start_disposition(still_owned) {
        StartDisposition::Immediately => execution::begin(editor, request),
        StartDisposition::ParkAsPending => {
            let replaced = {
                let mut slot = editor.imp().replacement.slot.borrow_mut();
                match &mut *slot {
                    ReplacementSlot::Running { pending, .. } => pending.replace(Box::new(request)),
                    ReplacementSlot::Idle => None,
                }
            };
            if let Some(replaced) = replaced {
                // Only the newest intent survives, and the request it displaces
                // gets its terminal now rather than waiting for one that will
                // never come.
                replaced.body.return_on_cancel();
                (replaced.callback)(BufferReplacementOutcome::Cancelled {
                    ticket: replaced.ticket,
                    reason: BufferReplacementCancelReason::Superseded,
                    metrics: BufferReplacementMetrics::default(),
                });
            }
        }
    }
}

/// Release every request this editor owns without publishing widget state.
pub(super) fn dispose(editor: &LushtextEditorPage) {
    let (pending, active) = {
        let mut slot = editor.imp().replacement.slot.borrow_mut();
        match &mut *slot {
            ReplacementSlot::Idle => (None, None),
            ReplacementSlot::Running { active, pending } => {
                (pending.take(), Some(Rc::clone(active)))
            }
        }
    };
    if let Some(pending) = pending {
        pending.body.return_on_cancel();
        (pending.callback)(BufferReplacementOutcome::Cancelled {
            ticket: pending.ticket,
            reason: BufferReplacementCancelReason::Disposed,
            metrics: BufferReplacementMetrics::default(),
        });
    }
    if let Some(active) = active {
        execution::cancel_session(&active, BufferReplacementCancelReason::Disposed, true);
    }
}

#[cfg(test)]
mod pairing_tests {
    //! Correct-by-construction proof for body-kind / callback-kind pairing.
    //!
    //! The illegal pairings — a guarded cancellation/completion callback with a
    //! plain body, or a plain callback with a guarded body — are
    //! *unconstructible*: every constructor that accepts a guarded callback
    //! (`new_guarded_returning_body_on_{cancel,complete}`) also requires a
    //! `DisposalOwned<String>` body, and the only plain constructors (`new`,
    //! `new_returning_body_on_cancel`) accept no guarded callback. No
    //! `BufferReplacementRequest` value can therefore pair a guarded callback
    //! with a plain body, which is why `ReplacementBody::return_on_cancel` and
    //! `ReplacementBody::into_completed_body` match every legal pairing
    //! exhaustively with no runtime panic arm. These tests exercise the plain
    //! routing (guarded routing needs the GTK disposal lane and is covered by
    //! the replacement widget tests).

    use super::*;

    #[cfg(feature = "test-utils")]
    #[test]
    fn plain_body_returns_through_its_matched_cancel_callback() {
        let returned = Rc::new(RefCell::new(None));
        let sink = Rc::clone(&returned);
        let body = ReplacementBody::Plain {
            body: "abc".to_string(),
            on_cancel: Some(Box::new(move |body| *sink.borrow_mut() = Some(body))),
        };
        body.return_on_cancel();
        assert_eq!(returned.borrow().as_deref(), Some("abc"));
    }

    #[test]
    fn plain_body_completion_yields_the_retained_body() {
        let body = ReplacementBody::Plain {
            body: "done".to_string(),
            #[cfg(feature = "test-utils")]
            on_cancel: None,
        };
        assert_eq!(body.into_completed_body(), "done");
    }

    #[test]
    fn default_body_is_an_empty_plain_placeholder() {
        // `Default`/`mem::take`/`unwrap_or_default` placeholder values must stay
        // representable without weakening the pairing guarantee.
        let body = ReplacementBody::default();
        assert!(body.is_empty());
        assert_eq!(body.into_completed_body(), "");
    }
}
