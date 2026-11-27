// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: evidence surface — the transient dismissal workflow's single
//! observable state.
//!
//! One accessor reads the whole surface. Three constraints follow, and each one
//! is the reason for a specific line below rather than general advice:
//!
//! * **Reading must not mutate.** The workflow's own Escape path *consumes* the
//!   child-handled latch with `Cell::replace(false)`, because one Escape may
//!   spend it and only one. This surface reads it with `Cell::get()`. A surface
//!   that consumed the latch would make observing the shell change what the next
//!   Escape does — silently, because both spellings compile and both return the
//!   same `bool`.
//! * **No field may be read from inside a mutable borrow** of the state it
//!   reads. This surface takes no `RefCell` borrow at all: every field is a
//!   `Cell<bool>`, a widget property, or a pure derivation, so the constraint
//!   holds by construction rather than by discipline.
//! * **A disposed widget is a stage.** Every template child is reached through
//!   `try_get()` — but not in this file. The observation lives in the row's
//!   called presentation surface, `surfaces::observed_surface_state`, and this
//!   module *calls* it. A second observer written for the surface to read would
//!   compile in both configurations and drift in exactly one, so a probe and the
//!   ladder read the same state by construction rather than by review.
//!
//! Reading materializes nothing: `reveals_child()`, `is_active()`, and
//! `selected_page()` all inspect already-realized widgets, and no accessor here
//! reaches a lazily-populated GTK collection.

use gtk4::subclass::prelude::ObjectSubclassIsExt;

use super::super::LushtextWindow;
use super::policy::{
    EscapeOutcome, TransientDismissal, TransientSurfaceState, escape_outcome, topmost_dismissible,
};

/// Everything a test or probe may observe about transient dismissal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransientDismissalEvidence {
    /// The live visibility of each dismissible shell surface.
    pub surfaces: TransientSurfaceState,
    /// Whether a child widget has latched the current Escape request.
    ///
    /// Read, never consumed — see the module doc.
    pub child_escape_handled: bool,
    /// Whether Focus Mode is active, which Escape leaves only after the ladder
    /// declines.
    pub focus_mode_active: bool,
    /// The surface the ladder would give up right now, or `None`.
    pub topmost_dismissible: Option<TransientDismissal>,
    /// What one Escape press would resolve to right now.
    pub escape_outcome: EscapeOutcome,
}

/// Read the whole transient dismissal surface.
#[must_use]
pub fn transient_dismissal_evidence(window: &LushtextWindow) -> TransientDismissalEvidence {
    // The facade's own stage-1 observation, called rather than repeated.
    let surfaces = super::surfaces::observed_surface_state(window);
    let child_escape_handled = window.imp().transient_child_escape_handled.get();
    let focus_mode_active = window.is_focus_mode_active();

    TransientDismissalEvidence {
        surfaces,
        child_escape_handled,
        focus_mode_active,
        topmost_dismissible: topmost_dismissible(surfaces),
        escape_outcome: escape_outcome(child_escape_handled, surfaces, focus_mode_active),
    }
}
