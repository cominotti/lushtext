## Context

LushText's Markdown preview is a GTK-native renderer backed by pulldown-cmark 0.13. Most Markdown content renders through `GtkTextBuffer` tags, while tables, local images, and code blocks use embedded child-anchor widgets. The renderer currently enables tables, task lists, footnotes, strikethrough, and GFM behavior, but it does not enable `Options::ENABLE_DEFINITION_LIST`, so colon-style definition lists remain raw paragraph text.

pulldown-cmark exposes definition lists as explicit `DefinitionList`, `DefinitionListTitle`, and `DefinitionListDefinition` start/end events. That gives LushText a parser-native boundary for implementation. The screenshot that motivated this work came from markdown-it and includes compact `~` markers, but the user explicitly chose to focus on what pulldown-cmark provides.

## Goals / Non-Goals

**Goals:**
- Render pulldown-cmark definition-list events as readable term/definition content.
- Preserve supported inline formatting inside terms and definitions by reusing the existing inline tag path.
- Preserve supported nested block rendering inside definitions, including existing code-block widgets.
- Keep nested code-block width behavior consistent with the existing `markdown-preview-code-blocks` contract.
- Add tests that lock down pulldown-cmark event shape before relying on renderer behavior.

**Non-Goals:**
- Do not add markdown-it compatibility for compact `~` definition markers.
- Do not introduce an HTML or WebKit preview renderer.
- Do not add a new Markdown parser dependency.
- Do not change table, footnote, list, image, or code-block semantics outside definition-list nesting.
- Do not implement the broader pulldown-cmark extension batch for smart punctuation, superscript, subscript, math, metadata blocks, or wikilinks.

## Decisions

### Use pulldown-cmark events as the sole source of truth

Enable `Options::ENABLE_DEFINITION_LIST` and handle the emitted `Tag::DefinitionList`, `Tag::DefinitionListTitle`, and `Tag::DefinitionListDefinition` variants in the existing streaming renderer.

Alternative considered: pre-process markdown-it-style definition syntax before parsing. That would blur parser ownership, create compatibility behavior pulldown-cmark does not provide, and make future parser upgrades harder to reason about.

### Render definition lists on the text/tag path

Definition-list terms and simple definition prose should stay in the `GtkTextBuffer` path. Terms get a dedicated tag for visual emphasis; definitions get a dedicated tag for indentation, spacing, and wrapping. This matches existing list, blockquote, and footnote rendering and keeps the feature lightweight.

Alternative considered: render the whole definition list as an anchored GTK widget. That would make nesting and inline tag reuse more complex, and it would isolate definition-list text from the existing buffer-based link, emphasis, and code-span behavior.

### Track definition state separately from ordinary lists

Definition lists are structurally related to lists, but their row flow is different: terms do not have bullet or number markers, and definitions can contain multiple paragraphs or nested block content. The renderer should use dedicated definition-list state rather than forcing definition entries through `list_stack` and `ListItemRenderState`.

Alternative considered: map each definition to a fake unordered list item. That would reuse existing indentation code, but it would produce misleading markers and make nested block spacing harder to control.

### Reuse nested block renderers inside definitions

When a definition contains paragraphs, ordinary lists, blockquotes, or code blocks, the current nested renderers should run with the definition tag context still active. Embedded code-block widgets should continue to receive explicit width requests derived from the preview text column, not from their natural child-anchor allocation.

Alternative considered: flatten definition content to plain text. That would avoid renderer state work but would fail common definition-list documents and lose the exact value of using pulldown-cmark's block-aware events.

### Keep unsupported markers readable as ordinary text

Syntax that pulldown-cmark does not expose as definition-list events, including markdown-it compact `~` markers, should remain readable source text. This is a deliberate parser-boundary decision rather than a partial implementation gap.

Alternative considered: silently reinterpret `~` as a definition marker. That would be surprising for a pulldown-cmark-backed preview and could conflict with existing inline parsing behavior.

## Risks / Trade-offs

- Definition-list state could disturb paragraph spacing in ordinary lists or footnotes -> Add focused widget tests for adjacent normal lists, definition lists, and nested content.
- Code blocks inside definitions could regress to narrow child-anchor allocation -> Add a nested definition-list code-block test that checks false horizontal overflow is absent for short code.
- Dedicated tags could make dark/light styling drift from other preview elements -> Create tags through the existing tag-update path and verify they exist in widget tests.
- pulldown-cmark event shape could be misremembered -> Add event-stream tests for colon syntax, inline markup, nested code blocks, and unsupported `~` syntax before renderer assertions.

## Migration Plan

This is a preview-only rendering change. No persisted user data, settings, sidecar files, or document contents require migration. Rollback is removing the parser option and definition-list handlers, after which documents return to the current raw-text rendering behavior.

## Open Questions

None.
