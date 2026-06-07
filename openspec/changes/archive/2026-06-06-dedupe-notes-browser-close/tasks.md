## 1. Dialog Shell

- [x] 1.1 Add a populated notes-browser shell/header that owns the dialog title and exactly one `build_dialog_close_button` instance.
- [x] 1.2 Wrap the populated `AdwNavigationSplitView` in that shell and set the shell as the `AdwDialog` child.
- [x] 1.3 Remove the page-local Close/X button from the notes sidebar page while preserving search, results, limit-label, and Escape dismissal.
- [x] 1.4 Remove the page-local Close/X button from the preview page while preserving the Back button, preview title/meta, preview content, Open action, and Escape dismissal.
- [x] 1.5 Confirm the empty notes-browser state still exposes exactly one visible Close/X control.

## 2. Adaptive Behavior

- [x] 2.1 Ensure the shell-owned Close/X remains visible when the populated notes browser is unfolded with sidebar and preview visible together.
- [x] 2.2 Ensure the shell-owned Close/X remains visible when the populated notes browser is collapsed to the sidebar page.
- [x] 2.3 Ensure the shell-owned Close/X remains visible when the populated notes browser is collapsed to the preview page.
- [x] 2.4 Keep the preview Back button limited to navigation and verify it does not replace or duplicate dialog dismissal.
- [x] 2.5 Keep `Escape` dismissal working immediately after opening, including before the user clicks inside the dialog.

## 3. Tests

- [x] 3.1 Add or update widget coverage so a populated unfolded `Browse Notes...` dialog exposes exactly one visible Close/X control.
- [x] 3.2 Add or update widget coverage for closing the populated collapsed sidebar state through the visible Close/X control.
- [x] 3.3 Add or update widget coverage for closing the populated collapsed preview state through the visible Close/X control.
- [x] 3.4 Add or update widget coverage confirming the preview Back button navigates without acting as a duplicate Close/X control.
- [x] 3.5 Keep or update empty-state coverage for one visible Close/X control and immediate `Escape` dismissal.

## 4. Validation

- [x] 4.1 Run `cargo fmt --check`.
- [x] 4.2 Run `cargo check -p lushtext-core -p lushtext`.
- [x] 4.3 Run targeted widget tests for notes-browser close/adaptive behavior.
- [x] 4.4 Run `cargo clippy -p lushtext-core -p lushtext --all-targets -- -D warnings`.
- [x] 4.5 Run `openspec validate dedupe-notes-browser-close --strict`.
- [x] 4.6 Run `git diff --check`.
- [x] 4.7 Confirm `openspec status --change dedupe-notes-browser-close --json` reports all tasks complete after implementation.
