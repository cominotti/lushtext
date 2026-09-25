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
- **Every axiom is pinned by an isolated headless probe** in the GTK Lush
  family crate `gtk-lush-axioms` (`crates/gtk-lush/axioms/`: a minimal
  pure-GTK fixture, no LushText or GTK Lush widget), or its row records why it
  cannot be pinned. Each probe has one runnable sample,
  `crates/gtk-lush/axioms/examples/<name>.rs`, that builds the same fixture:
  interactive by default for a person to watch
  (`make gtk-axiom-sample AXIOM=<id>`), and a headless verdict with `--check`
  (`CHECK=1`). `make gtk-axioms` runs every probe and every sample's `--check`
  under a private `mutter --headless`, never the live desktop; the probe
  runner is the `axiom_probes` test of `gtk-lush-adoption-lab`. "Indirectly"
  means a consumer test would fail if the axiom broke, but no probe isolates
  it. LushText's consumer tests stay beside every probe: the probe says what
  GTK does, the consumer test says LushText, under its own CSS and
  application, stays inside those conditions.
- **The ledger, the catalogue, the probes and the samples agree.**
  `make check-gtk-axioms` (in `make check-policy`) fails when an id is in one
  and not the other, a probed entry lacks its sample or names a probe for
  another id (the build already rejects a missing probe function), a sample
  names an unknown axiom, a row claims a probe the catalogue lacks (or
  omits one it has), or a "Pinned by" cell names the retired LushText-hosted
  probes. Adding an axiom: see the crate README.
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

"Verified against" is copied from the `gtk`/`adw` fields of the observation
lines `make gtk-axioms` prints, never assumed, and names where it ran, because
the host and the CI container can carry different micro versions. The GNOME 50
platform floor is GTK 4.22 / Libadwaita 1.9.

| Id | Axiom | Dependent designs | Pinned by | Verified against | Sample |
|---|---|---|---|---|---|
| A1 | `GtkListView` realizes at most about `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200) row widgets per visible range — the cap plus a few tracker extras, **205 measured**, both at rest and in a 10 000 px viewport that needs 294 rows — and "visible" means the range its **own** vadjustment describes. (Recorded 2026-09-24, not asserted: the **first** host `set_value` past the realized rows is not followed — the list keeps its old anchor, re-derives the value within a pixel, and maps **no** row until the next `value-changed`.) | `ViewportSliceBin` exists because of it; the sidebar file tree | **probe**: `gtk_lush_axioms::probe_a01` (205 realized at 300 px and at 10 000 px; after a second move rows 300+ are mapped). Consumer tests: `workspace_tree_virtualization::test_thousand_rows_keep_realized_widgets_bounded_and_range_exact`, `test_boundary_row_counts_around_the_gtk_cap_render_last_row`, `test_large_directory_renders_last_row_after_scroll` | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a01_realized_rows_are_capped.rs` |
| A2 | a list outside a scroller reports its whole content as its minimum height | `ViewportSliceBin::measure` (full content to the outer scroller) | **probe**: `gtk_lush_axioms::probe_a02` (a 400-row list allocated 300 px measures minimum = natural = 13 600, its whole content). Consumer tests: every rendered-bounds slice-bin test (`gtk_lush_adoption::test_adoption_slice_bin_*`) measures the outer range from it | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a02_unscrolled_list_minimum_is_its_content.rs` |
| A3 | `GtkViewport` allocates a non-scrollable child its **minimum** in the scroll direction | `ViewportSliceBin::measure` reports the full content as minimum while an outer scroller exists | **probe**: `gtk_lush_axioms::probe_a03` (default `vscroll-policy` is `minimum`; a child asking 2000 minimum / 5000 natural gets 2000 and the scroller range is 2000; the control under `natural` gets 5000). Consumer test: `workspace_tree_virtualization::test_large_directory_renders_last_row_after_scroll` (a smaller minimum truncates the outer range) | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a03_viewport_allocates_the_minimum.rs` |
| A4 | the list applies `scroll_to` and focus scrolling inside its **own** allocation, as a write to its vadjustment | `classify_child_scroll` reads the child's value after `child.allocate` | **probe**: `gtk_lush_axioms::probe_a04` (nothing moves at the call; the one `value-changed` from `scroll_to(200)` and the one from focusing row 20 both fire inside the host's `child.allocate`). Consumer positive controls: `gtk_lush_adoption::test_adoption_slice_bin_still_forwards_a_child_scroll_request`, `workspace_tree_virtualization::test_focus_traversal_to_the_last_row_keeps_it_in_the_outer_viewport` | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a04_scroll_to_applies_inside_allocation.rs` |
| A5 | a **zero-height** allocation makes the list rewrite the host's adjustment — page to 0 and the value re-derived from its anchor — with a `value-changed`, although nothing asked it to scroll | `viewport_slice` never shrinks the band at the bottom edge | **probe**: `gtk_lush_axioms::probe_a05` (value 1200 → 34, page 300 → 0, one `value-changed`; identical to the retired in-app probe on the same toolkit, 2026-09-24) | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a05_zero_height_rewrites_the_adjustment.rs` |
| A6 | a `GtkScrollable` works in its CSS content box, so `page = allocation − inset` | `ViewportSliceBin::publish_slice_offset` publishes content-box `upper`/`page`; inset learning | **probe**: `gtk_lush_axioms::probe_a06` (7 px + 5 px CSS padding turns a 300 px allocation into a 288 page, equal to the list's content-box height; unpadded control 300). Consumer tests: `gtk_lush_adoption::test_adoption_padded_slice_bin_*` | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a06_scrollable_works_in_its_content_box.rs` |
| A7 | the child re-derives its value from its scroll anchor when `page` or `upper` change: `value = anchor_position − align × page`, a straight line in the page height, so a page change moves the value although nobody wrote it. The alignment is **not** confined to `[0, 1]` | the settle bound (`reconfigure_shift`) in `classify_child_scroll` | **probe**: `gtk_lush_axioms::probe_a07`, page changes only (after a host `set_value(1360)` the values at pages 300/250/200/150 are 1360/1139/918/697, one line with align −4.42; after `scroll_to(200)` the anchor is row 200's top at align 0.887, so a 100 px shrink moves the value 6534 → 6622). `upper` changes are not probed. Consumer tests: `gtk_lush_adoption::test_adoption_padded_slice_bin_keeps_rows_still_across_selection_*` | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a07_value_is_rederived_from_the_anchor.rs` |
| A8 | a settle is bounded by the geometry correction that caused it, and does not survive into a stable-geometry frame | `ChildScrollDecision::Defer`; the learning-frame residual | **evidence**: formal-verification phase 0 instrumented all 48 slice-bin widget tests — real consumers produced 39 requests, 0 settles, and no `Defer`; the only `Defer` came from the synthetic `GtkScrollable` in `gtk_lush_adoption::test_adoption_slice_bin_honours_a_request_made_while_the_child_reconfigures`. **Not isolable by a probe**: the bound is a statement about every possible child, and `GtkListView` never produced a settle on demand, so no minimal fixture can exhibit its maximum. **Counter-evidence, 2026-09-24:** the A7 probe shows `GtkListView` itself moving its value 4.42 px per pixel of page change (442 px for a 100 px correction) when its anchor alignment lies outside `[0, 1]`, which a host `set_value` produces. So "bounded by the correction" is **not** a GTK guarantee for an arbitrary anchor; it stays an envelope assumption, and `extend-closed-loop-geometry-verification` must revisit it | GTK 4.22 / Adw 1.9 (phase-0 instrumentation, 2026-09); A7 counter-evidence GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24) | — not isolable: the bound quantifies over every child |
| A9 | `GtkListBase` drops a pending `scroll_to` on **any** `value-changed` of its adjustment | a honoured request must land where the re-slice republishes exactly the child's value (`child_value − viewport_top`); exact landing is Kani-proved on whole pixels | **probe**: `gtk_lush_axioms::probe_a09` (control reaches row 200 at value 6534; a 10px nudge first leaves the list at 10; identical to the retired in-app probe, 2026-09-24) | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a09_value_changed_drops_a_pending_scroll_to.rs` |
| A10 | rows around focus and selection stay realized but unmapped when off screen | rendered-row tests count **mapped** rows only | by probe discipline: `common::mapped_list_rows` filters on `is_mapped`, and every rendered-bounds assertion goes through it. **Not pinned** separately: which rows GTK keeps realized is an internal tracker policy, not a contract a design relies on beyond "mapped ⇒ drawn". (The A1 probe sees it in passing: scrolled to row 300 the list keeps 206 rows realized, row 0 among them, beside the 11 it maps.) | GTK 4.22.5 / Adw 1.9.3 (observed by the A1 probe, Fedora 44 toolbox host, 2026-09-24) | — probe discipline, not a toolkit contract |
| A11 | `Adjustment::configure` and `set_value` clamp into `[lower, upper − page_size]`, and a `set_value` that leaves the value unchanged emits nothing | the bin reads its published offset back after `configure`; an unhonourable outer request ends quietly (the loop guard in `follow_child_request`) | **probe**: `gtk_lush_axioms::probe_a11` (500 → 200, −5 → 0, 1000 → 200; no emission for unchanged values, one for a change) | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a11_adjustments_clamp_and_skip_unchanged_values.rs` |
| A12 | the child's natural height is independent of the outer scroll position | the outer range stays exact while scrolling | **not pinned: only approximately true.** `GtkListView` estimates unrealized rows from realized ones, so its natural height drifts as different rows realize. Designs must tolerate the drift (A7's settle bound does); nothing may rely on exact independence | GTK 4.22 / Adw 1.9 (observed, no probe) | — only approximately true, so nothing to pin |
| A13 | moving the outer adjustment from **inside** layout does not reliably schedule a relayout: a widget that re-slices on `value-changed` loses its own `queue_allocate` issued while it is being allocated | `ViewportSliceBin::follow_child_request` applies the outer move from an idle | **probe**: `gtk_lush_axioms::probe_a13` (the same move from an idle re-allocates, 1 → 2; from inside `size_allocate` it does not, 3 → 3; identical to the retired in-app probe, 2026-09-24) | GTK 4.22.5 / Adw 1.9.3 (Fedora 44 toolbox host, 2026-09-24); GTK 4.22.2 / Adw 1.9.0 (GNOME 50 SDK, `make gtk-axioms-runtimes`, 2026-09-25); GTK 4.22.5 / Adw 1.9.4 (Fedora 44 CI `GTK Axioms`, run 36091205809, 2026-09-25) | `crates/gtk-lush/axioms/examples/a13_an_in_layout_scroll_does_not_relayout.rs` |

## Envelope use

The Kani closed-loop model of the slice bin
(`crates/gtk-lush/widgets/src/kani_proofs.rs`, `slice_loop`) cites these ids in
its `kani::assume` clauses: A4 for where a request appears, A7/A8 for how far a
settle may move the child, A9 for why landing must be exact, A11 for outer
clamping, and A13 for why the outer move happens after the allocation.

The A7/A8 settle envelope is an **assumption**, not a measured GTK bound: the
A7 probe measured `GtkListView` moving its value 4.42 px per pixel of page
change when its anchor alignment lies outside `[0, 1]` (see the A8 row). LushText's
consumers have not produced such a settle, but the closed-loop model's settle
bound must be revisited against that measurement by
`extend-closed-loop-geometry-verification`.
