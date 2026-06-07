## Why

Bookmark rows in `Browse Notes...` currently preview only metadata such as source, file path, and line number, which makes the preview pane feel empty even when the bookmark points into useful document content. The browser should let users inspect why a bookmarked line matters before opening the file.

## What Changes

- Add contextual bookmark previews in `Browse Notes...` using a bounded excerpt around the bookmarked line.
- Prefer live open-editor buffer text for bookmark previews when the bookmarked file is already open, so unsaved edits and freshly moved bookmarks are reflected.
- For closed files, load a bounded read-only excerpt on a background thread through the filesystem boundary, with explicit loading and unavailable states.
- Render bookmark excerpts as Markdown when the bookmarked file is Markdown-like, using the existing Markdown preview widget and the bookmarked file's render context.
- Render non-Markdown bookmark excerpts as raw monospace text, preserving line breaks and emphasizing the bookmarked line.
- Include nearby context before and after the bookmarked line rather than only starting at the bookmark.
- Keep preview loading bounded and non-blocking; do not turn `Browse Notes...` into workspace-wide content search.
- Add regression coverage for Markdown bookmark preview, raw text bookmark preview, open-editor live preview, closed-file fallback states, and layout/selection stability.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `line-bookmarks`: Bookmark entries in `Browse Notes...` gain anchored content previews instead of metadata-only placeholder previews.
- `workspace-notes`: The unified `Browse Notes...` preview pane gains bounded asynchronous bookmark excerpt rendering while preserving existing workspace/document/workspace-note browsing semantics.

## Impact

- Affected UI: `crates/lushtext-core/src/ui/window/notes.rs` bookmark-preview orchestration and `LushtextMarkdownPreview` usage.
- Affected services: a new or extended GTK-free bounded excerpt helper under `crates/lushtext-core/src/services/` for closed-file bookmark previews.
- Affected tests: widget tests for `Browse Notes...` bookmark preview states and service/unit tests for bounded excerpt extraction.
- No bookmark sidecar schema, workspace-note sidecar schema, document-note sidecar schema, or persistence migration is expected.
