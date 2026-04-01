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

## Size-Dependent Constraints (size_allocate vs notify)

When a widget's behavior depends on its parent's size (e.g., sidebar ≤ 1/3 window width), use `WidgetImpl::size_allocate()` — NOT property notifications:

- **`notify::default-width` / `notify::maximized`** fire *before* the new allocation is applied. Reading `window.width()` in these handlers returns the **old** stale value. This causes constraints to silently fail during maximize/unmaximize transitions.
- **`size_allocate(width, height, baseline)`** receives the **actual allocated dimensions as parameters**. No timing issues.
- **`size_allocate` is top-down only** — it fires when the widget itself is resized, not when children change internally. For child-initiated changes (e.g., user drags a `GtkPaned` divider), also connect `notify::position` on the child.
- `size_allocate` fires on every layout pass. Keep the handler cheap (comparison + maybe one `set_position`). Guard GSettings writes with a value-change check to avoid D-Bus overhead.

## Auto-Dismiss Timers (Generation Counter)

For timed UI operations (e.g., status bar message auto-dismiss), use a **generation counter** (`Cell<u32>`) instead of storing/cancelling `glib::SourceId` handles:

1. Each operation increments the counter.
2. The timer closure captures the counter value at scheduling time.
3. When the timer fires, it compares against the current counter — if different, the operation was superseded and the timer no-ops.

This avoids all `SourceId` lifecycle bugs (double-remove panics, stale handle references) and requires no cancellation logic.

## GTK4 Signal Delivery Pitfalls

Code that "looks wired correctly" can silently fail if GTK4's internal gesture system intercepts events before signals are emitted. When verifying that a user interaction reaches application code, check all three layers:

1. **Application wiring** — is the signal handler connected? (e.g., `connect_file_activated` → `open_document`)
2. **Widget signal emission** — does the widget actually emit the signal on user input? (e.g., `GtkListView::activate` on click)
3. **Gesture interception** — does an internal gesture on a child widget claim the event before the parent can process it?

**Lesson learned:** The sidebar file-activation code was correctly wired (`connect_activate` → `open_document` → `tab_view.append`), and `open_document` always creates tabs. But `GtkTreeExpander`'s internal gesture claimed all click events at BUBBLE phase, preventing `GtkListView::activate` from ever firing via mouse. The first fix attempt (`single-click-activate=true`) appeared to work for some files but (a) changed the UX to single-click which the user didn't want, and (b) still failed for the first file because the expander gesture claims events at ANY propagation phase when it runs before the list view's activation. The correct fix: a CAPTURE-phase `GtkGestureClick` on the `GtkListView` that detects `n_press == 2` and reads the `SingleSelection`'s selected position. CAPTURE fires before BUBBLE, bypassing the expander entirely. Two activation paths are needed: the CAPTURE gesture for mouse double-click, and `GtkListView::connect_activate` for Enter key (keyboard is unaffected by gesture interception).

## Testing

Every wired signal must have a widget test that asserts the expected state change (button click hides widget, entry propagates value, toggle flips state). Tests must also cover the action enabled/disabled lifecycle (disabled when no tabs, enabled after tab creation, disabled again after closing all tabs).

**Test the preconditions, not just the wiring:** When a feature depends on a GTK widget property (like `single-click-activate=true`), write a test that asserts the property value directly. This catches template regressions even when end-to-end click simulation isn't possible in headless tests.

**`is_visible()` in widget tests:** `WidgetExt::is_visible()` checks the entire parent chain — it returns `false` for any widget inside an unrealized/unpresented window (which is the case in all widget tests). To check a widget's own visibility property, use `widget.property::<bool>("visible")` instead.
