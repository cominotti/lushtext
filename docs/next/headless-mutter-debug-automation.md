# Headless Mutter Debug Automation Follow-Ups

## Status: Mostly completed

## Context

LushText has a working automated inspection path for visual GTK regressions:

```bash
.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py \
  --file PATH \
  --search needle \
  --expected-search-matches 3 \
  --enable-minimap \
  --output /tmp/lushtext-mutter.png
```

The helper launches an isolated `mutter --headless` Wayland session, opens
LushText with temporary XDG state and keyfile GSettings, and captures the
virtual monitor through Mutter's `RecordMonitor("Meta-0")` screencast stream.

The automation spine added after this note provides `win.set-search-query`,
`dev.cominotti.lushtext.Automation1.GetSnapshot`, `GetActionCatalog`, and
`WaitForIdle`. The headless Mutter helper now uses those surfaces for
search/minimap captures and keeps AT-SPI for controls that truly need visible
editable-widget assertions.

## Follow-Ups

### 1. Search Text D-Bus Action - completed

The exported `org.gtk.Actions` window action `win.set-search-query` accepts a
string parameter, opens or updates the in-tab search UI through the normal
workflow, and is documented in `docs/automation-reference.md`.

Acceptance:

- A headless Mutter run can open search and set the query using only
  `org.gtk.Actions`.
- Fixture-backed visual smoke can wait for the expected Automation1 match count
  before taking a screenshot.
- Existing keyboard and search-bar behavior remains unchanged.
- The action updates match highlighting and minimap search markers through the
  same path as typing in the search entry.

### 2. Search Entry Accessible Name

When intentionally testing the visible search entry through AT-SPI, helpers can
currently find it by application and role, but the entry's accessible name is
empty (`name='' role='entry'`). That works while there is only one visible entry
in the searched layout, but it is brittle if another editable widget becomes
visible.

Give the in-tab search entry a stable accessible name such as `Find`.

Acceptance:

- `atspi-set-text.py --application-regex '^lushtext$' --name-regex '^Find$'`
  can identify the visible search entry.
- The accessible name is stable across restored sessions, narrow layouts, and
  search-bar reopen/close cycles.

### 3. Queryable Debug State - completed

Screenshots prove the visual result, but automation still has to infer state
from pixels or GTK widget tests. Add a narrow read-only inspection surface for
debugging and smoke checks.

The app-owned read-only D-Bus object at `/dev/cominotti/lushtext/Automation`
now exposes `dev.cominotti.lushtext.Automation1.GetSnapshot` and
`WaitForIdle`. See `docs/automation.md` and
`docs/automation-reference.md`.

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
