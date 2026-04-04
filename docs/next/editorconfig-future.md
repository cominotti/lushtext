# EditorConfig: Deferred Properties

The following EditorConfig properties are recognized by the spec but are
not yet supported by LushText. Each requires new features beyond the
settings provider layer.

## `end_of_line` (lf / crlf / cr)

Requires line-ending detection on file load and conversion on save.
GtkSourceView buffers normalize to LF internally; conversion must happen
at the I/O boundary in `editor_io.rs`.

## `charset` (utf-8 / utf-8-bom / latin1 / utf-16be / utf-16le)

Requires encoding detection on load and conversion on save. Currently
LushText only supports UTF-8. Would need `encoding_rs` or similar crate
plus a status bar encoding selector.

## `trim_trailing_whitespace` (true / false)

Requires an on-save hook that strips trailing whitespace from each line
before writing to disk. Must be careful not to mutate the buffer
(strip from the snapshot text, not the live buffer).

## `insert_final_newline` (true / false)

Requires an on-save hook that ensures the file ends with a newline (or
removes the trailing newline if `false`). Similar to `trim_trailing_whitespace`.

## `max_line_length` (number / off)

Maps to GtkSourceView's `right-margin-position` + `show-right-margin`
properties. Would need a new GSettings key and preferences row for the
right margin, plus EditorConfig override support in the provider chain.

## Implementation Priority

Recommended order based on user impact and implementation effort:

1. `insert_final_newline` — small scope, high value (POSIX compliance)
2. `trim_trailing_whitespace` — small scope, high value (code cleanliness)
3. `max_line_length` — medium scope, maps directly to existing GtkSourceView properties
4. `end_of_line` — medium scope, common need for cross-platform teams
5. `charset` — large scope, niche need (most modern codebases are UTF-8)
