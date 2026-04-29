# EditorConfig: Remaining Properties

The following EditorConfig properties are recognized by the spec but are
either now supported by LushText's provider chain or still need product work
beyond the settings layer.

## `end_of_line` (lf / crlf / cr)

Supported. EditorConfig now sets the active document's save line-ending policy
when `end_of_line` is present. The status bar refreshes after async
EditorConfig resolution so users can see the effective policy.

## `charset` (utf-8 / utf-8-bom / latin1 / utf-16be / utf-16le)

Partially supported. EditorConfig now maps `utf-8`, `utf-8-bom`, `utf-16be`,
and `utf-16le` onto the active document's save encoding policy. `latin1` is
recognized as present but intentionally not mapped because LushText's current
save pipeline does not expose an ISO-8859-1 encoder, and guessing Windows-1252
would be incorrect.

## `trim_trailing_whitespace` (true / false)

Supported. Save snapshots strip trailing spaces and tabs before writing when
the nearest EditorConfig rule enables it. After a successful save, LushText
mirrors the saved text back into the live buffer before marking the buffer
clean, so the visible editor state and the bytes on disk do not diverge.

## `insert_final_newline` (true / false)

Supported. Save snapshots add or remove the final newline according to the
resolved EditorConfig value, preserving the document's effective line-ending
policy when a newline is inserted.

## `max_line_length` (number / off)

Maps to GtkSourceView's `right-margin-position` + `show-right-margin`
properties. Would need a new GSettings key and preferences row for the
right margin, plus EditorConfig override support in the provider chain.

## Implementation Priority

Recommended remaining order based on user impact and implementation effort:

1. `max_line_length` - medium scope, maps directly to existing GtkSourceView properties
2. `latin1` save support - only if LushText adds a real ISO-8859-1 encoding path
3. Load-time charset hints - only if the product should let EditorConfig influence initial decoding rather than save policy only
