# Widget Wiring Rules

Every interactive element in a widget must be fully wired -- no dead-end buttons, entries, or signals.

## Checklist

- **Buttons**: connect `clicked` (or equivalent action signal).
- **Close/dismiss**: wire close buttons, Escape key (`stop-search`, `close-request`, or the window transient-surface helper), and dismiss gestures.
- **Entries**: propagate values where needed (search queries, replace text, etc.).
- **Toggles/switches**: read and write to the relevant state.
- **Child widget signals**: connect in the parent's `constructed()` (e.g., `SearchBar.connect_close`).

## Action Enabled State

Actions that depend on app state (e.g., `save`, `toggle-search`, `close-tab` require an active tab) must disable themselves when preconditions are not met. Use `SimpleAction::set_enabled(bool)` so menu items gray out and shortcuts become no-ops.

**Initialization order matters:** `add_action_entries()` creates actions with `enabled = true`. If initial state should be disabled, call the state-sync method (e.g., `update_content_stack()`) _after_ `setup_actions()` in the constructor.

## Declarative Projection Bindings

Use GTK-native bindings for pure state-to-view projections when the source and
target are already GObjects and the binding can express the existing behavior
without changing workflow order. Prefer `gio::Settings::bind` for direct
settings-to-widget projections and `ObjectExt::bind_property` for direct
object-to-object projections. Keep transforms pure and small.

Keep imperative handlers when the code performs persistence, async work,
debounce/search, model or factory recycling, focus restoration, notification
lifecycle, readiness/layout orchestration, or any ordered side effect. If a
binding replaces only the projection part of a handler, keep the side-effect
handler explicit and document the split in the local audit or test when the
ordering is easy to misread.

## Signal And Binding Lifetimes

Use `gtk_lush_signals` for GTK registration ownership that has an explicit
teardown or recycling lifecycle:

- `SignalBag` for signal handlers connected to settings, style managers,
  buffers, search contexts, tree rows, or other GObjects that can outlive the
  widget/workflow closure they call.
- `BindingBag` for row-local or workflow-local `glib::Binding` values that must
  be unbound when a factory row is recycled or a projection is rebound.
- `RegistrationBag` for controller-like cleanup callbacks that are not
  represented by a signal id or binding.

Keep ordinary untracked `connect_*` calls when the source and target share the
same widget/action lifetime and no later disconnect is needed. Keep
event-controller ownership explicit when the controller itself is the lifetime
owner. Remaining row-data uses should be scalar state, not hidden signal or
binding ownership.

## Action Catalog And Automation Docs

Every user operation exposed through an app action, window action, command
palette command, static menu item, keyboard shortcut, context menu action group,
or automation-only target-state action must have a row in
`services::action_catalog` unless it is intentionally private widget plumbing.
When adding or changing an externally observable action, update
`docs/automation.md` and `docs/automation-reference.md`, keep the row's
enablement, parameter, state, safety, surfaces, and coverage lanes current, and
run `make check-automation-docs`.

Prefer target-state actions for automation setup when the surface has toggle
semantics (`set-*-visible`, `set-focus-mode`, `set-preview-mode`). Target-state
actions must drive the same production path as the user toggle and then be
assertable through the read-only automation snapshot.

## Overlay Widgets and CSS

Widgets rendered via `GtkOverlay` must use opaque backgrounds. Adwaita's `@card_bg_color` is 80% transparent — use `@window_bg_color` for overlay containers like the search bar. If a freeze or snapshot overlay sits above a live native widget while that widget repaints underneath, give the cover an opaque background matching the captured surface; otherwise transparent pixels can leak the live widget's stale rendered state through the snapshot.

## Tab-Dependent UI Updates

Any UI element that displays per-tab state (status bar metadata, title bar subtitle) must refresh when the active tab changes. Wire `tab_view.connect_notify_local(Some("selected-page"), ...)` in `constructed()` to react to tab switches.

**Pair every structural tab operation** (`new_tab`, `open_document`, `close-tab`) with explicit refresh calls for all tab-dependent UI. Do not rely solely on GTK property notifications — signal ordering during `close_page()` is not guaranteed, and `selected-page` may not fire when closing a non-selected tab.

If a structural tab operation selects an editable document, it must also clear window-level projections that can hide or replace the editor surface, such as Markdown preview-only mode. These projections can outlive the previous selected tab, so reset their action state and visible surface before scheduling editor focus restoration.

## Markdown Preview Presentation

Markdown preview uses an Adwaita-owned shell: `AdwMultiLayoutView` switches the
single preview widget between the full-content preview layout and the
end-position `AdwOverlaySplitView` sidebar in the editor layout. Do not
reintroduce a preview-specific `GtkPaned`, timed paned animation, or
`shrink-start-child`/`shrink-end-child` choreography for preview transitions.

Preview actions must keep these states mutually exclusive:

- editor-only: `preview_layout_view=editor`, `preview_split_view.show-sidebar=false`;
- side-by-side: `preview_layout_view=editor`, `preview_split_view.show-sidebar=true`;
- preview-only: `preview_layout_view=preview`, `preview_split_view.show-sidebar=false`.

After switching preview visibility, preview layout, fixed preview width, or
Focus Mode readable-column margins, queue the preview layout-settle path so
embedded Markdown code blocks repair their width after allocation. Automation
may still report the compatibility blocker `preview-animation`, but it means
preview layout/code-block repair is pending, not that a custom animation exists.

## Size-Dependent Constraints (allocation signals vs notify)

When a widget's behavior depends on its parent's size (e.g., sidebar ≤ 1/3
window width), prefer the closest live allocation signal over property
notifications, and prove that signal actually fires in the relevant widget:

- **`notify::default-width` / `notify::maximized`** fire *before* the new allocation is applied. Reading `window.width()` in these handlers returns the **old** stale value. This causes constraints to silently fail during maximize/unmaximize transitions.
- **`size_allocate(width, height, baseline)`** receives the **actual allocated dimensions as parameters** when GTK calls the vfunc for that subclass. Prove it fires before wiring repair logic to it. GTK4 can skip a subclass override on widgets whose class installs a layout manager; `LushtextEditorPage` inherits the `GtkBoxLayout` path, so editor viewport reflow must be observed through `gtk_lush_viewport::ViewportObserver` on the source view's scroll-adjustment page sizes instead.
- **`size_allocate` is top-down only** — it fires when the widget itself is resized, not when children change internally. For child-initiated changes (e.g., user drags a `GtkPaned` divider), also connect `notify::position` on the child. For `GtkTextView`/`GtkSourceView` viewport width or height changes, the horizontal and vertical adjustments' `page-size` values are a reliable allocation-derived signal; use `gtk_lush_viewport::RestState`/`RestPause` to keep GTK-preserved transient offsets from replacing user scroll intent.
- `size_allocate` fires on every layout pass. Keep the handler cheap (comparison + maybe one `set_position`). Guard GSettings writes with a value-change check to avoid D-Bus overhead.
- If allocation-derived geometry updates an `AdwBreakpoint` condition, cache the derived condition or threshold and call `set_condition()` only when it actually changes. Reparsing or reinstalling breakpoint conditions on every animation frame adds main-thread layout churn.
- If `notify::position` also persists state, suppress that persistence while a programmatic paned animation is in flight. Clamp can stay live every frame; debounced settings writes should run once from the animation completion path.
- Treat `notify::position` primarily as the **user-drag** path. If a timed animation is already driving valid paned positions directly, short-circuit the `notify::position` handler while that animation is active so the same frame is not reprocessed as if it were a manual drag.
- When a layout action is expected to leave neighboring chrome unchanged,
  protect that chrome with same-session visual geometry comparisons rather
  than relying on separate before/after launches. Protected regions must be
  exact except for declared masks, and allowed-changing regions must have
  geometry-anchor assertions such as scroll anchors, visibility, and allocation
  bounds.
- When the visual effect itself is rendered from code, CSS, or native toolkit
  styling, verify the rendered pixels with screenshot-derived `pixel_anchors`
  instead of comparing only an inferred rectangle. Automation1 may expose broad
  crop bounds, readiness, scale, and diagnostic anchors, but those app-owned
  rectangles are not a substitute for the screenshot detector. For example, a
  minimap viewport highlight needs geometry assertions plus a mandatory
  pixel-anchor assertion for its native top edge; nearby content-row anchors can
  be added by scenarios that need them, but the rendered highlight must not rely
  on rectangle-only checks.
- If Automation exposes a raw toolkit-derived geometry estimate, keep it
  separate from the crop-safe geometry consumed by visual surfaces and pixel
  anchors. Raw estimates are useful diagnostics, but crop/search rectangles must
  be fitted to the visible allocation they describe so off-surface algebra cannot
  make widget tests fail or make screenshot searches miss the rendered pixels.

**Known Flatpak animation regression:** sidebar and document-properties open/close animations looked like they were running below the monitor refresh rate even though no obvious blocking I/O was present. The root cause was allocation-frame churn: `size_allocate()` repeatedly synchronized split-view widths, the properties fraction notify path rewrote GSettings, and the adaptive properties breakpoint was reparsed/reinstalled for each animated frame. The durable fix pattern is:

- `size_allocate()` compares the new allocated width against a cached width before doing split-view work.
- allocation and programmatic notify paths clamp runtime geometry only; they do not persist sidebar fractions to GSettings.
- the derived properties breakpoint threshold is cached, and `AdwBreakpoint::set_condition()` runs only when that integer threshold changes.
- persistence remains tied to explicit user intent, restore, or animation completion, not to every layout tick.

## GtkPaned Position Constraints

Any code that sets a `GtkPaned` position must ensure it's valid for the current allocation width. GTK4's `measure()` phase runs BEFORE `size_allocate()` — if a paned position is stale from a previous frame, GTK warns "Trying to measure ... for width of X, but needs at least Y."

**Position restore from GSettings**: Always pre-clamp in `constructed()` immediately after `set_position()`. Use the restored window width from GSettings as the `for_width` parameter. Store the original unclamped value in `saved_*_pos` for animations that target the preferred position at wider widths.

**Hidden restore state**: If the pane starts hidden, do **not** leave the live `GtkPaned::position` at the expanded saved width. Restore the live position to the same collapsed endpoint the hide animation uses, while `saved_*_pos` keeps the preferred visible width.

**Animation targets**: When computing an animation target for a paned position (e.g., sidebar show), start from the saved preferred width but clamp the target against the current allocation before writing it. Clamp per-frame animation writes too — do not rely on `size_allocate` / `notify::position` to sanitize an already-invalid animation tick. If async child population can change the budget (for example restored workspaces adding sidebar sections), refresh the measured budget immediately before the animation starts.

**Real-session proof beats harness-only proof**: Presented widget tests can verify action state, wrapper visibility, and persistence guards, but they can still miss live `GtkBox ... needs at least ...` warnings. Any sidebar/paned animation fix must also be exercised through `make run` against restored workspaces while watching stderr. Treat widget green + live warning as a failed fix, not a partial success.

**Animation persistence**: Programmatic paned animations may still trigger `notify::position` and `size_allocate` on every frame. Those paths may clamp, but they must not enqueue debounced GSettings writes on every tick. Persist the preferred visible width once from `connect_done` (or the immediate-completion test path) after the animation settles.

**Clamp against the real end-child**: If the warning references the end-child container (for example `GtkBox`), budget against that container's measured minimum, not a nested child that usually dominates it. One-pixel mismatches often come from clamping against the inner stack while GTK is actually measuring the wrapper box.

**Use GTK's legal opposite-axis floor, not only the live-height floor**: `gtk_widget_measure()` validates a supplied `for_width` or `for_height` against the opposite orientation measured with `-1` before it continues. For paned budgeting, resolve each child's width floor as `max(measure(Horizontal, -1), measure(Horizontal, current_height))`. If you only use the height-adjusted floor, the app can still emit `Trying to measure ... needs at least ...` warnings even though the clamp looked correct in app code.

**Prefer GTK's runtime paned budget**: Once a `GtkPaned` is allocated, prefer `max-position` / `min-position` over reverse-engineering the legal range from child widths. Those properties already include the current handle width and realized child constraints.

**But know where `max-position` comes from**: GTK source (`gtkpaned.c`) computes positions with the handle widget's measured natural size and the current child minimums. During animated `GtkRevealer` transitions those minimums can round up by one pixel. If a live warning persists, inspect GTK's own `gtk_paned_compute_position` / `gtk_revealer_measure` behavior before adding more local clamp churn.

**Wrap zero-width pane animations**: If a pane must fully disappear, wrap the real child widget in a `GtkRevealer` (or equivalent clipping wrapper) and animate the paned against the wrapper, not the raw child. Keep the wrapper revealed while the paned is shrinking, then drop `reveal-child` and `visible` together when the animation finishes so GTK stops reserving handle width while hidden without changing the layout budget too early.

**Heavy child optimization**: If the wrapped pane contains a large tree/list that causes visible stutter, consider swapping the live child out for a frozen snapshot during the animation. But do not assume an intermediate container stops affecting layout; validate that the real child is truly out of the paned's measurement path in the live app.

**Hide-time clamps stay live until the wrapper is hidden**: A toggle action may set a logical `*-visible` flag to `false` before the `GtkRevealer` has actually left layout. Clamp/budget helpers must keep treating the pane as layout-active while the wrapper's own `visible` property is still true, or a stale restored position can slip into the first hide frame.

**New paned children**: When adding a new child to a `GtkPaned` with `shrink-end-child=false`, set `width-request` on the end-child to match its measured minimum. This makes the paned's minimum constraint explicit and prevents GTK from even attempting to measure the child at below its minimum during layout negotiation. Re-sync that width-request after map or async child restoration if the realized minimum changes.

## Focus Restoration on Overlay Close

When an overlay widget steals focus (command palette, search bar, inline rename), the close path **must** explicitly restore focus. GTK4's default behavior after `GtkRevealer.set_reveal_child(false)` walks the widget tree to the first focusable widget — typically a sidebar button, not the editor.

**Pattern for window-level overlays** (command palette):
1. Before opening: save `window.focus()` into a `RefCell<Option<glib::WeakRef<gtk4::Widget>>>` on the imp struct.
2. On close: take the saved ref, `upgrade()` it, and call `grab_focus()`. If the widget is gone (tab closed), fall back to `active_editor().source_view().grab_focus()`. If no editor exists, call `window.set_focus(Widget::NONE)`.
3. Use `glib::WeakRef` (not a strong ref) to avoid preventing widget finalization.

**Pattern for editor-level overlays** (search bar):
- The focus target is always the same editor's `source_view` — no saved state needed. Call `source_view.grab_focus()` in the close handler.

**Do not rely on GTK4's automatic focus assignment** after hiding a revealer or removing a widget from the focus chain.

## Window-Level Transient Surface Dismissal

Shell-owned transient surfaces must share a single topmost dismissal order instead of each surface installing unrelated Escape behavior.

- Install window-level Escape handlers in `PropagationPhase::Bubble` so focused children, dialogs, popovers, dropdowns, and text entries get first chance to handle Escape.
- If Escape reaches the shell, close exactly one topmost visible dismissible surface and return handled. Do not close the palette, search bar, search panel, menus, and Focus Mode all from one key press.
- Focus Mode exit comes after transient dismissal. If the command palette or another dismissible surface is visible above Focus Mode, the first Escape closes that surface; the next Escape may exit Focus Mode.
- Command-palette click-away belongs at the window shell boundary. Primary presses outside the palette bounds must call `close_command_palette()` so the existing saved-focus restoration and cleanup path runs. Presses inside palette controls, result rows, scrollbars, and child popup/dropdown roots must not dismiss it.
- When click-away dismisses the palette, claim or otherwise consume that pointer sequence so the same press does not also activate an underlying shell control.

## Superseding Timers And Settle Helpers

For timed UI operations where only the newest request should run, use the
`gtk_lush_settle` helpers instead of hand-rolled generation counters:

- `Debounce` for trailing input or persistence work after a quiet window
  (search entries, index rebuilds, workspace/session persistence, refreshes).
- `SupersedingTimer` for one delayed action that is re-armed by newer state
  (status pulses, Focus Mode affordance hide, first-dirty draft autosave).
- `SettleBurst` for layout or repair work that must expose queryable pending
  state to readiness (preview layout settle, minimap reflow settle).

Keep recurring pollers, heartbeats, explicit lifecycle sources, idle/chunked
population loops, worker-result freshness tokens, and pure domain/model
generations explicit. Those are not settle helpers unless a later OpenSpec
change adds a dedicated primitive and proves the timing unchanged. Markdown
code-block repair currently remains explicit because it pairs idle and timeout
sources with `SourceId` cancellation and completion callbacks.

`gtk-lush-settle`, `gtk-lush-tasks`, `gtk-lush-viewport`, and
`gtk-lush-widgets` are functional in-tree crates, not Phase 5
publication-ready external dependencies. LushText may consume them through the
workspace path dependencies; external stability still waits for the publishing
gates in `crates/gtk-lush/GOVERNANCE.md`.

## GTK4 Signal Delivery Pitfalls

Code that "looks wired correctly" can silently fail if GTK4's internal gesture system intercepts events before signals are emitted. When verifying that a user interaction reaches application code, check all three layers:

1. **Application wiring** — is the signal handler connected? (e.g., `connect_file_activated` → `open_document`)
2. **Widget signal emission** — does the widget actually emit the signal on user input? (e.g., `GtkListView::activate` on click)
3. **Gesture interception** — does an internal gesture on a child widget claim the event before the parent can process it?

**Lesson learned (3 iterations):** The sidebar file-activation code was correctly wired (`connect_activate` → `open_document` → `tab_view.append`), and `open_document` always creates tabs. But `GtkTreeExpander`'s internal `GtkGestureClick` intercepted mouse clicks at BUBBLE phase, preventing `GtkListView::activate` from firing. Fix attempt 1 (`single-click-activate=true`) changed the UX to single-click. Fix attempt 2 (CAPTURE-phase gesture) was fragile and failed for the first file due to `SingleSelection::selected()` timing. The correct fix: disable the expander's gesture for file rows by setting `propagation_phase = None` via `observe_controllers()` in `connect_bind`. This lets `GtkListView`'s built-in double-click activation work for files while preserving directory expand/collapse. When a child widget's internal gesture blocks parent behavior, **disable the gesture at the source** rather than trying to race it from a higher phase.

**Keyboard shortcut variant of the same pitfall:** `GtkListView` keyboard focus usually lands on a realized row widget inside the list, not on the `GtkListView` wrapper itself. For list-wide shortcuts such as sidebar `Space` peek, attach the `EventControllerKey` to the list view in `PropagationPhase::Capture` and gate the handler against focused controls that should still own their keys (for example inline rename `GtkEntry`s or row buttons). If the shortcut logic assumes `list_view.has_focus()` or only works when tests emit the key directly on the list widget, real users will see "nothing happens" even though synthetic tests pass.

**First-activation rendering paths:** Signals such as `notify::visible-child-name` often do more work on the first activation than on later toggles. If the handler renders Markdown, replaces placeholders, mounts scrollers, or otherwise changes the active page's child tree, verify the first user activation as its own path. A mode switch that looks wired correctly can still be wrong if the first activation changes dialog geometry, focus, or content padding.

**MenuButton popup lifecycle:** Do not rebuild, clear, or replace a `GtkMenuButton`'s `menu-model` from `notify::active` or any other popup-open path. GTK is actively creating the popover at that point, and replacing the model can cancel the visible popup even though the menu model looks correct in tests. Keep the model ready from ordinary state refreshes, skip no-op `set_menu_model()` calls, and cover regressions with a widget test that calls `MenuButton::popup()` / `popdown()` and asserts active or popover-visible state rather than only inspecting `GMenuModel`.

## Testing

Every wired signal must have a widget test that asserts the expected state change (button click hides widget, entry propagates value, toggle flips state). Tests must also cover the action enabled/disabled lifecycle (disabled when no tabs, enabled after tab creation, disabled again after closing all tabs).

**State-extreme coverage is mandatory for collection and browser surfaces:** If a
widget presents rows, tabs, notes, bookmarks, snapshots, command results, search
results, workspaces, files, or any other variable-sized set, do not test only
the normal populated path. Cover the relevant state matrix:

- no items or no required context, especially when the action should still be
  reachable without an open tab or workspace;
- one or a few representative items;
- many items, long labels, deep paths, or capped results when the surface can
  encounter them.

For these tests, assert the user-visible contract, not only model state: the
right empty copy is shown, required header/menu/close controls are visible,
item regions scroll instead of expanding the shell, critical controls are not
pushed off-screen, and no unintended horizontal or vertical scrollbar appears
in an empty status-only surface. When an allocation, overflow, or rendered
legibility bug prompted the change, add an assertion for that failure mode
directly. If the bug is about pixels staying identical while another surface
changes, pair the widget test with the visual geometry lane so the proof captures
both screenshots in one process and compares protected regions after
`visual-geometry-settled`.
If the bug is about a rendered edge, highlight, marker, overlay, or other
effect staying aligned, add widget tests for the app-owned allocation/projection
and add visual geometry `pixel_anchors` that detect the actual screenshot
pixels. The root visual geometry summary must list the matching
`pixel_verified_invariant_ids` when the changed files require a named pixel
invariant, and the case rows must include screenshot-derived pixel evidence plus
final-frame rendered-anchor stability plus final sidebar/editor/minimap
geometry. For live-only regressions, capture the current Automation1 state with
`scripts/lushtext-automation.py
visual-geometry-capture ...` and replay the generated headless scenario before
accepting the fix.

**Test the preconditions, not just the wiring:** When a feature depends on a GTK widget property (like `single-click-activate=true`), write a test that asserts the property value directly. This catches template regressions even when end-to-end click simulation isn't possible in headless tests.

**`is_visible()` in widget tests:** `WidgetExt::is_visible()` checks the entire parent chain — it returns `false` for any widget inside an unrealized/unpresented window (which is the case in all widget tests). To check a widget's own visibility property, use `widget.property::<bool>("visible")` instead.

**GTK4 Window Resizing in Tests:** `set_default_size` does not shrink a window that has already been presented. If a test needs to verify layout behavior at multiple widths (e.g., breakpoint collapse), instantiate separate windows at each target size or instantiate narrow from the start.

**`spawn_blocking_then` results in tests:** Tests that depend on results from `gtk_lush_tasks::spawn_blocking_then` (e.g., command palette search results, file index rebuilds, workspace folder adds) must wait for the background thread to complete before asserting. `flush_events()` alone is insufficient — it only drains what's already on the main loop, but the background thread may not have posted its `idle_add_once` callback yet. Use `spin_until(|| predicate())` or `wait_until(timeout, || predicate())` to pump the main loop until results arrive. Without this, tests are flaky under parallel execution because thread scheduling varies.

**Wait helpers are shared — do not copy them:** Use `wait_until` / `flush_events` / `flush_after_delay` / `present_window` from `crates/lushtext/tests/widget/common.rs`. They are imported, not re-defined per module (there were once five copy-pasted `wait_until`s; fixing one missed the rest). `wait_until` sleeps briefly then **drains all ready main-loop sources** — that drain is required to dispatch `spawn_blocking_then`'s low-priority `idle_add_once` completion. Do not rewrite it to a single blocking `MainContext::iteration(true)`: a higher-priority timeout source starves the idle and async-backed waits time out (verified failing 5/5). A private copy in a test module also gets missed when the shared helper changes.

**Budget the wait to what it actually waits on:** Async/scheduling-dependent waits (window realization, `spawn_blocking_then` completion, file I/O) need a generous budget (≥5–10s); a tight 2s budget flakes under load. The predicate returns the instant the work lands, so a larger ceiling costs nothing on the fast path and only matters when a loaded machine delays the work. Keep short budgets for synchronous UI-state flips. If a widget test "passes on retry" or prints `FLAKY:`, treat it as a blocker per `preexisting-blockers.md` and the `gtk-testing` skill's Flake Discipline — investigate and fix the cause, do not just bump a number.

**Timed animations in the custom widget harness:** Presented-window tests do not reliably advance `AdwTimedAnimation` frame clocks under the `crates/lushtext/tests/widget.rs` subprocess harness. Do not write tests that wait for the real animation duration to elapse. If the assertion depends on the settled post-animation state, expose a narrow test-only immediate-completion path keyed off `LUSHTEXT_WIDGET_CHILD`, or assert a state transition that does not depend on frame-clock progress.

**Live paned warnings are a separate acceptance gate:** If a change touches `GtkPaned`, `GtkRevealer`, or a heavy animated sidebar subtree, widget tests are necessary but not sufficient. The acceptance checklist must include a real `make run` cycle with restored workspaces and an stderr check for `Trying to measure GtkBox ...` warnings.
