## Why

LushText still sees recurring visual and geometry regressions where GTK state is logically valid but the human-visible surface clips, shifts, overlaps, or changes unexpectedly. The minimap top-edge clipping after hiding the workspace sidebar is the current symptom; the larger problem is that visual invariants are not yet first-class contracts with repeatable screenshot, pixel, allocation, and runtime-warning proof.

## What Changes

- Introduce a cross-surface visual geometry invariant contract covering clipping, stable chrome, scroll ownership, expected movement, masked pixel comparisons, and state-extreme coverage.
- Harden the minimap so width-only shell reflow, sidebar visibility changes, theme differences, top-of-file state, word wrap, and dynamic overscroll cannot clip the top rendered minimap content or leave stale viewport projection.
- Extend desktop visual smoke from representative screenshots into paired, automation-backed captures with region masks, crop-level comparisons, allowed-difference manifests, and explicit failure artifacts.
- Extend the automation spine and automation client so smoke helpers can wait for visual geometry readiness, retrieve bounded geometry/crop anchors, and summarize visual comparison artifacts without exposing document contents.
- Strengthen adaptive shell and template-fidelity requirements so geometry-sensitive UI edits name the invariant they can affect and prove unaffected regions did not change.
- Update project rules, skills, and documentation so future visual, adaptive, template, or screenshot-reported work must include either widget allocation assertions or agent-owned visual proof with preserved artifacts.
- No breaking user-facing UI changes are intended.

## Capabilities

### New Capabilities
- `visual-geometry-invariants`: Cross-surface visible layout contracts for stable chrome, clipping, scroll ownership, expected movement, masked pixel comparison, state extremes, and evidence artifacts.

### Modified Capabilities
- `desktop-visual-smoke-coverage`: Add paired capture, crop/mask comparison, invariant manifests, extended theme/geometry matrix, and failure artifact requirements.
- `editor-minimap`: Require top-edge-safe minimap rendering after sidebar toggles, width-only reflow, word-wrap changes, theme changes, and top-of-file scroll state.
- `adaptive-editor-geometry`: Require shell transitions to preserve editor/minimap top and left anchors, avoid stale scroll adjustment drift, and expose settled geometry for visual proof.
- `dbus-automation-spine`: Add bounded visual geometry state, crop anchors, and readiness predicates needed for screenshot comparison without leaking document contents.
- `automation-client-tools`: Add support for summarizing visual comparison artifacts and geometry readiness failures through the stable client result envelope.
- `ui-template-source-fidelity`: Require geometry-sensitive template edits to run or update the visual invariant matrix and explain any nonzero pixel differences.

## Impact

- Affected code areas: `crates/lushtext-core/src/ui/editor_page/`, `crates/lushtext-core/src/ui/window/`, `crates/lushtext-core/src/ui/automation.rs`, `scripts/run-visual-smoke.sh`, `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py`, `scripts/lushtext-automation.py`, screenshot/assertion helper scripts, widget tests, visual smoke docs, automation docs, and `.agents/rules/`.
- Affected specs: new `visual-geometry-invariants`; deltas for desktop visual smoke, editor minimap, adaptive editor geometry, D-Bus automation, automation client tools, and UI template fidelity.
- Verification impact: adds targeted widget tests for the minimap/shell bug, script-level tests for mask/crop comparisons, documentation drift checks for new automation fields/flags, and host-sensitive visual smoke captures that skip clearly when compositor/screenshot tooling is unavailable.
- Data/privacy impact: automation geometry state must stay bounded to widget roles, surface names, rectangles, dimensions, scroll positions, hashes, and artifact paths; it must not expose document text, note bodies, draft bodies, local-history contents, or private persistence identifiers.
