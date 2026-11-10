// SPDX-License-Identifier: MIT OR Apache-2.0

//! Version bounds for axioms whose verdict differs between supported toolkits.
//!
//! LushText ships one runtime (GNOME 51: GTK 4.24, GtkSourceView 5.22) and
//! keeps an older source floor (GNOME 50: GTK 4.22, GtkSourceView 5.20). When
//! a toolkit release changes an axiom, the ledger states each verdict with its
//! version bound, and the probe asserts the verdict of the toolkit it runs on:
//! below the bound, the old statement; at or above it, the new one. Either
//! side still fails on any further change, and the observation records which
//! branch was asserted.
//!
//! The bound is named once, in the catalogue entry ([`Axiom::version_bound`](crate::Axiom::version_bound)),
//! and the probe reads it from there through [`VersionBound::reached`].

use std::fmt;

use crate::AxiomId;
use crate::observation::{Recorder, Stop};

/// The toolkit library a [`VersionBound`] compares against.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Toolkit {
    /// GTK (`gtk_get_*_version`).
    Gtk,
    /// GtkSourceView (`gtk_source_get_*_version`).
    SourceView,
}

impl Toolkit {
    /// The name the ledger uses for the library.
    #[must_use]
    pub const fn ledger_name(self) -> &'static str {
        match self {
            Self::Gtk => "GTK",
            Self::SourceView => "GtkSourceView",
        }
    }

    /// The running library's `(major, minor, micro)`.
    #[must_use]
    pub fn running(self) -> (u32, u32, u32) {
        match self {
            Self::Gtk => (
                gtk4::major_version(),
                gtk4::minor_version(),
                gtk4::micro_version(),
            ),
            Self::SourceView => (
                sourceview5::major_version(),
                sourceview5::minor_version(),
                sourceview5::micro_version(),
            ),
        }
    }
}

/// The first release of a toolkit at which an axiom's newer verdict holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct VersionBound {
    /// The library compared.
    pub toolkit: Toolkit,
    /// The first release with the newer verdict, `(major, minor, micro)`.
    pub release: (u32, u32, u32),
}

impl VersionBound {
    /// A GTK bound.
    #[must_use]
    pub const fn gtk(major: u32, minor: u32, micro: u32) -> Self {
        Self {
            toolkit: Toolkit::Gtk,
            release: (major, minor, micro),
        }
    }

    /// A GtkSourceView bound.
    #[must_use]
    pub const fn sourceview(major: u32, minor: u32, micro: u32) -> Self {
        Self {
            toolkit: Toolkit::SourceView,
            release: (major, minor, micro),
        }
    }

    /// Whether `version` is at or above the bound.
    #[must_use]
    pub const fn reached_by(self, version: (u32, u32, u32)) -> bool {
        let (major, minor, micro) = version;
        let (bound_major, bound_minor, bound_micro) = self.release;
        if major != bound_major {
            return major > bound_major;
        }
        if minor != bound_minor {
            return minor > bound_minor;
        }
        micro >= bound_micro
    }

    /// Whether the running toolkit is at or above the bound.
    #[must_use]
    pub fn reached(self) -> bool {
        self.reached_by(self.toolkit.running())
    }

    /// The branch a probe asserts on the running toolkit, for its
    /// observation: `GTK >= 4.24.0` or `GTK < 4.24.0`.
    #[must_use]
    pub fn asserted_branch(self) -> String {
        let relation = if self.reached() { ">=" } else { "<" };
        format!("{} {relation} {self}", self.toolkit.ledger_name())
    }
}

impl fmt::Display for VersionBound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (major, minor, micro) = self.release;
        write!(formatter, "{major}.{minor}.{micro}")
    }
}

/// The bound the catalogue names for `axiom`, recorded with the branch the
/// probe asserts on the running toolkit (`asserted_branch`). A catalogue
/// entry without a bound makes the fixture invalid.
pub(crate) fn catalogued_bound(
    recorder: &mut Recorder,
    axiom: AxiomId,
) -> Result<VersionBound, Stop> {
    let bound = recorder.require(
        crate::catalogue::find(axiom).and_then(|entry| entry.version_bound),
        "control: the catalogue entry names the axiom's version bound",
    )?;
    recorder.measure("asserted_branch", bound.asserted_branch());
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUND: VersionBound = VersionBound::gtk(4, 24, 0);

    #[test]
    fn a_release_at_the_bound_reaches_it() {
        assert!(BOUND.reached_by((4, 24, 0)));
        assert!(BOUND.reached_by((4, 24, 1)));
    }

    #[test]
    fn a_later_minor_or_major_reaches_it_whatever_its_micro() {
        assert!(BOUND.reached_by((4, 25, 0)));
        assert!(BOUND.reached_by((5, 0, 0)));
    }

    #[test]
    fn an_earlier_release_does_not_reach_it() {
        assert!(!BOUND.reached_by((4, 22, 5)));
        assert!(!BOUND.reached_by((4, 23, 99)));
        assert!(!BOUND.reached_by((3, 99, 99)));
    }

    #[test]
    fn a_micro_bound_compares_the_micro() {
        let bound = VersionBound::sourceview(5, 22, 2);
        assert!(!bound.reached_by((5, 22, 1)));
        assert!(bound.reached_by((5, 22, 2)));
        assert!(bound.reached_by((5, 23, 0)));
    }

    #[test]
    fn the_bound_prints_as_the_ledger_writes_it() {
        assert_eq!(BOUND.to_string(), "4.24.0");
        assert_eq!(Toolkit::SourceView.ledger_name(), "GtkSourceView");
    }
}
