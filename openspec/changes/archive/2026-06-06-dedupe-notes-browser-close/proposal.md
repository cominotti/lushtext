## Why

`Browse Notes...` now has reliable visible dismissal, but the populated browser can show two equivalent Close/X controls when the adaptive split view is wide enough to display both the sidebar and preview pages. That makes one dialog look like it has two window-close controls, which is visually noisy and less aligned with GNOME HIG guidance for sparse, window-level header controls.

## What Changes

- Ensure the populated `Browse Notes...` dialog shows a single canonical Close/X affordance when the notes sidebar and preview are both visible.
- Preserve a visible Close/X affordance in collapsed layouts where either the sidebar page or preview page may be the only visible page.
- Preserve the empty notes-browser state's visible Close/X affordance.
- Keep immediate keyboard dismissal with `Escape` after opening the dialog.
- Add or adjust widget coverage so wide/unfolded layouts do not expose duplicate Close/X controls, while collapsed preview and sidebar states remain dismissible.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workspace-notes`: Refine the `Browse Notes...` dismissal-control requirement so the dialog exposes one canonical visible Close/X control in unfolded/wide layouts while still keeping close reachable from every collapsed visible page and from the empty state.

## Impact

- Affected UI: `crates/lushtext-core/src/ui/window/notes.rs` notes browser dialog composition.
- Affected tests: `crates/lushtext/tests/widget/window.rs` notes-browser close and adaptive-layout coverage.
- No persistence, sidecar, workspace-scope, bookmark, document-note, or menu data model changes are expected.
