## Why

The document properties surface already behaves like a GNOME-style adaptive pane, but the current implementation manually moves the same panel between a right-side split view and a bottom sheet. That rehosting makes resize, focus, and visibility state harder to reason about and harder to test than the user-visible behavior requires.

## What Changes

- Replace the manual document-properties rehosting flow with a slot-based adaptive layout that lets Libadwaita place the same logical properties surface in either a right-side pane or a compact bottom sheet.
- Preserve the current perceptual behavior: the same header-bar toggle, `F9` shortcut, wide right-pane presentation, compact bottom-sheet presentation, mutual exclusion with the workspace sidebar, and Focus Mode suppression rules.
- Shape the implementation as a GTK driving-adapter refactor: keep the behavior inside the window UI layer, avoid new domain/service APIs, avoid unnecessary trait wrappers, and use named layout/state types where they clarify pane-versus-sheet semantics.
- Strengthen regression coverage so tests assert the semantic behavior across wide, compact, breakpoint, workspace-sidebar, Focus Mode, and focus-restoration cases rather than relying on incidental widget internals.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `document-properties-pane`: Clarify that adaptive pane/sheet transitions preserve one logical document-properties surface, its content, its requested visibility state, and focus behavior across layout changes.

## Impact

- Affected UI template and window orchestration code:
  - `resources/ui/window.ui`
  - `crates/lushtext-core/src/ui/window/imp.rs`
  - potentially a focused sibling module under `crates/lushtext-core/src/ui/window/` if the adaptive-surface workflow needs extraction from `imp.rs`
  - `crates/lushtext-core/src/ui/window/actions.rs` where toggle commands coordinate requested state
- Affected tests:
  - `crates/lushtext/tests/widget/window.rs`
  - any helper code used by the widget-test harness for window sizing or surface-state assertions
- No expected dependency, runtime-baseline, data-model, persistence, or user-facing preference changes.
