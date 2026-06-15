## 1. Header Template Order

- [x] 1.1 Move `open_menu_button` before `new_tab_button` in `resources/ui/window.blp` without changing either control's object ID, action name, tooltip, accessible metadata, or popover wiring.
- [x] 1.2 Regenerate `resources/ui/window.ui` and `resources/ui/template-contract.json` from Blueprint.
- [x] 1.3 Review the generated diff to confirm only the intended start-side relative order changed for Open and New.

## 2. Focused Coverage

- [x] 2.1 Add or update widget tests proving the header start-side order is Open before New in the live `LushtextWindow` widget tree.
- [x] 2.2 Keep shortcut/action assertions for `Ctrl+K`, `Ctrl+O`, and `Ctrl+N` so the reorder does not change command behavior.
- [x] 2.3 Add or update constrained-header coverage proving the compact folder-symbolic Open presentation remains before New and both controls stay reachable.

## 3. Visual And Accessibility Proof

- [x] 3.1 Add or update visual geometry coverage so a rendered header state proves Open appears before New in the GNOME-style header surface.
- [x] 3.2 Ensure constrained geometry coverage catches clipping, overlap, or unintended horizontal scrolling caused by the reordered controls.
- [x] 3.3 Confirm existing accessibility anchors remain stable for Open and New after the visual order changes.

## 4. Validation

- [x] 4.1 Run `make check-blueprint`.
- [x] 4.2 Run the focused widget test target covering header control order and Open popover behavior.
- [x] 4.3 Run `make visual-geometry-smoke` or the focused visual geometry lane that includes the header order cases.
- [x] 4.4 Run `make pre-commit`.
- [x] 4.5 Run `openspec validate order-open-before-new-tab --strict` and `git diff --check`.
