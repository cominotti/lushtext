## Why

With the sidebar scrolled to the top, clicking between file-tree rows sometimes
shifts the rendered tree a few pixels up or down; sometimes it stays shifted,
sometimes it comes back. Reported 2026-09-19 on v0.8.1.

The defect is in `gtk-lush-widgets`. `ViewportSliceBin` hands its `GtkListView`
child border-box geometry every allocation — `upper` = the bin's content height,
`page_size` = the slice height — but the child works in its CSS content box. The
child carries Libadwaita's `navigation-sidebar` style, whose compiled rule is
`padding-top: 6px; padding-bottom: 4px` (verified in the installed
`libadwaita-1.so.0`), so it overwrites both numbers 10px smaller on every
allocation (measured: `upper` 15248→15238, `page_size` 665→655). Its scroll
anchor arithmetic, `value = anchor_y − align · page_size`, is then evaluated
against a page 10px different from the one the bin's `value` was chosen for, and
lands on a different pixel each time the anchor item changes — which a click
does. The bin's transform stays where it was; the child renders against the
moved value; rows draw `d` pixels off. v0.7.1 and earlier forwarded that `d` to
the outer scroller (the 55px drift v0.8.1 fixed); v0.8.1 stopped forwarding it
and adopted it as the new baseline, which gave the outer scroller a fixed point
and left the rendering with none.

"Shifts and stays" is one allocation whose settle was non-zero. "Shifts and
comes back" is a later allocation (focus change, row re-bind) whose settle was
zero. The screen shows the `d` of whichever ran last.

Two review passes (2026-09-19, 2026-09-21) shaped this change; their retracted
hypotheses are recorded in `design.md` so they are not re-attempted.

## What Changes

- **Publish geometry in the child's frame.** The bin keeps publishing
  authoritative `value`, `upper`, and `page_size` every allocation, but `upper`
  and `page_size` are reduced by the child's content-box inset, which the bin
  derives from what the child reports after its first allocation rather than
  from a deprecated style API. `value` is **not** offset: with `value =
  slice_top` a row at child content `y` already lands at bin `y + padding_top`,
  independent of the slice. After this the child has nothing to overwrite and
  `reconfigure_shift` is zero on ordinary allocations.
- **Own the residual, on the evidence.** Any settle that survives is corrected
  by re-asserting the published value unconditionally, under the `allocating`
  guard (which is currently cleared too early), with acceptance measured as the
  bin's own allocation count at rest.
- **Test the screen, not the number.** Rendered-bounds coverage: a named row's
  bounds relative to its section header are pixel-identical across a sampled
  sequence, with layout forced between samples, driven by selection and by
  focus on rows already fully visible, at the top and mid-content; a padded
  child fixture in the widget's own adoption suite so the contract is proved
  where it lives; a positive control that a genuine one-pixel keyboard request
  still moves the outer scroller.
- **Evidence, not a panic.** A test-only counter of the bin's own allocations
  and of corrections applied, exposed as evidence; a `debug_assert!` is not used
  because a panic inside a `size_allocate` vfunc aborts the harness process.

## Capabilities

### Modified Capabilities

- `gtk-lush-viewport-slice`: adds a render-alignment requirement with
  measurable scenarios; extends the synchronization requirement with the
  in-allocation settle scenario shipped in v0.8.1.

## Impact

- `crates/gtk-lush/widgets/src/viewport_slice_bin/imp.rs`,
  `crates/gtk-lush/widgets/src/scroll_request.rs` (doc), possibly a pure
  `child_geometry` decision beside them
- `crates/lushtext/tests/widget/gtk_lush_adoption.rs`,
  `crates/lushtext/tests/widget/workspace_tree_virtualization.rs`
- `crates/lushtext-core/tests/properties/viewport_slice.rs`
- GTK Lush governance: `crates/gtk-lush/widgets/{CHANGELOG.md,README.md}`,
  `make check-gtk-lush-policy`, `check-gtk-lush-adoption`,
  `gtk-lush-doctests`, `gtk-lush-examples`, public-API advisory diff
- Widget tests are visual-sensitive; the visual-geometry lane gates those
  commits. Land the failing tests first; rerun the lane once per distinct
  visual-sensitive file set.
