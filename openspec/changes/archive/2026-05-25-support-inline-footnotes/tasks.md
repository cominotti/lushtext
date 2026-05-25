## 1. Inline Footnote Lowering

- [x] 1.1 Add a GTK-free Markdown preview helper that lowers eligible `^[...]` inline footnotes into generated reference-style footnote syntax for preview-only parser input.
- [x] 1.2 Use a preliminary `pulldown-cmark` offset pass to transform only parser-confirmed prose source spans while leaving code, code blocks, raw HTML, and other non-prose regions untouched.
- [x] 1.3 Implement inline-footnote delimiter scanning with escape handling, nested bracket handling, empty-body rejection, and no recursive inline-footnote expansion.
- [x] 1.4 Collect existing reference-style footnote labels and generate collision-free internal labels for lowered inline footnotes.

## 2. Preview Integration

- [x] 2.1 Route `render_markdown_with_context` through the lowering helper before constructing the main parser, while avoiding extra allocation when no eligible `^[` marker is present.
- [x] 2.2 Preserve existing footnote reference and definition rendering so inline footnotes reuse the current numbering, text tags, and definition flow.
- [x] 2.3 Ensure generated internal labels never appear in rendered preview text, link targets, or user-visible fallback states.

## 3. Regression Coverage

- [x] 3.1 Add unit coverage for simple inline footnote lowering, supported inline formatting inside the generated definition body, mixed reference-style labels, and generated-label collision avoidance.
- [x] 3.2 Add unit coverage for escaped syntax, inline code spans, fenced code blocks, indented code blocks, raw HTML, malformed delimiters, and empty inline-footnote bodies.
- [x] 3.3 Add widget coverage proving rendered inline footnotes replace raw `^[...]` source with markers and matching definitions.
- [x] 3.4 Add widget coverage for mixed inline and reference-style footnotes so every marker matches its rendered definition number.

## 4. Samples And Verification

- [x] 4.1 Update `samples/markdown-test.md` with inline footnote examples that exercise simple and mixed footnote rendering.
- [x] 4.2 Update any Markdown preview documentation or follow-up notes that describe footnote coverage if they would otherwise imply reference-style-only support.
- [x] 4.3 Run focused Markdown preview unit and widget tests.
- [x] 4.4 Run `openspec validate support-inline-footnotes --strict`.
