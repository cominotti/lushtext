# EditorConfig: Deferred Properties

The following EditorConfig properties are recognized by the spec but are
not yet supported by LushText. Each requires new features beyond the
settings provider layer.

## `end_of_line` (lf / crlf / cr)

The encoding-toolkit work now provides line-ending detection on load plus
save-time normalization at the `editor_io.rs` boundary. The remaining work
here is to let EditorConfig set or warn on the already-shipped save policy for
the active document.

## `charset` (utf-8 / utf-8-bom / latin1 / utf-16be / utf-16le)

The encoding-toolkit work now provides encoding-aware load/save behavior,
status-bar controls, and lossy-conversion confirmation. The remaining work
here is EditorConfig enforcement: mapping `charset` onto the current
open/save encoding policy without fighting explicit user choices.

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
4. `end_of_line` — medium scope, now mostly EditorConfig policy wiring
5. `charset` — medium scope, now mostly EditorConfig policy wiring
