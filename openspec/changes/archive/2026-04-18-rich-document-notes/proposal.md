## Why

LushText already has saved-file range annotations, but the current note story is split between lightweight line-range highlights and separate markdown preview workflows. Users need a richer note system that can capture context at the range, document, and workspace levels without turning LushText into a separate desktop sticky-notes app.

Now is the right time because the app already has stable sidecar identity, workspace-scoped note browsing patterns, and a reusable GTK-native markdown rendering surface. Extending those foundations into one coherent notes model is a better fit than introducing an unrelated out-of-process notes product.

## What Changes

- Expand the existing range-annotation model into richer range notes that support edit and rendered-reading modes while remaining sidecar data that never modifies source bytes.
- Add document-level notes for saved files so users can keep one richer note attached to a file as a whole instead of forcing all note content into line ranges.
- Add workspace-level notes so users can keep project-scoped scratchpads and reference notes inside the current workspace context.
- Provide a unified notes browsing surface that groups range, document, and workspace notes while preserving clear scope boundaries.
- Reuse LushText's native markdown rendering surface for note render mode and markdown-aware preview flows.
- Keep the feature in-process and LushText-scoped: note persistence survives restarts, but the change does not introduce OS-level sticky notes that remain visible when LushText is not running.

## Capabilities

### New Capabilities
- `document-notes`: file-level rich notes attached to one saved document and persisted outside the source file.
- `workspace-notes`: workspace-level rich notes attached to one LushText workspace scope rather than to a single document path.

### Modified Capabilities
- `sidecar-annotations`: extend saved-file range annotations into richer range notes with markdown-aware edit/render flows and unified note browsing behavior.
- `document-notes-menu`: expand the existing Notes menu contract so range, document, and workspace note workflows are grouped coherently by scope.

## Impact

- Affected specs: `sidecar-annotations`, `document-notes-menu`, plus new `document-notes` and `workspace-notes`.
- Affected code areas likely include `crates/lushtext-core/src/model`, `services/annotation_service.rs`, new note persistence services, `ui/window/notes.rs`, note-related actions and dialogs, and reuse of `ui/markdown_preview/`.
- Persistence remains under app data instead of user document trees or version-controlled project files.
