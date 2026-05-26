## Context

The Markdown preview is intentionally GTK-native: `pulldown-cmark` events are
rendered into a read-only `GtkTextView`, and richer blocks such as tables and
local images are inserted as anchored GTK widgets when text tags are not enough.

Code blocks currently stay on the text-tag path. The renderer pushes a
`code-block` tag on `Tag::CodeBlock`, inserts all `Event::Text` into the parent
text buffer, and applies `paragraph_background` plus margins. This keeps the
implementation small, but `GtkTextTag` does not provide real interior padding or
a single card-like surface across blank lines. The same path also ignores fenced
language info, so code blocks cannot use GtkSourceView syntax highlighting.

## Goals / Non-Goals

**Goals:**
- Render each Markdown code block as one continuous read-only block surface.
- Preserve blank lines and source order inside fenced and indented code blocks.
- Use fenced code info strings for GtkSourceView syntax highlighting when a
  matching language is available.
- Keep the preview GTK-native and consistent with the existing table/image
  embedded-widget pattern.

**Non-Goals:**
- Do not introduce WebKit, HTML rendering, or remote code-rendering services.
- Do not implement browser-level Markdown styling or custom syntax highlighters.
- Do not change inline code spans; they remain inline text-buffer content.
- Do not require every language alias to be supported in the first iteration.

## Decisions

1. Buffer code block contents before rendering.

   When the renderer sees `Tag::CodeBlock(kind)`, it should enter a dedicated
   code-block collection state instead of pushing `TAG_CODE_BLOCK` onto the
   normal text tag stack. All text events are accumulated verbatim until
   `TagEnd::CodeBlock`, then one anchored widget is inserted into the parent
   preview flow.

   Alternative considered: keep using `paragraph_background` and add synthetic
   spaces or non-breaking characters for padding. That would still be fragile for
   blank lines, wrapping, copy semantics, and syntax highlighting.

2. Render code blocks with a native GtkSourceView widget.

   Each completed block should become a small GTK container with a CSS class for
   padding/background and a read-only `sourceview5::View` backed by a
   `sourceview5::Buffer`. The view should hide editor-only affordances such as
   the cursor, line numbers, current-line highlight, and editing behavior.

   Alternative considered: copy GtkSourceView-highlighted tags into the parent
   `GtkTextBuffer`. That would be harder to keep correct and would still leave
   the visual block-surface problem to text tags.

3. Resolve syntax language from fenced info strings.

   Fenced code blocks should use the first word of the info string as the
   language hint, matching CommonMark and `pulldown-cmark` HTML behavior. The
   renderer should map common aliases that GtkSourceView may not expose directly
   in file-guessing form, starting with JavaScript (`js` -> `javascript`). If no
   language resolves, the code block remains plain monospaced text.

   Alternative considered: rely only on `LanguageManager::guess_language` with a
   synthetic file name. That is useful as a fallback, but explicit aliases make
   common README fences reliable.

4. Use the active GtkSourceView style scheme for token colors.

   The embedded source buffer should use the same light/dark style scheme as the
   editor so syntax colors match the rest of LushText. The outer code-block
   widget owns the block padding and background. If GtkSourceView paints an
   incompatible text background, introduce a small preview-specific CSS or
   derived scheme adjustment rather than changing editor schemes globally.

   Alternative considered: use fixed hand-picked token colors. That would drift
   from user-selected editor schemes and duplicate GtkSourceView's styling work.

5. Prefer horizontal scrolling over wrapping inside code blocks.

   Code blocks should preserve indentation and line shape. The embedded block
   may use a horizontal scroller when lines exceed the preview width; the parent
   Markdown preview remains the vertical scroller.

   Alternative considered: wrap code lines to the preview width. That improves
   narrow layouts but makes code structure less faithful and can be surprising
   for indentation-heavy samples.

6. Drive embedded code-block width from the preview text column.

   `GtkTextView` child anchors do not make anchored widgets fill the buffer's
   visible text column simply because the widget has `hexpand=true`. The preview
   must compute the usable column width from the rendered `GtkTextView`
   allocation minus its left and right margins, then apply that width to each
   embedded code-block container. This width must be updated after render and
   whenever the preview allocation or readable-column margins change.

   Horizontal scrolling remains valid only when the code content is wider than
   that computed text-column width. In a wide preview with short code lines, the
   scroller's horizontal adjustment should have no meaningful overflow.

   Alternative considered: rely on GTK expansion flags alone. The current
   screenshot proves that child anchors can allocate code widgets at a narrow
   natural width even when the editor area has plenty of room.

7. Use one visual background for the whole code surface.

   The outer `.markdown-code-block` surface and the inner GtkSourceView text
   area must resolve to the same background color. The preferred source is the
   active GtkSourceView style scheme's text background because syntax token
   colors are already chosen against that palette. If no scheme background is
   available, use the existing themed preview fallback, but apply it consistently
   to both the block container and the source text area.

   Alternative considered: make only the inner source view transparent. That is
   fragile because GtkSourceView style schemes may still paint the text node,
   and the mismatch is exactly the visual defect reported here.

## Risks / Trade-offs

- Nested scroll behavior inside the preview could feel awkward on very narrow
  panes -> keep vertical scrolling owned by the parent preview and limit the
  embedded scroller to horizontal overflow.
- GtkSourceView language IDs and Markdown fence aliases will not match perfectly
  for every language -> provide explicit mappings for common aliases and fall
  back to readable plain code.
- Anchored code widgets add more GTK objects than text tags -> only create one
  widget per code block and continue clearing stale embeds through the existing
  rendered embed cleanup path.
- The code block may not naturally fill the preview width when inserted into a
  `GtkTextView` child anchor -> set responsive width constraints from the
  preview text viewport, following the existing embedded-block pattern rather
  than relying on text tag backgrounds.
- Width requests can become stale after resize or Focus Mode margin changes ->
  refresh code-block width from the current preview text view allocation after
  render and on relevant allocation/margin changes.
- Matching the source scheme background may make the outer code block darker or
  lighter than neighboring preview prose -> accept that trade-off so syntax
  colors remain readable and the code block reads as one coherent surface.
