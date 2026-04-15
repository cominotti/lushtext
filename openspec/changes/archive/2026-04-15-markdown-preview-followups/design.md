## Context

LushText's Markdown preview intentionally stays native and lightweight. Most content is rendered directly into a `GtkTextBuffer` with `GtkTextTag`s, while tables already use `GtkTextChildAnchor` plus a buffered `GtkGrid` because column layout does not fit the plain text path well. That architecture is still a good fit for the next follow-up slice, but the current renderer has four gaps:

- preview links are styled with `TAG_LINK` but they are presentation-only,
- table cells only support a small inline markup subset and flatten links to plain text,
- image events are skipped entirely,
- nested lists share one fixed list-item margin, so deeper or mixed list structures lose hierarchy.

The follow-up note keeps a strict product constraint: stay GTK-native, stay simple to maintain, do not introduce HTML or WebKit, and do not add GitHub-context-dependent behavior. The implementation also needs to work with the existing preview refresh flow in `window/preview.rs`, which already has access to the active editor's file path and the sidebar's workspace roots when it decides to re-render preview content.

## Goals / Non-Goals

**Goals:**
- Make supported Markdown links activatable from the read-only native preview.
- Preserve supported links inside rendered table cells without sacrificing the readable table layout that just shipped.
- Render local Markdown images, including workspace-relative paths, as native preview blocks with explicit fallback states.
- Improve nested ordered and unordered list readability without replacing the current parser-driven text rendering model.
- Keep the change deterministic and testable with preview-focused unit and widget coverage.

**Non-Goals:**
- Introducing an HTML renderer, WebKit path, or in-preview browser navigation.
- Fetching remote images or supporting raw HTML image markup.
- Adding captions, responsive browser-like image layout, or full GitHub Markdown parity.
- Turning preview links into editor navigation or mutating the source document from preview interactions.

## Decisions

### 1. Pass a small render context into the preview instead of teaching the widget about window state

The preview needs more than raw Markdown text for this follow-up: relative links and images need a source-path base, and workspace-relative images need the current workspace roots. The cleanest way to do that is to extend preview rendering with a small context object supplied by `window/preview.rs`, containing the active document path and the current workspace roots in their existing sidebar order.

That keeps `LushtextMarkdownPreview` focused on rendering and hit-testing while reusing data the window already owns. It also makes widget and unit tests straightforward because they can pass an empty or synthetic context directly.

Alternatives considered:
- Letting the preview reach back into the window or sidebar would couple a reusable widget to higher-level shell state.
- Restricting resolution to file-relative paths only would be simpler, but it would ignore the note's explicit workspace-relative scope.

### 2. Centralize target resolution and launching for links and local assets

Links and images both need the same base rules: classify the target, resolve local paths against the preview context, reject remote image fetches, and route supported activations through one launcher path. A small preview-local helper should own that logic so text-buffer links, table-cell links, and image blocks all interpret destinations the same way.

This helper should distinguish at least three outcomes:
- supported external target, ready to launch,
- supported local file target, resolved from file-relative or workspace-relative context,
- unsupported or unresolved target, which becomes explicit fallback UI instead of a silent no-op.

Alternatives considered:
- Handling each destination shape separately in every render branch would duplicate path rules and make failures inconsistent.
- Embedding a browser or richer navigation surface would violate the native/simple constraint.

### 3. Keep prose links on the `GtkTextView` path and extend table cells with native label link activation

Normal preview prose, footnotes, and callouts should stay on the existing text-buffer rendering path. During render, the preview should record link spans alongside their resolved targets, then use click and motion controllers on the `GtkTextView` to hit-test those spans for activation and pointer feedback. Because the preview rerenders whole documents, span metadata can be rebuilt from scratch on every render instead of incrementally maintained.

Rendered tables can stay lighter. Their cell builder already produces Pango markup for `GtkLabel`, so supported links can become `<a href=\"...\">...</a>` markup and use the label's native link-activation signal to call the same launcher helper. That preserves the current no-wrap table layout and avoids introducing per-cell mini layouts unless a later change truly needs them.

Alternatives considered:
- Converting every inline link in prose into an anchored widget would fragment the text flow and add more cleanup state than this slice needs.
- Leaving tables as plain-text labels would fail the table-cell link requirement.

### 4. Generalize anchored widget cleanup from tables to a broader embedded-preview block path

Tables already proved that `GtkTextChildAnchor` is a maintainable escape hatch when a Markdown block needs native GTK layout. Local images fit that same pattern well. The preview should generalize its tracked anchored widgets from "rendered tables" to a broader rendered-embeds collection so rerender, clear, and placeholder transitions remove both old table grids and old image widgets in one place.

For images, the anchored widget should be a bounded native image block, such as a `GtkPicture` inside a simple container that can also host fallback text when loading fails. The preview stays read-only because the widget is informational only; activation remains on surrounding links, not on the image itself.

Alternatives considered:
- Replacing images with plain fallback text only would miss the main value of README-style image previews.
- Using custom HTML/CSS layout for images would break the renderer strategy this note is explicitly trying to preserve.

### 5. Make list indentation depth-aware by deriving formatting from parser nesting state

The parser already keeps `list_stack` and delayed item-prefix state so task lists can override the default bullet or number. The same state can drive readable nesting: the preview should derive indentation from list depth and marker kind instead of applying one fixed `TAG_LIST_ITEM` margin to every item.

A simple approach is to create or reuse depth-specific list tags on demand and apply them as each list item starts. That keeps rendering parser-driven, maintains the current task-list marker flow, and avoids brittle whitespace-only indentation that depends too heavily on font metrics.

Alternatives considered:
- Rebuilding lists as embedded widgets would be heavier than the problem requires.
- Inserting literal tabs or spaces would be easy short-term but visually unreliable across fonts and themes.

## Risks / Trade-offs

- [Text-view link hit-testing could drift from the rendered spans] -> Rebuild link-span metadata on every render and cover activation with widget tests that exercise real preview coordinates or controller callbacks.
- [Workspace-relative paths can be ambiguous across multiple roots] -> Resolve relative to the current file first, then scan workspace roots in stable sidebar order, and show an explicit unresolved fallback when no single usable match exists.
- [Embedded images can overwhelm the preview width or fail to load] -> Bound image presentation to the preview column, keep loading local-only, and show an in-flow fallback block when GTK cannot load the file.
- [Table-cell links could accidentally reintroduce wrapping or layout regressions] -> Keep the current non-wrapping table layout and add regression coverage for link-bearing cells with uneven widths.
- [The follow-up still will not reach browser parity] -> Keep the supported destination and layout rules explicit so users gain trustworthy native behavior without implying full HTML rendering.

## Migration Plan

No data migration is required. The change is isolated to Markdown preview rendering, preview refresh call sites, and tests. Shipping the change simply makes more Markdown constructs render natively. Rollback is low risk because it only removes preview behavior; source files and persisted user data are unchanged.

## Open Questions

None blocking. If later feedback shows that relative Markdown links should open inside LushText instead of externally, that can be proposed separately without changing the local-resolution and rendering work from this slice.
