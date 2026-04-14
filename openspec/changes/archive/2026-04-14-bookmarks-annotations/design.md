## Context

LushText already has the editor, sidebar, draft, session, and JSON persistence foundations needed for personal file metadata, but it does not yet model or render bookmarks or annotations. The new feature crosses model, service, editor, window, and preference layers, and it must preserve a hard constraint from the product note: bookmarks and annotations must never modify the underlying file bytes.

This design also has to work with the existing file lifecycle in LushText: files can be opened from the sidebar, renamed from the sidebar, saved under a new path, restored from session state, and edited while file monitors report external changes. The feature therefore needs a clear document identity model, a live in-buffer representation for open editors, and sidecar persistence that stays predictable when paths change.

## Goals / Non-Goals

**Goals:**
- Add persistent bookmarks for file-backed documents with gutter visibility, labels, and next/previous navigation.
- Add persistent annotations for file-backed documents with lightweight viewing and editing that never touches the source file.
- Keep bookmark and annotation metadata in the app data directory and restore it automatically when a file is reopened.
- Track bookmark and annotation positions while a document is open so normal inserts and deletes keep notes attached to the intended lines.
- Support exporting annotations to markdown for handoff and review workflows.

**Non-Goals:**
- Multi-user synchronization, shared comment threads, or network-backed collaboration.
- Rich-text annotations, embedded attachments, or always-visible inline note rendering in the first iteration.
- Automatic reconciliation of every possible external file rewrite; first release favors predictable recovery over invisible heuristics.
- Support for untitled buffers before they are saved to a stable file path.

## Decisions

### 1. Use path-based sidecar identity with explicit in-app migration hooks

Bookmarks and annotations will be stored under new app-data directories, keyed by a hash of the file's canonical path, with the human-readable path also stored inside each payload. This keeps duplicated files from accidentally sharing private notes and matches the user's mental model that notes belong to a document path inside a workspace.

When LushText itself renames a file from the sidebar, the existing rename callback path will also move the sidecar records so metadata survives the rename. `Save As` will intentionally create a new document identity with no copied bookmarks or annotations, which prevents personal notes from silently leaking into derived files.

Alternatives considered:
- Reusing a plain `DefaultHasher` file identifier from existing draft flows: familiar, but too implicit for rename and duplication semantics.
- Inode-based identity: resilient to renames, but harder to reason about across copies, moves between filesystems, and export/import workflows.
- Content-hash identity: avoids path coupling, but identical copies would incorrectly share notes.

### 2. Use `GtkSourceMark` as the live bookmark source of truth for open documents

Bookmarks are fundamentally point-in-buffer metadata, so `GtkSourceMark` is the right live representation. It already integrates with GtkSourceView gutters and moves correctly as the buffer is edited, which avoids a custom line-tracking layer for the common case.

Persisted bookmark records will be loaded into marks when a file-backed editor opens, and mark changes will feed back into the bookmark service for saving. The persisted record remains the long-term source of truth across sessions; the marks are the open-document projection.

Alternatives considered:
- Storing only raw line numbers in the editor state: simpler on paper, but brittle under edits and disconnected from gutter APIs.
- Building a custom mark model in parallel with GtkSourceView: unnecessary duplication and more opportunities for desynchronization.

### 3. Represent annotations as persisted range records plus paired live anchors

Annotations need both structured persisted data and open-buffer tracking. Each annotation record will store an ID, line-range data, note text, timestamps, and presentation style. When the document is open, LushText will create paired live anchors for the start and end lines so inserts and deletes can update the range without rewriting the whole annotation model on every keystroke.

The UI layer will treat annotations as lightweight gutter affordances that open a popover or panel for reading and editing. This keeps the first iteration focused and avoids turning notes into a second document layer that competes with the main editor text.

Alternatives considered:
- Raw integer line ranges only: easy to persist, but too fragile while the buffer is actively edited.
- Always-visible inline rendered annotations: discoverable, but much higher complexity and visual weight for a first release.

### 4. Split implementation into small, testable layers

The feature will introduce explicit models and services for bookmarks and annotations rather than hiding logic inside `EditorPage` or `window` modules. `EditorPage` remains responsible for the buffer-local projection (marks, gutter affordances, selection context), while services own sidecar load/save/export behavior and models define stable serialized payloads.

This follows the repo's existing architecture style: GTK adapters stay thin, persistence lives in services, and the data model stays GTK-free. It also keeps benchmark and unit-test coverage possible without requiring live widget tests for every behavior.

Alternatives considered:
- Putting persistence and UI mutation directly in `EditorPage`: faster short-term, but makes reuse and deterministic testing harder.
- A single combined "notes manager" spanning all windows and files: possible later, but unnecessary before the base document-scoped workflow exists.

## Risks / Trade-offs

- [External file edits can make stored line ranges stale] -> Keep source files untouched, prefer best-effort restore, and surface recovery or cleanup paths instead of silently rewriting user notes.
- [Too many gutter indicators can add visual noise] -> Use compact default marks and move labels/details into on-demand UI such as popovers or lists.
- [This touches multiple layers at once] -> Keep models and services small, add focused persistence tests first, and use widget tests only for visible editor behavior.
- [Path-based identity means metadata does not automatically follow every filesystem move] -> Handle in-app rename explicitly and document that external moves behave like a new file unless a future migration feature is added.

## Migration Plan

No user data migration is required because this is a new capability. Implementation will lazily create new bookmark and annotation sidecar directories in the existing LushText data home. Rollback is low risk because disabling the feature simply leaves the sidecar files unused; source documents remain unchanged.

## Open Questions

None blocking. The implementation may choose a dedicated list panel, command-palette entry point, or both for bookmark and annotation browsing as long as the capability requirements remain satisfied.
