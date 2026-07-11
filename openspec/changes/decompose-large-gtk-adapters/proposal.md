## Why

The codebase's domain and service boundaries are strong, but three GTK driving-adapter files have accumulated several unrelated workflows: window notes and bookmarks, adaptive-shell policy in the window implementation, and workspace-section row factory/context-menu/accessibility wiring. Their size now increases review cost and merge conflicts even though the underlying abstractions are mostly correct. A final behavior-neutral decomposition will make those boundaries easier to maintain after the preceding policy changes settle.

## What Changes

- Split the window notes adapter by user workflow: bookmark lifecycle/navigation, document and folder note editors, and notes-browser/palette coordination.
- Move pure adaptive-shell calculations and policy decisions out of `ui/window/imp.rs` into a plain Rust sibling module while leaving GObject lifecycle and widget ownership in `imp.rs`.
- Split workspace-section row-factory projection, context-menu/gesture wiring, and accessibility metadata into focused sibling modules while preserving section orchestration ownership.
- Keep `mod.rs`/`imp.rs` as the established public GObject boundary; do not introduce manager types, generic repository traits, new crates, or GTK dependencies in domain services.
- Preserve every action, signal, automation anchor, focus path, empty state, geometry rule, persistence invariant, and runtime behavior.
- Update affected module maps and local agent guidance, then verify source, widget, accessibility, automation, and visual contracts.

## Capabilities

### New Capabilities

- `gtk-adapter-module-boundaries`: Defines behavior-neutral workflow decomposition and ownership rules for large GTK window and sidebar adapters.

### Modified Capabilities

None.

## Impact

- Affects large files under `ui/window/` and `ui/sidebar/workspace_section/`, plus nested `AGENTS.md` and architecture/module documentation when paths change.
- Intentionally changes no user-facing feature or persistence format.
- Must be implemented last in this portfolio, after draft, memory, preview, responsiveness, watcher, and ranking work, to avoid moving active targets underneath behavior changes.
