# Changelog

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
