# GTK Axiom Ledger

The normative list of GTK, `GtkListBase`, `GtkScrollable`, and Adwaita
behaviours that LushText and GTK Lush geometry designs depend on. Most geometry
bugs in 2026 came from a wrong belief about one of these, not from code missing
its own spec (see `docs/next/formal-verification.md` §1), so each belief is
written down here once, with a stable id, the designs that rely on it, and how
it is pinned against real GTK.

Rules:

- **A design that starts relying on a GTK behaviour not listed here adds an
  entry in the same change.** Ids are stable and never reused.
- **Every axiom is pinned by an isolated headless probe** in
  `crates/lushtext/tests/widget/gtk_axioms.rs` (a minimal pure-GTK fixture, no
  LushText or GTK Lush widget), or its row records why it cannot be pinned.
  "Indirectly" means a consumer test would fail if the axiom broke, but no
  probe isolates it.
- **A probe that fails after a toolkit update is an axiom change**, reviewed as
  such: revisit the entry, every dependent design, and every verification
  envelope that cites the id before the failure is resolved. Do not adjust the
  probe to match the new behaviour first.
- **Verification envelopes derive from this ledger.** A model that treats GTK
  as nondeterministic (the Kani slice-bin loop model in
  `crates/gtk-lush/widgets/src/kani_proofs.rs`) restricts GTK only with
  `kani::assume` clauses that cite an id below; an envelope narrower than the
  pinned statement is recorded as an explicit assumption in the model and in
  the programme record.

Measured against GTK 4.22 / Libadwaita 1.9 (the GNOME 50 platform floor).

| Id | Axiom | Dependent designs | Pinned by |
|---|---|---|---|
| A1 | `GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200) + 2 row widgets per visible range, and "visible" means the range its **own** vadjustment describes | `ViewportSliceBin` exists because of it; the sidebar file tree | yes: `workspace_tree_virtualization::test_thousand_rows_keep_realized_widgets_bounded_and_range_exact`, `test_boundary_row_counts_around_the_gtk_cap_render_last_row`, `test_large_directory_renders_last_row_after_scroll` |
| A2 | a list outside a scroller reports its whole content as its minimum height | `ViewportSliceBin::measure` (full content to the outer scroller) | indirectly: every rendered-bounds slice-bin test (`gtk_lush_adoption::test_adoption_slice_bin_*`) measures the outer range from it |
| A3 | `GtkViewport` allocates a non-scrollable child its **minimum** in the scroll direction | `ViewportSliceBin::measure` reports the full content as minimum while an outer scroller exists | indirectly: `workspace_tree_virtualization::test_large_directory_renders_last_row_after_scroll` (a smaller minimum truncates the outer range) |
| A4 | the list applies `scroll_to` and focus scrolling inside its **own** allocation, as a write to its vadjustment | `classify_child_scroll` reads the child's value after `child.allocate` | positive controls: `gtk_lush_adoption::test_adoption_slice_bin_still_forwards_a_child_scroll_request`, `workspace_tree_virtualization::test_focus_traversal_to_the_last_row_keeps_it_in_the_outer_viewport`; and the control half of the A9 probe |
| A5 | a **zero-height** allocation makes the list rewrite the host's adjustment — page to 0 and the value re-derived from its anchor — with a `value-changed`, although nothing asked it to scroll | `viewport_slice` never shrinks the band at the bottom edge | **probe**: `gtk_axioms::test_axiom_a5_zero_height_allocation_rewrites_the_hosts_adjustment` (value 1200 → 34, page 300 → 0) |
| A6 | a `GtkScrollable` works in its CSS content box, so `page = allocation − inset` | `ViewportSliceBin::publish_slice_offset` publishes content-box `upper`/`page`; inset learning | indirectly: `gtk_lush_adoption::test_adoption_padded_slice_bin_*` |
| A7 | the child re-derives its value from its scroll anchor when `page` or `upper` change | the settle bound (`reconfigure_shift`) in `classify_child_scroll` | indirectly: `gtk_lush_adoption::test_adoption_padded_slice_bin_keeps_rows_still_across_selection_*` |
| A8 | a settle is bounded by the geometry correction that caused it, and does not survive into a stable-geometry frame | `ChildScrollDecision::Defer`; the learning-frame residual | **evidence**: formal-verification phase 0 instrumented all 48 slice-bin widget tests — real consumers produced 39 requests, 0 settles, and no `Defer`; the only `Defer` came from the synthetic `GtkScrollable` in `gtk_lush_adoption::test_adoption_slice_bin_honours_a_request_made_while_the_child_reconfigures`. **Not isolable by a probe**: the bound is a statement about every possible child, and `GtkListView` never produced a settle on demand, so no minimal fixture can exhibit its maximum |
| A9 | `GtkListBase` drops a pending `scroll_to` on **any** `value-changed` of its adjustment | a honoured request must land where the re-slice republishes exactly the child's value (`child_value − viewport_top`); exact landing is Kani-proved on whole pixels | **probe**: `gtk_axioms::test_axiom_a9_value_changed_drops_a_pending_scroll_to` (control reaches row 200; a 10px nudge first leaves the list at the nudge) |
| A10 | rows around focus and selection stay realized but unmapped when off screen | rendered-row tests count **mapped** rows only | by probe discipline: `common::mapped_list_rows` filters on `is_mapped`, and every rendered-bounds assertion goes through it. **Not pinned** separately: which rows GTK keeps realized is an internal tracker policy, not a contract a design relies on beyond "mapped ⇒ drawn" |
| A11 | `Adjustment::configure` and `set_value` clamp into `[lower, upper − page_size]`, and a `set_value` that leaves the value unchanged emits nothing | the bin reads its published offset back after `configure`; an unhonourable outer request ends quietly (the loop guard in `follow_child_request`) | **probe**: `gtk_axioms::test_axiom_a11_adjustments_clamp_and_skip_unchanged_values` |
| A12 | the child's natural height is independent of the outer scroll position | the outer range stays exact while scrolling | **not pinned: only approximately true.** `GtkListView` estimates unrealized rows from realized ones, so its natural height drifts as different rows realize. Designs must tolerate the drift (A7's settle bound does); nothing may rely on exact independence |
| A13 | moving the outer adjustment from **inside** layout does not reliably schedule a relayout: a widget that re-slices on `value-changed` loses its own `queue_allocate` issued while it is being allocated | `ViewportSliceBin::follow_child_request` applies the outer move from an idle | **probe**: `gtk_axioms::test_axiom_a13_an_in_layout_scroll_does_not_schedule_a_relayout` (the same move from an idle re-allocates; from inside `size_allocate` it does not) |

## Envelope use

The Kani closed-loop model of the slice bin
(`crates/gtk-lush/widgets/src/kani_proofs.rs`, `slice_loop`) cites these ids in
its `kani::assume` clauses: A4 for where a request appears, A7/A8 for how far a
settle may move the child, A9 for why landing must be exact, A11 for outer
clamping, and A13 for why the outer move happens after the allocation.
