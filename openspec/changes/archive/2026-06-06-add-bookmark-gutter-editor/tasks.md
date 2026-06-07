## 1. Editor Plumbing

- [x] 1.1 Add editor-page APIs to resolve a LushText bookmark from an activated gutter line and expose the selected bookmark by stable ID.
- [x] 1.2 Add an editor-page API that updates an existing bookmark by stable ID, normalizes the label, validates the 1-based target line, rejects collisions with other bookmarks, and returns actionable validation errors.
- [x] 1.3 Wire `GtkSourceView::line-mark-activated` during bookmark setup so activating an existing LushText bookmark invokes a window-owned edit callback while non-bookmark line marks are ignored.
- [x] 1.4 Move accepted line edits through the existing live `GtkSourceMark` and bookmark record instead of deleting and recreating bookmark identity.

## 2. Dialog Flow

- [x] 2.1 Replace the label-only edit flow with an `Edit Bookmark` dialog that shows the current label and 1-based line number for the selected bookmark.
- [x] 2.2 Keep the dialog open with clear validation feedback when the user enters an out-of-range line or a line containing a different bookmark.
- [x] 2.3 Emit the existing bookmark-changed callback after successful edits so minimap refresh and debounced bookmark sidecar persistence run normally.
- [x] 2.4 Update user-visible action labels, status messages, README/manual checks, and related documentation references from label-only editing to bookmark editing where needed.

## 3. Verification

- [x] 3.1 Add editor-page coverage for updating labels, moving bookmarks to first/last/middle lines, preserving bookmark IDs, and rejecting target-line collisions and out-of-range inputs.
- [x] 3.2 Add widget coverage for gutter-mark activation opening the edit dialog and successful line edits persisting through reload.
- [x] 3.3 Run focused bookmark/editor tests, formatting, `openspec validate add-bookmark-gutter-editor --strict`, and the final OpenSpec status check.
