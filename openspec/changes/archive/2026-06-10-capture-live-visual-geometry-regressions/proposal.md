## Why

The minimap/sidebar regression was visible in the user's live window but escaped the earlier automated checks because the framework did not capture the live state, did not probe the failing intermediate window size, and accepted a readiness predicate before sidebar geometry had actually reached its final allocation. We need the visual-geometry lane to turn human-reproduced geometry into repeatable headless evidence and fail on rendered-pixel drift, not only on broad app-owned rectangles.

## What Changes

- Add a live visual-geometry capture workflow that records the current LushText window state, derives a matching headless scenario, and preserves the live snapshot plus generated scenario as repro evidence.
- Strengthen visual-geometry sidebar settling so captures wait for final sidebar/editor allocations, stable across multiple snapshots, before comparing pixels.
- Extend minimap/sidebar coverage with the live failure class: light theme, word wrap enabled, long plain lines, top-of-file viewport, and an intermediate desktop-sized window around `1822x1272`.
- Treat disagreement between app-owned geometry anchors and screenshot-derived pixel anchors as a first-class failure with clear report output.
- Improve visual-geometry summaries so agents can see per-case pixel invariant IDs, row deltas, sidebar final geometry, and small before/after minimap crops without hunting through raw artifacts.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `visual-geometry-invariants`: require live-state repro capture, final sidebar geometry settling, intermediate-size minimap/sidebar regression coverage, app-vs-pixel disagreement failures, and clearer per-case evidence.
- `automation-client-tools`: require a reusable agent-facing command or helper that captures live visual-geometry state and emits a runnable scenario from the current window.
- `dbus-automation-spine`: require visual-geometry readiness to reflect final stable allocations for animated sidebar/editor transitions rather than only broad readiness.

## Impact

- Affected scripts: `scripts/visual-geometry-smoke.py`, `scripts/visual_geometry_png.py`, `scripts/lushtext-automation.py`, and related visual proof checks.
- Affected scenario fixtures: `scripts/visual-geometry-scenarios/minimap-sidebar-top.json` and any generated/live scenario support.
- Affected automation contracts and documentation: `docs/automation.md`, `docs/automation-reference.md`, and visual proof policy docs/rules.
- Affected tests: visual-geometry smoke, visual proof policy unit tests, automation client self-test, and any widget tests that can prove final sidebar allocation state.
