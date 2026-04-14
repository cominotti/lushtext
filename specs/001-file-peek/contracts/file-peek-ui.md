# UI Contract: File Peek

## Entry Points

- `Space` on the currently selected sidebar file row toggles peek for that file.
- `Enter` while peek is visible promotes the currently previewed file through
  the existing normal open-document flow.
- Pointer users can dismiss by clicking away and can promote through the peek's
  explicit `Open` action.
- `Space` on directories, placeholders, empty-folder markers, or workspace
  header controls does nothing beyond preserving current behavior.

## Presentation Contract

- Peek appears as a floating card anchored beside the selected sidebar row.
- The card may overlap the center content area, but it must not change the
  left, center, or right pane widths.
- The card remains usable across Small, Comfy, and Large sidebar presets.
- The card shows:
  - file name
  - absolute file path
  - human-readable size
  - modified timestamp
  - either a bounded preview sample or an explicit fallback explanation

## Keyboard And Focus Contract

- Opening peek from the keyboard keeps the sidebar `GtkListView` as the default
  focus owner so Up and Down continue scanning files immediately.
- While peek is visible, moving selection to another previewable file refreshes
  the card in place instead of opening tabs.
- Dismissing peek with `Escape`, repeated `Space`, click-away, or selection
  invalidation restores keyboard navigation to the currently selected sidebar row.
- Promoting a file hands focus off to the normal editor-opening workflow.

## Dismissal Contract

- Peek closes when:
  - the user presses `Escape`
  - the user presses `Space` again on the currently previewed file
  - the user clicks away
  - selection moves to a non-file row
  - the workspace filter hides the section
  - a section rebuild or row recycle removes the anchor row
  - the user promotes the file into normal editing

## Preview States

### Loading

- The card appears immediately with a loading state while the bounded snapshot
  request is in flight.

### Text Preview

- Render a bounded, read-only text sample only.
- No draft, undo, save, monitor, or session behavior is created.
- Large but still openable files may fall back to plain-text preview with no
  syntax affordances if the existing file-size policy requires that.

### Binary Or Unsupported

- Show explicit language that inline preview is unavailable for this file type.
- Explain whether normal open is also unavailable under the existing app rules.

### Unreadable

- Show a clear error state for permission or read failures.
- Do not leave the user with a blank card or silent no-op.

### Too Large To Open

- State clearly that the file exceeds the existing open refusal threshold.
- Do not offer the file as normally openable from this state.

## Promotion Contract

- Promotion always delegates to the same document-opening path used elsewhere
  in the app.
- If the file is already open, promotion focuses the existing tab instead of
  creating a duplicate.
- Promotion closes peek after the open action is dispatched.

## Non-Goals For v1

- No hover-to-peek behavior.
- No preview tabs.
- No rich-media or thumbnail rendering.
- No peek entry point from search results, command palette items, or other
  non-sidebar surfaces.
