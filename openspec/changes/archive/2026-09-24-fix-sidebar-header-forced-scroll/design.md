## Context

`ViewportSliceBin` hands its `GtkScrollable` child a bin-owned adjustment,
writes the slice offset into it on every allocation, and reads it back after
`child.allocate(...)` because `GtkListView` applies `scroll_to` and
keyboard-focus scrolling inside its own allocation. Anything the child left
behind is a request the bin forwards to the outer scroller.

Two quantities describe where the bin sits, and the shipped code confused them:

- `published_offset` — the slice offset the bin wrote, clamped into
  `[0, content_height - slice_height]`.
- `viewport_top` — the outer viewport's top edge in the bin's content
  coordinates. Negative while other content in the same scroller sits above the
  bin; larger than the content once the bin has scrolled by.

They agree only when the bin starts exactly at the viewport's top edge.

## Goals / Non-Goals

**Goals:**

- A bin at rest never moves the outer scroller, at any position, with any number
  of bins in the scroller.
- A genuine child request still reaches the outer scroller and still settles.
- The child's own visibility decisions match what is on screen.
- The defect class becomes provable without GTK.

**Non-Goals:**

- Changing how the bin measures, how it discovers its outer scroller, or how it
  owns adjustments.
- Any LushText production change. The sidebar is a consumer of the fixed widget.
- Reworking overscan.

## Decisions

### D1: The resting baseline is the published offset

`outer_scroll_request(published_offset, child_value, viewport_top)` reports
`None` unless `child_value` diverges from `published_offset` by at least the
existing half-pixel epsilon. The travel for a real request stays
`child_value - viewport_top`, which is what lands the requested offset at the
viewport's top edge; only the resting test changed.

*Alternative rejected:* keeping the comparison and suppressing the forwarded
delta when it equals the chrome height. That encodes one host's layout in a
generic widget and still fights a bin scrolled past its content.

### D2: The decision is pure and lives beside the slice geometry

The sibling decision, `viewport_slice`, has been a pure, property-tested
function since the widget landed; the request decision was inline GTK code with
no pure counterpart, which is why a baseline error survived the whole suite.
`scroll_request.rs` gives it the same treatment and the same property file.

### D3: The band is the viewport minus chrome above, and is never shrunk at the bottom

The child sizes `scroll_to` and keyboard-focus visibility from its own
allocation. A band taller than the visible area therefore places revealed rows
behind the chrome — which the defect had been hiding, because forcing the bin to
the viewport top made the two agree.

Deducting at the bottom edge as well is symmetric and wrong: the band collapses
to zero once the content scrolls past, and a zero-height allocation makes
`GtkListView` rewrite the bin's adjustment, which reads as a request and starts
the fight again. Measured while trying it: the sidebar's end became unreachable
and rows stopped 6px short of the fold. Deducting only above leaves every other
position byte-identical to the previous behaviour.

## Risks / Trade-offs

- [A bin whose chrome above changes height re-slices, changing how many rows are
  realized] → The band only shrinks; realization stays bounded, and the outer
  scroller's advertised range is unaffected because it comes from `measure`.
- [Rows now stop at the fold rather than being drawn past it, so the last row at
  maximum scroll can end up to one row short] → A virtualized list can only tile
  at row boundaries. The tiling assertion now states that bound explicitly and
  additionally forbids rows drawn below the fold, which the previous 1px
  tolerance was silently accepting from a recycled out-of-range widget.
- [The fix is in a GTK Lush crate with other consumers] → The adoption lab and
  the standalone example put the bin at the top of its scroller, where behaviour
  is unchanged; the new adoption tests cover the below-chrome case generically.

## Migration Plan

None. No persisted state, setting, action, or template changes.
