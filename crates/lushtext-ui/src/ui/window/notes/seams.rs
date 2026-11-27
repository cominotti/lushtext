// SPDX-License-Identifier: GPL-3.0-or-later

//! Seam value objects for the notes and bookmarks workflow.
//!
//! Three of this workflow's stages dispatch background work and then have to
//! decide, in a completion closure, whether the result may still be published.
//! Before this module each of them compared the same clauses by hand:
//!
//! ```text
//! coordinator.is_current(generation) && state.mode.get() == mode && !state.disposed.get()
//! ```
//!
//! The bundle crossed two function boundaries (dispatch and completion) and was
//! reconstructed at three call sites, which is exactly the seam the readability
//! convention asks to be reified. It is reified in the established
//! Ticket/Facts/predicate shape: [`NotesBrowserTicket`] captures the expectation
//! at dispatch, [`NotesBrowserFacts`] captures observed live state at
//! completion, and one predicate validates them together.
//!
//! # Why the ticket is phantom-typed by flight
//!
//! The three stages own **three different generation counters** — the source
//! refresh coordinator, the browser query coordinator, and the closed-file
//! excerpt preview coordinator. A plain `{generation, mode}` struct would let a
//! query generation be validated against source facts: both are `u64`, both are
//! called `generation`, and the mistake is invisible to review and to tests
//! because either value reads correctly on its own. That is the archetype defect
//! the "a value must not be renamed while crossing a seam" rule exists to make
//! unrepresentable, so the flight is part of the ticket's *type* and a
//! cross-coordinator comparison is a compile error rather than a stale publish.
//!
//! # Why one flight is not mode-gated
//!
//! [`PreviewFlight`] declares `MODE_GATED = false`. A closed-file bookmark
//! excerpt belongs to a *selected row*, and its publication path revalidates
//! that row's path and line through `selected_bookmark_matches` before
//! rendering, which is strictly stronger than a mode comparison. Gating it on
//! mode as well would be a behavior change, so the weaker predicate is declared
//! per flight rather than applied by accident — the shape slot 3b used for
//! `installation_is_current`.

use std::marker::PhantomData;

use lushtext_model::values::FlightGeneration;
use lushtext_services::palette::NotesBrowserMode;

/// One of the browser's three independent background flights.
///
/// Implementors are zero-sized markers; the trait exists to give each flight its
/// own ticket type and to declare whether its completion is mode-gated.
pub(super) trait NotesBrowserFlight {
    /// Whether a completion is rejected when the browser's inventory mode moved
    /// on while the worker was running.
    const MODE_GATED: bool;
}

/// The bounded source construction that publishes the browser's entry set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceFlight;

impl NotesBrowserFlight for SourceFlight {
    const MODE_GATED: bool = true;
}

/// The full-source query that publishes the browser's filtered row order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct QueryFlight;

impl NotesBrowserFlight for QueryFlight {
    const MODE_GATED: bool = true;
}

/// The closed-file bookmark excerpt load that publishes one row's preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PreviewFlight;

impl NotesBrowserFlight for PreviewFlight {
    const MODE_GATED: bool = false;
}

/// Identity captured once when a browser flight is dispatched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NotesBrowserTicket<F: NotesBrowserFlight> {
    generation: FlightGeneration,
    mode: NotesBrowserMode,
    flight: PhantomData<F>,
}

impl<F: NotesBrowserFlight> NotesBrowserTicket<F> {
    /// Capture one flight's identity at dispatch time.
    pub(super) const fn new(generation: FlightGeneration, mode: NotesBrowserMode) -> Self {
        Self {
            generation,
            mode,
            flight: PhantomData,
        }
    }

    /// Return the coordinator generation this flight was admitted under.
    ///
    /// Completion paths need it to call `finish`, which must run whether or not
    /// the result is publishable so the coordinator's active slot always clears.
    pub(super) const fn generation(self) -> FlightGeneration {
        self.generation
    }

    /// Return the inventory mode this flight was issued for.
    pub(super) const fn mode(self) -> NotesBrowserMode {
        self.mode
    }

    /// Return whether this flight's result may still be published.
    ///
    /// Reads no state itself: the caller gathers [`NotesBrowserFacts`] under the
    /// one borrow it already holds, which is what keeps the freshness decision
    /// out of the coordinator's `borrow_mut` scope.
    pub(super) fn may_publish(self, facts: &NotesBrowserFacts<F>) -> bool {
        facts.coordinator_is_current
            && !facts.disposed
            && (!F::MODE_GATED || facts.live_mode == self.mode)
    }
}

/// Whether the owning coordinator still recognizes a ticket's generation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CoordinatorCurrency {
    /// Ticket still holds the coordinator's latest generation.
    Current,
    /// Ticket was superseded by a newer generation.
    Stale,
}

impl CoordinatorCurrency {
    pub(super) const fn from_is_current(current: bool) -> Self {
        if current { Self::Current } else { Self::Stale }
    }
}

/// Whether the notes browser has already been torn down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BrowserDisposal {
    /// Notes browser is still mounted and may publish results.
    Alive,
    /// Notes browser teardown has invalidated all publication.
    Disposed,
}

impl BrowserDisposal {
    pub(super) const fn from_disposed(disposed: bool) -> Self {
        if disposed {
            Self::Disposed
        } else {
            Self::Alive
        }
    }
}

/// Live state observed at one browser flight's completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NotesBrowserFacts<F: NotesBrowserFlight> {
    /// Whether the owning coordinator still reports this generation as latest.
    pub(super) coordinator_is_current: bool,
    /// The inventory mode the browser is in **now**, not the one requested.
    pub(super) live_mode: NotesBrowserMode,
    /// Whether dialog teardown has invalidated all publication.
    pub(super) disposed: bool,
    flight: PhantomData<F>,
}

impl<F: NotesBrowserFlight> NotesBrowserFacts<F> {
    /// Record the live state one completion must be validated against.
    pub(super) const fn new(
        coordinator: CoordinatorCurrency,
        live_mode: NotesBrowserMode,
        disposal: BrowserDisposal,
    ) -> Self {
        Self {
            coordinator_is_current: matches!(coordinator, CoordinatorCurrency::Current),
            live_mode,
            disposed: matches!(disposal, BrowserDisposal::Disposed),
            flight: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: NotesBrowserMode = NotesBrowserMode::AllNotes;
    const BOOKMARKS: NotesBrowserMode = NotesBrowserMode::Bookmarks;

    #[test]
    fn mode_gated_flight_rejects_a_completion_from_a_superseded_mode() {
        let ticket = NotesBrowserTicket::<SourceFlight>::new(FlightGeneration::new(7), ALL);
        assert!(ticket.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            ALL,
            BrowserDisposal::Alive
        )));
        assert!(!ticket.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            BOOKMARKS,
            BrowserDisposal::Alive
        )));
    }

    #[test]
    fn query_flight_is_mode_gated_too() {
        let ticket = NotesBrowserTicket::<QueryFlight>::new(FlightGeneration::new(3), BOOKMARKS);
        assert!(ticket.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            BOOKMARKS,
            BrowserDisposal::Alive
        )));
        assert!(!ticket.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            ALL,
            BrowserDisposal::Alive
        )));
    }

    #[test]
    fn preview_flight_ignores_mode_by_declaration() {
        // Behavior preservation: the closed-file excerpt path never compared
        // mode, because row identity is revalidated instead.
        let ticket = NotesBrowserTicket::<PreviewFlight>::new(FlightGeneration::new(3), ALL);
        assert!(ticket.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            BOOKMARKS,
            BrowserDisposal::Alive
        )));
    }

    #[test]
    fn every_flight_rejects_a_stale_generation_and_a_disposed_dialog() {
        let source = NotesBrowserTicket::<SourceFlight>::new(FlightGeneration::new(1), ALL);
        let query = NotesBrowserTicket::<QueryFlight>::new(FlightGeneration::new(1), ALL);
        let preview = NotesBrowserTicket::<PreviewFlight>::new(FlightGeneration::new(1), ALL);
        assert!(!source.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Stale,
            ALL,
            BrowserDisposal::Alive
        )));
        assert!(!query.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Stale,
            ALL,
            BrowserDisposal::Alive
        )));
        assert!(!preview.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Stale,
            ALL,
            BrowserDisposal::Alive
        )));
        assert!(!source.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            ALL,
            BrowserDisposal::Disposed
        )));
        assert!(!query.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            ALL,
            BrowserDisposal::Disposed
        )));
        assert!(!preview.may_publish(&NotesBrowserFacts::new(
            CoordinatorCurrency::Current,
            ALL,
            BrowserDisposal::Disposed
        )));
    }

    #[test]
    fn ticket_reports_the_generation_its_coordinator_must_finish() {
        let generation = FlightGeneration::new(42);
        let ticket = NotesBrowserTicket::<SourceFlight>::new(generation, BOOKMARKS);
        assert_eq!(ticket.generation(), generation);
        assert_eq!(ticket.mode(), BOOKMARKS);
    }

    #[test]
    fn mode_gating_is_declared_per_flight() {
        const { assert!(SourceFlight::MODE_GATED) };
        const { assert!(QueryFlight::MODE_GATED) };
        const { assert!(!PreviewFlight::MODE_GATED) };
    }
}
