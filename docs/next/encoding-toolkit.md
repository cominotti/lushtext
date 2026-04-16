# Polyglot Encoding & Line Ending Toolkit

## Status: Implemented (Initial Release)

## Shipped Scope
LushText now treats encoding and line endings as first-class document metadata
instead of assuming every file is UTF-8 text.

- File I/O now starts from raw bytes and records the document's open/save
  encoding, decode confidence, BOM state, and line-ending policy.
- The status bar now shows interactive encoding and line-ending controls plus a
  conditional file-health entry point.
- `Reopen as <encoding>` now re-reads saved files with another decoder and
  reuses the normal unsaved-changes safety flow before discarding edits.
- `Save as <encoding>` now sets the next-save policy and warns before lossy
  conversions. The actual save path re-checks that risk before writing.
- Mixed line endings are detected on open, surfaced through file health, and
  can be normalized through the info bar + line-ending picker workflow.
- Invisible-character modes now support `Off`, `Whitespace Only`, and `All`,
  using GtkSourceView's native space drawer for spaces, tabs, non-breaking
  spaces, and newline markers.

## Intentional First-Release Limits
- The encoding shortlist is deliberately small and geared toward common editing
  workflows instead of trying to expose every legacy charset immediately.
- Lossy save preview is a bounded warning/details flow, not a full side-by-side
  diff viewer.
- Zero-width characters and BOM state remain discoverable through the file
  health surface instead of custom inline glyph rendering.
- The current detection path uses lightweight heuristics and explicit reopen
  controls instead of a broader detector such as `chardetng`.

## Follow-Up Candidates
- Broaden the automatic charset detector if real-world files show the current
  shortlist is too narrow.
- Add a richer diff-style preview for lossy save conversions.
- Consider stronger inline affordances for zero-width characters if the file
  health surface is not discoverable enough in practice.
- Wire the shipped encoding and line-ending state into EditorConfig enforcement
  for `charset` and `end_of_line`.
