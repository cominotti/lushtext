## Why

`Browse Notes...` is meant to be the single workspace-scoped surface for bookmarks, workspace notes, and document notes, but freshly changed bookmarks can be missed because the browser reads persisted sidecars while bookmark saves are debounced. The same dialog also lacks a visible close affordance, leaving users dependent on fragile Escape behavior.

## What Changes

- Ensure `Browse Notes...` includes bookmark changes from open saved editors even when their sidecar save has not completed yet.
- Preserve the existing workspace-scope filtering, bookmark preview, explicit Open behavior, and sectioned `AdwSidebar` presentation.
- Add an explicit Close/X affordance to the unified notes browser so users can dismiss it without relying on focus-dependent Escape handling.
- Add regression coverage for freshly toggled bookmarks appearing in `Browse Notes...` and for closing the notes browser through the visible affordance.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `line-bookmarks`: Fresh bookmark changes from open saved editors must appear in the unified notes browser without waiting for the debounced sidecar save.
- `workspace-notes`: The unified `Browse Notes...` dialog must expose an explicit close affordance and remain dismissible without requiring prior interaction inside the dialog.

## Impact

- Affected code: `crates/lushtext-core/src/ui/window/notes.rs` and supporting widget-test helpers or tests in `crates/lushtext/tests/widget/window.rs`.
- Affected behavior: `Browse Notes...` bookmark rows become current with open editor state, and the dialog gains a reliable visible close path.
- Dependencies: no new crate or runtime dependencies expected.
