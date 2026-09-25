// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every ledger axiom, with its statement, dependent designs, and probe.
//!
//! # Adding an axiom
//!
//! Append one [`Axiom`] entry here with the next unused id, in id order. When
//! a minimal fixture isolates it, give it a probe in `src/probes/aNN.rs` and a
//! sample `examples/<name>.rs`; `make check-policy` fails until the ledger,
//! this catalogue, the probe, and the sample all agree. The crate README walks
//! through the whole step.

use crate::{AxiomId, Observation, probes};

/// One behavioural axiom about GTK or Libadwaita.
#[derive(Clone, Copy, Debug)]
pub struct Axiom {
    /// The stable ledger id.
    pub id: AxiomId,
    /// The stable lower-case name, `aNN_<slug>`. For a probed axiom it is
    /// also its sample's file stem (`examples/<name>.rs`) and the probe
    /// runner's test name.
    pub name: &'static str,
    /// The precise statement the ledger records.
    pub statement: &'static str,
    /// The designs that rely on the statement.
    pub dependent_designs: &'static [&'static str],
    /// The isolated probe, or `None` when no minimal fixture can exhibit the
    /// axiom; the ledger row records why.
    pub probe: Option<fn() -> Observation>,
}

/// Every ledger axiom, in id order.
#[must_use]
pub fn catalogue() -> &'static [Axiom] {
    CATALOGUE
}

/// The catalogue entry for `id`.
#[must_use]
pub fn find(id: AxiomId) -> Option<&'static Axiom> {
    CATALOGUE.iter().find(|axiom| axiom.id == id)
}

const CATALOGUE: &[Axiom] = &[
    Axiom {
        id: AxiomId::new(1),
        name: "a01_realized_rows_are_capped",
        statement: "GtkListView realizes at most about GTK_LIST_VIEW_MAX_LIST_ITEMS (200) row \
                    widgets per visible range, the cap plus a few tracker extras (205 measured), \
                    and \"visible\" means the range its own vadjustment describes",
        dependent_designs: &[
            "ViewportSliceBin exists because of it",
            "the sidebar file tree",
        ],
        probe: Some(probes::probe_a01),
    },
    Axiom {
        id: AxiomId::new(2),
        name: "a02_unscrolled_list_minimum_is_its_content",
        statement: "a list outside a scroller reports its whole content as its minimum height",
        dependent_designs: &["ViewportSliceBin::measure (full content to the outer scroller)"],
        probe: Some(probes::probe_a02),
    },
    Axiom {
        id: AxiomId::new(3),
        name: "a03_viewport_allocates_the_minimum",
        statement: "GtkViewport allocates a non-scrollable child its minimum in the scroll \
                    direction",
        dependent_designs: &[
            "ViewportSliceBin::measure reports the full content as minimum while an outer \
             scroller exists",
        ],
        probe: Some(probes::probe_a03),
    },
    Axiom {
        id: AxiomId::new(4),
        name: "a04_scroll_to_applies_inside_allocation",
        statement: "the list applies scroll_to and focus scrolling inside its own allocation, \
                    as a write to its vadjustment",
        dependent_designs: &["classify_child_scroll reads the child's value after child.allocate"],
        probe: Some(probes::probe_a04),
    },
    Axiom {
        id: AxiomId::new(5),
        name: "a05_zero_height_rewrites_the_adjustment",
        statement: "a zero-height allocation makes the list rewrite the host's adjustment — page \
                    to 0 and the value re-derived from its anchor — with a value-changed, \
                    although nothing asked it to scroll",
        dependent_designs: &["viewport_slice never shrinks the band at the bottom edge"],
        probe: Some(probes::probe_a05),
    },
    Axiom {
        id: AxiomId::new(6),
        name: "a06_scrollable_works_in_its_content_box",
        statement: "a GtkScrollable works in its CSS content box, so page = allocation − inset",
        dependent_designs: &[
            "ViewportSliceBin::publish_slice_offset publishes content-box upper/page",
            "inset learning",
        ],
        probe: Some(probes::probe_a06),
    },
    Axiom {
        id: AxiomId::new(7),
        name: "a07_value_is_rederived_from_the_anchor",
        statement: "the child re-derives its value from its scroll anchor when page or upper \
                    change: value = anchor position − align × page, with align not confined \
                    to [0, 1]",
        dependent_designs: &["the settle bound (reconfigure_shift) in classify_child_scroll"],
        probe: Some(probes::probe_a07),
    },
    Axiom {
        id: AxiomId::new(8),
        name: "a08_a_settle_is_bounded_by_its_correction",
        statement: "a settle is bounded by the geometry correction that caused it, and does not \
                    survive into a stable-geometry frame",
        dependent_designs: &["ChildScrollDecision::Defer", "the learning-frame residual"],
        probe: None,
    },
    Axiom {
        id: AxiomId::new(9),
        name: "a09_value_changed_drops_a_pending_scroll_to",
        statement: "GtkListBase drops a pending scroll_to on any value-changed of its adjustment",
        dependent_designs: &[
            "a honoured request must land where the re-slice republishes exactly the child's \
             value (child_value − viewport_top)",
        ],
        probe: Some(probes::probe_a09),
    },
    Axiom {
        id: AxiomId::new(10),
        name: "a10_rows_near_focus_stay_realized_unmapped",
        statement: "rows around focus and selection stay realized but unmapped when off screen",
        dependent_designs: &["rendered-row tests count mapped rows only"],
        probe: None,
    },
    Axiom {
        id: AxiomId::new(11),
        name: "a11_adjustments_clamp_and_skip_unchanged_values",
        statement: "Adjustment::configure and set_value clamp into [lower, upper − page_size], \
                    and a set_value that leaves the value unchanged emits nothing",
        dependent_designs: &[
            "the bin reads its published offset back after configure",
            "an unhonourable outer request ends quietly (the loop guard in \
             follow_child_request)",
        ],
        probe: Some(probes::probe_a11),
    },
    Axiom {
        id: AxiomId::new(12),
        name: "a12_natural_height_ignores_scroll_position",
        statement: "the child's natural height is independent of the outer scroll position",
        dependent_designs: &["the outer range stays exact while scrolling"],
        probe: None,
    },
    Axiom {
        id: AxiomId::new(13),
        name: "a13_an_in_layout_scroll_does_not_relayout",
        statement: "moving the outer adjustment from inside layout does not reliably schedule a \
                    relayout: a widget that re-slices on value-changed loses its own \
                    queue_allocate issued while it is being allocated",
        dependent_designs: &[
            "ViewportSliceBin::follow_child_request applies the outer move from \
                              an idle",
        ],
        probe: Some(probes::probe_a13),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_in_order() {
        let ids: Vec<u16> = catalogue().iter().map(|axiom| axiom.id.number()).collect();
        assert!(ids.array_windows().all(|[low, high]| low < high), "{ids:?}");
        assert_eq!(ids.first(), Some(&1));
    }

    #[test]
    fn the_catalogue_covers_every_ledger_id_up_to_a13() {
        for number in 1..=13 {
            assert!(find(AxiomId::new(number)).is_some(), "A{number} missing");
        }
    }

    #[test]
    fn names_carry_their_zero_padded_id_and_are_unique() {
        let mut names: Vec<&str> = Vec::new();
        for axiom in catalogue() {
            let prefix = format!("a{:02}_", axiom.id.number());
            assert!(
                axiom.name.starts_with(&prefix),
                "{} must start with {prefix}",
                axiom.name
            );
            assert!(
                axiom
                    .name
                    .chars()
                    .all(|character| character.is_ascii_lowercase()
                        || character.is_ascii_digit()
                        || character == '_'),
                "{} must be lower-case snake case",
                axiom.name
            );
            assert!(!names.contains(&axiom.name), "{} is duplicated", axiom.name);
            names.push(axiom.name);
        }
    }

    #[test]
    fn every_entry_states_its_behaviour_and_a_dependent_design() {
        for axiom in catalogue() {
            assert!(!axiom.statement.is_empty(), "{} has no statement", axiom.id);
            assert!(
                !axiom.dependent_designs.is_empty(),
                "{} has no design",
                axiom.id
            );
        }
    }

    #[test]
    fn ids_display_with_their_ledger_prefix() {
        assert_eq!(AxiomId::new(5).to_string(), "A5");
        assert_eq!(AxiomId::new(13).to_string(), "A13");
    }
}
