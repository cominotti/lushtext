# Changelog

## Unreleased

- Completions are delivered from one GLib source at the new
  `completion_priority()` (`GDK_PRIORITY_REDRAW + 4`), one completion per
  dispatch, instead of one `G_PRIORITY_DEFAULT_IDLE` idle each. A
  `GtkTextView` validating a large buffer no longer holds them back (GTK axiom
  ledger A80), and frames and input still dispatch first
  (`spawn-completion-priority`).
- The delivery source **ages**: after eight consecutive completions, or 16 ms
  of continuous occupancy, it detaches, lets the main loop reach
  `G_PRIORITY_DEFAULT_IDLE` once (or, when a validation backlog keeps the loop
  from getting there, waits 16 ms), and returns at `completion_priority()`. The
  worker cap alone did not bound it: a FIFO deeper than the cap or a completion
  that spawns its successor kept the source ready for the whole chain and
  starved every priority from 125 to 200.
- Added `set_completion_observer` behind the new `completion-observer`
  feature, for measurement builds: it reports each completion's span.

## 0.0.0

- First functional in-tree pre-publication implementation for bounded
  background task dispatch and explicit freshness tokens.
- Adoption-lab and matrix evidence before functional publication.
