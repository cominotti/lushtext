## Why

Markdown preview currently renders code blocks as plain monospaced `GtkTextTag`
paragraphs. That makes fenced blocks with blank lines look like multiple blocks,
leaves the highlighted background tight against the code text, and ignores the
fence language info that should enable syntax highlighting.

After the first embedded-widget pass, code blocks can still become illegible on
ordinary preview widths because `GtkTextView` child anchors do not automatically
expand anchored widgets to the available text column. A code block may therefore
allocate at a tiny natural width and show a horizontal scrollbar even when the
preview/editor area has enough space to display the full line. The inner source
view can also paint a different text background than the surrounding block,
making the code look like a nested patch instead of one coherent surface.

## What Changes

- Render Markdown fenced and indented code blocks as one continuous read-only
  code surface with interior padding.
- Preserve blank lines inside code blocks without visually splitting a single
  block into multiple highlighted regions.
- Use fenced code info strings to apply syntax highlighting when GtkSourceView
  has a matching language.
- Keep unsupported or missing code languages readable as plain monospaced code.
- Keep inline code rendering on the existing inline text path.
- Size embedded code blocks to the available Markdown preview text column so
  horizontal scrolling appears only for real line overflow.
- Render the outer code-block surface and inner code text area with one matching
  background color.

## Capabilities

### New Capabilities
- `markdown-preview-code-blocks`: Rendering behavior for fenced and indented
  Markdown code blocks in the native preview.

### Modified Capabilities

## Impact

- Affected UI: `crates/lushtext-core/src/ui/markdown_preview/`.
- Affected tests: Markdown preview widget tests under
  `crates/lushtext/tests/widget/markdown_preview.rs`.
- Affected dependencies: existing `pulldown-cmark`, GTK4, and GtkSourceView
  paths only; no WebKit or remote rendering dependency is introduced.
