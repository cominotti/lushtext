## Why

The note editor popup can shrink slightly when a user creates a new range note, types text, and switches from Edit to Render. Modal geometry must be visually stable throughout interaction, especially for note editing surfaces where users switch between representations of the same content.

## What Changes

- Establish an app-wide modal geometry contract: modal shells must not resize, drift, or reflow unexpectedly during in-modal interactions.
- Tighten note editor mode switching so document notes, workspace notes, and range notes remain stable even when the note starts empty and the user types before the first Render switch.
- Strengthen widget coverage to exercise real modal interactions, including typed empty-note first render paths and exact outer-size stability.
- Audit existing modal flows for dynamic content transitions and either prove they are fixed-size or add targeted stabilization work.
- No storage, sidecar format, command, shortcut, or dependency changes.

## Capabilities

### New Capabilities

- `modal-geometry-stability`: App-wide modal stability requirements for dialogs, popups, and modal browser surfaces that can change content after presentation.

### Modified Capabilities

- `document-notes`: Document-note editor mode switching must stay geometry-stable after typing into an initially empty note before first Render.
- `sidecar-annotations`: Range-note editor mode switching must stay geometry-stable after typing into an initially empty note before first Render.
- `workspace-notes`: Workspace-note editor mode switching must stay geometry-stable after typing into an initially empty note before first Render.

## Impact

- Affects shared note editor construction in `crates/lushtext-core/src/ui/window/notes.rs`.
- Affects widget geometry coverage in `crates/lushtext/tests/widget/window.rs`.
- May affect reusable modal test helpers for measuring presented dialog allocation and text-origin stability.
- May affect documentation or agent rules only if implementation reveals a durable modal-stability lesson not already captured.
