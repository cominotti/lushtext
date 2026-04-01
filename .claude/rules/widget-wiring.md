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

## Tab-Dependent UI Updates

Any UI element that displays per-tab state (status bar metadata, title bar subtitle) must refresh when the active tab changes. Wire `tab_view.connect_notify_local(Some("selected-page"), ...)` in `constructed()` to react to tab switches.

**Pair every structural tab operation** (`new_tab`, `open_document`, `close-tab`) with explicit refresh calls for all tab-dependent UI. Do not rely solely on GTK property notifications — signal ordering during `close_page()` is not guaranteed, and `selected-page` may not fire when closing a non-selected tab.

## Auto-Dismiss Timers (Generation Counter)

For timed UI operations (e.g., status bar message auto-dismiss), use a **generation counter** (`Cell<u32>`) instead of storing/cancelling `glib::SourceId` handles:

1. Each operation increments the counter.
2. The timer closure captures the counter value at scheduling time.
3. When the timer fires, it compares against the current counter — if different, the operation was superseded and the timer no-ops.

This avoids all `SourceId` lifecycle bugs (double-remove panics, stale handle references) and requires no cancellation logic.

## Testing

Every wired signal must have a widget test that asserts the expected state change (button click hides widget, entry propagates value, toggle flips state). Tests must also cover the action enabled/disabled lifecycle (disabled when no tabs, enabled after tab creation, disabled again after closing all tabs).

**`is_visible()` in widget tests:** `WidgetExt::is_visible()` checks the entire parent chain — it returns `false` for any widget inside an unrealized/unpresented window (which is the case in all widget tests). To check a widget's own visibility property, use `widget.property::<bool>("visible")` instead.
