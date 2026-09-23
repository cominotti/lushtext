# gtk-lush-widgets

`gtk-lush-widgets` is a `0.0.0` GTK Lush family crate for small reusable GTK
widgets and render-hold helpers.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so LushText can keep geometry and render-hold
helpers small, local, and reviewable.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## Scope

`ClipBin` is a single-child widget that reports zero minimum size while still
delegating natural size to its child and clipping snapshots to its allocation.

`ViewportSliceBin` hosts a `GtkScrollable` child, typically a `GtkListView`,
inside an outer `GtkScrolledWindow` without giving up virtualization. It
advertises the child's full content height to the outer scroller, allocates the
child only the band that intersects the outer viewport (plus an optional
overscan), owns the child's adjustments, and forwards the child's own
`scroll_to` and keyboard-focus scrolling to the outer scroller. It exists
because `GtkListView` realizes at most `GTK_LIST_VIEW_MAX_LIST_ITEMS` (200,
plus two extra) row widgets for one visible range: a list view handed its
whole content as viewport renders blank space after roughly the two-hundredth
row. Without an outer scroller ancestor (or the `outer-scrolled-window`
property) the bin is a plain host and the child receives the whole allocation.

Two pure functions carry its decisions, so both are unit- and property-testable
without GTK. `viewport_slice` / `ViewportSlice` picks the band, deducting
whatever chrome the host draws above the content from it: the child judges row
visibility from its own allocation, so a band taller than the visible area makes
it place revealed rows behind that chrome. `outer_scroll_request` decides
whether the child asked to scroll, by comparing the child's adjustment against
the offset the bin published rather than against the outer viewport's unclamped
top edge — those agree only when the bin starts exactly at that edge, and
comparing against the wrong one makes a bin below other content scroll that
content away and pin itself to the top. `outer_scroll_request` is the
request-only projection of `classify_child_scroll`, which returns the whole
`ChildScrollDecision`: rest, request, settle, or defer.

Neither function panics for any `f64`. Their geometric guarantees — the slice
lies inside the content and covers the visible intersection, and a honoured
request lands exactly on the child's value — hold on the whole-pixel domain GTK
produces (integer values in the `i32` range), where in-tree Kani harnesses
prove them (`make kani` in the LushText workspace). They are deliberately not
claimed for arbitrary `f64`: Kani keeps the counterexamples as `should_panic`
harnesses.

The geometry the bin publishes is expressed in the child's CSS content box. A
`GtkScrollable` such as `GtkListView` measures its page and content there, so a
padded child handed border-box numbers rewrites them on every allocation and
re-derives its value from its scroll anchor against a different page, a few
pixels from where the bin placed it — rows then render that far off. The bin
learns the vertical inset from the page the child reports after its first
allocation and deducts it from `upper` and `page_size`; the value itself is not
offset (see `publish_slice_offset`). A settle that still survives is written
back to the published offset inside the allocation. A divergence that falls
within a geometry correction the child made in that same allocation may be a
settle or a genuine `scroll_to` (a list that realizes rows of unexpected height
while it moves its anchor), so it is deferred instead: the bin leaves the
child's value in place, hands it back on one more allocation, and classifies it
there by the ordinary rule once the geometry is stable. Writing it back would
erase the request, because a `GtkListBase` re-anchors on any value it is handed.
`allocation_count()` and
`correction_count()` are layout diagnostics: a bin at rest stops allocating and
needs no corrections, and either count growing across idle frames names which
side of the contract is broken.

`RenderHoldOverlay` captures already-rendered child pixels into a non-targetable
cover picture, hides the live child, lets callers warm the live child beneath
the cover, and clears the cover with paired opacity restoration.

The crate does not schedule reflow timing, own readiness predicates, or encode
application-specific minimap behavior.
