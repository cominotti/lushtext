# Changelog

## Unreleased

- `viewport_slice`, `classify_child_scroll`, and `outer_scroll_request` now
  state the input domain of their guarantees. They never panic for any `f64`;
  slice containment, coverage of the visible intersection, and exact request
  landing are guaranteed on the **whole-pixel domain** GTK produces (integer
  values in the `i32` range, overscan up to 4096, a reconfiguration shift that
  fits a `u16`), where Kani proves them. Kani found that containment and exact
  landing both fail for general `f64` (near `1e260`, and at non-integer
  values), so the earlier unqualified rustdoc promise was wider than the truth;
  behaviour is unchanged. The crate carries in-tree `#[cfg(kani)]` proof
  harnesses, run by `make kani`; they add nothing to the public API or to any
  ordinary build. The bin's whole-pixel rounding moved into a private pure
  helper so its `i32::clamp` is proved panic-free too.
- Fixed `ViewportSliceBin` erasing a child `scroll_to` made in the same
  allocation in which the child also moved the adjustment's `upper` or
  `page_size`, when the requested value lay within that correction of the
  published offset: the bin read it as a settle and wrote the published offset
  back, the child re-anchored on it, and the requested row stayed clipped. Such
  a divergence is now deferred -- the bin skips the write-back, hands the
  child's value back on one more allocation, and classifies it there with
  stable geometry, so it is honoured as a request or written back as a settle
  one allocation later. The decision is the new pure, unit- and
  property-tested `classify_child_scroll` / `ChildScrollDecision`;
  `outer_scroll_request` keeps its signature as the request-only projection.
- `ViewportSliceBin` now publishes the child adjustment's `upper` and
  `page_size` in the child's CSS content box, learning the vertical inset from
  the page the child reports after its first allocation. A `GtkListView` works
  in its content box, so a padded list (Libadwaita's `navigation-sidebar` gives
  10px) overwrote the border-box geometry on every allocation and re-derived
  its value from its scroll anchor a few pixels off, and the rows rendered that
  far from where the bin had placed them on every focus change. Any value the
  child still settles on that is neither the published offset nor a request is
  written back to the published offset inside the allocation, and the new
  `allocation_count()` / `correction_count()` layout diagnostics expose both. The v0.8.1 behaviour of adopting the settled value as the new
  baseline is gone: it gave the outer scroller a fixed point and left the
  rendering with none.
- Fixed `ViewportSliceBin` moving the outer scroller while at rest. It compared
  the child's adjustment against the unclamped viewport top rather than against
  the offset it had itself published, so any container placed below other
  content in the same scroller read a standing scroll request: one scrolled that
  content out of view and pinned itself to the top, and two in one scroller
  oscillated against each other and made the end of the content unreachable.
  The decision is now the pure, unit- and property-tested
  `outer_scroll_request`.
- `viewport_slice` now deducts chrome above the content from the band it
  returns, so the child's own `scroll_to` and keyboard-focus visibility
  decisions match what is on screen instead of running past the fold by the
  height of that chrome. The band is never shrunk at the bottom edge: a
  zero-height allocation makes `GtkListView` rewrite the container's adjustment,
  which would read as a scroll request.

## 0.0.0

- Added `ViewportSliceBin`, a single-child container that keeps a
  `GtkScrollable` child (typically `GtkListView`) virtualized inside an outer
  `GtkScrolledWindow`: it advertises the child's full content height to the
  outer scroller but allocates only the visible band, owns the child's
  adjustments, and forwards child `scroll_to`/focus requests to the outer
  scroller. Motivated by `GtkListView`'s `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200)
  realized-widget cap, which left LushText's workspace tree blank after ~205
  rows. The pure slice geometry is exposed as `viewport_slice` /
  `ViewportSlice` for unit and property tests.
- First functional in-tree pre-publication implementation for `ClipBin` and
  `RenderHoldOverlay`.
- Adoption-lab and matrix evidence before functional publication.
