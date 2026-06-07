## 1. Live Bookmark Rows

- [x] 1.1 Add a window-side helper that snapshots bookmark records from open saved editors on the GTK main thread.
- [x] 1.2 Filter live bookmark snapshots through the same current workspace roots used by `Browse Notes...`.
- [x] 1.3 Merge live bookmark snapshots with the sidecar bookmark listing so open-editor rows replace stale persisted rows for the same document identity and bookmark ID.
- [x] 1.4 Preserve existing bookmark ordering, preview metadata, search matching, and explicit Open navigation after the merge.

## 2. Notes Browser Dismissal

- [x] 2.1 Add a reusable compact Close/X button for notes-browser content that closes the owning `AdwDialog`.
- [x] 2.2 Show the Close/X control in the populated notes browser sidebar page.
- [x] 2.3 Show the Close/X control in the populated notes browser preview page.
- [x] 2.4 Show the Close/X control in the empty notes browser state.
- [x] 2.5 Ensure pressing Escape after opening `Browse Notes...` dismisses the dialog without requiring a prior click inside the dialog.

## 3. Regression Coverage

- [x] 3.1 Add widget coverage for a freshly toggled bookmark appearing in `Browse Notes...` before the debounced sidecar save completes.
- [x] 3.2 Add widget coverage that stale persisted bookmark rows for an open file are not duplicated or shown instead of current live bookmark state.
- [x] 3.3 Add widget coverage for closing populated `Browse Notes...` from the visible Close/X control.
- [x] 3.4 Add widget coverage for closing the empty notes browser state and for Escape dismissal immediately after opening.

## 4. Verification

- [x] 4.1 Run focused notes-browser and bookmark widget tests.
- [x] 4.2 Run formatting and relevant lint/test gates for the touched Rust code.
- [x] 4.3 Run `openspec validate fix-browse-notes-bookmarks-and-close --strict`.
- [x] 4.4 Confirm `openspec status --change fix-browse-notes-bookmarks-and-close --json` reports all tasks complete after implementation.
