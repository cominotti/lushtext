## Why

When preview-only or side-by-side Markdown preview is active and the user opens a `.md` file, the new tab shows an empty preview until preview is toggled off and on. The preview renders the buffer at tab-selection time and is never told that the document finished loading, so every path that installs content with projections suspended (open, eviction reload, external-change reload, Replace All auto-reload, reopen with encoding, draft restore, local-history restore and undo, save mirror-back) leaves the preview stale. The same symptom appears when the tab's language identity changes without a load: Save As to `notes.md` or a sidebar rename leaves "Not a Markdown file" on screen.

## What Changes

- Add one editor-level "content republished" fan-out that fires at four sites: document-load success publish, document-load failure publish, every buffer-replacement terminal that restores the projection guard (all reasons except `Disposed`), and `republish_document_identity` (path or language identity change). The window registers one listener per editor and refreshes the preview when that editor is the selected page.
- While the selected Markdown editor is still installing content (`load_state == Loading`, a projection-suspending replacement in flight, or an incomplete load installation), `refresh_preview` shows the existing "Preparing Markdown preview…" content placeholder instead of rendering the empty or partial buffer.
- Record the UX decision: preview mode stays window-level state. Opening an existing document keeps the current preview mode and renders the new document in it. New untitled tabs keep exiting preview-only mode, which `new-document-flow` already owns.
- Widget tests at window level (none exist today for the preview): open a `.md` with preview active and assert rendered text after `Loaded`; assert the preparing placeholder during load; assert a refresh after a buffer replacement and after Save As on the selected tab; assert a background tab's terminal does not re-render.

## Capabilities

### New Capabilities
- `markdown-preview-content-freshness`: the Markdown preview reflects the selected document's installed content and language identity at every install and identity terminal, and names the installing interval honestly.

### Modified Capabilities

## Impact

- `crates/lushtext-core/src/ui/editor_page/load/mod.rs` (fan-out registration beside `connect_file_loaded`), `load/execution.rs` (success and failure publish fire it), `editor_page/buffer_replacement/execution.rs` (`finish_session` fires it after `restore_guard`), `editor_page/document_identity.rs` (`republish_document_identity` fires it).
- `crates/lushtext-core/src/ui/window/documents.rs` (one wiring site beside `wire_modified_indicator`), `window/preview.rs` (installing-state placeholder branch).
- `crates/lushtext/tests/widget/window.rs` or a new window-level preview module for the widget tests.
- `AGENTS.md` key design decisions (Markdown preview bullet); `docs/workflow-readability-matrix.md` row `WFR-MARKDOWN-PREVIEW` (its size cell already records `ui/window/preview.rs` at 572/556 while the file is 586, so re-derivation is owed regardless), plus `WFR-DOCUMENT-LOAD` and `WFR-BUFFER-REPLACEMENT` if their cells move.
- No GSettings, D-Bus, action, readiness, or persistence changes. No new `*_for_test` seam; the `*_for_test` ratchet stays at its recorded ceiling.
