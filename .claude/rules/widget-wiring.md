# Widget Wiring Rules

Every interactive element in a widget must be fully wired -- no dead-end buttons, entries, or signals.

## Checklist

- **Buttons**: connect `clicked` (or equivalent action signal).
- **Close/dismiss**: wire close buttons, Escape key (`stop-search`, `close-request`), and dismiss gestures.
- **Entries**: propagate values where needed (search queries, replace text, etc.).
- **Toggles/switches**: read and write to the relevant state.
- **Child widget signals**: connect in the parent's `constructed()` (e.g., `SearchBar.connect_close`).

## Action Enabled State

Actions that depend on app state (e.g., `save`, `toggle-search`, `close-tab` require an active tab) must disable themselves when preconditions are not met. Use `SimpleAction::set_enabled(bool)` so menu items gray out and shortcuts become no-ops.

**Initialization order matters:** `add_action_entries()` creates actions with `enabled = true`. If initial state should be disabled, call the state-sync method (e.g., `update_content_stack()`) _after_ `setup_actions()` in the constructor.

## Overlay Widgets and CSS

Widgets rendered via `GtkOverlay` must use opaque backgrounds. Adwaita's `@card_bg_color` is 80% transparent — use `@window_bg_color` for overlay containers like the search bar.

## Testing

Every wired signal must have a widget test that asserts the expected state change (button click hides widget, entry propagates value, toggle flips state). Tests must also cover the action enabled/disabled lifecycle (disabled when no tabs, enabled after tab creation, disabled again after closing all tabs).
