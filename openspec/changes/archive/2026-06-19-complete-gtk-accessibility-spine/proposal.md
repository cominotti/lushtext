## Why

LushText now has a real GTK accessibility spine, but it is not yet complete enough to call the work finished or release-grade. The remaining gap is not a single missing label; it is the absence of a full acceptance matrix, complete AT-SPI and visual proof for every major surface/state extreme, manual Orca validation, and drift controls that prove the current commit rather than a previous good run.

## What Changes

- Complete the app-wide accessibility inventory so every major surface has an explicit semantic contract, state extremes, proof lane, and manual-screen-reader expectation.
- Expand accessibility smoke coverage from the current broad first pass into a comprehensive matrix covering shell/header/status/tab controls, editor states, Markdown preview modes, workspace sidebar and file tree, Open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, preferences, save/close/destructive dialogs, context menus, focus mode, preview mode, minimap, compact/bottom-sheet layouts, and recovery/error surfaces.
- Add first-class proof for dynamic states and announcements: readonly, busy, invalid, hidden, selected/current, expanded/collapsed, pressed/checked, result counts, Replace All completion/undo availability, failed loads, durability warnings, destructive confirmations, long-running operations, and repeated/throttled updates.
- Add release-grade manual Orca validation guidance and artifact capture so headless AT-SPI evidence is complemented by normal GNOME-session screen-reader verification.
- Strengthen policy checks from diff-only guardrails into current-tree drift checks for direct GTK accessibility calls, missing row cleanup, stale stable anchors, helper bypasses, unproved transient surfaces, and accessibility smoke freshness.
- Normalize implementation guidance so app-owned accessibility metadata flows through `ui::accessibility` unless a documented GTK contract requires an exception.
- Preserve bounded artifact and data-safety rules so proof never dumps private document, note, draft, local-history, or complete search-result contents.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `gtk-accessibility-spine`: complete the existing GTK accessibility contract with full surface/state coverage, stronger proof requirements, manual Orca release evidence, current-tree/current-commit guardrails, and bounded privacy-preserving artifacts.

## Impact

- UI implementation across `crates/lushtext-core/src/ui/**`, especially custom rows, transient surfaces, editor state projection, preview/read-only surfaces, search flows, sidebar/file-tree affordances, dialogs, and status/announcement paths.
- Accessibility helper and tests in `crates/lushtext-core/src/ui/accessibility.rs` and `crates/lushtext/tests/widget/**`.
- Smoke tooling in `scripts/run-accessibility-smoke.sh`, `scripts/run-visual-smoke.sh`, `scripts/check-accessibility-policy.py`, `scripts/check-automation-docs.py`, `scripts/lushtext-automation.py`, and the headless Mutter/AT-SPI capture helper as needed.
- Documentation in `docs/accessibility.md`, `docs/end-user-coverage.md`, `docs/automation.md`, `docs/automation-reference.md`, and `.agents/rules/*.md`.
- Existing Make targets: `make accessibility-smoke`, `make visual-smoke`, `make visual-geometry-smoke`, `make check-accessibility-policy`, `make check-automation-docs`, `make check-policy`, and release validation flows.
