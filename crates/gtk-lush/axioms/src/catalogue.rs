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
                    change: value = anchor position − align × page; after a host moves the \
                    value the align is not confined to [0, 1], after scroll_to it is",
        dependent_designs: &[
            "classify_child_scroll_in_frame writes back a settle under a published anchor",
            "the anchor model of the Kani slice loop",
            "the settle bound (reconfigure_shift) in classify_child_scroll",
        ],
        probe: Some(probes::probe_a07),
    },
    Axiom {
        id: AxiomId::new(8),
        name: "a08_a_settle_is_bounded_by_its_correction",
        statement: "a settle the child makes as a consequence of its own geometry correction is \
                    bounded by that correction, and does not survive into a stable-geometry \
                    frame; page changes the host makes are A7's, not this bound's",
        dependent_designs: &[
            "ChildScrollDecision::Defer",
            "the learning-frame residual",
            "the child's own estimate settle in the Kani slice loop",
        ],
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
        name: "a10_mapped_rows_are_not_necessarily_on_screen",
        statement: "mapped does not mean drawn on screen: a GtkListView maps rows whose bounds \
                    lie wholly outside its own allocation, one row past the bottom edge at rest \
                    and one past each edge when its value falls on a row boundary (a host \
                    set_value to a row top, scroll_to, a focus scroll_to), so a rendered-row \
                    assertion must intersect each mapped row's bounds with the viewport",
        dependent_designs: &[
            "rendered-row widget tests count mapped rows intersecting the outer viewport \
             (drawn_labels in workspace_tree_virtualization)",
        ],
        probe: Some(probes::probe_a10),
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
    Axiom {
        id: AxiomId::new(14),
        name: "a14_max_width_sp_is_scaled_px_and_inclusive",
        statement: "an AdwBreakpoint condition max-width: N sp matches a width w in px (the \
                    width the breakpoint bin or window is allocated) exactly when \
                    w ≤ N × text scale, the text scale being gtk-xft-dpi / (96 × 1024) (unset \
                    counts as 1.0): the edge is inclusive and the widest matching width is \
                    floor(N × scale), and a text-scale change alone re-evaluates it at the \
                    allocation that follows",
        dependent_designs: &[
            "the breakpoint-loop Kani model (ui/window/geometry/kani_proofs.rs)",
            "properties_breakpoint_condition and workspace_breakpoint_condition compare a px \
             window width against whole-sp thresholds (ui/window/geometry/policy.rs)",
        ],
        probe: Some(probes::probe_a14),
    },
    Axiom {
        id: AxiomId::new(15),
        name: "a15_set_condition_applies_a_frame_later",
        statement: "AdwBreakpoint::set_condition on an installed breakpoint re-evaluates \
                    nothing synchronously: current-breakpoint, apply/unapply, and the setters \
                    change only at the bin allocation set_condition schedules by itself, in the \
                    next frame, and the child sees the new values in the frame after that",
        dependent_designs: &[
            "the breakpoint-loop Kani model (ui/window/geometry/kani_proofs.rs)",
            "sync_properties_breakpoint re-tunes the properties breakpoint with set_condition \
             from size_allocate (ui/window/geometry/execution.rs)",
        ],
        probe: Some(probes::probe_a15),
    },
    Axiom {
        id: AxiomId::new(16),
        name: "a16_setters_lag_a_frame_and_restore_the_add_time_value",
        statement: "setters are applied and unapplied inside the breakpoint bin's allocation \
                    after it has allocated its child for that frame, so across a resize the \
                    child is allocated once at the new width with the old values and sees the \
                    new ones one frame later; unapply restores the value the property held \
                    when add_setter was called, discarding every later application write, \
                    including one made while the breakpoint was applied; when the current \
                    breakpoint switches directly between two breakpoints that both set the same \
                    property (by width or by text scale alone), the outgoing unapply fires with \
                    the property still at its value, one notify carries the incoming value (even \
                    an equal one), then the incoming apply fires, so the property ends at the \
                    incoming value and the add-time value is never observable in between; \
                    leaving both still restores the add-time value",
        dependent_designs: &[
            "the breakpoint-loop Kani model (ui/window/geometry/kani_proofs.rs)",
            "sync_secondary_surfaces writes properties_layout_view layout-name, which the \
             properties breakpoint's setter also owns (ui/window/geometry/execution.rs)",
        ],
        probe: Some(probes::probe_a16),
    },
    Axiom {
        id: AxiomId::new(17),
        name: "a17_split_view_sidebar_is_a_clamped_fraction",
        statement: "AdwOverlaySplitView allocates its sidebar fraction × split width clamped \
                    to [min-sidebar-width, max-sidebar-width] in sidebar-width-unit (sp by \
                    default, so the bounds scale with the text), raised to the sidebar's own \
                    minimum; collapsed, the overlaid sidebar is max-sidebar-width wide and the \
                    content takes the whole width; toggling show-sidebar or collapsed changes \
                    the split view's minimum but not the toplevel window's width",
        dependent_designs: &[
            "the breakpoint-loop Kani model (ui/window/geometry/kani_proofs.rs)",
            "split_fraction and the sp min/max sidebar widths the shell sets \
             (ui/window/geometry/execution.rs)",
        ],
        probe: Some(probes::probe_a17),
    },
    Axiom {
        id: AxiomId::new(18),
        name: "a18_breakpoints_lower_the_minimum_and_the_last_match_wins",
        statement: "without breakpoints a window's minimum width is its content's even under a \
                    smaller width-request; with a breakpoint installed it is the \
                    width-request, and a window asked narrower is allocated the request; where \
                    several breakpoints match (below the smallest max-width condition all of \
                    them do) only the last one added applies, and the others' setters are \
                    unapplied",
        dependent_designs: &[
            "the breakpoint-loop Kani model (ui/window/geometry/kani_proofs.rs)",
            "install_split_view_breakpoints adds the properties, workspace, then Open-button \
             breakpoints under the window's width-request of 640 \
             (ui/window/geometry/execution.rs, resources/ui/window.blp)",
        ],
        probe: Some(probes::probe_a18),
    },
    Axiom {
        id: AxiomId::new(19),
        name: "a19_viewport_notifies_after_allocating_its_child",
        statement: "GtkViewport freezes property notification on its adjustments for its \
                    whole size_allocate and thaws it only after allocating its child, while \
                    the value-changed of a clamp in gtk_adjustment_configure fires at once: \
                    value-changed precedes the child's allocation, and notify::page-size and \
                    notify::value follow it inside the same layout phase, so a queue_allocate \
                    issued from notify::page-size for a widget inside the viewport is not \
                    served in that frame",
        dependent_designs: &[
            "ViewportSliceBin::rebind_outer re-slices on notify::page-size from an idle \
             instead of queueing inside layout",
        ],
        probe: Some(probes::probe_a19),
    },
    Axiom {
        id: AxiomId::new(20),
        name: "a20_a_value_announced_before_allocation_strays_the_anchor",
        statement: "a value-changed a GtkListView receives before it has been allocated at \
                    the new value (a host set_value) leaves its scroll anchor far from the \
                    view, with an alignment outside [0, 1], so a later page change re-derives \
                    the value along a steep line; the same value re-announced once after the \
                    list was allocated there anchors it with an alignment in [0, 1]",
        dependent_designs: &[
            "ViewportSliceBin::size_allocate re-announces the value after the child's \
             allocation whenever its publish changed the value",
            "the anchor_stray ghost of the Kani slice loop (gtk-lush-widgets kani_proofs.rs)",
        ],
        probe: Some(probes::probe_a20),
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
    fn the_catalogue_covers_every_ledger_id_up_to_a20() {
        for number in 1..=20 {
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
