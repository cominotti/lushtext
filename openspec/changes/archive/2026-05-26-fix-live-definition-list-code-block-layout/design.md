## Context

Markdown preview code blocks render as embedded `sourceview5::View` widgets inside a `GtkTextView` child anchor. The previous layout fix stores a captured nested block context and sets a width request on each `.markdown-code-block` container. That path passes standalone `LushtextMarkdownPreview` widget tests, but the user-visible failure remains in the real app when Markdown Preview is opened from the hidden preview shell.

The real shell adds lifecycle complexity the existing tests do not cover:

```text
LushtextWindow
└─ GtkPaned preview_paned
   ├─ editor_box
   └─ LushtextMarkdownPreview, initially hidden
      └─ GtkScrolledWindow
         └─ GtkTextView
            └─ child anchor: embedded code block
```

Preview-only and side-by-side preview both transition the preview from hidden to visible through `GtkPaned` animation. The screenshot shows the definition-list code block is indented under the definition body, but its visible width is still close to natural content width and the inner horizontal scrollbar remains. That means width-request assertions on a directly mounted preview are not sufficient evidence; the acceptance surface is the final live allocation and scroller adjustment state after the preview shell settles.

pulldown-cmark behavior is not the problem. Version 0.13.4 exposes `Options::ENABLE_DEFINITION_LIST` plus `Tag::DefinitionList`, `Tag::DefinitionListTitle`, and `Tag::DefinitionListDefinition`, and the parser accepts indented code blocks inside definitions. This change stays focused on GTK layout and tests.

## Goals / Non-Goals

**Goals:**
- Reproduce the screenshot-style failure through `LushtextWindow`, not only a standalone preview widget.
- Make nested definition-list code blocks settle to the effective definition-body code column after preview-only mode enters from a hidden preview.
- Cover side-by-side preview pane activation from hidden state as the same lifecycle class.
- Assert real allocation and horizontal adjustment state for the code block container, scroller, and source view.
- Preserve true horizontal scrolling for genuinely long code lines.
- Keep the existing parser boundary around pulldown-cmark definition-list support.

**Non-Goals:**
- Do not add markdown-it compact `~` definition-list compatibility.
- Do not change pulldown-cmark options or parser preprocessing.
- Do not wrap Markdown code-block lines.
- Do not replace the GTK-native preview with WebKit or HTML rendering.
- Do not redesign definition-list typography beyond making nested code blocks correctly sized and readable.
- Do not introduce a new runtime dependency.

## Decisions

### Treat the live preview shell as the regression surface

The primary regression tests should construct a `LushtextWindow`, create an active Markdown tab, populate the screenshot-style sample, activate Markdown Preview, wait for the preview animation and render debounce to settle, then inspect embedded code-block geometry through the window's `markdown_preview`.

Standalone preview tests remain useful for parser and renderer primitives, but they cannot prove hidden-to-visible preview-shell behavior. The failure came from the app shell, so at least one test must follow that path.

Alternative considered: keep strengthening only `markdown_preview.rs` tests. That would repeat the earlier mistake because the standalone mount does not exercise `GtkPaned`, hidden initial visibility, preview-only animation, or shell-level render timing.

### Assert allocated geometry, not just requested geometry

Tests should measure the final visible geometry using widget allocation and `compute_bounds` relationships. The contract should include:

- the code-block container width is near the expected nested code column;
- the scroller bounds sit inside the container and are wider than the natural tiny clipped surface from the screenshot;
- the horizontal adjustment overflow is absent for the screenshot line;
- the code block remains visually nested under the definition body instead of expanding to the root preview column.

Width request remains a useful diagnostic, but it is not the acceptance condition by itself. GTK can retain stale child-anchor geometry even when an outer widget has a plausible request.

Alternative considered: assert only `GtkAdjustment::upper - page_size`. That catches the scrollbar symptom but misses the tiny-box allocation itself and would not distinguish a correctly sized block from a hidden or zero-width widget.

### Refresh embedded code-block layout when the preview shell settles

The implementation should make code-block width refresh happen after the real preview shell obtains its final allocation, not only at render time and the next idle. Candidate hooks include preview-only animation completion, side-by-side animation completion, `GtkTextView` allocation/width notifications, and explicit refresh after the window calls `refresh_preview()`.

The exact code shape can be narrow, but the invariant is clear: after the preview becomes visible or its paned position changes, embedded code blocks must re-evaluate their width from the current `GtkTextView` text column and each block's captured layout context. If GTK child anchors need more than a `width_request`, the fix should also queue resize on the relevant text view/container and, if necessary, propagate the computed width to the inner scroller/source-view layer.

Alternative considered: render only after the preview animation completes. That may avoid one timing window but makes the UI feel stale and does not solve later width changes or side-by-side resizing.

### Keep parser and definition-list semantics stable

This change should not reinterpret Markdown syntax. Definition-list parsing remains whatever pulldown-cmark 0.13 emits. The affected contract is how embedded code-block widgets are sized and refreshed once such a block exists inside a definition body.

Alternative considered: special-case definition-list source text before parsing. That is unrelated to the observed failure and would blur the parser boundary chosen in `support-markdown-definition-lists`.

## Risks / Trade-offs

- Window-level widget tests can be more timing-sensitive -> Wait for animation state, preview content, positive allocations, and adjustment page sizes before asserting.
- Refresh hooks can become noisy during paned animation -> Keep expensive work narrow to existing embedded code blocks and avoid settings writes or full rerenders.
- Fixing only preview-only mode could leave side-by-side mode broken -> Include tests for both hidden-to-visible paths or explicitly justify one shared helper that both paths use.
- Child-anchor allocation can ignore outer requests until the text view relayouts -> Tests should inspect visible bounds so the implementation is forced to queue the necessary GTK relayout work.
- Existing active change artifacts say the previous fix is complete -> This change should supersede that proof with live-shell requirements and should not treat old task completion as acceptance.

## Migration Plan

This is a preview-only rendering fix. It does not change user documents, settings, sidecar files, sessions, parser output, or persisted data. Rollback would restore the current behavior where standalone preview tests may pass while real preview-shell definition-list code blocks can still allocate as tiny clipped boxes.

## Open Questions

None.
