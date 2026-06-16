## Why

Existing document and folder notes are usually opened to be read first, but the shared note dialog currently starts every note in Edit mode even when meaningful saved content already exists. The dialog also keeps Save active when the buffer is unchanged, which makes a read-first note surface feel noisier than it needs to be.

## What Changes

- Open document-note and folder-note dialogs with Render selected whenever the loaded note body contains non-empty, non-whitespace content.
- Keep new, missing, cleared, or whitespace-only note bodies opening in Edit mode so creation still starts in the writable surface.
- Keep Save visible, but disable it while the normalized buffer text matches the loaded note body or is not meaningful enough to save.
- Re-enable Save as soon as the user makes a meaningful unsaved change, including edits made before switching to Render for review.
- Preserve existing Clear, Cancel, Edit/Render switching, markdown rendering, and layout-stability behavior.
- Add broad regression coverage across pure state logic, document-note workflows, folder-note workflows, switching modes, reverting edits, and modal geometry.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `document-notes`: Document-note dialogs gain read-first initial mode for existing content and dirty-state Save enablement.
- `workspace-notes`: Folder-note dialogs gain read-first initial mode for existing content and dirty-state Save enablement.

## Impact

- Affected UI: shared note editor surface and document/folder note dialog response-state wiring in `crates/lushtext-core/src/ui/window/notes.rs`.
- Affected model/service logic: possible small pure helper for normalized note dirty-state decisions using the existing `RichNoteBody` normalization rules.
- Affected tests: unit tests for the helper, widget tests for document notes and folder notes, and geometry/visual validation for the modal’s first presentation and Edit/Render transitions.
- No new runtime dependencies, file-format changes, or automation API changes are expected.
