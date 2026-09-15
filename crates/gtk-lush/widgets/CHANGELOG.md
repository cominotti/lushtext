# Changelog

## Unreleased

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
