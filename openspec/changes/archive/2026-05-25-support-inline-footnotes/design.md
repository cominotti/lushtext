## Context

LushText's Markdown preview is a GTK-native renderer. It feeds Markdown through `pulldown-cmark`, streams events into a `GtkTextBuffer`, and uses anchored GTK widgets only for structures that need native layout such as tables, images, and code blocks.

The current preview enables `Options::ENABLE_FOOTNOTES` and handles `Event::FootnoteReference` plus `Tag::FootnoteDefinition`. That parser extension covers reference-style footnotes such as `[^label]` and `[^label]: definition`, but it does not recognize markdown-it-style inline footnote definitions written as `^[Text of note]`.

## Goals / Non-Goals

**Goals:**
- Render `^[...]` inline footnotes without changing the saved Markdown source.
- Reuse the existing `pulldown-cmark` footnote event path, text tags, numbering helper, and definition rendering.
- Keep generated inline footnotes out of code spans, code blocks, raw HTML, escaped literals, and other non-prose regions.
- Preserve existing reference-style footnote behavior and mixed-document numbering.
- Keep the implementation small enough for deterministic unit and widget coverage.

**Non-Goals:**
- Introduce a WebKit, HTML, or second Markdown rendering engine.
- Add editor-side Markdown rewriting or source-buffer decorations.
- Support recursive inline footnotes inside another inline footnote body.
- Add navigation/backreference behavior for footnote markers.
- Change how existing `[^label]` references and `[^label]:` definitions render.

## Decisions

### 1. Lower inline footnotes before the main render pass

Inline footnotes should be converted into parser-native reference-style footnotes before the existing render loop runs:

```text
Original preview source
  Body text^[Inline **note**].

Lowered preview source
  Body text[^__lush_inline_footnote_1].

  [^__lush_inline_footnote_1]: Inline **note**
```

The source buffer remains untouched. The lowered string is temporary input for the preview render only.

Alternatives considered:
- Handling `^[...]` directly inside the GTK render loop would require a parallel footnote model and duplicate existing numbering/definition rendering.
- Switching Markdown engines would solve this one extension at the cost of replacing a working native renderer.

### 2. Transform only parser-confirmed prose source spans

The lowering helper should perform a lightweight preliminary parse with the same Markdown options and use `Parser::into_offset_iter()` to derive eligible prose source spans plus protected non-prose ranges. Paragraph and heading source spans are eligible for inline-footnote recognition, while ranges for `Event::Code`, `Tag::CodeBlock`, raw HTML events, links, images, tables, and embedded widget inputs remain untouched.

This keeps the recognizer aligned with the parser's view of the document instead of trying to reimplement Markdown block and inline boundaries from scratch. It also lets a single inline footnote body contain supported inline formatting such as emphasis or links, because the scanner can capture the original source across nested inline events while still skipping protected regions.

Alternatives considered:
- Scanning the entire source string would be simpler but would incorrectly recognize `^[...]` inside code fences, inline code, and HTML.
- Waiting until after the main parser pass would be too late because `pulldown-cmark` does not emit a distinct inline-footnote event.

### 3. Use generated labels that cannot collide with user labels

The lowering pass should collect existing footnote labels from native footnote references and definitions, then generate labels with an internal prefix such as `__lush_inline_footnote_1`. If a source document already uses that label, the generator must advance until it finds an unused label.

The rendered marker text should still use the existing numeric display, so internal labels never appear in the preview.

Alternatives considered:
- Reusing the inline footnote body as the label would leak long text into parser labels and create collision problems.
- Using fixed labels without collision checks would make rare but confusing documents render incorrectly.

### 4. Keep inline footnote body parsing inline-oriented

The scanner should match `^[` followed by a balanced `]`, honoring backslash escapes and nested bracket pairs. The captured body is inserted as one generated footnote definition. Supported inline Markdown inside that body, such as emphasis, links, and inline code, is parsed by the existing footnote definition path.

The first implementation should not try to support recursive inline footnotes or block-level Markdown inside the body. Newlines inside a body can be preserved as text, but multi-paragraph footnote bodies remain the job of reference-style definitions.

Alternatives considered:
- Supporting arbitrary block content inside `^[...]` would make delimiter matching and generated-definition layout much more complex than the user-facing syntax implies.
- Treating all `]` as the close delimiter would break common inline content such as links and bracketed text.

### 5. Keep verification focused on the preview boundary

Unit coverage should exercise the lowering helper without GTK. Widget coverage should verify the rendered preview text and tags through `LushtextMarkdownPreview`, including mixed reference and inline footnotes, escaped literals, and code/code-block exclusions. The canonical sample file should include inline footnote examples so the visual showcase matches shipped behavior.

Alternatives considered:
- End-to-end window tests would add slower coverage without much extra confidence because preview refresh already delegates into `LushtextMarkdownPreview`.

## Risks / Trade-offs

- [Generated labels could collide with user labels] -> Collect existing labels and generate unused internal labels.
- [Lowering could accidentally alter code or HTML] -> Transform only parser-confirmed prose spans from `into_offset_iter()` and add regression coverage for code spans, fenced blocks, and escaped syntax.
- [Bracket matching could be surprising] -> Support escapes and nested bracket pairs, document that recursive inline footnotes and block-level bodies are out of scope.
- [Mixed footnote ordering may differ from a browser renderer] -> Preserve LushText's existing preview-local numbering rule and require marker/definition agreement rather than browser-identical layout.
- [Preliminary parsing adds work on every preview refresh] -> Keep the helper linear in input size and skip allocation-heavy lowering when no eligible `^[` marker exists.

## Migration Plan

No persisted data migration is required. The change affects only Markdown preview rendering. Rollback is low risk: removing the lowering helper returns `^[...]` to raw text rendering without touching source files, sessions, drafts, notes, or sidecar data.

## Open Questions

None blocking.
