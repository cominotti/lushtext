## Why

LushText's Markdown preview now handles tables, but common GitHub-flavored Markdown still loses meaning in preview mode because task list state, alert callout semantics, and footnote structure are either flattened or dropped. That makes README-style documents harder to trust when users switch into preview.

## What Changes

- Extend the native Markdown preview parser configuration to recognize task lists, GitHub alert callouts, and footnotes alongside the existing table and strikethrough support.
- Render task lists, alert callouts, and footnote references and definitions in the existing GTK-native preview without introducing an HTML or WebKit rendering path.
- Keep the new rendering paths aligned with the current preview architecture: `GtkTextBuffer` and `GtkTextTag` rendering for flow content, with focused helper state where parser events need coordination.
- Add deterministic preview tests that cover the new GFM subset and protect against regressions where raw source syntax leaks through or semantic structure disappears.

## Capabilities

### New Capabilities
- `markdown-preview-gfm-basics`: Render task lists, GitHub alert callouts, and footnotes as readable native preview content.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, preview styling in `resources/style/style.css` when needed, and Markdown preview tests in `crates/lushtext/tests/widget/markdown_preview.rs`.
- Affected systems: `pulldown-cmark` parser options and preview event handling for list items, blockquotes, and footnote definitions and references.
- Dependencies and APIs: no new external dependency is expected; the change extends the existing parser integration and preview-local helper state.
