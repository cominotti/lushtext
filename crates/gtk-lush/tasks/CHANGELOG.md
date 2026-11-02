# Changelog

## Unreleased

- Completions are delivered from one GLib source at the new
  `completion_priority()` (`GDK_PRIORITY_REDRAW + 4`), one completion per
  dispatch, instead of one `G_PRIORITY_DEFAULT_IDLE` idle each. A
  `GtkTextView` validating a large buffer no longer holds them back (GTK axiom
  ledger A80), and frames and input still dispatch first
  (`spawn-completion-priority`).

## 0.0.0

- First functional in-tree pre-publication implementation for bounded
  background task dispatch and explicit freshness tokens.
- Adoption-lab and matrix evidence before functional publication.
