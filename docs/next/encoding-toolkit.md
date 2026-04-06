# Polyglot Encoding & Line Ending Toolkit

## Status: Proposed

## Description
Go beyond "show encoding in status bar" to make LushText the best editor for encoding
work. Click the encoding label to convert between encodings with a live preview diff.
Toggle line endings the same way. Show invisible characters on demand with distinct
glyphs. Detect and warn on mixed encodings or mixed line endings on open.

## Current State
- Status bar shows a static "UTF-8" encoding label (read-only)
- Only UTF-8 is supported (`std::fs::read_to_string` enforces it)
- No line ending detection or conversion
- No invisible character visualization beyond GtkSourceView's built-in
  `draw-spaces` property
- `docs/next/encoding-support.md` covers basic encoding support — this feature doc
  extends it into a comprehensive toolkit
- `docs/next/editorconfig-future.md` lists `end_of_line` and `charset` as deferred
  EditorConfig properties that this feature would unblock

## Motivation
Every developer who works with legacy files, CSVs from Excel, or cross-platform projects
hits encoding pain regularly. Every text editor handles it poorly, hiding conversion
behind "Save As" dialogs with no preview of what you're about to destroy. A dedicated
encoding toolkit with live preview, line ending management, and invisible character
visualization would make LushText the go-to editor for "I need to fix this file's
encoding" tasks.

## Implementation Plan

### Phase 1: Encoding Detection & Conversion (extends encoding-support.md)
1. Replace `std::fs::read_to_string()` with `std::fs::read()` → `Vec<u8>`
2. Add `encoding_rs` crate for charset detection and transcoding
3. Auto-detect encoding on file open using `chardetng` (Mozilla's encoding detector)
4. Store detected encoding on `EditorPage`: `encoding: Cell<&'static Encoding>`
5. Status bar encoding label becomes a `GtkMenuButton` with:
   - Dropdown of common encodings grouped by region (Unicode, Western, Asian, etc.)
   - Current encoding highlighted
   - "Reopen with Encoding..." option at the top

### Phase 2: Encoding Conversion with Preview
1. Selecting a new encoding from the dropdown shows a preview dialog before converting:
   - Side-by-side diff: current interpretation vs. new interpretation
   - Highlight characters that would change (mojibake → correct, or correct → lossy)
   - Warning badge if conversion is lossy (characters that can't be represented)
2. "Reopen" re-reads the raw bytes and decodes with the selected encoding
3. "Convert" transcodes the buffer content to the new encoding (for save)
4. Distinguish between "reopen as" (change interpretation) and "save as" (change output)

### Phase 3: Line Ending Detection & Conversion
1. Detect line endings on file open: LF, CRLF, CR, or mixed
2. Store on `EditorPage`: `line_ending: Cell<LineEnding>`
3. `LineEnding` enum: `Lf`, `Crlf`, `Cr`, `Mixed`
4. Status bar line ending indicator (next to encoding label): "LF" / "CRLF" / "CR"
5. Click to convert: selecting a different line ending converts all line endings in the
   buffer immediately
6. Mixed line ending warning: `LushtextInfoBar` (yellow) on open with "Fix" button
   that normalizes to the majority line ending

### Phase 4: Invisible Character Visualization
1. Extend GtkSourceView's `draw-spaces` to cover all invisible characters:
   - Tabs: `→` (already supported by GtkSourceView)
   - Spaces: `·` (already supported)
   - Non-breaking spaces: `°` (important — causes subtle bugs)
   - Zero-width characters: red `∅` marker
   - BOM (byte order mark): highlighted indicator at file start
   - Mixed line endings: show `↵` for LF, `←↵` for CRLF inline
2. Toggle via: `win.toggle-invisible-chars` action, `Ctrl+Shift+I` shortcut
3. GSettings key: `show-invisible-characters` (b, default false)
4. Three levels: Off, Whitespace Only (tabs + spaces), All (including zero-width, BOM)

### Phase 5: File Health Report
1. On file open, silently analyze for encoding issues:
   - Mixed line endings
   - BOM presence (warn if UTF-8-BOM, which is usually unintentional on Linux)
   - Trailing whitespace statistics
   - Non-UTF-8 encoding detection confidence level
   - Null bytes (likely binary file misidentified as text)
2. Show a subtle indicator in the status bar when issues are detected
3. Click opens a "File Health" popover with findings and one-click fix buttons
4. Integrates with EditorConfig: if `.editorconfig` specifies `end_of_line` or `charset`,
   mismatches are flagged

## Architecture Considerations
- `encoding_rs` handles all the heavy lifting for encoding detection and transcoding. It
  supports all encodings in the WHATWG Encoding Standard (which covers the vast majority
  of real-world encodings).
- `chardetng` (by the same author as `encoding_rs`) provides Mozilla-quality auto-detection.
  It's what Firefox uses.
- Line ending handling must happen at the I/O boundary. GtkTextBuffer normalizes to LF
  internally. On save, the stored `LineEnding` determines what gets written. On load,
  the raw bytes are scanned for line ending style before being fed to the buffer.
- The live preview diff for encoding conversion needs careful memory management — for a
  large file, displaying two full decoded versions simultaneously could use significant
  memory. Consider showing only the first N changed lines with a summary count.
- This feature overlaps with `docs/next/encoding-support.md` (basic encoding) and
  `docs/next/editorconfig-future.md` (end_of_line, charset properties). Implementation
  should be sequenced: basic encoding support first, then this toolkit builds on it,
  then EditorConfig integration last.

## Dependencies
- `encoding_rs` crate (WHATWG encoding transcoding)
- `chardetng` crate (encoding auto-detection)
- GtkSourceView `draw-spaces` API
- Status bar refactoring (label → menu button for encoding and line ending)
- `LushtextInfoBar` for mixed line ending warnings
- EditorConfig integration (for charset/end_of_line enforcement)

## Risks
- Encoding conversion can be lossy — converting from UTF-8 to Latin-1 silently drops
  characters that can't be represented. The preview dialog is essential to prevent data
  loss, but users may still click "Convert" without reading the diff.
- Line ending conversion in very large files could cause a noticeable pause. The buffer
  replacement should be done as a single irreversible action to avoid filling the undo
  stack.
- The "invisible characters" visualization may conflict with GtkSourceView's built-in
  `GtkSourceSpaceDrawer`. Need to verify that custom zero-width character rendering can
  coexist with the built-in space drawer.
