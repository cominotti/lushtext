# Design

## Context

See `proposal.md` (Why). The tab strip is one `AdwTabBar` bound to the window's
`AdwTabView` (`resources/ui/window.blp:102-109`). It has no start or end action
widget and no scroll controller of LushText's. Pin, reorder and bulk close are
`WFR-TAB-STRIP` (`ui/window/tab_strip/`). The strip's visibility is
`sync_tab_bar_visibility` (`ui/window/documents.rs:938-943`). Tab-derived
projections are coalesced during tab storms by
`tab_projection_refresh_defer_depth` and published once by
`refresh_tab_model_projections` (`documents.rs:1140-1175`).

### Verified toolkit facts

Read from Libadwaita sources at tags `1.9.4` (GNOME 50 floor) and `1.10.0`
(shipped GNOME 51). The two tags differ in `adw-tab-box.c` only by
`0` → `GDK_NO_MODIFIER_MASK` in three binding calls. `adw-tab-bar.c`,
`adw-tab-bar.ui` and `adw-tab-view.c` are byte-identical. Line numbers are from
`1.10.0`; fetch at `https://gitlab.gnome.org/GNOME/libadwaita/-/raw/<tag>/src/<file>`.
Facts marked *(inferred)* are read from source and proven only by the probe named.

| # | Fact | Source |
|---|---|---|
| F1 | The wheel handler is a `GTK_EVENT_CONTROLLER_SCROLL_VERTICAL` controller. It acts only for `GDK_SOURCE_MOUSE` devices and propagates every other source. | `adw-tab-box.c:974-1004`, `:3776-3777` |
| F2 | The only overflow cue is an edge fade (`FADE_WIDTH` 36 px, `FADE_OFFSET` 6 px), drawn while `value > 0` or `value + page_size < upper`. | `adw-tab-box.c:40-41`, `adw_tab_box_snapshot` |
| F3 | The bar's template is `GtkRevealer` → `GtkBox` → [`start_action_bin`, `pinned_scrolled_window` (CSS class `pinned`), `scrolled_window`, `end_action_bin`]. Both scrollers use `hscrollbar-policy=external`. The start action widget sits **before the pinned tabs**. | `adw-tab-bar.ui` |
| F4 | `AdwTabBar:is-overflowing` is read-only and public. It is recomputed on each adjustment's `changed` as `is_overflowing(unpinned) \|\| is_overflowing(pinned)`. While `resize_frozen`, a computed `FALSE` does not clear a current `TRUE` (the OR at `:247`), but frozen never turns a current `FALSE` into `TRUE` (the early return at `:244` runs first). `TabBar::is_overflowing()` is ungated in libadwaita-rs 0.9. | `adw-tab-bar.c:227-255`, `:649-657`, `:785-791` |
| F5 | The box rewrites its adjustment inside its own allocation: `gtk_adjustment_configure(value, 0, allocated_width, 0.1·width, 0.9·width, width)`, under `block_scrolling`. So `changed` and `notify::is-overflowing` can fire inside allocation. | `adw-tab-box.c:3294-3307` |
| F6 | The configured `value` is `get_scroll_animation_value`. That is the adjustment value **unless the box's own scroll animation is `PLAYING` or `FINISHED`**; then it is that animation's target. `FINISHED` lasts until the next allocation resets it. An outside `value-changed` pauses the box's animation (`:885`). *(inferred)* An outside write therefore holds while the box animation is idle or playing. A write made after the box's animation finished, but before the next allocation in the same frame, can be overwritten by that allocation. **Probe A132.** | `adw-tab-box.c:822-837`, `:869-888`, `:1014-1018`, `:3309-3313` |
| F12 | `gtk_adjustment_configure` emits `changed` only when a property other than `value` changes (it sets the value without a notify, then thaws). A configure that moves only the value emits a bare `value-changed`, indistinguishable from `gtk_adjustment_set_value`. This is consistent with ledger A112. | GTK `4.22.2` `gtk/gtkadjustment.c:833-871` |
| F7 | Selecting a page scrolls the strip **only if the tab is outside the page**. The scroll is minimal, with padding `min(tab_width, page − tab_width)/2`. | `adw-tab-box.c:929-961`, `select_page` |
| F8 | `adw_tab_view_open_overview` makes child-visible only pages whose `live_thumbnail` or `invalidated` is set. | `adw-tab-view.c:249-256`, `:4838-4856` |
| F9 | In RTL the box lays tabs out from the right and mirrors the value on a direction change, so the adjustment's lower end is the end of the tab sequence. | `adw-tab-box.c:3275`, `:3535-3546`, `scroll_cb`'s flip |
| F10 | `AdwTabView` binds `Ctrl+PgUp/PgDn`, `Alt+1..9` and `Alt+0`. LushText removes only the `Ctrl+Home/End` family. | `adw-tab-view.c:2150-2243`; `ui/window/actions.rs:810-824` |
| F11 | `set_end_action_widget(w)` shows `end_action_bin` iff `w != NULL`, whatever `w`'s own visibility. | `adw-tab-bar.c:1024-1042` |

## Goals / Non-Goals

**Goals:** the requirements in `specs/tab-strip-overflow-navigation/spec.md`,
using public API wherever it exists, with the one non-public reliance gated by
probes.

**Non-Goals (design level):**

- no custom tab-strip widget and no fork of `AdwTabBox`;
- no change to tab sizing;
- no change to `WFR-TAB-STRIP`'s pin, reorder and close stages or its evidence.

## Decisions

### D1. All controls in one trailing group: `|PP| tabs | ‹ › [N]`

Back, Forward and All Tabs are one `GtkBox` set as
`AdwTabBar:end-action-widget`.

- *Rejected: Back as `start-action-widget`.* By F3 it would sit left of the
  pinned tabs, not beside the scrolling ones, and the tabs would shift right when
  overflow begins. The explore sketch drew this and could not have been built as
  drawn.
- *Rejected: an arrow between the pinned and unpinned tabs.* No public slot exists
  there.
- Side-by-side arrows let a repeated click stay in one place.

**Empty group:** with nothing to show (fewer than two tabs and no overflow), the
group is removed with `set_end_action_widget(None)`. If the All Tabs popover is
open at that moment (a tab closed down to one, or Focus Mode entered while it is
open), it is popped down, with D6's focus restore, before the group is
detached. By F11 a hidden child would
leave a visible `end_action_bin` with its padding. It is set again when something
shows.

### D2. Visibility comes from the unpinned scroller; reconciling is deferred

- **Show** the scroll buttons when the unpinned adjustment overflows
  (`upper − page_size ≥ 1 px`, D4). **Hide** them when it no longer overflows,
  measured with the buttons shown.
  - Showing narrows the strip, so it stays overflowing; hiding widens it, so it
    still fits. That is a hysteresis band of the buttons' width with no
    oscillation at a fixed width (spec "Between the two widths").
  - Pinned-only overflow shows nothing (spec "Only pinned tabs overflow"),
    which `is-overflowing` alone would get wrong (F4).
- **Hold during close animations:** while `is-overflowing` is `TRUE` and the
  pinned adjustment does not overflow, a hide is postponed. That case is F4's
  frozen hold, so the buttons do not jump under the pointer while a closed tab
  animates.
- **Enablement** of Back and Forward comes from the unpinned adjustment only (D4).
- **Deferred.** By F5 the signals can arrive inside the box's allocation. A
  visibility change there queues a resize inside a layout pass. This is avoided by
  analogy with the in-frame invalidation faults the ledger records (A37 for
  `GtkTextView`), not by a measured fault of `AdwTabBar`. The observer only
  schedules one reconcile through a `gtk_lush_settle::SettleBurst`. That makes
  the pending state queryable for readiness and coalesces bursts to the latest
  value. Probe A133 records whether the notify runs inside allocation on each
  toolkit.
- **During restore and tab storms** the reconcile is a tab-derived projection.
  While `tab_projection_refresh_deferred()`, it waits, and
  `refresh_tab_model_projections` runs it once at the terminal. The controls
  therefore appear once after a restored session's tabs are in place (spec
  "Restoring a session with many tabs").
- **First frame:** none of these controls appear in the first frame of a
  window. The strip itself is shown there only for queued activation opens
  (`ui.md`, "Staged Window Composition"). Those pages are admitted in stage B1,
  after the first frame, and the controls follow the projection publish. The
  shift that comes with them is the same one adding tabs causes, inside the
  strip only; the content below does not move. The visual-geometry case checks
  that the strip height is constant (task 2.8).

### D3. Scrolling writes the unpinned scroller's adjustment, gated by probe A132

- **Lookup.** `find_unpinned_tab_scroller(&gtk::Widget) -> Option<gtk::ScrolledWindow>`
  is a pure-in-GTK function over a root widget. It walks at most the template's
  depth (F3) and returns the first `GtkScrolledWindow` without CSS class
  `pinned`. The pinned scroller is found the same way, for D2. It runs once per
  bar, after the bar's template is built.
- **Step.** The step is the adjustment's own `page-increment` (0.9 × visible
  width, F5). The target is clamped to `[lower, upper − page_size]`.
- **Motion.** An `AdwTimedAnimation` with a `PropertyAnimationTarget` on the
  adjustment's `value`, `TAB_SCROLL_ANIMATION_MS` (200, matching
  `SCROLL_ANIMATION_DURATION`, `adw-tab-box.c:33`), ease-out cubic. It completes
  at once when animations are off. A new step while one plays starts from the
  playing step's target.
- **Cancellation by the strip's own input.** By F12 a foreign write cannot be
  told from the box's own allocation by adjustment signals alone. The box also
  starts its own scrolls in many places (`adw-tab-box.c` 1.10.0): selection and
  focus on the selected tab (`:1729`), a page attach (`:1928`), keyboard reorder
  (`:1301`), a press on a partly visible tab (`:2963`), drag placeholders
  (`:2220`, `:2332`, `:2630`), and drag autoscroll (`:1463`). The guard
  therefore cancels our animation and the repeat source on every public event
  that precedes one of them, so the strip's own scroll wins.
  **Cancelling means `adw_animation_pause()`, never `reset()` or `skip()`.**
  Both of those write a value into the adjustment, and that outside
  `value-changed` would pause the box animation that just started
  (`adw-tab-box.c:882-885`). The guarded events are:
  - `AdwTabView` `notify::selected-page`, `page-attached`, `page-detached`,
    `page-reordered` and `notify::is-transferring-page`. Without these, our
    per-frame writes would pause the box's `scroll_to_tab` animation (F6), and a
    newly selected tab could stay out of view (spec "Selecting a tab during a
    scroll").
  - A capture-phase `GtkEventControllerScroll` on the `AdwTabBar`, flags
    `BOTH_AXES` and no `KINETIC`. Its handler always returns propagate, so it
    runs before the box's bubble-phase vertical controller (F1) and steals
    nothing (spec "Wheel during a scroll step").
  - A capture-phase `GtkGestureClick` on the `AdwTabBar` that observes
    `pressed` and never claims. It uses `set_button(0)` so it sees any button,
    and it also fires on touch. It covers a press on a partly visible tab, the
    start of a reorder drag and drag autoscroll, none of which has a public
    signal. It stays in state NONE, so it never denies the box's bubble-phase
    gestures (`adw-tab-box.c:3780-3796`).
  - A press on our own end group is ignored. The observer resolves
    `bar.pick(x, y)` and tests whether the result descends from the group. Both
    observers act, and count, only while a step or hold is active, so presses
    and scrolling inside the open popover (whose parent chain runs through the
    bar) are inert.
  - A direction change.
  - While `is-transferring-page` is true, a step requested by keyboard or
    automation is refused: the box's drag autoscroll (`:1463`) owns the strip
    then.
- **One-frame race, accepted.** By F6, a step written in the same frame in which a
  box scroll animation reaches `FINISHED`, before that frame's allocation, can be
  reverted to the box's target. The user sees a step that did not move, and the
  next click works. There is no re-assert mechanism: F12 gives no reliable way to
  detect the revert. Probe A132 records whether the race reproduces on each
  toolkit. If it does, it is listed in the ledger row as a known toolkit
  interaction.
- **Hold to repeat.** The scroll buttons are `focus-on-click: false`, so a pointer press never takes focus from the editor. A `GtkGestureClick` on each scroll button in
  `PropagationPhase::Capture` handles the pointer:
  - a press does one step at once and claims the sequence, so no `clicked`
    and no extra step on release;
  - after `TAB_SCROLL_REPEAT_DELAY_MS` (400) it repeats every
    `TAB_SCROLL_REPEAT_INTERVAL_MS` (200) until release, cancel, the edge, the
    guard, or the button hiding.

  The repeat is one explicit `glib::SourceId`, because a recurring source is not a
  `gtk_lush_settle` helper (`widget-wiring.md`, "Superseding Timers"). It is
  cancelled on each of those terminals and on dispose. Keyboard activation
  (Space or Enter on the focused button) goes through the action: one step,
  without repeat.
- **Focus.** A focused button that becomes disabled hands focus to the other
  scroll button if it is enabled, else to All Tabs, else to the selected editor
  (spec "A focused button becomes disabled"). A focused button that hides does
  the same, skipping the hidden ones.
- **Direction.** Back is toward the first tab. In RTL it maps to the
  adjustment's upper end (F9), and the icons are `go-previous-symbolic` and
  `go-next-symbolic`, which GTK mirrors.
- **Fail closed.** If the lookup returns `None`, the scroll buttons never show,
  both scroll actions stay disabled, and evidence reports
  `scroller_found: false`. Nothing is logged at warning level: the lanes treat
  warnings as failures, and this is a supported degradation. All Tabs is
  unaffected. This is tested without an actuation seam:
  - a pure policy test over `scroller_found`;
  - a lookup test on a hand-built, bar-shaped widget tree with no unpinned
    scroller.
- **If probe A132, or a gating item of A133 (the overflow notify, the held
  overflow with the end widget, the unchanged height), fails on either toolkit**,
  implementation stops at task 1.4
  and the probe results go back to the maintainer, because the scroll buttons are
  the user's literal request. Shipping All Tabs alone is one option then, not a
  decision this design takes.
- *Rejected: buttons that select the previous or next tab.* They switch the
  document being read, and `Ctrl+PgUp/PgDn` already does that.
- *Rejected: synthesized scroll events.* They have no device, so F1 drops them.

### D4. A new workflow, `WFR-TAB-OVERFLOW-NAVIGATION`, with its own role home

Scrolling the strip and picking a tab from a list are new user operations with
their own stage order: observe overflow → reconcile controls → scroll, or list →
filter → select. That is not `WFR-TAB-STRIP` (pin, reorder, bulk close), so it
gets its own row instead of growing that row's policy and evidence.

The role home is a **nested** per-workflow subdirectory,
`crates/lushtext-core/src/ui/window/tab_overflow/`, with the popover widget in
its own widget subdirectory (`workflow-convention.md`, "Role homes": "a directory
and a widget subdirectory of it").

| File | Role |
|---|---|
| `tab_overflow/mod.rs` | narrative facade: the stage order and the two inversions (the settle-deferred reconcile; the popover's activation callback) |
| `tab_overflow/policy.rs` | pure policy (whole pixels, below) |
| `tab_overflow/watch.rs` | coordination (`watch`): observes both adjustments, `is-overflowing`, `selected-page` and the tab model; owns the `SettleBurst` reconcile and the projection deferral |
| `tab_overflow/scroll_execution.rs` | coordination (stage-qualified `execution`): lookup, step animation, cancellation by the strip's own input, hold repeat |
| `tab_overflow/evidence.rs` | evidence (`TabOverflowEvidence`, `test-utils`-gated) |
| `tab_overflow/test_policy.rs` | test-only timing: immediate animation completion and short repeat delays, compiled only with `test-utils` |
| `tab_overflow/controls.rs` | **called presentation surface**: builds the end group, buttons, icons, tooltips and accessible names, and the focus hand-off |
| `tab_overflow/all_tabs.rs` | **called presentation surface**: window-side snapshot rows, popup wiring, focus save and restore, and `select_tab_from_all_tabs` |
| `tab_overflow/all_tabs_popover/{mod,imp,item}.rs` | **called presentation surfaces**: widget, factory and row object |

**Policy** (`policy.rs`, `#![forbid(clippy::float_arithmetic,
clippy::disallowed_methods)]`, a new whole-pixel table row at `forbid` / 0) works
over `StripScrollExtent { value_px, page_px, upper_px }`. The adapter rounds
each `f64` once to the nearest whole pixel. Functions:

- `unpinned_overflows`, `controls_visible` (show, hide and hold, from D2);
- `can_scroll_back`, `can_scroll_forward`, `next_scroll_target`, in reading
  order;
- `all_tabs_button_visible(tab_count, strip_shown)`;
- `group_attached` (D1);
- `scroll_controls_available(scroller_found)`;
- `focus_after_disable`;
- the untitled-row subtitle, the empty-state copy, the match-count
  announcement, and the button's accessible name.

The 200 ms animation runs on `f64` in the adapter, because it animates a GTK
adjustment value, which the convention keeps fractional.

**Evidence fields:**

- `scroller_found`, `controls_visible`, `group_attached`, `can_scroll_back`,
  `can_scroll_forward`;
- `animation_active`, `repeat_active`, `reconcile_pending`,
  `guard_cancellations`;
- `all_tabs_button_visible`, `all_tabs_count`;
- `popover_visible`, `popover_rows`, `popover_query_len`,
  `popover_highlighted_position`, `row_growth_pending`,
  `row_retirement_pending`.

All are read through `try_get()`, with no lazy materialization and no read
inside a mutable borrow. The surface owes the three driven proofs.

The matrix row is new, with every file declared by backticked path and its
measured cells derived. `ui/window/AGENTS.md` lists `tab_overflow/` among the
window's workflows. `WFR-TAB-STRIP` is unchanged.

### D5. The All Tabs button is a `GtkMenuButton` showing the full count

- A flat `GtkMenuButton` with `focus-on-click: false`. Its child is a label with
  the open-tab count in tabular figures (Adwaita `numeric`).
- Visible per `all_tabs_button_visible`.
- Its count follows the same deferred projection as D2, so it updates once at a
  restore's terminal.
- *Rejected: `AdwTabButton`.* It is an action button and cannot own a popover. A
  `GtkPopover` needs a parent that presents it during its own allocation, and
  `GtkMenuButton` is GTK's widget for that.
- *Rejected: show it only on overflow.* `Ctrl+Shift+O` would have no anchor
  whenever the tabs fit, and the count is useful at a glance.
- The count is never abbreviated, so no display cap or constant exists. Its width
  is measured at 1, 2 and 3 digits and with large text (task 3.1). The strip's
  usable width at the 640 px window minimum (`window.blp:7`) is asserted there
  too.

### D6. The All Tabs popover

`LushtextAllTabsPopover` is a `GtkPopover` subclass with template
`resources/ui/all-tabs-popover.blp`. It follows the shape of
`ui/open_popover/` (template `resources/ui/open-popover.blp`): a
`GtkSearchEntry` above a `GtkListView` in a `GtkScrolledWindow` with a bounded
natural height, plus an empty state.

It is **not** a copy of Recent Open's row code. Those rows use `GtkLabel`
(`ui/open_popover/imp.rs:810-811`), and Recent Open has no staged publish or
retirement.

- **Rows** draw text with `LushtextRowText` (`ui.md`, "Realized List Rows"),
  because filtering rebinds rows. This is the helper's first popover consumer, so
  it owes the in-situ measure comparison against a `GtkLabel` (task 3.1).
  - A row shows the title, then the path or "Not saved to a file", the
    unsaved-changes dot, a pinned icon and a current-tab check.
  - `RowAccessibility` is set at bind and cleared at unbind.
- **Snapshot, not live widgets.** At popup, and on tab changes while open, the
  window takes one compact row per tab: a weak `TabPage` reference, title,
  display path, and the modified, pinned and selected flags. It holds no
  document text and does no buffer read. Untitled tabs are included. The
  palette's `Open Tabs` group is file-backed only
  (`ui/window/palette_shell.rs:366-389`).
- **Filtering** uses `services::fuzzy::FuzzyQuery`, the matcher Recent Open's
  service uses (`services/recent_documents.rs`). It scores the title first, then
  the path, on the GTK thread. Ordering and highlight:
  - an empty query keeps strip order and highlights the selected tab;
  - a non-empty query ranks by score and highlights the first row.

  The cost is O(tabs) short-string scores per keystroke. The
  `all_tabs_filter_1024` bench case records a figure (Timing below).
- **Lists:**
  - the first publish into the empty list is staged with
    `ui::list_growth::RowGrowth`;
  - after the popover is unmapped, realized rows are retired with
    `ui::list_retirement::RowRetirement`.

  Retained bytes are the open tabs' titles and paths, bounded by the tabs. There
  is no `RetainedBudget` cut and no "showing X of Y".
- **Live updates** while visible: page attach and detach, `notify::title`,
  pinned, and the editor's modified and path changes go through one
  `gtk_lush_settle::Debounce` (zero delay, latest wins) to a re-snapshot. The
  query and the highlighted page are kept.
- **Activation** closes the popover and calls `select_tab_from_all_tabs(&TabPage)`.
  A dead reference is a no-op. Otherwise it selects the page, the strip scrolls
  by F7 after D3's guard cancels any scroll of ours, and focus goes to the
  editor through `ui/window/editor_focus/`.
- **Dismissal and focus:**
  - The popover's own Escape and click-away handling runs first, so the
    window's transient ladder is not reached.
  - Focus is saved in `GtkMenuButton`'s create-popup function
    (`set_create_popup_func`), which runs before the popover takes focus on
    both click and action. It is stored as a `glib::WeakRef`.
  - A close that did not activate a row restores that focus, else the selected
    editor, else `set_focus(None)`. This overrides the menu button's default of
    focusing itself.
  - `Ctrl+Shift+O` while open closes it the same way.
- **Announcement:** the match count goes through `ui::accessibility`'s debounced
  results lane, never per keystroke.
- *Rejected: the palette's `Open Tabs` category.* It is file-backed only, a
  whole-content overlay with mode and workspace groups, and fed by the worker
  pipeline. It is the wrong weight for picking one of the window's tabs, and
  changing its source would move the palette's grouped-result specs.

### D7. `AdwTabOverview` rejected

It must be the window's content root (restructuring `window.blp`, staged
composition and the `ClipBin` shell). By F8, a never-shown background tab keeps
an empty thumbnail. A real one needs `invalidated`, which makes the page
child-visible. That reattaches and lays out its document, which
`ui/editor_page/view_attachment.rs` (A81–A83) prevents.

### D8. Actions and shortcut

| Action | Accel | Enabled when |
|---|---|---|
| `win.scroll-tabs-back` / `win.scroll-tabs-forward` | none | the buttons are visible and `can_scroll_*` holds |
| `win.toggle-all-tabs` | `Ctrl+Shift+O` | the All Tabs button is visible; toggles the popover |
| `win.set-all-tabs-visible` (b) | none | as above; target-state for automation (`widget-wiring.md`, "Action Catalog") |
| `win.set-all-tabs-query` (s) | none | the popover is visible; precedent `win.set-open-popover-query` (`services/action_catalog/mod.rs:398`) |

- `Ctrl+Shift+O` is free in `ui/window/actions.rs:764-798`, `app.rs:301-315`,
  `resources/` and `AdwTabView` (F10). A widget test proves that no
  `GtkTextView` or `GtkSourceView` default binding claims it in a focused
  editor. It also enters `resources/ui/shortcuts.blp` and README's shortcut
  table.
  - *Rejected: `Alt+0`* (selects the tenth tab).
  - *Rejected: `Ctrl+Shift+A`* (no mnemonic).
- Every action gets a `services::action_catalog` row and is documented in
  `docs/automation.md` and `docs/automation-reference.md`.

### D9. Automation and proof surfaces (delta `specs/dbus-automation-spine/spec.md`)

- **Snapshot** (production accessors; the evidence is `test-utils`-gated, so no
  `EVIDENCE_PROJECTIONS` entry):
  - `window.tab_strip.overflowing`, `scroll_back_enabled`,
    `scroll_forward_enabled` and `tab_count`;
  - `surfaces.all_tabs_popover_visible`;
  - `active_transient_surface` gains `all-tabs-popover`, ranked after
    `open-popover` (`ui/automation.rs:2327-2344`).
- **Readiness:** a new blocker `tab-overflow` is pending while any of these
  holds:
  - our scroll animation, the repeat source, or the reconcile `SettleBurst`;
  - the popover's `RowGrowth` or `RowRetirement`.

  `visual-geometry-settled`, `accessibility-settled` and `idle` include it, and
  a `tab-overflow` workflow event is derived from it like the existing workflow
  events. Without it, a smoke could capture mid-animation.
- **Visual-geometry surfaces:** `tab-strip-overflow-controls` (the end group's
  bounds) and `all-tabs-popover`, following the open popover's entries
  (`ui/automation.rs:1579-1608`). Both have pixel anchors on the rendered
  buttons. They are reported absent with reasons `controls-hidden` and
  `popover-closed`. The generic `active-transient` surface
  (`push_active_transient_surface`, `ui/automation.rs:1562-1577`) also
  reports the popover.
- **One derivation.** The blocker, the snapshot fields and `TabOverflowEvidence`
  all read one production state, the workflow's `TabOverflowState` held by the
  window. The evidence is a `test-utils` view of that state, so the exported
  contract and the surface cannot drift (`workflow-convention.md`, "Evidence
  surfaces"). No `EVIDENCE_PROJECTIONS` entry is added, since the surface itself
  is not compiled in production.

### D10. New axioms

- **A132:** `AdwTabBar`'s unpinned tabs live in the first `GtkScrolledWindow` of
  its template box without CSS class `pinned`. An outside write of that
  scroller's hadjustment `value` must:
  - move the tabs (a tab's computed bounds shift by the written delta);
  - hold across the next two frames when the box's animation is idle;
  - observe whether a write made in the frame in which a selection-driven box
    animation finishes is reverted by that frame's allocation (F6, the accepted
    race), recorded as an observation rather than a pass condition;
  - pause a box animation that is playing.

  Its `page-increment` is 0.9 × `page-size`, and in RTL the lower end is the
  last tab. The probe uses a presented bar with 40 tabs, plus a template-shape
  control.
- **A133:** `AdwTabBar:is-overflowing` notifies across the overflow threshold.
  The probe records whether each notify runs inside `size_allocate`. The
  unpinned adjustment's overflow stays true with a 3-button end widget attached
  at the width where it first became true. Attaching or removing the end widget
  leaves the bar's allocated height unchanged. Selecting an out-of-view page
  scrolls it inside the page (F7).
- Both get `crates/gtk-lush/axioms` probes and samples (catalogue,
  `probes/mod.rs`, `fixtures.rs` and `CHANGELOG.md`, as A122 did in
  `3a086b0e`), checked on `org.gnome.Sdk//50` and `//51`.
- **Probe results (task 1.3, 2026-11-18).** Both hold on the host (GTK 4.24.1 /
  Adw 1.10.0) and inside `org.gnome.Sdk//50` (4.22.2 / 1.9.0) and `//51`.
  Three findings the implementation relies on:
  - **Right-to-left is the interface direction, not a widget direction.** The
    box reads its own `gtk_widget_get_direction`, and a direction set on the
    `AdwTabBar` alone did not reach it (the lower end still showed the first
    tab). The probe therefore sets the default direction, as an RTL locale
    does. The implementation reads the bar's effective direction
    (`widget.direction()`) at each step and observes `direction-changed` on
    the bar, which a default-direction change reaches.
  - **The one-frame race reproduces.** `race_outcome` was `reverted` inside
    both SDKs and on one of two host runs (`held` on the other, where the
    write's tick ran before the animation's). It stays the accepted race of D3,
    listed in the A132 row as a known toolkit interaction.
  - **Every `is-overflowing` notify ran inside allocation** on both toolkits,
    which confirms D2's deferral. Each page attach also scrolls the strip to
    the new tab, which is one of D3's guarded events.
- **Existing axioms relied on:**
  - A1: the popover list is a real scroller under the realized-row cap;
  - A108 and A110: row text and retirement;
  - A81–A83: the hidden-tab contract behind D7.
  - A37 is cited only as an analogy in D2.

### D11. Implementation decisions (scroll half, tasks 2.1–2.8)

Recorded during implementation, each with its reason:

- **No unpinned tab, no overflow.** With every tab pinned the unpinned tab box
  is not reallocated and its adjustment keeps the range of the tabs it held, so
  the reconcile reads `unpinned_overflows` only while
  `n_pages > n_pinned_pages`. Without it, pinning every tab of an overflowing
  strip would leave the scroll buttons shown over an empty box.
- **A new action surface, `tab-strip-control`.** No existing
  `ActionSurface` named a button in the tab strip's end group
  (`header-button` is the header bar), so `ActionSurface::TabStripControl`
  joins the catalog vocabulary; the All Tabs half reuses it.
- **A `tab-bar` visual surface.** The visual-geometry case checks the strip's
  rectangle (so its height) before and after a step; no surface named the bar.
- **The visual-geometry case steps Forward** in a restored session of 41 tabs
  rather than crossing the overflow point in one session: no single action
  crosses it at a fixed size. The session's first tab is the case's activation
  file, so the first tab is selected and the strip is not at its end; the
  relationship requires Forward enabled before the step and Back enabled after
  it. The start position itself is not fixed: the restore selects the first tab
  and then attaches the rest, and each attach scrolls the strip to its new tab
  (A132), so a restored strip can show neither its first nor its last tab, with
  the selected first tab out of view. That is main's behaviour, recorded as an
  open item for the restore workflow, not changed here.
- **Widget tests turn `gtk-enable-animations` off** so the tab box's own
  appear, close and scroll animations complete at once, and wait for one painted
  frame before writing the adjustment: a box scroll that completed at once stays
  `FINISHED` until the box's next allocation, and a write before it is reverted
  (the A132 race, here deterministic). In the running app the next frame
  follows at once, so this does not change the accepted-race analysis of D3.
- **A selection change moves focus into the strip.** With keyboard focus inside
  the bar (on a scroll button), `AdwTabBox`'s `select_page` focuses the newly
  selected tab (`adw_tab_bar_tabs_have_visible_focus`), before the reconcile
  runs. That is toolkit behaviour and keeps focus in the strip; the hand-off
  rule of D3 applies when a focused button disables or hides with the selection
  unchanged.
- **The keyboard step refused during a tab drag is not widget-tested.**
  `is-transferring-page` is read-only and set only by a real drag, so the
  refusal is covered by the guard in `step_tab_strip` and the live check (task
  4.3); the cancellation it triggers is widget-tested by emitting its notify.

## Risks / Trade-offs

- **[A future Libadwaita changes the template or the adjustment handling]** →
  The probes fail in CI on both toolkits first. At runtime the lookup fails closed
  (D3), and All Tabs keeps working.
- **[Our writes fight the strip's own scrolling]** → The cancellation events
  (D3) make the strip's scroll win. They are tested during a hold (selection,
  page attach, reorder, transfer, and the observers' signals emitted directly),
  and counted in evidence.
- **[A step reverted in the frame a box animation finishes]** → An accepted
  one-frame race (D3), observed by A132. The user's next click works.
- **[The All Tabs button takes width from the tabs]** → One button's width,
  measured in task 3.1, only with two or more tabs. The proposal states it as
  user-visible impact.
- **[A layout shift when controls appear after restore]** → One shift, inside the
  strip only, at the restore terminal rather than per batch (D2).
- **[`GtkMenuButton` refocusing itself]** → The focus saved in the create-popup
  function is restored (D6). Tested from the editor by shortcut and by click.
- **[Hold repeat outliving its button]** → One owned `SourceId`, cancelled on
  every terminal. Tests hide the button mid-hold.
- **[Animations do not advance in presented widget tests]** → `test_policy.rs`
  completes them at once under `test-utils` (`widget-wiring.md`, "Timed
  animations"). Cancellation is therefore tested during a hold, whose repeat
  source stays pending under a long test delay. Tests start and end the hold by
  emitting the hold gesture's `pressed` and `released` signals by name, and they
  emit the bar's observers, found through `observe_controllers()`, the same
  way. Widget tests are outside `check-signal-connections`. A claim is a no-op
  without an event sequence. Cancellation is asserted as `repeat_active` and
  `animation_active` false, `guard_cancellations` incremented, and the
  adjustment value unchanged by the cancellation. The box's own animation does
  not advance in these tests either, so nothing more can be asserted. Writing the adjustment directly
  only proves that enablement follows the position. Pointer behaviour (no extra
  step on release, `focus-on-click`) cannot be synthesized in widget tests, which
  create no event sequence. Widget tests assert the properties (`focus-on-click`,
  the gesture's phase and claim). The real press is proved in the accessibility
  smoke and the live check (task 4.3).

## Timing

Timing: none (no timed path touched). The `all_tabs_filter_1024` bench case is
regression evidence run through `scripts/heavy.sh`, not a timing verdict.

## Migration Plan

No data or settings migration and no preference. Rollback is a revert.
