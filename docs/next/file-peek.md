# Smart File Peek

## Status: Implemented

## Description
Preview a file from the workspace sidebar without opening a tab. Press `Space` on a
selected file row to show a temporary peek surface with bounded content, file identity
metadata, and a clear path to open the file normally. The goal is to let users inspect
several candidates in sequence without polluting the tab strip or disturbing the document
they are already editing.

## Shipped Design
- Ownership stays inside `ui/sidebar/workspace_section/`: one reusable `GtkPopover`
  belongs to each `LushtextWorkspaceSection`.
- `services/file_peek.rs` performs the blocking work on a background thread:
  metadata read, size-policy check, bounded prefix read, UTF-8 validation, truncation,
  and explicit fallback classification.
- The preview is intentionally plain and read-only: a bounded `GtkTextView` sample
  inside the popover, not a full `EditorPage`, draft, monitor, or undo-capable buffer.
- Promotion reuses the existing sidebar `file_activated` path, so `open_document()` keeps
  sole ownership of duplicate-tab reuse and editor-focus behavior.
- The popover remains anchored to the selected row and repositions or closes as selection,
  visibility, or row realization changes.

## UX Contract

- `Space` on the selected sidebar file toggles peek for that file.
- `Space` on the same file again, `Escape`, click-away, or selection moving onto a
  non-file row closes the peek.
- `Enter` while peek is visible promotes the current file through the normal
  `open_document()` path, then closes the peek.
- Keyboard-triggered peek keeps the sidebar list as the default focus owner so Up/Down
  continues navigating files while the surface updates in place.
- The peek surface is an overlay anchored to the selected row. It must not resize the
  left, center, or right panes or interfere with the fixed top and bottom sidebar rows.
- Users see enough information to decide quickly: file name, absolute file path,
  human-readable size, modified timestamp, and a bounded preview sample or an explicit
  unsupported/error state.

## Behavior Notes
- The snapshot service reads at most 16 KB and renders at most 60 lines.
- Binary, unreadable, or too-large files always render explicit fallback copy instead of
  silently doing nothing.
- The `Open` action is only enabled when the existing size and text rules still allow the
  normal document-opening workflow.
- The preview is currently plain text rather than syntax-highlighted preview. That keeps
  the first release lightweight and avoids accidental editor-state coupling.

## Dependencies
- `GtkPopover` for anchored overlay presentation.
- `GtkTextView` for the bounded read-only sample body.
- Existing sidebar selection and file-activation wiring for navigation and promotion.
- `spawn_blocking_then` for asynchronous snapshot loading.
- `services/file_limits.rs` for large-file policy alignment.

## Validation
- Unit coverage lives in `services/file_peek.rs` for truncation, stale-request tokens,
  binary/unreadable handling, and open refusal.
- Widget coverage lives in `crates/lushtext/tests/widget/workspace_section.rs` and
  `crates/lushtext/tests/widget/window.rs` for open, refresh, dismiss, fallback, and
  promotion flows.
- Live runtime validation is still recommended for final UX sign-off at `Small`,
  `Comfy`, and `Large` sidebar presets because popover positioning and warning-free
  behavior still depend on the real compositor session.
