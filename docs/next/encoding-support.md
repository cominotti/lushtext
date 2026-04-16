# File Encoding Support

## Status: Folded Into `encoding-toolkit`

## Summary
This note started as the narrow "make encoding interactive" foundation. That
foundation now lives inside the broader encoding-toolkit implementation, so it
should not be planned or implemented as a separate change anymore.

## What Shipped
- File load/save now works from raw bytes instead of a UTF-8-only `read_to_string` path.
- LushText tracks per-document open/save encoding state and line-ending policy.
- The status bar now exposes interactive encoding and line-ending controls.
- Saved documents can be reopened with another encoding and can be configured to
  save using another encoding, with lossy-conversion confirmation when needed.
- File-health findings now surface low-confidence decode results, mixed line
  endings, UTF-8 BOMs, and other encoding-adjacent issues.

## Remaining Follow-Up
- Broader charset auto-detection than the current small first-release shortlist.
- Richer conversion previews than the current bounded warning/details flow.
- EditorConfig enforcement for `charset` once the current interactive save/open
  policy proves stable in day-to-day use.
