## Context

LushText already ships two relevant foundations for richer notes: saved-file range annotations persisted outside the source file, and a GTK-native markdown preview widget that can render markdown into a read-only surface. The app also has proven patterns for browse-heavy secondary workflows through adaptive dialogs, and it already scopes workspace-aware note flows through the shared current workspace scope.

This change crosses model, service, editor, sidebar, and window layers. It must preserve the current promise that note workflows never modify user file bytes, stay predictable across rename and Save As flows, and remain inside LushText's process instead of becoming an always-on desktop sticky-notes system.

## Goals / Non-Goals

**Goals:**
- Extend existing range annotations into richer range notes with edit and rendered-reading modes.
- Add one document-level rich note for each saved file.
- Add one workspace-level rich note for each workspace root.
- Provide one workspace-scoped note browser that can surface range, document, and workspace notes together.
- Reuse LushText's existing markdown preview implementation instead of introducing a second rendering stack.
- Keep note persistence outside source files and version-controlled project trees.

**Non-Goals:**
- OS-level sticky notes or note windows that remain visible after LushText exits.
- Multi-user synchronization, shared comments, or network-backed note storage.
- Multiple titled document notes or multiple titled workspace notes in the MVP.
- Rich-text WYSIWYG editing, embedded attachments, or always-visible inline rendered note blocks.
- Expanding export beyond the existing range-note export workflow in the MVP.

## Decisions

### 1. Keep scope-specific note stores, but share one markdown-capable note-body contract

Range, document, and workspace notes all need the same core note-body behavior: editable UTF-8 text, rendered markdown reading mode, timestamps, and read-only preview. The design will keep separate persisted document types and services for each scope, but the note body itself should stay uniform so the UI can reuse the same edit/render flow everywhere.

For existing range annotations, the stored `note_text` field remains a plain text string and becomes markdown-capable by interpretation rather than by schema change. That preserves compatibility with existing sidecar data and avoids a migration that rewrites annotation payloads only to change meaning, not structure.

Alternatives considered:
- A single monolithic "all notes" store with one generic polymorphic record type: flexible, but heavier than the current architecture needs and harder to reason about for persistence and scope-specific behavior.
- Independent note-body implementations per scope: simpler to start, but it would duplicate the same edit/render rules and drift quickly.

### 2. Keep the MVP intentionally small: many range notes, one document note, one workspace note

Range notes already naturally support multiple records because line ranges are the identity boundary. Document notes and workspace notes do not need that complexity in the first release. One document note per saved file and one workspace note per workspace root gives users the missing scope levels without introducing titles, ordering, multi-note CRUD, or a second note-management taxonomy.

This keeps the first browse surface understandable: one workspace note row per workspace, one document note row per saved file that has a note, and zero or more range note rows under those files.

Alternatives considered:
- Multiple titled notes at every scope: more powerful, but it would force list management, note naming, ordering, and more complex actions immediately.
- A single global scratchpad instead of workspace notes: simpler, but it would not honor LushText's workspace-first mental model.

### 3. Key file-scoped notes by canonical file identity and workspace notes by canonical root identity

Saved-file range and document notes should follow the same durable identity contract: canonical file path determines the persisted sidecar identity, in-app renames migrate it, and Save As starts a fresh identity. This matches the existing note-sidecar and local-history behavior users already have.

Workspace notes should not be keyed by the persisted `WorkspaceId` alone. `WorkspaceId` is a UI/workflow identifier, but a workspace note conceptually belongs to the project root. If a user unlists a workspace and later re-adds the same root, the same workspace note should return. If the user explicitly replaces a workspace root with a different directory, the old workspace note should not silently follow to the new project. The design therefore uses a canonical-root identity for persistence, while the UI continues to target workspace notes through the current workspace selection.

Alternatives considered:
- Keying workspace notes by `WorkspaceId`: easy to wire, but notes would follow a replaced workspace slot to an unrelated project and would be lost on unlist/re-add of the same root.
- Storing workspace notes directly in the workspace root directory: visible to the user, but it breaks the app's contract of keeping note metadata outside user project trees.

### 4. Reuse `LushtextMarkdownPreview` for rendered note mode and read-only note previews

The existing markdown preview widget already handles markdown rendering, placeholders, and real-file context. Notes should reuse that widget for render mode and for read-only preview surfaces instead of building a second renderer, a webview, or a custom inline markup system.

Edit mode can remain a lightweight text editing surface. Render mode always derives from the current note text, so the system never has to synchronize a hidden transformed representation back into the stored note body.

Alternatives considered:
- A web-based renderer: more flexible visually, but inconsistent with the app's GTK-native direction and unnecessary given the current preview widget.
- Rich-text editing in the note surface: attractive, but it would create a second editing model and substantially increase complexity.

### 5. Provide one unified notes browser patterned after the local-history viewer

The app already has a strong pattern for browse-heavy secondary workflows: a large adaptive dialog with a browse rail and a preview-dominant reading area. The new notes browser should follow that pattern and operate on the current shared workspace scope. In concrete scope it lists only the selected workspace's notes; in aggregate scope it can list notes across all restored workspaces while preserving each row's scope metadata.

Opening a row from that browser should route into the scope-appropriate note surface:
- range note -> open file, focus range, reopen the range-note editor/view
- document note -> open file, open document note
- workspace note -> open workspace note for that workspace root

Alternatives considered:
- Separate browsers for range, document, and workspace notes: clear separation, but too fragmented for the "rich notes" story.
- Reusing the properties panel for note browsing: too narrow for a browse-and-read workflow and inconsistent with existing large viewer patterns in the app.

### 6. Keep the feature fully in-process and LushText-scoped

This design intentionally stops short of desktop sticky notes. Notes persist across restarts, but they are only visible while LushText is running. That keeps the feature aligned with the current application boundary, avoids background-process or session-service work, and preserves the editor as the primary product.

Alternatives considered:
- A companion sticky-notes app or background helper: potentially useful later, but a different product boundary than this change.

## Risks / Trade-offs

- [The shift from "annotations" to richer "range notes" can blur user expectations] -> Keep persisted annotation payloads compatible, but update user-facing labels and menu actions consistently so one note vocabulary is visible at the surface.
- [Workspace-note identity can feel surprising during root changes] -> Key notes by canonical root identity, document that explicit `Replace Workspace Root` starts fresh, and migrate only true in-app rename flows.
- [A unified notes browser could become a grab bag] -> Keep the MVP to one note per document/workspace and preserve explicit scope metadata in every row.
- [Markdown render mode could diverge from what users think is stored] -> Render directly from the current note text and never store a second transformed note representation.
- [This change touches multiple layers at once] -> Reuse existing sidecar, markdown preview, workspace-scope, and adaptive-dialog patterns instead of inventing new abstractions in every layer.

## Migration Plan

No destructive migration is required. Existing annotation sidecars remain valid because the stored note text shape does not need to change. New document-note and workspace-note data will live under new app-data directories, created lazily when the first note is saved.

Rollback is low risk. If the feature is removed later, source documents remain unchanged and the new sidecar data simply becomes unused app data. Existing annotation sidecars stay readable by the old workflow because the persisted record shape remains compatible.

## Open Questions

- Should the first unified notes browser support full-text filtering across note bodies immediately, or is metadata-first filtering sufficient for the MVP?
- Should a later follow-up broaden export from range notes to document and workspace notes once the richer note model has shipped and settled?
