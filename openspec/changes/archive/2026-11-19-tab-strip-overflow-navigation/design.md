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
    bar) are inert. Outside our group and while the popover is closed, they
    also end a running selection reveal (D13).
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
| `tab_overflow/all_tabs_execution.rs` (named `all_tabs.rs` until the D17 apply stage) | coordination (`execution`, stage-order-qualified: stage 6): window-side snapshot rows and their re-snapshot `Debounce`, popup wiring, the focus save and generation-guarded restore, the Escape latch, and `select_tab_from_all_tabs` |
| `tab_overflow/all_tabs_popover/{mod,imp,item}.rs` | **called presentation surfaces**: widget, factory and row object |

**Policy** (`policy.rs`, `#![forbid(clippy::float_arithmetic,
clippy::disallowed_methods)]`, a new whole-pixel table row at `forbid` / 0) works
over `StripScrollExtent { value_px, page_px, upper_px }`. The adapter rounds
each `f64` once to the nearest whole pixel. Functions:

- `strip_overflows` (named `unpinned_overflows` until D17), `controls_visible` (show, hide and hold, from D2);
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
- **Filtering** (`policy::rank_all_tabs`) runs on the GTK thread and ranks
  each tab into the first tier it matches, case-insensitively: the title as a
  prefix, the title as a substring, the folder (as shown, or as an absolute
  path) as a substring, then the title fuzzily through
  `services::fuzzy::FuzzyQuery`, the matcher Recent Open's service uses
  (`services/recent_documents.rs`), by descending score. The path is never
  matched fuzzily: a short query matches nearly every absolute path fuzzily,
  which would list every tab. Ties keep strip order. Ordering and highlight:
  - an empty query keeps strip order and highlights the selected tab;
  - a non-empty query ranks by tier and score and highlights the first row.

  The ranking costs O(tabs) short-string matches per keystroke. The
  `all_tabs_filter_1024` bench case records the ranking's figure (Timing
  below); a keystroke also builds the candidates, publishes the ranked rows
  and re-projects the realized band, which the bench does not cover:
  **at most 233 µs per query over 1,024 tabs** (prefix 233 µs, fuzzy 220 µs,
  substring 167 µs, folder 165 µs, no match 151 µs, empty 0.5 µs; one
  `heavy.sh` run on 2026-11-18, load 3.28 → 7.55, a regression figure, not a
  verdict), far inside `B_interact` (8 ms).
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
  the reconcile reads `strip_overflows` only while
  `n_pages > n_pinned_pages`. Without it, pinning every tab of an overflowing
  strip would leave the scroll buttons shown over an empty box.
- **A new action surface, `tab-strip-control`.** No existing
  `ActionSurface` named a button in the tab strip's end group
  (`header-button` is the header bar), so `ActionSurface::TabStripControl`
  joins the catalog vocabulary; the All Tabs half reuses it.
- **The `tab-strip` visual surface.** The visual-geometry case checks the
  strip's rectangle (so its height) before and after a step through the
  existing `tab-strip` surface. A duplicate `tab-bar` surface for the same
  widget was added first and removed in review, so one widget has one name.
- **The visual-geometry case steps Forward** in a restored session of 41 tabs
  rather than crossing the overflow point in one session: no single action
  crosses it at a fixed size. The session's first tab is the case's activation
  file, so the first tab is selected and the strip is not at its end; the
  relationship requires Forward enabled before the step and Back enabled after
  it. Group 3 fixed the start position (D12, "The restore leaves the selected
  tab in view"), so the case now also requires Back disabled before the step:
  a step that did nothing would fail it.
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
  This stays accepted in group 3: `adw_tab_view_transfer_page` (1.10.0)
  detaches and attaches without ever setting the property, so no test path
  exists short of a real drag.

### D12. Implementation decisions (All Tabs half and the open items, tasks 3.1–3.8)

- **The restore leaves the selected tab in view (open item 4 of group 2).**
  Each page a restore attaches after its selection scrolls the strip to that
  page's tab (A132), so a restored strip could show neither end with its
  selected tab out of view; `main` behaved the same, and
  `preexisting-blockers.md` has no exception. The fix uses the toolkit's own
  selection scrolling (A133): the restore terminal calls
  `reveal_selected_tab`, which re-announces the unchanged selection
  (`notify::selected-page` on the tab view), and `AdwTabBar` runs
  `select_page` → `scroll_to_tab`, which moves the strip only while the tab
  is outside the page. A one-shot notify at the terminal was tried first and
  failed its test: at the terminal the strip had never been allocated (its
  adjustment read 0 × 0), so `select_page` skipped the scroll (tab width
  -1), and the attach scrolls queued in the same dispatch still landed at the
  next allocation. The reveal therefore resumes from a tick callback on the
  bar, once per frame after the strip is allocated. It re-announces the
  selection **once**, on the first frame that finds the strip where the
  previous frame left it (so an attach scroll landing at that allocation
  finishes first), and ends on the first still frame once
  `SELECTION_REVEAL_SCROLL_WINDOW_US` (256 ms) has passed since the
  announcement, at most `SELECTION_REVEAL_MAX_FRAMES` (256) frames. The
  first version ended on the first repeated value and re-announced on every
  one; the full visual-geometry run then failed `tab-strip-overflow` in one
  case of four, and a traced rerun (`reveal-oscillation-before-fix.stderr` in
  the handoff directory) showed why: with animations on, the strip's own
  200 ms eased scroll stays on one whole pixel for two frames in its tail,
  each re-announcement restarted it from there, and the strip oscillated
  between 1 and 2 px until the frame cap, so readiness reported a strip one
  pixel from its start (Back enabled) that later settled at 0. After the fix
  the traced case lands at 0 and the filtered case passed 6 of 6 runs, then
  every full run; it holds the `tab-overflow`
  blocker meanwhile, ends at a step, a selection of the user's own, a wheel
  or press on the strip (D13), or disposal, and the window's own selected-page handler ignores the
  re-announcement (nothing changed, so no projection runs twice and no
  selection generation advances). Failing-first: three restore shapes in
  `test_a_restore_leaves_the_selected_tab_in_view` (the first of forty, the
  middle of forty, and a session that overflows only with its last tab),
  failing at strip value 4247 of 4320 before the fix
  (`failing-first-g3-items34.log`). The same reveal serves an All Tabs pick
  of the tab that is already selected, which no selection change would
  scroll to.
- **A projection batch released without a publish resumes the reconcile
  (open item 3).** A restore cancelled by a superseding empty session
  releases its batch through `cancel_tab_projection_refresh_batch`; at depth 0
  that now calls `resume_tab_overflow_after_unpublished_batch`, which
  schedules the reconcile when one was waiting for the terminal. Disposal
  schedules nothing (`schedule_tab_overflow_reconcile` returns while the
  window is disposing). Failing-first:
  `test_a_restore_cancelled_without_publishing_still_reconciles_the_controls`
  (`failing-first-g3-item3.log`).
- **The folder matches by substring only.** The filter ranks the title by
  prefix, then substring, then the shared fuzzy matcher, and the folder only
  by a case-insensitive substring: a short query fuzzily matches nearly every
  absolute path (`md` matches `/home/…/main.rs` through any `m…d`), which
  would list every tab and contradict the spec's "Filter by name" scenario.
  Ties keep strip order.
- **Live updates come from the tab pages themselves.** While the popover is
  open, page attach, detach and reorder, the selection, and each page's
  `notify::title` and `notify::pinned` re-take the snapshot through one
  zero-delay `Debounce`. A rename, a Save As and the modified dot all change
  the page title (`project_modified_indicator`), so no editor signal is
  needed. The handlers are kept per page and disconnected on detach and
  dispose.
- **The popover closes itself on `Ctrl+Shift+O`.** A capture-phase key
  controller on the popover handles the shortcut while it is open, so the
  toggle does not depend on the window's managed shortcut reaching a
  popover's surface; the window action covers the closed state.
- **Focus comes back after the menu button's own close handling.** The
  restore runs from an idle after `closed`, so a menu button that focuses
  itself on close cannot win; a stale activation restores the same way.
- **The match-count announcement is a `SettleBurst` of
  `ALL_TABS_ANNOUNCE_DELAY_MS` (500 ms)** on the debounced-results lane. It
  is not part of the `tab-overflow` blocker: it changes no geometry and no
  accessible tree, and holding readiness for it would slow every popover
  wait by half a second.
- **Large text in the count test** is `gtk-xft-dpi` raised by half (the
  desktop's text-scaling factor reaches GTK as that DPI), and the count is
  asserted to get its whole natural width at 1, 2 and 3 digits.
- **The scroll buttons' own click gesture is off (found by the live check).**
  The live check (task 4.3, `live_check.py` in the change's handoff
  directory) held Forward with a real pointer, selected the first tab during
  the hold, and released: the strip scrolled to the selected tab while held,
  then stepped forward again on the release. A claim of the hold gesture only
  denies GtkButton's own bubble-phase click gesture while the claim stands,
  and after the selection the release reached it and emitted `clicked`. The
  button's own click gesture is now disabled at the source, the way the file
  tree disables `GtkTreeExpander`'s (`widget-wiring.md`), so a pointer only
  ever reaches the hold gesture, which also sets the pressed look
  (`StateFlags::ACTIVE`); Space and Enter still emit `clicked` through the
  keybinding. A widget test pins the disabled gesture and the one keyboard
  step (a real press-and-hold cannot be synthesized there); the live check
  passes all eighteen of its steps after the fix.
- **The popover list's pixel anchor is checked against its own surface.**
  The popover is closed in the before capture, so a before/after anchor
  cannot be detected there (the first full run failed all four
  `all-tabs-popover` cases with "surface all-tabs-popover-list is not
  visible"). The anchor uses the runner's after-only mode
  (`reference_surface`, as `workspace-tree-reveal` does): the first ink row
  must sit exactly 6 px below the list's reported top (the highlighted row's
  own inset, measured identical at both sizes and both schemes), so the
  rows are proved drawn where the snapshot says they are.
- **Visual geometry.** A new scenario, `all-tabs-popover`, opens the popover
  with `win.set-all-tabs-visible` and filters it with `win.set-all-tabs-query`
  on the restored 41-tab session at 1260 × 900 and the 640 × 600 minimum;
  it requires the popover closed before and open after as the active
  transient surface, the strip and the All Tabs button unmoved, and the
  search entry and the list inside the window's height, with pixel anchors
  on the button and the rows. The `tab-strip-overflow` case gains the All
  Tabs button's anchor and the Back-disabled-before requirement (open item 1).

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

### D13. Review decisions (code review of PR #119)

- **The strip's own wheel or press ends a selection reveal.** Before, the
  bar's observers acted only during a step, so a reveal still waiting for a
  still frame re-announced the selection after the user had scrolled and
  undid the scroll. Both observers now end the reveal, outside our group and
  while the popover is closed (scrolling its list moves no strip). The call
  is not in `cancel_tab_scroll_for_strip_input`: the selected-page handler
  runs that for the reveal's own re-announcement. Failing-first:
  `test_a_wheel_or_press_on_the_strip_ends_the_selection_reveal`.
- **A reveal asked for while the strip is hidden is owed to its next map.**
  A restore behind Focus Mode left the selected first tab of forty out of view
  when the strip returned (strip value 4288). `reveal_selected_tab` now
  connects a one-shot `map` handler on the bar instead of returning; the debt
  does not hold the `tab-overflow` blocker, since Focus Mode can last
  indefinitely, a later request replaces it, and disposal drops it.
  Failing-first:
  `test_a_restore_behind_focus_mode_shows_the_selected_tab_when_the_strip_returns`.
- **Focus inside the popover is the popover's.** The popover is parented to
  the All Tabs button, so focus in its search entry read as the button's, and
  after `popdown` the menu button focuses itself before the popover's idle
  restore runs; a close down to one tab then started the reconcile's editor
  hand-off, which took the focus from the widget saved before opening.
  `focused_control` now answers none inside the popover, and the hand-off
  stands down while the restore is pending (`focus_restore_pending`).
  Failing-first:
  `test_a_close_to_one_tab_with_focus_in_the_popover_gives_the_saved_focus_back`.
- **The highlight is named to assistive technologies.** Up and Down move the
  highlight while the focus stays in the entry, so the list's value text now
  names the highlighted tab before the match count
  (`policy::all_tabs_list_value_text`, "Selected beta.txt. 3 open tabs"), as
  the command palette does. Failing-first:
  `test_moving_the_highlight_names_its_tab_in_the_lists_value_text`.
- **Declined:** re-deciding only the enablement on a value change (the
  observers only schedule the one coalesced reconcile, per
  `ui/window/AGENTS.md`); one tab-view observer set for both the reconcile
  and the re-snapshot (two owners, both deferred, so their order carries
  nothing); `SignalBag` for the per-page and per-row handlers (it releases
  only all at once, so the keyed map is still needed).

### D14. Implementation round of 2026-11-19 (programme-change `continue`)

- **A133 pins the unchanged-selection re-announce D12 relies on.** The probe
  scrolled the strip to a *different* page only. It now also sets the strip
  back to its start with the last page still selected, notifies
  `selected-page`, and asserts the tab is inside the page again. It holds on
  the host (GTK 4.24.1 / Adw 1.10.0) and inside `org.gnome.Sdk//50` (4.22.2 /
  1.9.0, the floor) and `//51` (value 0 → 3337, the tab at 467 + 126 of 598).
  The earlier ledger row already recorded an `Sdk//50` run of the rest of the
  probe; the new assertion had none until now.
- **`selected_tab_in_view` makes the restore shapes observable.** The edge
  flags prove only the first and last shapes; a middle tab leaves both
  enabled wherever the strip is. The pure `policy::tab_in_view` decides, in
  whole pixels, whether the selected tab's bounds in the unpinned scroller lie
  inside its width; `selected_tab_in_view` reads them through the same
  `AdwTab` walk as the A132/A133 probes and fails closed (`None`) while the
  strip is hidden, for a pinned or absent selection and when the tab cannot be
  found. The evidence surface and `window.tab_strip.selected_tab_in_view`
  read it, so the widget tests, the automation snapshot and the live check
  share one reading. Decision: a bool, never a position or a title (the
  snapshot's privacy boundary).
- **The live check covers the middle and last shapes and Focus Mode.** Runs
  `middle` (tab 20 of 40) and `last` (tab 39 of 40) assert
  `selected_tab_in_view` and the edge flags after `visual-geometry-settled`;
  run `focus` enters Focus Mode as soon as the automation object answers (18
  of 40 tabs restored at that point, so the rest restored behind the hidden
  strip), reads `null`, leaves Focus Mode and reads `true`. The drag step had
  silently stopped running after ebf932ae removed the duplicate `tab-bar`
  surface; it now keys on `tab-strip` and fails loudly when absent. All 26
  steps pass (`live-check-r1.out` in the handoff directory).
- **The delta specs name what the code exposes.** A requirement for the
  restore reveal (first, middle, last, behind Focus Mode); the automation
  delta lists every visual surface and pixel anchor of the change beside the
  existing `tab-strip`, and the new snapshot field.
- **Two policy self-tests failed under an agent stage's `TMPDIR`**
  (`$H/<change>/tmp/<stage>`, 117 bytes): `with-temp-scope.sh
  --session-runtime` refused the scope (socket path 191 > 99), which would
  also have refused every widget shard and axiom sample, and
  `next-rustc-wrapper.py`'s fixtures inherited the real `~/.cargo` config
  from an ancestor directory. Fixed in `7c6d37fa` (`preexisting-blockers.md`):
  a short owned `/tmp/lt-rt-<pid>-…` runtime directory when the scope is too
  deep, and fixtures under a base with no `.cargo` config above it. The merge
  commit before it ran its hook with `TMPDIR` unset, the one recorded
  exception. The first smoke run then found the same limit in three more
  runtime-directory creators: `cargo-gtk-proof`'s live runner fell back to
  the (deep) system temp dir, and the Mutter capture helper and two Python
  smokes made theirs in it unchecked, so the session helper refused all
  three lanes. Each now falls back to `/tmp` by the same rule (the
  `cargo-gtk-proof` host tests failed first under the stage `TMPDIR`, 2 of
  9), committed with this group's UI files so the smokes run once. The
  first recorded gate run then failed twice more for the same reasons:
  `gtk-lush-proof-harness`'s `a_caller_runtime_dir_too_long_for_the_sockets_is_refused`
  made its "short" directory inside the deep `TMPDIR` (its fixture now sits
  under `/tmp`), and `scripts/gate-trees.sh` exported `LC_ALL=C` to the gated
  command, so Mutter refused the accessibility smoke's
  `protected-view-inspector` capture, whose steps carry an en dash
  ("Failed to configure: Invalid byte sequence in conversion input"); the
  gated command now runs in the caller's locale (self-test case added). The
  same smoke had passed outside `gate-trees.sh` on the same content an hour
  earlier, which is how the cause was found rather than taken for a flake.
  The second recorded run's `make test` then failed six widget tests
  deterministically (`notes_browser`'s 131,072-row list refuses a temporary
  root over its 112-byte folder, the Open popover's path tooltips, the
  skipped-files dialog and the sidebar's 131,072-entry directory, which
  charge path bytes against their budgets) and one,
  `search_panel::test_an_apply_while_another_window_runs_is_refused`,
  passed on retry (a separate race, D16). The seven rerun with a short
  `TMPDIR` all passed (`ab-short-tmpdir-1.log` in the handoff directory).
  Rerun under the stage `TMPDIR` (`ab-deep-tmpdir-1.log`), four failed again
  with the gate run's own failures: the notes browser's "temporary root …
  is 188 bytes, too long for a 112-byte folder", the two Open popover
  tooltip waits (2 s) and the skipped-files dialog's wait (10 s). The
  131,072-entry directory failed again only at its 240 s wait, which names
  no cause; the settle-budget test reported nothing before the run's
  timeout ended it; the apply test passed. The stall recording showed
  storage was not a bottleneck. So `with-temp-scope.sh` now makes its whole
  scope under `/tmp` when `TMPDIR` is too deep for a session in it, where
  the lanes were proved, and the runtime-directory fallback of `7c6d37fa`
  is gone. With it, all seven passed under the stage `TMPDIR`
  (`ab-deep-tmpdir-fixed.log`): evidence of the fix for the four that
  failed with their cause, and no evidence about the other three, whose
  failure under the deep `TMPDIR` was never shown to have that cause.
- **The run-mutants stub failure of `6ee67e14` is reproduced.** The stub
  extracted from `6ee67e14^`, run twice at once 2,000 times, interleaved its
  records 181 times ("ARGS mutants --shard 1/2ARGS"); the fixed stub, 0 of
  2,000 (`stub-race-repro/` in the handoff directory). The cause is the one
  the commit names: several `printf` writes per record under `O_APPEND`.

### D15. Apply stage of the programme template's review (2026-11-19)

The review lenses' findings, as the independent apply agent decided them
(the full list with each rejection is
`review-findings-applied.md` in the handoff directory):

- **The overflow actions are published once they exist.** The install
  stage's reconcile runs from `constructed`, before `setup_actions`
  registers the actions, so `win.scroll-tabs-back`, `win.scroll-tabs-forward`,
  `win.toggle-all-tabs` and `win.set-all-tabs-visible` started enabled until
  a later reconcile happened to run. `setup_actions` now calls
  `sync_tab_overflow_actions` right after registering them. Failing-first:
  `test_the_overflow_actions_start_disabled_before_the_first_dispatch`.
- **A wheel over our own trailing group cancels nothing.** The tab box's
  wheel handler is on the box (F1), so a wheel over the group moves no
  strip; the capture-phase scroll observer now ignores it, as the press
  observer already did, through a motion controller on the group. Failing-
  first: `test_a_wheel_over_the_end_group_cancels_nothing_and_keeps_the_reveal`.
- **A popover reopened before the last close's focus restore keeps the
  focus.** The restore idle was untracked: a close and a reopen in one
  dispatch (two `win.set-all-tabs-visible` calls, or the shortcut twice)
  let it pull the focus back to the editor under the open popover. A
  generation now cancels it on reopen and at disposal, the reopen keeps the
  focus saved before the first open, and the pending restore holds the
  `tab-overflow` blocker. Failing-first:
  `test_a_reopen_before_the_focus_restore_keeps_the_focus_in_the_popover`.
- **The All Tabs filter matches the absolute folder too.** The spec says
  "title or path"; the row shows the home folder as `~`, and the filter
  matched only that label. `AllTabsCandidate::folder_path` carries the
  absolute folder, matched by the same substring rule (unit test
  `filtering_by_folder_matches_the_absolute_path_behind_the_home_label`;
  the `all_tabs_filter_1024` bench builds both).
- **A reveal on a strip that fits ends without announcing.** The pure step
  `policy::reveal_frame` (moved out of `scroll_execution.rs` with its two
  constants, unit-tested) ends on the first still frame that also keeps the
  strip's width when neither the unpinned nor the pinned tabs overflow (a
  pinned selection in an overflowing pinned scroller is still announced), so
  a restore of a session that fits holds `tab-overflow` for a frame or two,
  not 256 ms. Failing-
  first: `test_a_reveal_on_a_strip_that_fits_announces_nothing`.
- **A reveal holds readiness for at most `SELECTION_REVEAL_READINESS_US`
  (8 s).** The reveal runs on the frame clock, so a window whose compositor
  sends no frames (minimized, occluded) held `tab-overflow` until it painted
  again. Ending the reveal on a timer would instead leave the selected tab
  out of view when such a window returns, so the reveal keeps running and
  only its hold on readiness is bounded (`policy::reveal_holds_readiness`,
  unit-tested; the harness cannot stop a frame clock, so no widget test fails
  first). `SELECTION_REVEAL_MAX_FRAMES` is relabelled a coverage window: a
  smaller value would leave the selected tab out of view.
- **The reconcile publishes one pure plan.** `policy::plan_overflow_controls`
  replaces the identity `scroll_controls_available` and the inline `&&`s, so
  "no scroller, no buttons" and "no scroll action enabled while its button is
  hidden" are unit-tested invariants. A detached group also clears the
  pointer-over-group flag of the wheel observer, whether or not GTK delivered
  its `leave`.
- **Smaller ones:** the folder label is the pure
  `policy::all_tabs_folder_label`, with the home folder read once per
  snapshot; the action names and `TOGGLE_ALL_TABS_ACCEL` are the constants
  `actions.rs` registers and binds, and the popover closes on the parsed
  accelerator; the popover's remaining copy moved to `policy.rs`; one
  `pause_step` and one `publishing` read; the All Tabs re-snapshot ignores
  the reveal's re-announcement, and `ui/window/AGENTS.md` makes that a rule
  for every `selected-page` observer.

### D16. Implementation round 2 of 2026-11-19 (the gate's warning and flake)

The apply stage's `make test` (D15) failed on two lines that are not this
change's code: a `Gdk-WARNING` from the sidebar file peek, and
`search_panel::test_an_apply_while_another_window_runs_is_refused`, which
passed on retry. Both were attributed in the handoff directory (`attr-*`,
`flaky-apply/`) before this round.

- **The GDK warning is classified in the widget runner only.**
  `gdk_frame_timings_presented() called on discarded frame.` is GTK 4.24's,
  printed once per process when the compositor discards a popover's
  unpainted buffer on a reposition and the toplevel's feedback of the same
  frame is presented after it: the popover shares the toplevel's frame clock
  and GDK marks the shared frame discarded from the popover's feedback alone.
  GTK 4.22 has no such state; GTK `main` removed the transition in d2595c6b
  (MR !10242), which `gtk-4-24` does not carry. The maintainer decided on
  2026-11-19 to classify it: one exact `Gdk-WARNING` line from a harness
  child in `BENIGN_WIDGET_NOISE_REGEX` of `scripts/run-widget-tests.sh`,
  with ten near misses in its self-test that must still fail (skipped and
  submitted frames, the reverse-order and duplicate-call messages,
  `Gtk-WARNING`, `Gdk-CRITICAL`, a `(lushtext:N)` process, trailing text, a
  shared test-name prefix). The self-test failed first. The smoke lanes stay
  strict. The removal condition is in `build.md` and step 8 of the
  toolkit-update runbook.
- **A134 is a gated probe.** It runs a peek-like popover fixture for 60
  cycles in a child traced with `WAYLAND_DEBUG=client`. A discard alone is
  not a positive control (a content change without a reposition discards
  too and prints nothing), so the control is the ordered pair in the trace:
  in one paint the popover's feedback `discarded`, then the toplevel's
  `presented`. The pair is required only at or above the GTK 4.24.0 bound;
  below it it is recorded, because the floor's claim is GDK's alone and the
  floor CI image's Mutter 50 was not traced. It held in 52 consecutive
  rounds each on the host (GTK 4.24.1), inside `org.gnome.Sdk//51` and
  inside `//50` (4.22.2), 32 of them under 12 busy-loop CPU hogs (load up
  to 18.4), and five more through the probe runner; `//master` (4.25.1,
  with d2595c6b) violates it, as the fix should (`a134-reliability/`,
  `a134-runtimes-*.log`). The id was the next free one on `main` and every
  open branch (the ledger's reservations end at A133).
- **The sidebar peek's anchor is not tidied.** Re-anchoring the popover
  only when its row moved would avoid one trigger, but it rests on an
  unconfirmed inference about which re-anchor discards, it touches a UI
  prefix, and the warning would stay possible from any other repositioned
  popover; the classification is the decision.
- **The search_panel flake is a product race, fixed in tests only here.**
  Each window's startup palette file-index build reserves 64 MiB of the
  128 MiB ordinary disposal lane, about 300 ms after its scope refresh,
  which in a two-window test is when the second window confirms; today's
  Confirm refuses a 64 MiB undo reservation that does not fit and does not
  retry it. The product fix is its own change,
  `replace-all-confirm-disposal-lane`. Here every test that confirms a
  Replace All in a window confirms through `confirm_with_settled_file_index`
  (`tests/widget/search_panel.rs`), which asks every window of the
  application for a build (superseding a pending debounce, which no
  evidence exposes) and waits until each has started and finished one and
  holds no admission. No assertion changed and no production code changed.
  The one windowless panel test keeps its plain Confirm: it has no window
  and no index. Evidence, with the attribution's forced 3 s index-build
  delay in a throwaway worktree (`attr-instrumentation.patch`, never
  committed): the unmodified test failed on `undo-reservation-refused`
  (`sp-forced-before.log`) and passed with the helper
  (`sp-forced-after.log`); the whole module with the delay then failed
  only `test_a_run_in_another_window_retires_this_windows_undo` the same
  way (`sp-module-delay.log`), passed after that test's Confirms took the
  helper too (`sp-module-delay-3.log`, 154 tests); and 72 concurrent runs
  of the apply test under 8 CPU hogs passed, none with an index reservation
  live at the second window's undo reservation (`sp-nat/`).
- **D14's "seven pass" is corrected** to what its logs show, test by test.

### D17. Second apply stage of the programme template's review (2026-11-19)

The lenses reviewed `11548532` with the second `/simplify` stage's edits;
the independent apply agent decided their findings as follows (the full
list, with each rejection's reason, is `review-findings-applied.md` in the
handoff directory):

- **One Escape in the All Tabs search entry closes the popover and nothing
  under it.** `stop-search` closes the popover, and the window's
  Bubble-phase Escape controller then sees the same key press, so an open
  find bar or search panel closed with it. The search entry's `stop-search`
  now sets the dismissal ladder's child-handled latch, as the palette's
  does. Failing-first:
  `test_escape_in_the_search_entry_closes_the_popover_and_nothing_under_it`
  (`apply2-ff-escape.log`).
- **A close drops a match count still waiting for the query to settle**,
  which described a list that is gone and would have been announced over
  the restored focus, and releases the snapshot, which the next popup
  replaces anyway. Failing-first:
  `test_a_close_before_the_query_settles_announces_no_match_count`
  (`apply2-ff-close.log`).
- **`all_tabs.rs` is `all_tabs_execution.rs`**, a stage-qualified
  coordination role: it owns stage 6's re-snapshot `Debounce`, the
  generation-guarded focus restore and the per-page handlers, which a called
  presentation surface may not. The module doc, the facade's role table and
  the matrix row say so; the row's figures are re-derived (3,947 lines).
- **The overflow predicate is scroller-neutral**: `strip_overflows`
  (formerly `unpinned_overflows`) is read for both scrollers, and the pinned
  scroller's read sits beside `current_extent` in `scroll_execution.rs`, so
  the two coordination modules no longer call each other for it. The
  reconcile binds `OverflowControlsPlan`'s fields by their own names.
- **The no-match title has one source**, `ALL_TABS_NO_MATCH_TITLE`; the
  Blueprint no longer repeats it.
- **The lookup and the extent type are re-exported for tests only**
  (`test-utils`), since no production caller outside the workflow names
  them.
- **Wording.** D6 states the filter's tiers as implemented (the folder is
  matched by substring only, never fuzzily) and what the bench measures (the
  ranking's share of a keystroke); A134's users name the All Tabs popover as
  a possible, unobserved trigger, since its height follows the filtered
  list.

## Timing

Timing: none (no timed path touched). The `all_tabs_filter_1024` bench case is
regression evidence run through `scripts/heavy.sh`, not a timing verdict.

## Migration Plan

No data or settings migration and no preference. Rollback is a revert.
