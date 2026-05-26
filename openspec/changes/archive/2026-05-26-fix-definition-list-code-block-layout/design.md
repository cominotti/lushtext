## Context

Markdown preview code blocks are embedded `sourceview5::View` widgets anchored into a `GtkTextView`. The existing code-block width fix refreshes those widgets to the preview text column width after render, map, allocation, and readable-column changes. That is enough for top-level code blocks, but definition-list support exposed a nested-context gap: text inside a definition receives `GtkTextTag` margins, while the child-anchor code-block widget does not inherit those tags or their effective layout column.

The result is visible in definition-list samples: the code block can render as a narrow clipped box with a horizontal scrollbar even when the code line would fit the available definition body column. The parser is already emitting a normal `CodeBlock` inside `DefinitionListDefinition`; this is a GTK layout-context issue, not a pulldown-cmark issue.

## Goals / Non-Goals

**Goals:**
- Size embedded Markdown code blocks against the effective text column for the block context where they are inserted.
- Make definition-list code blocks follow the same false-overflow contract as top-level code blocks.
- Preserve true horizontal scrolling for genuinely long code lines.
- Cover the bug with geometry-focused widget tests that inspect allocation/adjustments, not only rendered text.
- Keep the solution reusable for other nested contexts such as lists, blockquotes, alert bodies, and footnote definitions.

**Non-Goals:**
- Do not change pulldown-cmark options or definition-list parsing behavior.
- Do not wrap code lines or replace horizontal scrolling for genuinely long lines.
- Do not move Markdown preview to WebKit or HTML rendering.
- Do not redesign definition-list typography beyond layout correctness for embedded code blocks.
- Do not introduce a new runtime dependency.

## Decisions

### Store layout metadata with embedded widgets

The preview should track embedded widgets as records that include both the GTK widget and the layout context captured at insertion time. Code-block width refresh can then compute a per-widget width instead of applying one global preview-column width to every `.markdown-code-block`.

Alternative considered: infer context later from CSS classes or nearby text-buffer tags. That is brittle because child anchors are not normal tagged text, and the renderer already has the semantic context available when it inserts the widget.

### Model the effective code-block column explicitly

The renderer should compute an `EmbeddedBlockLayout` or equivalent value from the current Markdown block state: definition body margins, list depth, blockquote depth, alert body margin, and footnote definition margin. The code-block container should receive the visual horizontal offset, and the width request should be the preview text column minus that offset. This makes the child-anchor widget align with nearby text that is laid out through `GtkTextTag` margins.

Alternative considered: remove definition-list margins so anchored widgets and text both use the root column. That would hide this one symptom but would make definition bodies harder to scan and would not help future anchored widgets in other nested contexts.

### Keep code-block scrolling semantics unchanged

The inner `GtkScrolledWindow` should continue to own horizontal scrolling for code that is genuinely wider than the effective code-block viewport, while vertical scrolling remains owned by the Markdown preview. The fix should not set wrapping on the `GtkSourceView`.

Alternative considered: force wrapping for nested code blocks. That would avoid horizontal scrollbars, but it would change Markdown code-block semantics and make copied/visually inspected code less faithful.

### Strengthen tests around real geometry

The regression tests should assert actual allocated geometry and horizontal adjustment state after GTK layout settles. At least one test should use the screenshot-style definition-list sample, because the previous short-code test did not reproduce the failure strongly enough.

Alternative considered: test only buffer text and presence of a source view. That verifies parsing but not the child-anchor sizing bug.

## Risks / Trade-offs

- Nested margin math can drift from `GtkTextTag` values -> Centralize constants or helper functions so text tags and embedded layout use the same margin policy.
- Existing top-level code-block tests could start depending on nested layout defaults -> Keep top-level layout as the zero-offset baseline and rerun the full Markdown preview widget suite.
- GTK allocation timing can make geometry tests flaky -> Reuse the existing wait helpers and extend them to wait for both width request and allocated scroller state.
- Storing metadata for embedded widgets slightly broadens preview state -> Keep it local to `LushtextMarkdownPreview` and clear it with the existing rerender cleanup path.

## Migration Plan

This is a preview-only rendering fix. No user documents, settings, sidecar files, or persisted application data require migration. Rollback is restoring the previous global code-block width refresh behavior, which would reintroduce the nested false-overflow bug but not affect saved data.

## Open Questions

None.
