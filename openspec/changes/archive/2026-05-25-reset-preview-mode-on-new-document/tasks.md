## 1. Preview Shell State

- [x] 1.1 Add a `LushtextWindow` helper in `ui/window/preview.rs` that synchronously exits preview-only mode and keeps `preview_mode`, `toggle-preview-mode` action state, animation state, paned shrink state, and preview/editor visibility aligned.
- [x] 1.2 Ensure the helper preserves side-by-side preview state and only targets preview-only mode.

## 2. New Document Flow

- [x] 2.1 Call the preview-only reset from `LushtextWindow::new_tab()` so all New Document surfaces reveal the source editor before the delayed focus handoff completes.
- [x] 2.2 Confirm the existing stale-selection focus guard still prevents focus from returning to a no-longer-selected new tab.

## 3. Regression Coverage

- [x] 3.1 Add a window widget test that enters Markdown preview-only mode, activates New Document, and asserts preview-only mode is cleared.
- [x] 3.2 Extend the test to assert the new untitled tab is selected, the editor shell is visible, the preview-only action state is false, and the new editor receives focus.
- [x] 3.3 Confirm no user-facing docs need changes because the shortcut, menu label, and side-by-side preview behavior remain unchanged.

## 4. Verification

- [x] 4.1 Run `cargo fmt --check`.
- [x] 4.2 Run the focused window widget test for the preview-only New Document regression.
- [x] 4.3 Run `openspec validate reset-preview-mode-on-new-document --strict`.
