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
    /// Whether the probe runs for seconds by design (a fixture that must
    /// reach a toolkit time limit). The pull-request probe job skips slow
    /// probes and reports each as `skipped (slow): <id>`; local
    /// `make gtk-axioms` and the nightly workflow run them
    /// (`GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`), and the ledger row says where.
    pub slow: bool,
    /// The toolkit release from which the axiom's newer verdict holds, for an
    /// axiom whose verdict differs between supported toolkits (the GNOME 50
    /// floor and the shipped GNOME 51 runtime). The probe asserts the old
    /// statement below it and the new one at or above it, and the ledger row
    /// states both ranges. `None` for an axiom that holds alike everywhere.
    pub version_bound: Option<crate::version::VersionBound>,
    /// The isolated probe, or `None` when no minimal fixture can exhibit the
    /// axiom; the ledger row records why.
    pub probe: Option<fn() -> Observation>,
}

/// The environment switch that makes runners include slow probes: they run
/// only when it is `1`. Local `make gtk-axioms` and the nightly
/// responsiveness workflow set it; the pull-request `GTK Axioms` job does not.
pub const INCLUDE_SLOW_ENV: &str = "GTK_LUSH_AXIOMS_INCLUDE_SLOW";

/// Whether [`INCLUDE_SLOW_ENV`] asks runners to include slow probes.
#[must_use]
pub fn include_slow_probes() -> bool {
    std::env::var(INCLUDE_SLOW_ENV).as_deref() == Ok("1")
}

/// The line a runner prints for a slow probe it skipped:
/// `skipped (slow): <id>`.
#[must_use]
pub fn slow_skip_line(axiom: &Axiom) -> String {
    format!("skipped (slow): {}", axiom.id)
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
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a01),
    },
    Axiom {
        id: AxiomId::new(2),
        name: "a02_unscrolled_list_minimum_is_its_content",
        statement: "a list outside a scroller reports its whole content as its minimum height",
        dependent_designs: &["ViewportSliceBin::measure (full content to the outer scroller)"],
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a03),
    },
    Axiom {
        id: AxiomId::new(4),
        name: "a04_scroll_to_applies_inside_allocation",
        statement: "the list applies scroll_to and focus scrolling inside its own allocation, \
                    as a write to its vadjustment",
        dependent_designs: &["classify_child_scroll reads the child's value after child.allocate"],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a04),
    },
    Axiom {
        id: AxiomId::new(5),
        name: "a05_zero_height_rewrites_the_adjustment",
        statement: "a zero-height allocation makes the list rewrite the host's adjustment — page \
                    to 0 and the value re-derived from its anchor — with a value-changed, \
                    although nothing asked it to scroll",
        dependent_designs: &["viewport_slice never shrinks the band at the bottom edge"],
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a06),
    },
    Axiom {
        id: AxiomId::new(7),
        name: "a07_value_is_rederived_from_the_anchor",
        statement: "the child re-derives its value from its scroll anchor when page or upper \
                    change: value = anchor position − align × page; after scroll_to the align \
                    is in [0, 1] (both ranges); on GTK < 4.24, after a host moves the value the \
                    align is not confined to [0, 1]; on GTK >= 4.24 a host move anchors at the \
                    view's top edge with align 0, so a page change keeps the value",
        dependent_designs: &[
            "classify_child_scroll_in_frame writes back a settle under a published anchor",
            "the anchor model of the Kani slice loop",
            "the settle bound (reconfigure_shift) in classify_child_scroll",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::gtk(4, 24, 0)),
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a11),
    },
    Axiom {
        id: AxiomId::new(12),
        name: "a12_natural_height_ignores_scroll_position",
        statement: "the child's natural height is independent of the outer scroll position",
        dependent_designs: &["the outer range stays exact while scrolling"],
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
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
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a19),
    },
    Axiom {
        id: AxiomId::new(20),
        name: "a20_a_value_announced_before_allocation_strays_the_anchor",
        statement: "a value-changed a GtkListView receives before it has been allocated at \
                    the new value (a host set_value) leaves its scroll anchor far from the \
                    view, with an alignment outside [0, 1], so a later page change re-derives \
                    the value along a steep line, on GTK < 4.24; the same value re-announced \
                    once after the list was allocated there anchors it with an alignment in \
                    [0, 1]; on GTK >= 4.24 a host set_value already anchors at the view's top \
                    edge with alignment 0, re-announced or not",
        dependent_designs: &[
            "ViewportSliceBin::size_allocate re-announces the value after the child's \
             allocation whenever its publish changed the value",
            "the anchor_stray ghost of the Kani slice loop (gtk-lush-widgets kani_proofs.rs)",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::gtk(4, 24, 0)),
        probe: Some(probes::probe_a20),
    },
    Axiom {
        id: AxiomId::new(21),
        name: "a21_destroy_removes_a_window_but_the_last_unref_disposes_it",
        statement: "gtk_window_destroy on a window of a registered GtkApplication emits the \
                    application's window-removed for it before destroy returns, after which \
                    the window has no application and windows() no longer lists it; it does \
                    not dispose the window while another strong reference is alive (dispose \
                    has not run and a WeakRef still upgrades), and dispose runs only when the \
                    last strong reference drops, just before finalization",
        dependent_designs: &[
            "LushtextWindow leaves the process draft journal (ProcessDraftJournal: the lane, \
             its draft-id claims, orphan-cleanup ownership) from the application's \
             window-removed handler, with dispose's leave an idempotent second path \
             (ui/window/drafts/admission.rs)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a21),
    },
    Axiom {
        id: AxiomId::new(22),
        name: "a22_tree_row_expansion_is_synchronous",
        statement: "in a GtkTreeListModel with passthrough and autoexpand off, \
                    TreeListRow::set_expanded(true) on a collapsed row whose create function \
                    has not run calls it once, before set_expanded returns: when it returns a \
                    model the row is expanded on return, children() is that model, and the \
                    flattened model already holds the children (items-changed fired inside the \
                    call); when it returns None the row stays collapsed with no children, the \
                    flattened model is unchanged, and the row is marked empty, so a later \
                    set_expanded(true) calls nothing; items appended later to the returned \
                    store appear at parent + 1 + index, and set_expanded(false) collapses the \
                    row and removes its children before it returns. is_expandable() on such a \
                    row also runs the create function, installs nothing, and discards the \
                    model (a later expansion creates a fresh one); a None there marks the row \
                    empty",
        dependent_designs: &[
            "the workspace path reveal expands each ancestor with set_expanded and reads \
             is_expanded() straight back to tell a directory from a known-empty or \
             non-directory row, finds scanned children by walking the flattened model, and \
             collapses what it expanded on failure (ui/sidebar/reveal_execution.rs \
             advance_expansion, terminate_reveal)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a22),
    },
    Axiom {
        id: AxiomId::new(23),
        name: "a23_a_paragraph_is_one_layout",
        statement: "GtkTextView lays a paragraph out as one PangoLayout over its visible text and \
                    never lays out part of one: once any part of a wrapped paragraph is shown, it \
                    wraps into the display lines Pango gives the whole paragraph laid out alone \
                    at the view's wrap width, and its last character's location is already valid \
                    on the paragraph's last display line",
        dependent_designs: &[
            "document-scale-policy (C1): N_visible and the paragraph-length thresholds rest on \
             layout cost growing with a paragraph's visible length",
            "long-line-protected-view (C2): the display-only fold bounds layout only because a \
             paragraph is laid out whole",
            "sliced-protected-install (C3): a tail installed hidden keeps the visible text, and so \
             the layout, a fixed head",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a23),
    },
    Axiom {
        id: AxiomId::new(24),
        name: "a24_invisible_text_is_left_out_of_the_layout",
        statement: "text tagged invisible is left out of a paragraph's layout: a hidden tail adds \
                    no display height and no x-advance, so the paragraph's height and its visible \
                    head's character locations equal those of a buffer holding the head alone; \
                    applying or removing the tag re-lays out the paragraph",
        dependent_designs: &[
            "long-line-protected-view (C2): the display-only fold tags a giant paragraph's tail \
             invisible",
            "sliced-protected-install (C3): tail slices inserted hidden do not re-lay out the \
             growing paragraph",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a24),
    },
    Axiom {
        id: AxiomId::new(25),
        name: "a25_insertions_at_a_tagged_run_boundary_are_untagged",
        statement: "text inserted exactly at a tagged run's start or end boundary is untagged, and \
                    text inserted strictly inside the run is tagged",
        dependent_designs: &[
            "sliced-protected-install (C3): insert_with_tags at a hidden run's end inserts \
             untagged text first",
            "long-line-protected-view (C2): fold-at-insertion and edit-growth tagging",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a25),
    },
    Axiom {
        id: AxiomId::new(26),
        name: "a26_log_attrs_cover_the_whole_paragraph",
        statement: "GtkTextBuffer computes a paragraph's log attrs over the whole logical \
                    paragraph, hidden text included: a word that starts in a visible head and \
                    continues into an invisible run ends inside the hidden run (forward_word_end); \
                    a character change anywhere invalidates the cached attrs, so the next word or \
                    cursor movement recomputes the whole paragraph (cost evidence only)",
        dependent_designs: &[
            "long-line-protected-view (C2): giant paragraphs are caret-free because folding does \
             not bound cursor, word or Backspace cost",
            "sliced-protected-install (C3): the install never moves an iterator by cursor or word \
             position inside a protected paragraph",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a26),
    },
    Axiom {
        id: AxiomId::new(27),
        name: "a27_tag_changes_do_not_dirty_the_buffer",
        statement: "applying or removing a tag emits no changed signal, leaves the buffer's \
                    modified flag unset, and records no undo history, with undo enabled",
        dependent_designs: &[
            "document-scale-policy (C1): removing highlighting tags changes no text",
            "long-line-protected-view (C2): folding and unfolding by tag keep the document clean, \
             unsaved and un-undoable",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a27),
    },
    Axiom {
        id: AxiomId::new(28),
        name: "a28_pango_line_width_overflows_int32",
        statement: "Pango measures a line in int32 Pango units (1/1024 px): in a monospace font \
                    the x position of a character on one unwrapped line stops growing at i32::MAX \
                    / advance characters (238,319 for a 9011-unit advance) and wraps negative past \
                    it",
        dependent_designs: &[
            "document-scale-policy (C1): the document profile treats a wrap-off line past the \
             overflow length as unrenderable",
            "long-line-protected-view (C2): the protected thresholds stay below the overflow \
             length",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a28),
    },
    Axiom {
        id: AxiomId::new(29),
        name: "a29_the_context_engine_disables_highlighting_after_2000_ms",
        statement: "the GtkSourceView context engine analyses a line in one call with a fixed 2000 \
                    ms cut-off; a line whose analysis passes it logs a GtkSourceView CRITICAL from \
                    inside that call and turns highlighting off for the whole buffer, so a short \
                    line added afterwards is not highlighted; the engine also highlights text \
                    inside an invisible range",
        dependent_designs: &[
            "document-scale-policy (C1): syntax is turned off before the engine sees a giant \
             paragraph (L_syntax, S_syntax); folding cannot hide text from the engine",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a29),
    },
    Axiom {
        id: AxiomId::new(30),
        name: "a30_im_surrounding_text_spans_the_cursor_paragraph",
        statement: "an input method that requests surrounding text gets the cursor's whole \
                    paragraph from GtkTextView, and the view arms that request on every caret \
                    move, cursor movement, typing and scrolling (focus-in and clicks are driven \
                    only by the responsiveness lane)",
        dependent_designs: &[
            "long-line-protected-view (C2): the protected view keeps the caret out of giant \
             paragraphs (L_protect)",
            "document-scale-policy (C1): L_protect",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a30),
    },
    Axiom {
        id: AxiomId::new(31),
        name: "a31_validation_runs_in_2000_px_batches",
        statement: "after the on-screen part, GtkTextView validates the rest of a buffer in idle \
                    batches of 2000 px: each batch grows the vertical adjustment's upper by 2000 \
                    px plus at most one line, so a document of height H settles over about H / \
                    2000 batches (observed indirectly, through upper)",
        dependent_designs: &[
            "document-scale-policy (C1): R_batch, rows per 2000 px with wrapping off, and N_dense \
             = V_batch / R_batch",
            "long-line-protected-view (C2): the fold policy's density bound",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a31),
    },
    Axiom {
        id: AxiomId::new(32),
        name: "a32_detaching_a_source_map_with_a_pending_tick_is_critical",
        statement: "detaching a GtkSourceMap from its view (gtk_source_map_set_view(NULL)) while \
                    its update tick is pending makes the tick run against no view and log Gtk and \
                    GLib-GObject CRITICALs; a scroll queues the tick (a defect axiom: on \
                    GtkSourceView < 5.22 holds while the defect is present; GtkSourceView 5.22.0 \
                    fixed it, 8d679ccd, and on >= 5.22 the same detach must log nothing)",
        dependent_designs: &[
            "document-scale-policy (C1): the minimap retire-and-rebuild that removes the detach \
             CRITICAL",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::sourceview(5, 22, 0)),
        probe: Some(probes::probe_a32),
    },
    Axiom {
        id: AxiomId::new(33),
        name: "a33_search_skips_hidden_text_unless_visible_only_is_off",
        statement: "GtkSourceSearchContext's visible-only setting (GtkSourceView 5.12+) defaults \
                    to true, and then both plain-text and regex search skip text tagged invisible; \
                    with visible-only false, the plain-text search finds matches inside hidden \
                    text while the regex search still finds only the visible ones",
        dependent_designs: &[
            "long-line-protected-view (C2): find moves to a worker and must not rely on the stock \
             search context's treatment of hidden text",
            "document-scale-policy (C1): L_search_highlight",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a33),
    },
    Axiom {
        id: AxiomId::new(34),
        name: "a34_move_cursor_without_a_visible_cursor_scrolls",
        statement: "with cursor-visible false, GtkTextView's move-cursor scrolls the viewport and \
                    leaves the insert mark where it was; with gtk-keynav-use-caret set it moves \
                    the insert mark",
        dependent_designs: &[
            "long-line-protected-view (C2): navigation keys scroll a cursor-less protected \
             document (P3 extends this axiom)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a34),
    },
    Axiom {
        id: AxiomId::new(35),
        name: "a35_atspi_text_over_invisible_text",
        statement: "AT-SPI text contents and offsets of a paragraph whose tail is invisible, and \
                    the TextChanged payloads emitted on insert, are reachable only through an \
                    AT-SPI bus (unpinned in-crate; the responsiveness lane's AT case records \
                    payload sizes and offsets)",
        dependent_designs: &[
            "long-line-protected-view (C2): the accessibility contract of a folded paragraph (P4 \
             extends this axiom)",
            "sliced-protected-install (C3): per-slice AT payloads (A78 extends this axiom)",
        ],
        slow: false,
        version_bound: None,
        probe: None,
    },
    Axiom {
        id: AxiomId::new(36),
        name: "a36_btree_segment_merge_copies_both_strings",
        statement: "the GtkTextBuffer btree merges adjacent character segments by copying both \
                    strings, so appending next to a large segment costs time proportional to the \
                    segment (cost-only; recorded by the responsiveness lane's install case)",
        dependent_designs: &[
            "sliced-protected-install (C3): the adoption gate and segment separation",
            "long-line-protected-view (C2): the install bound for giant paragraphs",
        ],
        slow: false,
        version_bound: None,
        probe: None,
    },
    Axiom {
        id: AxiomId::new(37),
        name: "a37_validation_deferred_into_the_paint_moves_adjustments",
        statement: "a change that invalidates GtkTextView lines inside a frame (an edit, a tag \
                    change, or a change of a default-style property such as a margin, whose \
                    setter invalidates every line and queues no resize, allocation or redraw; \
                    in the update phase, as input is handled in the flush-events phase) \
                    leaves the view's validation idle pending at paint time, and unless the view \
                    is allocated in that frame's layout phase, gtk_text_view_paint flushes it \
                    from inside the snapshot of any frame that redraws the view, changing the \
                    scroll adjustments after the scrollbars were allocated (GTK then may warn \
                    that it snapshots the scrollbar's trough without a current allocation); \
                    queueing the view's allocation before the paint moves every such change \
                    into layout",
        dependent_designs: &[
            "LushText editor page (ui/editor_page/frame_validation.rs): queues the view's \
             allocation on every edit, caret move and default-style property change, and \
             before the paint after a tag change",
            "document-scale-policy (C1): wide lines and large documents reach the scrollbar \
             warning through it",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a37),
    },
    Axiom {
        id: AxiomId::new(38),
        name: "a38_a_keyfile_write_rewrites_the_file_on_the_calling_thread",
        statement: "a GSettings write through the keyfile backend replaces the settings file \
                    synchronously on the calling thread (g_file_replace_contents: a temporary \
                    file renamed over it, fsynced first when the file has content), exactly once \
                    per set_* call, and again when the value written equals the stored one",
        dependent_designs: &[
            "LushText settings writer (bound-window-construction-dispatch): no backend write \
             during window construction or in an interaction dispatch, and a value equal to \
             the persisted one is dropped before it reaches GSettings",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a38),
    },
    Axiom {
        id: AxiomId::new(39),
        name: "a39_delay_and_apply_write_pending_keys_once",
        statement: "in delay() mode a GSettings instance keeps written values in memory (it \
                    reads them back, another instance on the same backend still reads the stored \
                    values, the file is untouched) until apply(), which writes every pending key \
                    in one replacement of the settings file",
        dependent_designs: &[
            "LushText settings writer (bound-window-construction-dispatch): a flush writes its \
             whole batch with one delay()/apply() pair, one backend write per flush",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a39),
    },
    Axiom {
        id: AxiomId::new(40),
        name: "a40_a_second_keyfile_backend_stays_stale_until_its_monitor_reloads",
        statement: "GKeyfileSettingsBackend takes no lock around its in-memory key file (GLib \
                    2.88 gkeyfilesettingsbackend.c, source-cited), so one backend object must \
                    not be written on one thread while another uses it; a second backend object \
                    over the same file, written on a worker thread, leaves the first answering \
                    its previous value until the first backend's file monitor delivers the \
                    replacement on the main context, which reloads the file and emits changed",
        dependent_designs: &[
            "LushText settings writer (bound-window-construction-dispatch): the flush runs on \
             the GTK thread after the first frame as one delay()/apply() batch, because a \
             worker flush through the shared backend is a data race and one through a private \
             backend leaves every other reader stale for a monitor round trip",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a40),
    },
    Axiom {
        id: AxiomId::new(41),
        name: "a41_changed_reaches_another_instance_inside_the_write",
        statement: "a keyfile-backend write emits changed on a second GSettings instance on the \
                    same backend once, synchronously inside the writer's set_* and before the \
                    settings file is rewritten, and the backend's file monitor adds none for its \
                    own write; a delay()ed instance emits changed on itself inside set_* and on \
                    no other instance, and apply() emits it on the other instances inside \
                    apply() and not again on the delayed one",
        dependent_designs: &[
            "LushText settings writer (bound-window-construction-dispatch): every LushText \
             reader and subscriber shares one delayed instance, so a write reaches all of them \
             when it is made and the flush does not re-notify them",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a41),
    },
    Axiom {
        id: AxiomId::new(42),
        name: "a42_buffer_lines_end_at_pango_paragraph_delimiters",
        statement: "text inserted whole into a GtkTextBuffer gets exactly Pango's paragraphs as \
                    lines: a new line starts after \\n, \\r, \\r\\n (one delimiter) and U+2029, \
                    not after U+2028 or U+0085, and a line's chars_in_line counts its delimiter; \
                    the line structure is history-dependent, because an insertion splits only its \
                    own text at those delimiters (never joining with or splitting around its \
                    neighbours) and a deletion joins exactly the lines whose starts it removes \
                    (a line start x with start < x <= end)",
        dependent_designs: &[
            "document-scale-policy (C1): model/paragraph_boundary.rs, the boundary rule of the \
             install slicer and the paragraph profile (fresh text), and the edit-time watch \
             (model/paragraph_profile_watch.rs, whose core and Kani model follow the edit rule) \
             and the minimap's modified-line marks (edits)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a42),
    },
    Axiom {
        id: AxiomId::new(43),
        name: "a43_chars_in_line_walks_segments_not_characters",
        statement: "gtk_text_iter_get_chars_in_line costs time in proportion to the line's \
                    segment count, not its character count: it sums the char_count of the line's \
                    segments (verified by source, gtktextiter.c; a duration is not a structural \
                    observation, so no in-crate probe)",
        dependent_designs: &[
            "document-scale-policy (C1): the edit-time watch's bounded line-length reads \
             (ui/editor_page/document_scale.rs)",
        ],
        slow: false,
        version_bound: None,
        probe: None,
    },
    Axiom {
        id: AxiomId::new(44),
        name: "a44_a_language_cleared_after_insert_text_leaves_no_tags",
        statement: "clearing a GtkSourceBuffer's language (set_language(NULL) and \
                    set_highlight_syntax(FALSE)) inside an insert-text after-handler leaves the \
                    inserted text untagged after the main loop drains: the class handler only \
                    schedules the context engine's idle, and the engine is gone before it runs",
        dependent_designs: &[
            "document-scale-policy (C1): the edit-time watch downgrades syntax in the same \
             emission as the edit that makes a paragraph too long (A29)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a44),
    },
    Axiom {
        id: AxiomId::new(45),
        name: "a45_an_unparented_widget_never_runs_its_armed_tick",
        statement: "a widget unparented (and so unrealized) while a frame-clock tick callback is \
                    armed never runs that callback in a later frame, and its disposal logs \
                    nothing; unparenting and dropping a GtkSourceMap whose update tick a scroll \
                    armed logs no CRITICAL, unlike clearing its view in place (A32)",
        dependent_designs: &[
            "document-scale-policy (C1): the minimap retire-and-rebuild \
             (ui/editor_page/minimap/retirement.rs) that replaces the A32 detach",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a45),
    },
    Axiom {
        id: AxiomId::new(46),
        name: "a46_the_first_toplevel_realize_has_a_one_time_cost",
        statement: "the first gtk_widget_realize of a toplevel in a process costs a one-time \
                    amount that later toplevels do not pay; it is neither GL initialisation nor \
                    the window icon",
        dependent_designs: &[
            "LushText window composition (first-frame-window-shell): stage R realizes the \
             window in a dispatch of its own",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a46),
    },
    Axiom {
        id: AxiomId::new(47),
        name: "a47_present_measures_nothing_and_skips_invisible_subtrees",
        statement: "gtk_window_show validates CSS only for visible nodes, realizes, presents and \
                    maps without measuring; the window's size computation and first measure run \
                    in the first frame's layout phase, and an invisible subtree is neither \
                    measured, allocated nor mapped",
        dependent_designs: &[
            "LushText window composition (first-frame-window-shell): the staged content is \
             invisible in the template, so it costs the realize, present and first-frame \
             dispatches nothing",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a47),
    },
    Axiom {
        id: AxiomId::new(48),
        name: "a48_a_late_subtree_is_measured_in_the_next_frame",
        statement: "a subtree made visible or parented in a mapped window is styled and measured \
                    in the next frame's layout phase, not at the call",
        dependent_designs: &[
            "LushText window composition (first-frame-window-shell): a reveal is a visibility \
             flip whose measure lands in the next frame, and each reveal waits for its \
             predecessor's painted frame",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a48),
    },
    Axiom {
        id: AxiomId::new(49),
        name: "a49_a_concealed_revealer_measures_a_visible_child_at_full_width",
        statement: "a concealed vertical-slide GtkRevealer measures a visible child horizontally \
                    at full scale and returns 0 for an invisible child without measuring it",
        dependent_designs: &[
            "LushText window composition (first-frame-window-shell): a concealed revealer's \
             child (search panel, command palette, Focus Mode affordance) stays invisible",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a49),
    },
    Axiom {
        id: AxiomId::new(50),
        name: "a50_a_breakpoint_matching_after_map_swaps_through_a_snapshot",
        statement: "AdwBreakpointBin's first allocation after map applies a matching breakpoint \
                    before allocating its child; a breakpoint that starts matching later, \
                    including one added after map, allocates the child with the old values, \
                    hides it behind a snapshot for one frame, and swaps on the next tick",
        dependent_designs: &[
            "LushText window composition (first-frame-window-shell): the three breakpoints are \
             installed before the window is realized, so the first frame draws their state",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a50),
    },
    Axiom {
        id: AxiomId::new(51),
        name: "a51_a_realize_frame_runs_unmapped_and_paces_the_first_visible_frame",
        statement: "realizing a toplevel schedules a frame whose GDK_PRIORITY_REDRAW paint source \
                    runs ahead of G_PRIORITY_DEFAULT_IDLE sources; on a surface not yet mapped \
                    that frame emits after-paint and is never presented, and without \
                    presentation timings the frame clock starts the next frame no earlier than \
                    one FRAME_INTERVAL (16,667 us) after it; realizing and presenting in one \
                    dispatch runs no frame on the unmapped surface",
        dependent_designs: &[
            "LushText window composition (shorten-window-content-fill-in, D1): stage R \
             realizes and presents in one dispatch, so the window's first frame is its first \
             presented frame and is not paced behind an unseen one",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a51),
    },
    Axiom {
        id: AxiomId::new(52),
        name: "a52_an_unparented_text_view_keeps_laying_out_its_buffer",
        statement: "a GtkTextView unparented (and so unrealized) but still alive stays attached \
                    to its buffer and re-lays out the paragraphs an edit invalidates, from its \
                    own idle; dropping its last reference finalizes it and logs nothing",
        dependent_designs: &[
            "explore-editing-long-paragraphs (C5, D1): a retired GtkSourceMap must be \
             finalized; the minimap's reflow freeze (gtk-lush-widgets RenderHoldOverlay) \
             therefore refers to its live child weakly",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a52),
    },
    Axiom {
        id: AxiomId::new(53),
        name: "a53_a_paragraph_wraps_by_its_first_character",
        statement: "a paragraph's wrap mode is taken from the tags on its first character: a \
                    wrap-mode-none tag on that character unwraps the whole paragraph in a \
                    word-wrapping view while its neighbour keeps wrapping, and text inserted \
                    before that character is untagged (A25), so the paragraph wraps again until \
                    re-tagged",
        dependent_designs: &[
            "explore-editing-long-paragraphs (C5, D3): paragraphs longer than L_wrap_edit are \
             unwrapped by a no-wrap tag read from their first character, re-applied in the same insert-text \
             and delete-range emission",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a53),
    },
    Axiom {
        id: AxiomId::new(54),
        name: "a54_the_view_answers_surrounding_text_from_the_paragraph_start",
        statement: "a GtkTextView answers its input method's retrieve-surrounding with text from \
                    the cursor paragraph's start (walking word boundaries, A26); its handler, on \
                    the input-method context its key controller exposes, stops the emission, so \
                    a handler connected later is never called",
        dependent_designs: &[
            "explore-editing-long-paragraphs (C5, D6): the editor blocks the view's handler and \
             answers with a bounded window, because GNOME's text-input path asks on every \
             keystroke",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a54),
    },
    Axiom {
        id: AxiomId::new(59),
        name: "a59_a_capture_claim_keeps_presses_from_the_text_view",
        statement: "a capture-phase GtkGestureClick and GtkGestureDrag (button 0) that claim \
                    every sequence on a GtkTextView keep the view's own click and drag gestures \
                    from running: no press, middle press or drag moves the insert mark or the \
                    selection bound, resets the input method, pastes the primary selection, \
                    opens the default context menu or starts a selection drag or \
                    drag-and-drop; without the claim a non-editable view still does all of \
                    these except inserting (a touchscreen drag is not driven)",
        dependent_designs: &[
            "long-line-protected-view (C2): the capture-phase press claim of Protected View \
             (design D3, step 3)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a59),
    },
    Axiom {
        id: AxiomId::new(60),
        name: "a60_a_non_editable_view_keeps_the_input_method_out",
        statement: "set_editable(false) leaves a GtkTextView's key controller with no \
                    input-method context, so key presses never reach the input method's \
                    filter_keypress, and a later focus-in does not call \
                    gtk_im_context_focus_in; an editable view does both",
        dependent_designs: &[
            "long-line-protected-view (C2): a protected (non-editable) view hands no paragraph \
             to the input method on focus or typing, without changing the input-method module \
             (design D3, step 1)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a60),
    },
    Axiom {
        id: AxiomId::new(61),
        name: "a61_a_stopped_move_cursor_holds_under_caret_browsing",
        statement: "a move-cursor handler that scrolls the view and stops the emission keeps \
                    the insert mark and the selection bound still for every step kind, with \
                    and without extend_selection, also under gtk-keynav-use-caret, where the \
                    stock handler moves the insert mark (extends A34); the real navigation \
                    keys reach the same signal",
        dependent_designs: &[
            "long-line-protected-view (C2): the move-cursor intercept of Protected View \
             (design D3, step 2)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a61),
    },
    Axiom {
        id: AxiomId::new(62),
        name: "a62_atspi_text_and_caret_writes_on_a_folded_paragraph",
        statement: "for a paragraph whose tail is invisible, the AT-SPI text contents and \
                    offsets, the payload and cost of text queries at character, word, sentence \
                    and line granularity, the TextChanged payload when the hidden tail is \
                    inserted, and the cost of an AT-initiated caret or selection write into it \
                    are reachable only through an AT-SPI bus (unpinned in-crate, like A35; the \
                    responsiveness lane's AT case records them)",
        dependent_designs: &[
            "long-line-protected-view (C2): the accessibility contract of a folded paragraph \
             and the one write path it cannot pre-empt (extends A35)",
            "sliced-protected-install (C3): per-slice AT payloads",
        ],
        slow: false,
        version_bound: None,
        probe: None,
    },
    Axiom {
        id: AxiomId::new(63),
        name: "a63_a_text_view_drop_target_and_its_removal",
        statement: "a GtkTextView carries one GtkDropTarget (string, copy or move), one \
                    GtkDropControllerMotion and no GtkDragSource (drag-out starts from its own \
                    drag gesture); a non-editable view's drop handler refuses a drop and \
                    leaves the buffer unchanged, unless a tag marks the drop point editable; \
                    removing the drop target with gtk_widget_remove_controller leaves none",
        dependent_designs: &[
            "long-line-protected-view (C2): Protected View removes the drop target and relies \
             on the capture claim (A59) for drag-out (design D3, step 4)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a63),
    },
    Axiom {
        id: AxiomId::new(64),
        name: "a64_stopped_keybinding_signals_guard_a_non_editable_view",
        statement: "GtkTextView recomputes the per-instance enablement of selection.select-all \
                    (and the other clipboard.* and selection.* actions) on menu.popup, so \
                    disabling it is not a guard, and Ctrl+A reaches the select-all signal \
                    whatever it says; a handler that stops select-all, toggle-cursor-visible, \
                    change-number, change-case, join-lines or move-to-matching-bracket keeps \
                    its default handler from running, and a capture-phase key controller that \
                    claims Shift+F10 keeps menu.popup from opening the menu; without them \
                    change-number (Ctrl+Shift+X), change-case and join-lines change a \
                    non-editable buffer",
        dependent_designs: &[
            "long-line-protected-view (C2): the key-binding pre-emption of Protected View \
             (design D3, step 5)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a64),
    },
    Axiom {
        id: AxiomId::new(65),
        name: "a65_a_tail_inserted_hidden_is_never_laid_out",
        statement: "insert_with_tags of a run tagged invisible after a visible head in the \
                    same paragraph leaves the paragraph's display height (wrapping on) and the \
                    layout width (wrapping off) exactly those of the head alone, at every tail \
                    size; the cost of the insert and of the first validation per MiB of tail is \
                    recorded",
        dependent_designs: &[
            "long-line-protected-view (C2): the fold executor inserts a giant paragraph's tail \
             hidden (design D1, D13)",
            "sliced-protected-install (C3): the per-MiB insert cost is its adoption-gate input",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a65),
    },
    Axiom {
        id: AxiomId::new(66),
        name: "a66_chunk_reads_by_offset_and_by_byte_index",
        statement: "reading a fixed 64 Ki-character chunk of a single-segment paragraph yields \
                    the exact text by every positioning; its recorded cost grows with the depth \
                    by character offset, with the paragraph's length for a walking mark, and \
                    stays flat for one iterator positioned by byte index and advanced with \
                    forward_chars below MAX_LINEAR_SCAN (cost evidence, like A36)",
        dependent_designs: &[
            "long-line-protected-view (C2): the positioning of the edit-growth source capture \
             (design D4)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a66),
    },
    Axiom {
        id: AxiomId::new(67),
        name: "a67_a_language_parse_is_shared_by_its_buffers_and_stays_on_the_gtk_thread",
        statement: "a GtkSourceView language parses its definitions at its first attach and keeps \
                    the parse on the language object while a buffer holds it (a second buffer's \
                    attach parses nothing; an attach after the last buffer released it parses \
                    again); a buffer built on a worker takes the default style-scheme manager's \
                    classic scheme and an attach adds an idle to the global default main \
                    context, so the parse cannot leave the GTK thread; a private language \
                    manager built on a worker loads its list and guesses a language, and a \
                    second manager's language highlights exactly as the default manager's",
        dependent_designs: &[
            "bound-post-open-settle (C6) D2: the language attach stays a GTK dispatch, deferred to \
             a low-priority idle after input-quiet and recorded as a toolkit-floor exception; the \
             language catalog is built on a worker",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a67),
    },
    Axiom {
        id: AxiomId::new(68),
        name: "a68_highlight_on_refreshes_the_whole_buffer_and_inserts_only_invalidate",
        statement: "with a language attached, set_highlight_syntax(true) refreshes the whole \
                    buffer inside the call, so its cost grows with the buffer; inserting into a \
                    language-attached buffer only invalidates, and the analysis runs afterwards \
                    from the engine's idles",
        dependent_designs: &[
            "bound-post-open-settle (C6) D3: the publish order keeps the engine attached through a \
             sliced install instead of paying the document-sized toggle at publish",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a68),
    },
    Axiom {
        id: AxiomId::new(69),
        name: "a69_the_context_engine_analyses_in_fixed_low_priority_idle_slices",
        statement: "GtkSourceView's context engine analyses in a 10 ms first update at \
                    G_PRIORITY_HIGH_IDLE, then a 30 ms incremental worker at G_PRIORITY_LOW that \
                    reschedules as a continuing idle (no timeout, no gap); a \
                    G_PRIORITY_DEFAULT_IDLE idle runs before its next slice, a G_PRIORITY_LOW \
                    idle runs between every two slices, and a G_PRIORITY_LOW + 1 idle waits for \
                    the end",
        dependent_designs: &[
            "bound-post-open-settle (C6) D4 and R25: background highlighting is gated by total \
             size because the slices cannot be shortened, and the settle sentinel cannot rely on \
             outranking the worker",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a69),
    },
    Axiom {
        id: AxiomId::new(70),
        name: "a70_validation_after_an_insert_lays_out_only_the_inserted_lines",
        statement: "after an insert below the viewport, GtkTextView lays out only the inserted, \
                    invalid lines, in its incremental validation idle at \
                    GTK_TEXT_VIEW_PRIORITY_VALIDATE, whose cost grows with their visible \
                    characters in either wrap mode; inserts made back to back are validated \
                    together, and when each insert waits for the after-paint of the frame after \
                    the previous one, or for an idle below the validation idle queued by it, each \
                    validation dispatch holds one insert's lines",
        dependent_designs: &[
            "bound-post-open-settle (C6) D6: a dense install is sliced at V_SLICE visible \
             characters and paced from an idle below the validation idle, so no validation \
             dispatch holds more than one slice",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a70),
    },
    Axiom {
        id: AxiomId::new(71),
        name: "a71_the_first_keyfile_write_adds_no_monitor_changed",
        statement: "the first write through a keyfile settings backend whose file does not exist \
                    yet creates the file (and its directory), emits changed on a peer instance \
                    once inside set_*, and the file's creation adds no changed on any instance \
                    of that backend",
        dependent_designs: &[
            "bound-post-open-settle (C6) D7: the dispatch after a fresh profile's first settings \
             flush is not the backend's monitor re-delivering the write",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a71),
    },
    Axiom {
        id: AxiomId::new(72),
        name: "a72_a_tag_carried_document_style_against_hide_and_reveal",
        statement: "a lowest-priority tag carrying family, relative scale and tabs lays out every \
                    non-empty line exactly as the view-wide CSS font and tabs (the empty last line \
                    keeps the view font), re-tagging a range changes only its lines in at most two \
                    validation dispatches, a GtkSourceMap's font scales with it, printing ignores \
                    it and columns follow the tag font, while GtkSourceView's tab stops and the \
                    line-number renderer follow the view font; hiding the paragraphs outside the \
                    visible window, changing the view and revealing them in ranges keeps the top \
                    line and the insert mark, while a default search and the map see only the \
                    revealed paragraphs",
        dependent_designs: &[
            "bound-post-open-settle (C6) D9: which mechanism paces a user-started restyle of a \
             dense document",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a72),
    },
    Axiom {
        id: AxiomId::new(73),
        name: "a73_a_pixels_below_lines_tag_cannot_carry_the_overscroll",
        statement: "a pixels-below-lines tag on the empty last paragraph is a no-op, on the \
                    paragraph before it puts the space above the last line, and on a non-empty \
                    last paragraph becomes part of that line's y-range (current-line highlight, \
                    gutter) and reaches a GtkSourceMap unscaled; the print compositor ignores \
                    it; a bottom-margin change revalidates the whole document",
        dependent_designs: &[
            "bound-post-open-settle (C6) D6.2: a dense document's overscroll is a margin set once \
             before the install, and later resize-driven changes are recorded against this axiom",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a73),
    },
    Axiom {
        id: AxiomId::new(74),
        name: "a74_a_gtk_enable_animations_change_restyles_every_widget",
        statement: "after GtkSettings:gtk-enable-animations changes, libadwaita reloads a style \
                    provider, so the next frame restyles every widget and costs more than twice \
                    a redraw or a gtk-cursor-blink change, growing with the number of widgets",
        dependent_designs: &[
            "bound-post-open-settle (C6) review M4a: considered as the cause of the restyle \
             the keys typed during a document's settle wait for, and ruled out (no settings \
             notify in the lane; the cause is the pointer's first entry, :hover)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a74),
    },
    Axiom {
        id: AxiomId::new(75),
        name: "a75_a_text_width_change_relays_out_every_paragraph",
        statement: "after a wrapping text view's text width changes (a left gutter growing), \
                    once the view settles, its last, offscreen paragraph sits exactly where a \
                    view built at the new width puts it, and not where it sat before: every \
                    paragraph is laid out again; the revalidation time is recorded",
        dependent_designs: &[
            "protected-view-inspector-and-surfaces (C2b review S4): the fold-marker gutter \
             column keeps a fixed width, so entering or leaving Protected View never resizes \
             the text area",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a75),
    },
    Axiom {
        id: AxiomId::new(76),
        name: "a76_a_left_gravity_mark_keeps_slices_from_merging",
        statement: "text inserted at a left-gravity mark lands after it (before a right-gravity \
                    one); text inserted at a left-gravity mark at the start of a tagged run lands \
                    before the run, untagged; appending tagged slices costs a flat time per \
                    append when a left-gravity mark follows each slice, and a growing one \
                    without (A36), and deleting the slices between marks from the start costs \
                    a flat time per range, from the end a growing one (recorded); the merge half \
                    is cost-only, like A36",
        dependent_designs: &[
            "sliced-protected-install (C3): boundary marks between hidden-tail slices (design \
             D3), the deferred head (D2) and the backwards clear (D4)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a76),
    },
    Axiom {
        id: AxiomId::new(77),
        name: "a77_a_hidden_append_relays_out_only_visible_text",
        statement: "after each hidden slice appended to a paragraph (slices separated by \
                    left-gravity marks), the view's re-layout of the paragraph costs a flat time \
                    as the hidden tail grows, with the head in place, with only its first \
                    character in place, and with the head inserted after the tail; with a \
                    visible character before the hidden run, mapping x = 0 of the line to an \
                    iterator and a GtkSourceView snapshot also cost a flat time; the costs of \
                    every order and the one-turn install's first layout per MiB are recorded",
        dependent_designs: &[
            "sliced-protected-install (C3): a hidden tail may be split across dispatches \
             without a re-layout that grows with the tail; the tail-first order (design D2)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a77),
    },
    Axiom {
        id: AxiomId::new(78),
        name: "a78_atspi_text_changed_is_bounded_by_the_changed_run",
        statement: "(extends A35) the AT-SPI TextChanged event GtkTextView emits for an insertion \
                    or a deletion carries the inserted or deleted run (the inserted slice's text \
                    before it is hidden; no hidden characters on deletion), never the rest of \
                    the paragraph (unpinned in-crate, like A35; the responsiveness lane's AT \
                    case records the payload sizes of a sliced install and clear)",
        dependent_designs: &[
            "sliced-protected-install (C3): the AT cost of each install and clear slice is \
             bounded by the slice",
        ],
        slow: false,
        version_bound: None,
        probe: None,
    },
    Axiom {
        id: AxiomId::new(79),
        name: "a79_a_point_below_the_layout_walks_the_last_lines_hidden_bytes",
        statement: "gtk_text_view_get_iter_at_location for a point below the layout, when the \
                    buffer's last line holds a hidden run, costs time growing with the hidden \
                    bytes (gtk_text_iter_set_visible_line_index stepping at the end iterator); \
                    with a delimiter after the hidden run it stays under a millisecond",
        dependent_designs: &[
            "sliced-protected-install (C3): a sliced paragraph's delimiter goes in before its \
             tail, an undelimited last paragraph gets a stand-in terminator while its tail goes \
             in, and the cost on an installed undelimited last paragraph is a recorded toolkit \
             floor",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a79),
    },
    Axiom {
        id: AxiomId::new(80),
        name: "a80_validation_holds_back_lower_priority_idles",
        statement: "while a text view has lines left to validate, its incremental validation \
                    idle is ready again after every batch, so a G_PRIORITY_DEFAULT_IDLE source \
                    does not dispatch until the whole buffer is validated, on screen or on a \
                    hidden stack page alike, while a source one step above the validation idle \
                    dispatches before the next batch",
        dependent_designs: &[
            "spawn-completion-priority: gtk-lush-tasks delivers spawn_blocking_then completions \
             from one source one step above the validation idle, one completion per dispatch",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a80),
    },
    Axiom {
        id: AxiomId::new(81),
        name: "a81_equal_priority_sources_share_one_iteration",
        statement: "GLib dispatches every ready source of the top priority in one main-loop \
                    iteration, so that iteration lasts the sum of their callbacks, while a \
                    ready source one step lower, or one of the same priority added during the \
                    dispatch, runs in a later iteration",
        dependent_designs: &[
            "spawn-completion-priority: composition stages and the language catalog build run \
             at priorities of their own so they never share a dispatch with a restore turn",
            "keep-keystrokes-during-install: the held-typing settle priority",
            "defer-background-tab-layout: several validating views lay out one batch each per \
             iteration, so a hidden validating tab lengthens every iteration of the visible one",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a81),
    },
    Axiom {
        id: AxiomId::new(82),
        name: "a82_a_hidden_tab_page_lays_out_its_text_view",
        statement: "an AdwTabView page never selected is neither mapped nor realized, yet a text \
                    view in a scrolled window on it has a layout (the scroller's adjustments \
                    create it) and validates text loaded into its buffer in full while hidden, \
                    as does a source view in no widget tree; a page selected once and hidden again stays realized and validates the same \
                    way; the first page appended to an empty tab view is selected by the tab view",
        dependent_designs: &[
            "defer-background-tab-layout: leaving a background tab unselected cannot keep its \
             editor from laying out its document, so its view is given an empty buffer while \
             hidden (A83)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a82),
    },
    Axiom {
        id: AxiomId::new(83),
        name: "a83_an_empty_buffer_stops_a_hidden_view_laying_out",
        statement: "giving a hidden text view an empty buffer stops its validation at once and \
                    leaves the document buffer's text, modified flag, undo state and caret \
                    unchanged; setting the document buffer back lays the whole buffer out again \
                    and brings the view back at the buffer start, where scrolling to a kept mark \
                    restores the old top line, but one queued right after the reattach lands \
                    elsewhere unless scrolled to again as the view lays out; an unmapped validating view notifies its vertical \
                    adjustment's upper",
        dependent_designs: &[
            "defer-background-tab-layout: a background editor's view holds an empty buffer until \
             its tab is shown, and again once it is seen laying out while hidden",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a83),
    },
    Axiom {
        id: AxiomId::new(84),
        name: "a84_highlight_updated_ends_report_how_far_the_engine_analysed",
        statement: "after an insert at the end of a buffer whose language is attached, or an \
                    attach to a buffer holding one chunk, the largest highlight-updated end \
                    reported since then reaches the end of the last line's text once the context \
                    engine has analysed to the end, and a G_PRIORITY_LOW + 1 idle queued after \
                    the change always finds it there; a chunk the worker still analyses is not \
                    reported at the end by a G_PRIORITY_DEFAULT_IDLE idle",
        dependent_designs: &[
            "chunked-highlight-attach (C6b): a highlighted install attaches the language after \
             its first chunk and inserts the rest in chunks of at most 8 KiB, each next chunk \
             only once the reported coverage reached the buffer end or after one \
             G_PRIORITY_LOW + 1 wait",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a84),
    },
    Axiom {
        id: AxiomId::new(85),
        name: "a85_an_edit_s_first_update_is_timed_before_its_worker_runs",
        statement: "after an edit in a buffer whose language is attached, an idle queued at \
                    G_PRIORITY_HIGH_IDLE before the edit runs immediately before the context \
                    engine's first update, whose highlight-updated comes from that priority, and \
                    an idle queued at G_PRIORITY_LOW - 1 after the edit runs after it and before \
                    the first worker slice; ensure_highlight on an empty range at a bound, from \
                    an idle at G_PRIORITY_HIGH_IDLE - 1, re-analyses the edit to the line after \
                    the bound in one reported range, and once a report ends before its bound the \
                    first update and the worker find nothing; removing the language between \
                    such steps, or from the G_PRIORITY_LOW - 1 idle, removes the pending first \
                    update and worker and changes no text",
        dependent_designs: &[
            "bound-context-flip-keystrokes (C6c): an edit's re-analysis runs in paced \
             ensure_highlight steps before the first update, and a session that spends the \
             first-update allowance without merging removes the language before any worker \
             slice",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a85),
    },
    Axiom {
        id: AxiomId::new(86),
        name: "a86_a_zoom_class_swap_restyles_only_the_widgets_that_carry_it",
        statement: "after gtk_css_provider_load_from_string on a provider added for the display, \
                    even with unchanged text, the next frame revalidates every widget's CSS and \
                    its cost grows with the widgets the window holds, while a load whose rules \
                    are keyed by classes no widget carries changes no computed font and no \
                    layout extent; after a CSS class is swapped on a source view and on \
                    .monospace labels whose font rules are keyed by that class, they compute the \
                    new font size, and the next frame's cost does not grow with widgets outside \
                    the set",
        dependent_designs: &[
            "scope-font-zoom-restyle: the font provider carries a rule set per zoom level keyed \
             by a class, a zoom step swaps the class on the registered font-following widgets \
             (ui/font_zoom.rs), and a base-font change loads the next generation's rules while \
             no widget carries them, then swaps the classes on a later frame",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a86),
    },
    Axiom {
        id: AxiomId::new(87),
        name: "a87_a_presented_list_view_pays_per_items_changed_not_per_item",
        statement: "on a presented, visible GtkListView every items-changed of its model costs \
                    a fixed amount that does not grow with the item count, so single appends \
                    cost several times one splice of the same items; an unrealized list \
                    (unparented, or under an invisible GtkScrolledWindow) does not pay per item \
                    on an append either",
        dependent_designs: &[
            "bound-lists-by-budget-not-count D4: workspace search applies a turn's matches as \
             one splice per touched store, so the results list pays the per-emission cost once \
             per touched file per turn, not once per match",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a87),
    },
    Axiom {
        id: AxiomId::new(88),
        name: "a88_an_end_of_store_batch_splice_costs_the_batch",
        statement: "one splice appending a batch at the end of an expanded row's child \
                    GListStore, under a GtkTreeListModel (passthrough = false) shown by a \
                    presented GtkListView, costs a small fraction of a list turn and grows \
                    sub-linearly, far below the rows already in the store (16,384 against \
                    262,144)",
        dependent_designs: &[
            "bound-lists-by-budget-not-count D6: the workspace sidebar loads a directory of any \
             size in 256-row batch splices within LIST_TURN_SLICE_US",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a88),
    },
    Axiom {
        id: AxiomId::new(89),
        name: "a89_expanding_the_last_tree_row_costs_its_children",
        statement: "TreeListRow::set_expanded(true) on the last row of a flattened \
                    GtkTreeListModel shown by a presented list costs a few tens of microseconds \
                    and grows sub-linearly, far below the row's position (1,024 against 16,384 \
                    rows); extends A22",
        dependent_designs: &[
            "bound-lists-by-budget-not-count D4: workspace search expands the new tail file rows \
             of a turn by position, without walking the flattened model",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a89),
    },
    Axiom {
        id: AxiomId::new(90),
        name: "a90_revealing_a_list_costs_its_realized_rows",
        statement: "making the invisible GtkScrolledWindow around a GtkListView visible in a \
                    presented window maps the list at a cost bounded by the rows it realizes \
                    (A1), not by the model's rows (256 against 16,384)",
        dependent_designs: &[
            "bound-lists-by-budget-not-count D4: the workspace search results list is revealed \
             in the turn its first matches arrive, whatever the model already holds",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a90),
    },
    Axiom {
        id: AxiomId::new(91),
        name: "a91_collapsing_a_tree_row_costs_its_children",
        statement: "TreeListRow::set_expanded(false) on a row expanded over a large child \
                    store, shown by a presented GtkListView, removes every child row in one \
                    synchronous call whose cost grows with the children (16,384 against \
                    131,072), unlike expanding it (A89) or appending a batch (A88)",
        dependent_designs: &[
            "bound-lists-by-budget-not-count: collapsing a sidebar directory of tens of \
             thousands of entries is one dispatch over B_SETTLE_US, recorded as a measured \
             toolkit-floor exception",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a91),
    },
    Axiom {
        id: AxiomId::new(92),
        name: "a92_an_adw_sidebar_builds_a_row_per_item",
        statement: "AdwSidebar in sidebar mode builds one GtkListBoxRow per item, off-screen \
                    items included, so repopulating it (remove_all, then N items) costs in \
                    proportion to N (512 against 16,384)",
        dependent_designs: &[
            "bound-notes-browser-by-budget D1: the notes browser replaces its AdwSidebar with a \
             virtualized GtkListView with section headers",
        ],
        slow: true,
        version_bound: None,
        probe: Some(probes::probe_a92),
    },
    Axiom {
        id: AxiomId::new(93),
        name: "a93_a_lazy_section_model_publishes_by_remove_then_add",
        statement: "on a presented GtkListView with a header-factory over a GtkSingleSelection \
                    over a lazy GListModel + GtkSectionModel, one items-changed(0, old, new) \
                    replacing realized rows asks get_item for every added item, while \
                    items-changed(0, old, 0) then items-changed(0, 0, new) asks only around \
                    the realized rows and costs the same at 512 and 131,072 items; a fresh \
                    object per get_item is accepted",
        dependent_designs: &[
            "bound-notes-browser-by-budget D2: the notes browser's NotesBrowserRowModel creates \
             rows on demand and keeps no row-object cache",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a93),
    },
    Axiom {
        id: AxiomId::new(94),
        name: "a94_section_headers_are_not_focus_stops",
        statement: "section headers from a GtkListView header-factory are not focusable, and \
                    moving focus down from a section's last row focuses the next section's first \
                    row; the header widget's accessible role is recorded",
        dependent_designs: &[
            "bound-notes-browser-by-budget D9: each notes-browser row's accessible description \
             carries its section name",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a94),
    },
    Axiom {
        id: AxiomId::new(95),
        name: "a95_scrolling_to_a_far_row_realizes_only_around_it",
        statement: "on the A93 list, set_selected(p) plus scroll_to(p) for a far row realizes \
                    only around p: its get_item calls stay bounded (16,383 against 131,071); \
                    its time grows with the row count and is recorded",
        dependent_designs: &[
            "bound-notes-browser-by-budget D2: the selection restore and the last row are \
             reached by position at any row count",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a95),
    },
    Axiom {
        id: AxiomId::new(96),
        name: "a96_a_natural_height_lazy_list_measures_around_its_realized_rows",
        statement: "a GtkListView over a lazy GListModel in a GtkScrolledWindow with \
                    propagate-natural-height and max-content-height presents and publishes \
                    (remove then add) asking get_item only around its realized rows, flat in \
                    the item count (512 against 131,072)",
        dependent_designs: &[
            "drill-down-palette-categories D2: the command palette's category view keeps the \
             grouped scroller's natural-height sizing over its lazy PaletteCategoryModel",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a96),
    },
    Axiom {
        id: AxiomId::new(97),
        name: "a97_search_entry_escape_and_backspace_repeat",
        statement: "GtkSearchEntry emits stop-search on Escape with or without text and keeps \
                    the text; a capture-phase key controller on it sees Backspace before the \
                    entry handles it, on an empty entry, and its release, and a held Backspace \
                    arrives as several presses with no release before one release",
        dependent_designs: &[
            "drill-down-palette-categories D5: Escape in the palette entry leaves a category \
             view through stop-search; Backspace leaves only after a release on the empty query",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a97),
    },
    Axiom {
        id: AxiomId::new(98),
        name: "a98_a_property_less_text_tag_is_bookkeeping_only",
        statement: "a GtkTextTag with no properties costs a B-tree walk to apply, queues no \
                    relayout, creates no undo entry, moves with its text, is not carried by a \
                    paste into another tag table, triggers no GtkSourceView re-highlight, and \
                    leaves an edit signal's iterators valid when applied from an after-handler",
        dependent_designs: &[
            "bin-minimap-markers-per-row D4: modified-since-save lines are one property-less \
             tag, applied from the buffer's edit after-handlers and retired in slices",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a98),
    },
    Axiom {
        id: AxiomId::new(99),
        name: "a99_source_map_line_at_y_is_monotone_and_skips_hidden_lines",
        statement: "gtk_text_view_get_line_at_y on a presented GtkSourceMap is monotone in y, \
                    never returns a line hidden by an invisible tag, and costs about the same \
                    near the end of 262,144 lines as of 1,024",
        dependent_designs: &[
            "bin-minimap-markers-per-row D2: the marker strip's row table reads one line per \
             pixel row and groups rows showing the same line, hidden lines included",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a99),
    },
    Axiom {
        id: AxiomId::new(100),
        name: "a100_search_forward_is_cheap_only_inside_a_completed_scan",
        statement: "GtkSourceSearchContext forward costs microseconds inside a completed scan \
                    and scans synchronously to the match before it, skips hidden text by \
                    default, and occurrences-count reads -1 after an edit until its rescan \
                    ends, notifying when it does",
        dependent_designs: &[
            "bin-minimap-markers-per-row D5: search markers come from a completed scan and the \
             strip re-arms on notify::occurrences-count",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a100),
    },
    Axiom {
        id: AxiomId::new(101),
        name: "a101_an_overlay_sibling_is_not_redrawn_when_the_source_map_scrolls",
        statement: "a GtkDrawingArea overlaid on a GtkSourceMap is not redrawn when the map \
                    scrolls with its view; only its own queue_draw redraws it",
        dependent_designs: &[
            "bin-minimap-markers-per-row D7: the marker strip redraws on the map \
             vadjustment's value-changed",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a101),
    },
    Axiom {
        id: AxiomId::new(102),
        name: "a102_search_forward_async_never_blocks_a_dispatch",
        statement: "GtkSourceSearchContext forward_async on a context that has not scanned \
                    scans across main-loop turns, never blocking a dispatch past 8 ms, and a \
                    walk that starts near its match calls back sooner; a pending request \
                    superseded by a query change never calls back and GIO logs a CRITICAL",
        dependent_designs: &[
            "bin-minimap-markers-per-row D5: interim search markers come from a private copy \
             of the band's text, not forward_async, which a query change can supersede",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a102),
    },
    Axiom {
        id: AxiomId::new(103),
        name: "a103_source_map_skips_an_unchanged_slider",
        statement: "On GtkSourceView >= 5.22 a GtkSourceMap does not reallocate its slider while \
                    the slider rectangle is unchanged, so a resize of the slider (a CSS change \
                    of its margins, such as a class on the map that a rule uses) leaves it \
                    without a current allocation when the map is drawn (Gtk-WARNING Trying to \
                    snapshot GtkSourceMapSlider without a current allocation) until the \
                    rectangle next changes; on GtkSourceView < 5.22 the map allocates its \
                    slider on every size_allocate and the same resize draws cleanly",
        dependent_designs: &[
            "adopt-gnome-51 D7 (W1): the minimap applies its one-pixel wide-editor slider \
             correction class only below GtkSourceView 5.22 \
             (ui/editor_page/minimap/projection_execution.rs)",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::sourceview(5, 22, 0)),
        probe: Some(probes::probe_a103),
    },
    Axiom {
        id: AxiomId::new(104),
        name: "a104_list_view_is_height_for_width",
        statement: "On GTK >= 4.24 a vertical GtkListView is height-for-width, so a container \
                    taking its request mode from it is measured for a height and, forwarding \
                    it, measures the list's width for a height below the list's minimum (its \
                    whole content outside a scroller, A2), which logs Gtk-WARNING Trying to \
                    measure GtkListView for height of H, but it needs at least N; on GTK < 4.24 \
                    the list is constant-size and such a container is never measured for a \
                    height. A width measured directly for such a height warns on both",
        dependent_designs: &[
            "adopt-gnome-51 D7 (W2): ViewportSliceBin measures its child's width for no \
             height, since the child is only allocated the visible band",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::gtk(4, 24, 0)),
        probe: Some(probes::probe_a104),
    },
    Axiom {
        id: AxiomId::new(105),
        name: "a105_frame_time_is_grid_aligned",
        statement: "On GTK < 4.24 a frame's frame time is read when the frame starts; on GTK \
                    >= 4.24 the reworked frame clock aligns it to its refresh grid, up to about \
                    one refresh interval before or after the moment the frame began, so a frame \
                    dispatched right after an input can carry a frame time earlier than it",
        dependent_designs: &[
            "adopt-gnome-51 L1: the responsiveness probe records when each frame began \
             (began_us) and the lane driver attributes input to frames by it \
             (frame_start_us), not by the frame time",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::gtk(4, 24, 0)),
        probe: Some(probes::probe_a105),
    },
    Axiom {
        id: AxiomId::new(106),
        name: "a106_source_map_slider_follows_content_heights",
        statement: "On GtkSourceView >= 5.22 a GtkSourceMap places its slider from the two \
                    vertical adjustments and the two views' content heights, margins included, \
                    so the slider covers the map's drawing of the visible text only while the \
                    map's margins are the view's scaled by the map's text scale (a view bottom \
                    margin copied unscaled misplaces it); on GtkSourceView < 5.22 the slider \
                    follows the end iter's line, and a bottom margin, copied or scaled, does not \
                    move it",
        dependent_designs: &[
            "adopt-gnome-51 (visual geometry minimap-sidebar-mid): from GtkSourceView 5.22 the \
             minimap gives its map the editor's overscroll bottom margin scaled by the map's \
             text scale, not copied (ui/editor_page/minimap/projection_execution.rs, policy \
             scaled_source_map_bottom_margin)",
        ],
        slow: false,
        version_bound: Some(crate::version::VersionBound::sourceview(5, 22, 0)),
        probe: Some(probes::probe_a106),
    },
    Axiom {
        id: AxiomId::new(107),
        name: "a107_a_visual_cursor_step_is_quadratic_in_the_line",
        statement: "A GtkTextView move-cursor step of GTK_MOVEMENT_VISUAL_POSITIONS (Left and \
                    Right) goes through pango_layout_move_cursor_visually, which costs time \
                    super-linear (quadratic) in the display line's length, about a second on a \
                    10,000-character unwrapped line; a GTK_MOVEMENT_LOGICAL_POSITIONS step costs \
                    no layout and, on left-to-right text, lands on the same iter, a collapsing \
                    selection included; inside a right-to-left run the two land apart",
        dependent_designs: &[
            "adopt-gnome-51 L2: the editor answers a visual step with a logical one when the \
             cursor's paragraph holds no right-to-left text \
             (ui/editor_page/cursor_steps.rs)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a107),
    },
    Axiom {
        id: AxiomId::new(108),
        name: "a108_a_label_list_relayouts_its_realized_rows_on_a_retext",
        statement: "After the text of every realized GtkListView row changes, the next frame \
                    re-measures every realized row whose text is a GtkLabel (each shapes its \
                    text to report its size): more than half the frame is layout at the about \
                    205 realized rows (A1), and the layout grows from 16 rows to 205; with a \
                    text widget whose measure ignores its text the frame has little layout and \
                    does not grow",
        dependent_designs: &[
            "lighten-notes-browser-rows D2: the notes browser and palette rows draw their \
             text with LushtextRowText, so a query frame redraws the rows instead of laying \
             every one out (ui/row_text/)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a108),
    },
    Axiom {
        id: AxiomId::new(109),
        name: "a109_an_inscription_costs_several_labels_to_construct",
        statement: "Constructing a GtkInscription costs several times constructing a GtkLabel: \
                    gtk_inscription_init creates its Pango layout at once, computing a style \
                    for a CSS node with no parent yet, which the style cache cannot share",
        dependent_designs: &[
            "lighten-notes-browser-rows D2: GtkInscription is rejected for list-row text \
             (about 15 ms more per 205 realized rows in the dispatch that creates them); \
             LushtextRowText creates its layout on its first snapshot",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a109),
    },
    Axiom {
        id: AxiomId::new(110),
        name: "a110_removing_unrealized_rows_tears_nothing_down",
        statement: "On a GtkListView that is not mapped, removing the rows GTK never realized, \
                    after and before the realized band, destroys and creates no row widget; \
                    removing realized rows from the end of what is left destroys only those \
                    rows, at a cost in proportion to them, and creates none",
        dependent_designs: &[
            "lighten-notes-browser-rows D4: a closed notes browser's and a left palette \
             category's rows are retired in slices outside the input dispatch, the \
             unrealized rows first (ui/list_retirement.rs)",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a110),
    },
    Axiom {
        id: AxiomId::new(111),
        name: "a111_cairo_paints_a_dialog_animation_frame_in_milliseconds",
        statement: "Under the cairo renderer, an AdwDialog animation frame (the dialog drawn as \
                    one scaled, faded node) costs several milliseconds of paint, about as much \
                    with 8 list rows as with 205; the GL renderer draws the same node in a \
                    fraction of that",
        dependent_designs: &[
            "lighten-notes-browser-rows D1/D6: the widget harness runs GSK_RENDERER=cairo, so \
             its dialog animation frames are judged by their layout phase, never by their \
             paint",
        ],
        slow: false,
        version_bound: None,
        probe: Some(probes::probe_a111),
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
    fn the_catalogue_covers_every_ledger_id_up_to_a74() {
        // A55-A58 were C6's draft ids, renumbered to A67-A73 when C5's
        // A52-A54 landed; ids are never reused.
        for number in (1..=54).chain(59..=74) {
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
    fn unprobed_entries_are_never_slow() {
        for axiom in catalogue() {
            assert!(
                !axiom.slow || axiom.probe.is_some(),
                "{} is slow without a probe",
                axiom.id
            );
        }
    }

    #[test]
    fn a_slow_skip_names_the_axiom() {
        let axiom = find(AxiomId::new(29)).expect("A29 is catalogued");
        assert_eq!(slow_skip_line(axiom), "skipped (slow): A29");
    }

    #[test]
    fn ids_display_with_their_ledger_prefix() {
        assert_eq!(AxiomId::new(5).to_string(), "A5");
        assert_eq!(AxiomId::new(13).to_string(), "A13");
    }
}
