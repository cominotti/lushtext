// SPDX-License-Identifier: MIT OR Apache-2.0

//! Isolated probes and runnable samples for the GTK behaviours gtk-rs geometry
//! code relies on.
//!
//! Each entry of [`catalogue`] is one **axiom**: a precise statement about
//! GTK, `GtkListBase`, `GtkScrollable`, or Adwaita that some design depends
//! on. The normative list is LushText's GTK axiom ledger
//! (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`);
//! this crate is where each belief is made observable. A pinnable axiom has a
//! probe — a function that builds the smallest pure-GTK fixture showing the
//! behaviour, drives it to a bounded end, and returns an [`Observation`] — and
//! one runnable sample under `examples/` that builds the same fixture for a
//! person to watch.
//!
//! A probe that stops returning [`Verdict::Holds`] after a toolkit update is
//! an **axiom change**, not a test to adjust: revisit the ledger entry and
//! every design that depends on it first.
//!
//! # Running a probe
//!
//! Probes expect GTK and Libadwaita initialized on the calling thread, and a
//! display. They present their fixture in a plain window, so run them only in
//! a private headless session, never on a live desktop:
//!
//! ```no_run
//! gtk_lush_axioms::init_toolkit().expect("GTK and Libadwaita initialize");
//! for axiom in gtk_lush_axioms::catalogue() {
//!     if let Some(probe) = axiom.probe {
//!         println!("{}", probe().to_json_line());
//!     }
//! }
//! ```
//!
//! GTK Lush crates remain independently adoptable leaf crates. This one
//! depends on `gtk4`, `glib`, and `libadwaita` only; it does not own an
//! application's control flow, define a view DSL, or add a state or message
//! system.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod catalogue;
pub mod fixtures;
mod observation;
mod probes;
mod session;

use std::fmt;

pub use catalogue::{Axiom, catalogue, find};
pub use observation::{Observation, Verdict, adw_version, gtk_version};
pub use probes::*;

/// The stable id of one ledger axiom, displayed `A1`, `A2`, ….
///
/// Ids are never reused, so an id cited by a verification envelope keeps
/// meaning the same statement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AxiomId(u16);

impl AxiomId {
    /// The axiom numbered `number` (`AxiomId::new(5)` is `A5`).
    #[must_use]
    pub const fn new(number: u16) -> Self {
        Self(number)
    }

    /// The axiom's number.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.0
    }
}

impl fmt::Display for AxiomId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "A{}", self.0)
    }
}

/// Initialize GTK and Libadwaita on the calling thread.
///
/// # Errors
///
/// Returns the toolkit's error when no display is available.
pub fn init_toolkit() -> Result<(), glib::BoolError> {
    gtk4::init()?;
    libadwaita::init()
}
