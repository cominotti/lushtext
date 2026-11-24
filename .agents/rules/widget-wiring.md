---
description: GTK widget composition, signal wiring, and widget-test rules
paths:
  - "crates/lushtext-core/src/ui/**"
  - "crates/lushtext/tests/widget/**"
  - "crates/lushtext-core/src/services/action_catalog/**"
  - "resources/ui/**"
  - "crates/gtk-lush/widgets/**"
---

# Widget Wiring Rules

Every interactive element in a widget must be fully wired -- no dead-end buttons, entries, or signals.

## Checklist

- **Buttons**: connect `clicked` (or equivalent action signal).
- **Close/dismiss**: wire close buttons, Escape key (`stop-search`, `close-request`, or the window transient-surface helper), and dismiss gestures.
- **Entries**: propagate values where needed (search queries, replace text, etc.).
- **Toggles/switches**: read and write to the relevant state.
- **Child widget signals**: connect in the parent's `constructed()` (e.g., `SearchBar.connect_close`).

## Staged Content Reveals

The window's content stack, workspace sidebar and document-properties panel are
staged (`ui/window/composition/`): invisible until their composition stage or a
forced reveal.

- **Reveal or focus staged content only after `ensure_composed`.** Code under
  `ui/window/` (outside `composition/`) that makes a staged surface visible or
  focuses a widget inside one — the editor's source view, a sidebar tree — calls
  `self.ensure_composed(StagedSurface::Content | Workspace | Properties)` first
  in the same function. It completes that surface's stage (and the content
  stage it depends on) synchronously; the cost lands in the next frame. Rule 11
  of `make check-workflow-boundaries` fails on a `set_visible(true)` or
  `grab_focus()` of a staged receiver without it.
- **Never force a stage from a read or a projection** (`size_allocate`, status
  bar, notifications, tab projections, sidebar row states, evidence, snapshots).
  Every object exists from construction, so reading one is always valid.
- **Install breakpoints before the window is mapped.** `AdwBreakpointBin`
  applies a breakpoint that matches on its first allocation after `map` before
  allocating the child; one that starts matching later — including one added
  after map — draws one frame of a frozen snapshot of the old state and swaps on
  the next tick (ledger A50). The three shell breakpoints are installed in
  `constructed()`; a new one goes there too.
- Hidden surfaces (search panel, palette, Focus Mode affordance, Markdown
  preview) are not stages: their own open paths reveal their revealer, whose
  child the composition workflow makes visible as it opens.

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

For `GtkListView` or other factory-backed rows, bind-time projection is not
enough when a row control depends on parent collection state that can change
without rebinding the row. Centralize the predicate used by `connect_bind()` and
any explicit repair path, then run a cheap synchronization over realized rows
after membership, filter-visibility, in-place refresh, or reorder changes. Reset
stale overlay or drop-target state in the same pass so recycled rows cannot keep
old affordances. If the row's child composition itself changes because of a
parent workflow mode, force a real factory/model rebind (for example clear the
`GtkListView` model before restoring it) instead of setting the same model back
and assuming GTK will re-run `connect_bind()`.

## Accessibility Wiring

Use `docs/accessibility-matrix.md` to decide which state extremes, smoke cases,
stable anchors, and manual Orca expectations a widget change must cover. The
matrix row ids should appear in new accessibility, visual, or geometry smoke
manifests when a widget surface gains or changes coverage.

New widget metadata should go through `crate::ui::accessibility` so labels,
descriptions, roles, relations, states, row metadata, and announcements stay
consistent across surfaces. Use the helper's announcement lanes for debounced
results, progress milestones, repeated status updates, and high-priority alerts
instead of announcing directly from each workflow.

For `GtkSignalListItemFactory` rows, refresh item-specific accessible metadata
with `RowAccessibility` during `connect_bind()` and clear it with
`clear_row_accessibility()` during `connect_unbind()`. This is the accessibility
equivalent of clearing stale tooltips, overlays, or signal/binding ownership:
recycled rows must not keep a previous item's name, description, selection, or
set-position metadata.

Widget tests that need metadata proof but not a live accessibility bridge can
use `ui::accessibility::test_audit::AccessibleAudit`. It complements, but does
not replace, `make accessibility-smoke` for AT-SPI-visible focus, tree, and text
behavior.

## Realized List Rows

Factory rows whose text a publish retargets draw it with `LushtextRowText`,
not `GtkLabel` (ledger A108) and never `GtkInscription` (A109); see
`.agents/rules/ui.md` "Realized List Rows". A lazy model with a realized band
is emptied by a `ui::list_retirement::RowRetirement` started after its list
is unmapped (A110), never by one `items-changed` inside an input dispatch or
a dialog's `closed` handler, and a publish into an empty lazy list grows
through `ui::list_growth::RowGrowth`. Widget tests that read row text use
`common::labels_in` / `common::visible_text_of`, which see both widgets.

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

**String-named signals go through `ui::signal_connect`.** A signal without a
typed gtk-rs `connect_*` method (keybinding signals such as `backspace` or
`extend-selection`, `GtkTextBuffer` `insert-text` as an after-handler, a
frame-clock phase with `after`, an app-defined signal such as
`content-changed`) is connected with `signal_connect::connect_void` (the
handler returns `()` or `Option<()>`) or `signal_connect::connect_returning`
(the handler returns the declared type, for example `bool` for `gboolean`),
and emitted by name with `signal_connect::emit_void` or `emit_returning`. The
helper looks the signal up and checks its declared return type **when the
handler is connected**, so a mismatch fails every test that builds the
surface. glib-rs's own `connect_local` checks the closure's
`Option<glib::Value>` only inside the emission, in GLib's C marshaller, where
the panic aborts the process: a `gboolean` handler that answered `None`
aborted LushText on every double click (`fix-extend-selection-abort`, ledger
A122). `connect`, `connect_local`, `connect_closure`, `connect_unsafe` (and
their `_id` forms) and the `emit_by_name` family are banned in
`crates/lushtext-core/src` outside `ui/signal_connect.rs`
(`make check-signal-connections`, in `make check-policy`). Widget tests under
`crates/lushtext/tests/**` and the GTK axiom probes under
`crates/gtk-lush/axioms/**` are outside that gate: they connect and emit by
name on purpose to drive the toolkit, and own their answers.

Keep ordinary untracked `connect_*` calls when the source and target share the
same widget/action lifetime and no later disconnect is needed. Keep
event-controller ownership explicit when the controller itself is the lifetime
owner. Remaining row-data uses should be scalar state, not hidden signal or
binding ownership.

## Work Between Frames

Work that must land before the next frame but must not run inside it (the
sidebar's paced collapse, `slice-large-directory-collapse` D25, ledger
A124/A125) runs from a **one-shot `G_PRIORITY_HIGH_IDLE` source**: above the
frame clock's own priority and below input, so it runs between frames even
when a frame is due, and the frame then lays out only what it changed.

- Arm it from the input dispatch, from a frame's `after-paint` on the widget's
  frame clock, and from its own dispatch while work is left; at most one is
  armed, and its id is cleared at the start of its dispatch.
- Connect the `after-paint` handler only while work is pending on a mapped
  widget, one handler per owner, on the clock the widget has now; disconnect it
  on `unmap` and reconnect it on `map`, and make any periodic fallback reconnect
  it when it finds the widget mapped without it, because a tick callback alone
  keeps the clock running without anything ever arming the source.
- A tick callback may stay registered to keep frames coming, but takes no
  step: a step inside the frame clock's update phase is a step inside the frame.
- A per-frame budget read outside a frame keys on the frame counter, which
  between frames equals the counter of the frame just painted.
- When the first frame after an input must show the input's effect alone
  (the collapse's rung T1), gate the work on a flag the input sets and the
  next `after-paint` clears, not on the frame counter: an input GDK delivers
  in a frame's event flush already carries the counter of the frame that
  paints it, so a counter charged at the input would hold the work one frame
  too long.
- Widget state the work must re-apply after GTK writes it (a `map` that resets
  an accessible state) is re-applied from the same kind of source, armed by the
  event, never from the frame tick.

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
- **`size_allocate(width, height, baseline)`** receives the **actual allocated dimensions as parameters** when GTK calls the vfunc for that subclass. Prove it fires before wiring repair logic to it. GTK4 does not call the override on a widget whose class installs a layout manager: `gtk_widget_allocate` hands the allocation to the layout manager. `LushtextEditorPage` and `LushtextMarkdownPreview` inherit the `GtkBoxLayout` path, so editor viewport reflow and the preview's code-block width repair are observed through `gtk_lush_viewport::ViewportObserver` on the text view's scroll-adjustment page sizes instead (the preview's dead override left code blocks at their old width after a resize, `quiet-headless-session-services`). `GtkWidget` has no `width` property either, so a `notify::width` handler never runs.
- **`size_allocate` is top-down only** — it fires when the widget itself is resized, not when children change internally. For child-initiated changes, connect the child's own notification. For `GtkTextView`/`GtkSourceView` viewport width or height changes, the horizontal and vertical adjustments' `page-size` values are a reliable allocation-derived signal; use `gtk_lush_viewport::RestState`/`RestPause` to keep GTK-preserved transient offsets from replacing user scroll intent. A pause also hides real scrolls made during the burst, so a repair that runs when the burst settles must not undo one: the editor page records the value each vertical `changed` emission carries and counts a `value-changed` to any other value as a scroll (ledger A112: `configure` sets the value before `changed`, while `set_value` and `scroll_to_mark` move it with no `changed`), then the settle keeps that scroll and re-records the rest state, which the pause swallowed (`restore-main-ci`). Any path that ends a burst without its settle must take the same record: a hidden minimap's refresh cancels the settle, and `record_scroll_from_cancelled_reflow` keeps the rest state true there.
- `size_allocate` fires on every layout pass. Keep the handler cheap (a comparison against a cached width, then at most one write). Guard GSettings writes with a value-change check to avoid D-Bus overhead.
- If allocation-derived geometry updates an `AdwBreakpoint` condition, cache the derived condition or threshold and call `set_condition()` only when it actually changes. Reparsing or reinstalling breakpoint conditions on every animation frame adds main-thread layout churn.
- When a `SettleBurst` or similar readiness gate protects notify handlers during a toolkit-owned animation, arm the gate **before** setting the animated property. GTK/Libadwaita notify signals can run synchronously from the setter; arming the gate afterward lets same-frame reconciliation run before the guard exists.
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

**Split-view and animated surfaces** (the allocation-frame rules; [history](../../docs/memorials/gtkpaned-shell-animation.md)):

- `size_allocate()` compares the new allocated width against a cached width before doing split-view work.
- allocation and programmatic notify paths clamp runtime geometry only; they do not persist sidebar fractions to GSettings.
- the derived properties breakpoint threshold is cached, and `AdwBreakpoint::set_condition()` runs only when that integer threshold changes.
- persistence remains tied to explicit user intent, restore, or animation completion, not to every layout tick.

## Adjustment Ownership Under Custom Containers

When a custom container hosts a `GtkScrollable` child and owns its adjustments
(`gtk_lush_widgets::ViewportSliceBin` is the in-tree case):

- Install the container's own `GtkAdjustment` on the child through
  `Scrollable::set_vadjustment` when the child is set, and detach it when the
  child is replaced or the container is disposed.
- Write the child's adjustment only inside the container's `size_allocate`,
  behind a re-entrancy flag, and read it back **after** `child.allocate(...)`:
  `GtkListView` applies `scroll_to` and focus scrolling inside its own
  allocation, so the child's request appears as a divergence from the value the
  container just wrote.
- Tell a child-originated request apart from the container's own write by
  comparing against **the offset the container published**, never against the
  unclamped viewport position. The two agree only when the container starts
  exactly at the viewport's top edge; everywhere else the difference is read as
  a standing request and the container scrolls the outer window to satisfy it.
  Measure the resulting travel against the unclamped viewport top, and apply it from
  an idle: moving an outer adjustment from inside a layout pass does not
  reliably schedule another layout.
- Allocate the child exactly the band the viewport shows: deduct chrome drawn
  above the child at the top **and** content the viewport has scrolled past at
  the bottom. A child decides for itself whether a row is on screen, from its
  own allocation, so a band taller than the visible area makes it place
  revealed rows behind chrome, and a band left at a scrolled-past edge makes a
  `scroll_to` of one of its rows a no-op. Never let the band reach zero: a
  zero-height allocation makes `GtkListView` rewrite the adjustment the
  container owns (ledger A5), which reads as a request. Keep one pixel plus the
  child's learned inset at the nearest edge when nothing shows.
- **Anchor forwarded requests.** Record the outer value a batch's first request
  was measured against and have the idle set `anchor + accumulated delta`, not
  add the delta to the outer's current value: several containers sharing one
  outer scroller measure in the same frame against the same value, and adding
  lands the outer at the sum. Keep accumulating within one container's batch.
- **Re-announce a value you moved.** A `value-changed` a `GtkListView` sees
  before it is allocated at the new value leaves its scroll anchor stray (ledger
  A20); emit `value-changed` once more after `child.allocate(...)` whenever the
  publish changed the value, or a later page change (window resize) re-derives
  the value along that stray line.
- Treat a divergence as a settle, and write it back, when it cannot be a
  request: after a publish that emitted `value-changed` (it dropped any pending
  `scroll_to`, ledger A9), or after a page or content-height change the
  container made under an anchor it set. Forwarding it scrolls the outer on a
  resize.
- Re-slice on an outer `notify::page-size` from an idle: `GtkViewport` delivers
  it after it has allocated its child (ledger A19), and an allocation queued
  there is still pending when the frame is drawn.
- Never re-queue an allocation when the outer adjustment refused to move
  (`set_value` clamps and emits nothing); that is the loop guard.
- Report the child's full content as the container's natural height, and as its
  minimum too while an outer scroller is present, because `GtkViewport`
  allocates non-scrollable children their minimum in the scroll direction.

## GtkPaned

LushText has no `GtkPaned`. Before adding one (see `docs/next/split-panes.md`), read the `GtkPaned` sections of the `gtk4-libadwaita-internals` skill's geometry reference; the retired shell's paned rules are in [`docs/memorials/gtkpaned-shell-animation.md`](../../docs/memorials/gtkpaned-shell-animation.md).

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

These helpers are stable in-tree platform crates consumed through workspace path
dependencies; publication is governed by `crates/gtk-lush/GOVERNANCE.md`.

## Large Lists: Budgets, Time-Sliced Batches, Per-Batch Caches

A list that can grow with the workspace is bounded by retained bytes and time
slices, never by an item count:

- **Bound what is retained by bytes, not by a count.** Charge each item through
  `model::list_budget::RetainedBudget` (a refusal latches, so what is retained
  is a prefix of delivery order and "showing X of Y" stays true), size the
  budget as the power of two at or above a `derived` floor that is at least
  the worst case the replaced count allowed (a compile-time assertion
  per floor), and state any cut in the UI, to assistive technologies (once)
  and in the automation snapshot. A parallel producer serializes admission
  with its send, so channel order is admission order.
- **Apply rows in batches while the turn's time slice lasts.** Loop
  `turn_continues(elapsed_us, units_done)` (`LIST_TURN_SLICE_US`, the clock
  read by the GTK caller) over bounded units: one `splice` per touched store
  per turn, never one `append` per item. A presented `GtkListView` pays a
  fixed cost per `items-changed` (A87), while an end-of-store batch splice
  and a tail `set_expanded` grow sub-linearly in the store (A88, A89).
- **Keep derived caches current per applied batch.** A terminal that rebuilds
  a cache over the whole list grows with the list; splice the cache with the
  rows the batch inserted or removed instead (`splice_child_cache`,
  property-tested equal to the full rebuild), and maintain it only for the
  store the list currently shows: a `GtkTreeListModel` can create more than
  one child store for one row (A22's `is_expandable()` probe).
- **Re-project bound rows, not the model.** A mode switch that changes how
  rows render rebinds the rows the factory has bound (a registry filled in
  `bind` and emptied in `unbind`), never `set_model(None)`/`set_model(model)`,
  which rebuilds the list over its whole model.

## GTK4 Signal Delivery Pitfalls

Code that "looks wired correctly" can silently fail if GTK4's internal gesture system intercepts events before signals are emitted. When verifying that a user interaction reaches application code, check all three layers:

1. **Application wiring** — is the signal handler connected? (e.g., `connect_file_activated` → `open_document`)
2. **Widget signal emission** — does the widget actually emit the signal on user input? (e.g., `GtkListView::activate` on click)
3. **Gesture interception** — does an internal gesture on a child widget claim the event before the parent can process it?

**Own a toolkit gesture by claiming its sequence, not by reconfiguring it.** On GTK 4.24.1 `GtkTreeExpander`'s toggle `GtkGestureClick` lives on its `expander` icon child, which GTK destroys and re-creates with the row's expandability, and the expander itself carries none (ledger A119), so a bind-time `observe_controllers()` pass over the expander finds nothing. The sidebar takes the arrow click over with one capture-phase gesture installed on the expander at setup: it lets the press through and claims the sequence on a release inside the current icon, before the icon's bubble gesture sees it, so the row's press-side behaviour stays GTK's whatever icon GTK has created since. When a child widget's gesture must not act, claim the sequence it would claim, in a phase that runs first, at the moment and area it claims ([history](../../docs/memorials/tree-expander-activation.md)).

**Keyboard shortcut variant of the same pitfall:** `GtkListView` keyboard focus usually lands on a realized row widget inside the list, not on the `GtkListView` wrapper itself. For list-wide shortcuts such as sidebar `Space` peek, attach the `EventControllerKey` to the list view in `PropagationPhase::Capture` and gate the handler against focused controls that should still own their keys (for example inline rename `GtkEntry`s or row buttons). If the shortcut logic assumes `list_view.has_focus()` or only works when tests emit the key directly on the list widget, real users will see "nothing happens" even though synthetic tests pass.

**First-activation rendering paths:** Signals such as `notify::visible-child-name` often do more work on the first activation than on later toggles. If the handler renders Markdown, replaces placeholders, mounts scrollers, or otherwise changes the active page's child tree, verify the first user activation as its own path. A mode switch that looks wired correctly can still be wrong if the first activation changes dialog geometry, focus, or content padding.

**MenuButton popup lifecycle:** Do not rebuild, clear, or replace a `GtkMenuButton`'s `menu-model` from `notify::active` or any other popup-open path. GTK is actively creating the popover at that point, and replacing the model can cancel the visible popup even though the menu model looks correct in tests. Keep the model ready from ordinary state refreshes, skip no-op `set_menu_model()` calls, and cover regressions with a widget test that calls `MenuButton::popup()` / `popdown()` and asserts active or popover-visible state rather than only inspecting `GMenuModel`.

## Testing

Every wired signal must have a widget test that asserts the expected state change (button click hides widget, entry propagates value, toggle flips state). Tests must also cover the action enabled/disabled lifecycle (disabled when no tabs, enabled after tab creation, disabled again after closing all tabs).

**Read the workflow's evidence surface, not per-field test getters.** A migrated
workflow exposes one typed evidence surface (`evidence.rs`) that is the single
source of its observable state, and widget tests read that value instead of a
`pub fn *_for_test` getter per counter, pending flag, queue depth, bound, or
freshness token. Every workflow in the tree is migrated or otherwise terminal, so
a fact the surface does not expose is added to the surface rather than to a new
inspection function; the gate ratchets the externally reachable `*_for_test` count
recorded in `docs/workflow-readability-matrix.md`.

The surface's invariants — one accessor per surface (or per object for a nested
two-object home), no mutation on read, `try_get()` for disposed children including
transitive reach, no toolkit work or lazy materialization, bounded aggregation
over child sets, no read inside a mutable borrow, and quiescing worker-thread
counters before comparing — plus the three driven proofs each surface owes, are
normative in
[`.agents/rules/workflow-convention.md`](./workflow-convention.md), as is the
one-`test_policy.rs` rule for test-only timing and limit overrides. Test-only actuation seams remain a deferred category; see
the `gtk-testing` skill's seam taxonomy before adding one.

Widget tests that need accessibility metadata proof but not a live accessibility
bridge can use `ui::accessibility::test_audit::AccessibleAudit`; it complements,
but does not replace, `make accessibility-smoke`.

**State-extreme coverage is mandatory for collection and browser surfaces:** If a
widget presents rows, tabs, notes, bookmarks, snapshots, command results, search
results, workspaces, files, or any other variable-sized set, do not test only
the normal populated path. Cover the relevant state matrix:

- no items or no required context, especially when the action should still be
  reachable without an open tab or workspace;
- one or a few representative items;
- many items, long labels, deep paths, or capped results when the surface can
  encounter them.

When multiple actions select modes of one browser, reuse one live dialog and
one bounded source/query owner. Mode is part of source, query, preview, and
activation generation identity. Widget coverage must switch modes repeatedly
and rapidly, prove stale completions cannot publish, preserve mode-specific
empty/truncation/accessibility copy.

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

**`spawn_blocking_then` results in tests:** Tests that depend on results from `gtk_lush_tasks::spawn_blocking_then` (e.g., command palette search results, file index rebuilds, workspace folder adds) must wait for the background thread to complete before asserting. `flush_events()` alone is insufficient — it only drains what's already on the main loop, but the background thread may not have queued its completion yet (completions are delivered at `gtk_lush_tasks::completion_priority()`). Use `spin_until(|| predicate())` or `wait_until(timeout, || predicate())` to pump the main loop until results arrive. Without this, tests are flaky under parallel execution because thread scheduling varies.

**Wait helpers are shared — do not copy them:** Use `wait_until` / `flush_events` / `flush_after_delay` / `present_window` from `crates/lushtext/tests/widget/common.rs`. They are imported, never re-defined per module: a private copy is missed when the shared helper changes. `wait_until` sleeps briefly then **drains all ready main-loop sources** — that drain is required to dispatch `spawn_blocking_then`'s completions, delivered from one source at `gtk_lush_tasks::completion_priority()`. Do not rewrite it to a single blocking `MainContext::iteration(true)`: a higher-priority timeout source starves the idle and async-backed waits time out (verified failing 5/5).

`present_window` in particular is **defined** in `common.rs` (LushText-side, generic over `IsA<gtk4::Window>`), not the weaker `gtk_lush_proof_harness::present_window` that only presents and flushes. The shared helper presents, then waits on a realization/allocation predicate with a ≥5s async-scale budget, then drains a short post-realization settle. Do not re-add a per-module `fn present_window`. A caller that needs an extra settle adds its own `flush_after_delay` after calling `present_window`.

**Budget the wait to what it actually waits on:** Async/scheduling-dependent waits (window realization, `spawn_blocking_then` completion, file I/O) need a generous budget (≥5–10s); a tight 2s budget flakes under load. The predicate returns the instant the work lands, so a larger ceiling costs nothing on the fast path and only matters when a loaded machine delays the work. Keep short budgets for synchronous UI-state flips. If a widget test "passes on retry" or prints `FLAKY:`, treat it as a blocker per `preexisting-blockers.md` and the `gtk-testing` skill's Flake Discipline — investigate and fix the cause, do not just bump a number.

**Timed animations in the custom widget harness:** Presented-window tests do not reliably advance `AdwTimedAnimation` frame clocks under the `crates/lushtext/tests/widget.rs` subprocess harness. Do not write tests that wait for the real animation duration to elapse. If the assertion depends on the settled post-animation state, expose a narrow test-only immediate-completion path keyed off `LUSHTEXT_WIDGET_CHILD`, or assert a state transition that does not depend on frame-clock progress.

**Live warnings are a separate acceptance gate:** a change touching a `GtkRevealer` or a heavy animated sidebar subtree also needs a real `make run` cycle with restored workspaces and an stderr check for `Trying to measure …` warnings; widget tests are not sufficient (`testing.md`, "Runtime Warnings").
