# Headless Mutter Debug Automation Follow-Ups

## Status: Proposed

## Context

LushText now has a working automated inspection path for visual GTK regressions:

```bash
.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py \
  --file PATH \
  --search needle \
  --enable-minimap \
  --output /tmp/lushtext-mutter.png
```

The helper launches an isolated `mutter --headless` Wayland session, opens
LushText with temporary XDG state and keyfile GSettings, activates search
through `win.begin-search`, sets the visible search entry through AT-SPI, and
captures the virtual monitor through Mutter's `RecordMonitor("Meta-0")`
screencast stream.

This is good enough for current debugging and regression evidence, but a few
small app-side surfaces would make the path cleaner, more inspectable, and less
dependent on accessibility tree heuristics.

## Follow-Ups

### 1. Search Text D-Bus Action

Current automation can open the in-tab search UI through the exported
`org.gtk.Actions` window action `begin-search`, but D-Bus cannot set the search
query itself. The helper currently uses AT-SPI editable-text automation for that
last step.

Add a window action such as:

- `begin-search-with-text` with a string parameter, or
- `set-search-query` with a string parameter that assumes the search UI is
  already open.

Acceptance:

- A headless Mutter run can open search and set the query using only
  `org.gtk.Actions`.
- Existing keyboard and search-bar behavior remains unchanged.
- The action updates match highlighting and minimap search markers exactly as
  typing in the search entry does.

### 2. Search Entry Accessible Name

AT-SPI currently finds the search entry by application and role, but the entry's
accessible name is empty (`name='' role='entry'`). That works while there is only
one visible entry in the searched layout, but it is brittle if another editable
widget becomes visible.

Give the in-tab search entry a stable accessible name such as `Find`.

Acceptance:

- `atspi-set-text.py --application-regex '^lushtext$' --name-regex '^Find$'`
  can identify the visible search entry.
- The accessible name is stable across restored sessions, narrow layouts, and
  search-bar reopen/close cycles.

### 3. Queryable Debug State

Screenshots prove the visual result, but automation still has to infer state
from pixels or GTK widget tests. Add a narrow read-only inspection surface for
debugging and smoke checks.

Possible shapes:

- a normal D-Bus action/result path if GTK action plumbing can return the needed
  state cleanly;
- a development-only D-Bus interface on the window object; or
- a test-only helper exposed only in non-release builds.

Useful state:

- active document path;
- whether the minimap is visible;
- current in-tab search query;
- current visible search match count when available.

Acceptance:

- The headless Mutter helper can assert that the intended file, minimap state,
  and search query are active before it captures a screenshot.
- The surface is read-only unless an explicit action is meant to mutate UI
  state.
- Release builds do not expose broad debug controls unless they are useful and
  safe as normal app automation APIs.

## Non-Goals

- Do not replace the existing widget-test harness with screenshot-only tests.
- Do not make production behavior depend on a running accessibility registry.
- Do not add coordinate-based input or screenshot approval paths.
- Do not expose arbitrary widget-tree mutation over D-Bus.

## Discovered During

Headless Mutter automation work for the minimap search-marker regression on
2026-05-30.
