# Proposal

## Why

When the open tabs do not fit the tab strip, LushText gives no visible way to
move the strip or to reach a tab that is out of view. The only cue is
Libadwaita's 36 px edge fade (`FADE_WIDTH`, `adw-tab-box.c:41`, 1.9.4 and
1.10.0). The mouse wheel does scroll the strip, but only for devices that
report `GDK_SOURCE_MOUSE` (`scroll_cb`, `adw-tab-box.c:974-1004`), and nothing
on screen says so. The maintainer, a daily user, found the wheel only when
told on 2026-11-18. Until then the workaround was to click the last visible
tab, which scrolls the strip by about one tab, and repeat.

Reproducible baseline (main at `bc109b04`): `make run`, then open files until
the strip overflows at the default window size. The count needed was not
measured; it depends on title lengths and window width. The strip shows a fade at the
clipped edge and no other control. No keyboard path lists every open tab:
`Ctrl+PgUp`/`Ctrl+PgDn` step one tab at a time, and the palette's
`Open Tabs` group lists only file-backed tabs and is not reachable from the
strip.

## What Changes

- While the unpinned tabs overflow, a group of controls appears **at the strip's
  trailing end**: **Scroll Tabs Back** and **Scroll Tabs Forward**. Each moves the
  unpinned tabs by about one visible width without changing the selected tab,
  repeats while held, and is disabled at its edge.
- An **All Tabs** button with the full tab count, shown whenever the window has
  two or more tabs, opens a popover. The popover has a search entry and lists
  every open tab, untitled ones included, in strip order. Activating a row
  selects that tab, which scrolls the strip to it, and focuses its editor.
- New window actions:
  - `win.scroll-tabs-back` and `win.scroll-tabs-forward`;
  - `win.toggle-all-tabs` (`Ctrl+Shift+O`);
  - the automation target-state actions `win.set-all-tabs-visible` and
    `win.set-all-tabs-query`.

  Each is enabled only while its control applies, is in the action catalog, and
  is documented for automation.
- The automation snapshot reports the strip's overflow state, both scroll
  enablements and the popover's visibility. A readiness blocker, `tab-overflow`,
  covers pending scroll and popover work.
- Two new GTK axioms (ledger A132, A133), with probes on the GNOME 50 floor and on
  the shipped GNOME 51 toolkit. They pin the strip behaviour the scroll buttons
  rely on, which Libadwaita does not expose as API.

## Non-goals

- Touchpad vertical-swipe or Shift+wheel scrolling of the strip. These can be
  revisited later, under the same axiom, if users ask.
- `AdwTabOverview` (thumbnail grid). It must wrap the whole window content,
  and real thumbnails of background tabs would re-attach their documents,
  which `defer-background-tab-layout` forbids (design D7).
- Closing, pinning or reordering tabs from the popover. The tab context menu
  keeps those actions.
- Changing tab widths, `expand-tabs`, or the pinned segment.

## User-visible impact

- With few tabs, the strip looks the same as on main, except that two or more
  tabs now show the All Tabs button at the strip's end, which takes about one
  button's width from the tabs.
- With many tabs, two arrow buttons appear beside it. Nothing is lost compared
  with main: the wheel, drag autoscroll, `Ctrl+PgUp`/`Ctrl+PgDn`, `Alt+1..9`
  and the palette keep working.

## Capabilities

### New Capabilities

- `tab-strip-overflow-navigation`: how a user moves through and reaches tabs
  that do not fit the tab strip, with visible scroll controls and a searchable
  list of all open tabs. Purpose for the main spec: *"Let users scroll an
  overflowing tab strip and reach any open tab from visible, keyboard-accessible
  controls, without changing how the strip lays out its tabs."*

### Modified Capabilities

- `dbus-automation-spine`: added requirements for the `tab-overflow` readiness
  blocker, the `window.tab_strip` snapshot fields, the `all-tabs-popover`
  transient surface, and the two visual-geometry surfaces, following the
  per-blocker precedent of `workspace-path-reveal`.

`tab-context-actions` keeps its requirements. The new capability owns
the many-tabs state that requirement "Tab strip remains reachable for every
normal open-tab state" does not cover.

## Impact

- **Code:** a new workflow, `WFR-TAB-OVERFLOW-NAVIGATION`, with role home
  `ui/window/tab_overflow/`. It holds policy, a watch, scroll execution, evidence,
  test policy, two presentation surfaces, and the nested widget
  `tab_overflow/all_tabs_popover/` with template
  `resources/ui/all-tabs-popover.blp`. Also touched: `ui/window/actions.rs`,
  `ui/window/documents.rs` (projection publish), `services::action_catalog`,
  `ui/automation.rs`, `model/automation.rs`, and
  `resources/ui/shortcuts.blp`. `WFR-TAB-STRIP` is unchanged.
- **Toolkit reliance:** reaching the strip's internal `GtkScrolledWindow`
  (Libadwaita 1.9–1.10 template shape), gated by axiom probes. If a probe fails
  on either toolkit, implementation stops and returns to the maintainer
  (design D3).
- **Docs:**
  - README features and shortcut table, `AGENTS.md`, `ui/window/AGENTS.md`, and
    the `ui.md` widget hierarchy;
  - `docs/automation.md`, `docs/automation-reference.md`,
    `docs/accessibility.md`, `docs/accessibility-matrix.md`,
    `docs/accessibility-orca-checklist.md` and `docs/end-user-coverage.md`;
  - `docs/workflow-readability-matrix.md` and `docs/thresholds.md`;
  - the axiom ledger and the axioms crate changelog;
  - the whole-pixel table in `workflow-convention.md`.
- **Proof lanes:** widget tests, the GTK axiom samples A132/A133 on both
  toolkits, accessibility, visual and visual-geometry smokes.
