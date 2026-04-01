# File Encoding Support

## Status: Deferred

## Description
LushText currently uses `std::fs::read_to_string()` which enforces UTF-8. The status bar
encoding label displays a static "UTF-8" read-only indicator. Full encoding support would
allow opening files in other encodings and re-opening files with a different encoding.

## Current State
- Status bar shows "UTF-8" label (always, since only UTF-8 is supported)
- `LushtextStatusBar::set_metadata_visible()` controls the encoding label visibility
- The encoding label is a `GtkLabel` in `status-bar.ui`, ready to be replaced with a
  `GtkMenuButton` or `GtkDropDown` when interactive encoding selection is added

## Implementation Plan
1. Replace `std::fs::read_to_string()` with raw `std::fs::read()` (returns `Vec<u8>`)
2. Use `encoding_rs` crate for charset detection and decoding
3. Store detected encoding on `LushtextEditorPage` as a new `encoding: RefCell<String>` field
4. Replace the encoding `GtkLabel` with a `GtkMenuButton` that opens a popover
5. The popover lists common encodings (UTF-8, ISO-8859-1, Windows-1252, Shift_JIS, etc.)
6. Selecting an encoding re-reads the file from disk with the new encoding
7. On save, encode back using the active encoding
8. Update `LushtextStatusBar` API: add `set_encoding(encoding: &str)` method
9. Update `LushtextWindow::refresh_status_bar()` to read `editor.encoding()` and pass it

## Alternative: GtkSourceFileLoader
`GtkSourceFile` + `GtkSourceFileLoader` provide built-in encoding detection via `uchardet`.
This would replace the manual `read_to_string` / `write` pattern entirely. Trade-offs:
- **Pro**: encoding detection is automatic, handles BOM, integrates with GtkSourceView
- **Con**: requires rewriting the entire file load/save pipeline, more complex async handling
- **Recommendation**: evaluate when encoding support becomes a priority

## Dependencies
- `encoding_rs` crate (if not using GtkSourceFileLoader)
- Status bar widget (already implemented)
