## Why

LushText already renders reference-style Markdown footnotes, but markdown-it-style inline footnotes such as `^[Text of note]` still appear as raw source text in the native preview. Supporting them closes a visible fidelity gap for documents that use inline notes without forcing users to rewrite those notes into separate `[^label]:` definitions.

## What Changes

- Add rendered preview support for inline footnote definitions written as `^[...]`.
- Preserve the existing reference-style `[^label]` footnote behavior and numbering.
- Treat inline footnotes as preview-only Markdown rendering behavior; the source buffer and saved file text remain unchanged.
- Avoid recognizing inline footnote syntax inside code spans, fenced code blocks, indented code blocks, raw HTML, and escaped literal text.
- Add deterministic coverage and sample content for mixed inline and reference-style footnotes.

## Capabilities

### New Capabilities
- `markdown-preview-inline-footnotes`: Render markdown-it-style inline footnote definitions in the native Markdown preview while preserving existing reference-style footnotes.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, Markdown preview widget tests, and `samples/markdown-test.md`.
- Affected systems: Markdown preview preprocessing, `pulldown-cmark` parser input, footnote numbering, preview text/tag rendering, and sample documentation.
- Dependencies and APIs: no new runtime dependency is expected; the change should keep the existing GTK-native `pulldown-cmark` rendering path and avoid HTML/WebKit rendering.
