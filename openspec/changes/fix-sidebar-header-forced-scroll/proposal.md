## Why

v0.7.0 ships a sidebar whose workspace header — its label, collapse, add-folder
and refresh buttons — is scrolled out of view and cannot be brought back:
scrolling up snaps the list straight back to its first row. With more than one
workspace the sidebar oscillates between two positions and the end of the list
is unreachable.

The cause is in `gtk-lush-widgets`. `ViewportSliceBin` decides whether its child
asked to scroll by comparing the child's adjustment against the **unclamped**
outer viewport top. The value it publishes is the slice offset, which is clamped
into `[0, content - slice]`. The two agree only when the bin starts exactly at
the viewport's top edge, so with a separator and a 54px header above it the bin
reads a standing 55px request on every allocation and scrolls the outer window
to satisfy it. Measured: the sidebar rests at 55 with the header at `y=-54`, and
two sections oscillate 10838 ↔ 11558 forever.

Fixing that exposes a second defect the forced scroll had been masking. The bin
allocates its child a full viewport-height band even when chrome above the
content leaves less than that on screen, and the child decides for itself
whether a row is visible from its own allocation. With the header no longer
forced away, a revealed row lands behind it.

## What Changes

- Extract the request decision as a pure `outer_scroll_request` in
  `gtk-lush-widgets`, beside the existing pure `viewport_slice`. It reports a
  request only when the child's value diverges from the offset the bin
  published, which is what the spec already requires and what the shipped code
  did not do.
- `ViewportSliceBin` records the offset it publishes and consumes that decision.
- `viewport_slice` deducts chrome above the content from the band it returns, so
  the child's own visibility decisions match the screen. The band is **not**
  shrunk at the bottom edge, where it would collapse to zero and make
  `GtkListView` rewrite the bin's adjustment.
- Regression coverage at three levels: pure unit and property tests for both
  decisions, generic widget coverage in the GTK Lush adoption suite, and the
  sidebar user journey in the workspace tree suite.
- Correct the `widget-wiring.md` rule that prescribed the defective comparison.

## Capabilities

### New Capabilities
<!-- None: this restores and sharpens an existing contract. -->

### Modified Capabilities
- `gtk-lush-viewport-slice`: the adjustment-synchronization requirement gains the
  resting and multi-container cases the shipped code violated, and the slice
  geometry requirement gains the visible-band bound.

## Impact

- `crates/gtk-lush/widgets/src/scroll_request.rs` (new), `slice_geometry.rs`,
  `viewport_slice_bin/imp.rs`, `lib.rs`, `CHANGELOG.md`.
- `crates/lushtext-core/tests/properties/viewport_slice.rs`,
  `crates/lushtext/tests/widget/gtk_lush_adoption.rs`,
  `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`.
- `.agents/rules/widget-wiring.md`, `AGENTS.md`.
- No LushText production code changes: the sidebar consumes the fixed widget
  unchanged. No persisted format, action, or settings change.
