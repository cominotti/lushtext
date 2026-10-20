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
        probe: Some(probes::probe_a01),
    },
    Axiom {
        id: AxiomId::new(2),
        name: "a02_unscrolled_list_minimum_is_its_content",
        statement: "a list outside a scroller reports its whole content as its minimum height",
        dependent_designs: &["ViewportSliceBin::measure (full content to the outer scroller)"],
        slow: false,
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
        probe: Some(probes::probe_a03),
    },
    Axiom {
        id: AxiomId::new(4),
        name: "a04_scroll_to_applies_inside_allocation",
        statement: "the list applies scroll_to and focus scrolling inside its own allocation, \
                    as a write to its vadjustment",
        dependent_designs: &["classify_child_scroll reads the child's value after child.allocate"],
        slow: false,
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
        slow: false,
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
        probe: Some(probes::probe_a11),
    },
    Axiom {
        id: AxiomId::new(12),
        name: "a12_natural_height_ignores_scroll_position",
        statement: "the child's natural height is independent of the outer scroll position",
        dependent_designs: &["the outer range stays exact while scrolling"],
        slow: false,
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
        slow: false,
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
        probe: Some(probes::probe_a31),
    },
    Axiom {
        id: AxiomId::new(32),
        name: "a32_detaching_a_source_map_with_a_pending_tick_is_critical",
        statement: "detaching a GtkSourceMap from its view (gtk_source_map_set_view(NULL)) while \
                    its update tick is pending makes the tick run against no view and log Gtk and \
                    GLib-GObject CRITICALs; a scroll queues the tick (a defect axiom: holds while \
                    the defect is present)",
        dependent_designs: &[
            "document-scale-policy (C1): the minimap retire-and-rebuild that removes the detach \
             CRITICAL",
        ],
        slow: false,
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
            "sliced-protected-install (C3): per-slice AT payloads (S3 extends this axiom)",
        ],
        slow: false,
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
        probe: Some(probes::probe_a45),
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
    fn the_catalogue_covers_every_ledger_id_up_to_a45() {
        for number in 1..=45 {
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
