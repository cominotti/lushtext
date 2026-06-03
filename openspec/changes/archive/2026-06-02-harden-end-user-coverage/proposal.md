## Why

LushText now has strong deterministic coverage through unit, integration,
property, fuzz replay, fuzz smoke, widget, benchmark, and mutation lanes, but
the remaining end-user risks cluster around live desktop behavior that those
lanes intentionally do not prove. This change adds durable coverage contracts
for the real-session workflows most likely to affect users without implying the
production code is already perfect.

## What Changes

- Add focused coverage requirements for real desktop visual smoke checks,
  including geometry-sensitive surfaces that need rendered-session proof beyond
  widget allocation assertions.
- Add portal, file chooser, and sandbox workflow coverage so Flatpak/Snap-style
  access paths are tested instead of inferred from native widget tests.
- Add end-to-end coverage for external file monitor alerts, reload/discard
  behavior, and suppression of false warnings from LushText's own saves.
- Add close-request coverage for unsaved file-backed, untitled, and multi-tab
  documents so save/discard/cancel behavior is proved at the window level.
- Add desktop activation coverage for file-manager/CLI open-with paths through
  `ApplicationImpl::open`.
- Add menu workflow coverage for print, zoom, theme, and invisible-character
  actions that are visible to users but lighter than the core editor/search
  surfaces.
- Add keyboard-only and accessibility smoke coverage that exercises real focus
  and accessible-name behavior outside the current accessibility-disabled widget
  harness.
- Add lightweight performance and large-file regression coverage for user-facing
  startup, indexing, search, load/save, and memory-pressure paths.

## Capabilities

### New Capabilities
- `desktop-visual-smoke-coverage`: Real desktop and rendered-session smoke
  coverage for geometry-sensitive windows, dialogs, panels, themes, scale
  factors, and renderer differences.
- `portal-sandbox-workflow-coverage`: Portal, native file chooser, and confined
  packaging smoke coverage for open, save-as, workspace-folder selection, and
  inaccessible-path handling.
- `external-file-monitor-coverage`: End-to-end coverage for external on-disk
  modifications, reload/discard actions, and own-save warning suppression.
- `unsaved-close-safety-coverage`: Window-level close-request coverage for
  modified file-backed tabs, modified untitled tabs, multi-tab selections, save
  failures, discard paths, draft cleanup, and session persistence.
- `desktop-open-activation-coverage`: Desktop/file-manager/CLI activation
  coverage for `ApplicationImpl::open`, window reuse, tab creation, duplicate
  handling, and invalid path feedback.
- `menu-workflow-coverage`: User-visible menu and action coverage for print,
  zoom, theme selection, and invisible-character toggles.
- `accessibility-keyboard-coverage`: Keyboard-only and accessibility smoke
  coverage for focus traversal, shortcuts, accessible names, roles, and dialog
  operability in a real session.
- `performance-regression-coverage`: Lightweight automated performance and
  large-file regression checks for workflows whose slowdowns would be felt by
  users.

### Modified Capabilities
- None. Existing product capabilities remain authoritative for behavior; this
  change adds coverage contracts that prove those behaviors through the right
  runtime layers.

## Impact

- **Tests and harnesses**: expands widget, integration, headless desktop,
  packaging smoke, and benchmark/performance validation surfaces.
- **Scripts and CI**: may add or extend scripts for real-session screenshots,
  portal/sandbox smoke checks, accessibility smoke checks, and lightweight
  performance thresholds; long-running or environment-sensitive checks should be
  scheduled/manual or clearly gated.
- **Documentation**: updates testing documentation, build rules, and agent
  guidance so maintainers know which lane owns each coverage class.
- **Production code**: no direct user-facing behavior changes are required by
  the proposal, but small testability seams may be added where needed to observe
  existing behavior safely.
