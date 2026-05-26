## Why

Markdown preview currently leaves pulldown-cmark definition-list syntax on the raw paragraph path, so documents that use term/definition structure are harder to read and can expose layout regressions when nested blocks appear inside definitions. pulldown-cmark 0.13 already provides native definition-list parser events, making this a focused renderer fidelity improvement that fits the existing GTK-native preview architecture.

## What Changes

- Enable pulldown-cmark's native `ENABLE_DEFINITION_LIST` option for Markdown preview rendering.
- Render colon-based pulldown-cmark definition lists as readable term and definition blocks instead of showing raw definition markers.
- Preserve supported inline Markdown inside definition-list terms and definitions.
- Preserve nested supported Markdown blocks inside definitions, including paragraphs, blockquotes, ordinary lists, and code blocks.
- Keep the existing code-block width contract when code blocks appear inside definition-list definitions, so horizontal scrolling appears only for genuinely long code lines.
- Explicitly exclude markdown-it-only definition-list marker compatibility such as compact `~` markers from this change.

## Capabilities

### New Capabilities
- `markdown-preview-definition-lists`: Defines native Markdown preview rendering for pulldown-cmark 0.13 definition-list events, including term styling, definition indentation, nested inline content, nested block content, and parser-boundary non-goals.

### Modified Capabilities
- None.

## Impact

- Affected renderer code: `crates/lushtext-core/src/ui/markdown_preview/mod.rs` and `crates/lushtext-core/src/ui/markdown_preview/imp.rs`.
- Affected tests: Markdown preview widget tests under `crates/lushtext/tests/widget/markdown_preview.rs`, with parser event-shape coverage before renderer behavior assertions.
- Affected documentation: Markdown preview follow-up notes should stop listing definition lists as future work once implemented.
- No new runtime dependency is expected; this uses the existing `pulldown-cmark` 0.13 dependency.
