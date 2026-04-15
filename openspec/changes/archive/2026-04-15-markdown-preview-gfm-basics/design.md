## Context

LushText's Markdown preview is intentionally native and lightweight. The preview widget renders most Markdown blocks directly into a `GtkTextBuffer` with `GtkTextTag`s and only steps outside that flow when GTK already provides a clean native primitive, as it now does for tables via `GtkTextChildAnchor` plus `GtkGrid`.

That architecture is a good fit for the next GFM slice as well. Task lists, alert callouts, and footnotes all come through `pulldown-cmark` as structured parser events, but today the preview only enables tables and strikethrough. As a result:

- Task list markers are not rendered with task-state semantics.
- GitHub alert callouts degrade to plain blockquotes with no alert identity.
- Footnote references and definitions are dropped when their dedicated events arrive.

The important constraint is to keep the change GTK-native and simple. This slice should not introduce an HTML renderer, a second preview engine, or repo-aware GitHub behaviors such as issue autolinks.

## Goals / Non-Goals

**Goals:**
- Render checked and unchecked task list items distinctly in the native preview.
- Render GitHub alert callouts (`[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`) as visually distinct preview blocks instead of generic quotes.
- Render footnote references and definitions so the preview preserves the document's meaning without showing raw footnote syntax.
- Keep the implementation inside the current preview architecture and cover it with deterministic tests.

**Non-Goals:**
- Bare-URL autolinking, raw HTML rendering, math support, or GitHub repo-aware autolinks.
- Interactive preview behaviors such as clickable footnote jumps or togglable task checkboxes.
- Full browser parity for every GFM nuance outside this subset.

## Decisions

### 1. Enable the parser extensions that directly map to this slice

The preview will enable `pulldown-cmark` support for task lists, footnotes, and GitHub-flavored alerts in addition to the existing table and strikethrough support. This keeps feature detection parser-driven instead of re-parsing source markers by hand.

Alternatives considered:
- Hand-parsing list markers or callout markers in preview text would duplicate parser work and drift from `pulldown-cmark` behavior.
- Deferring alerts until a later pass would keep the parser simpler, but it would miss one of the highest-value README semantics in this slice.

### 2. Keep these features on the `GtkTextBuffer` path instead of adding more anchored widgets

Unlike tables, task lists, alert callouts, and footnotes do not require grid-style layout. They fit the existing text-and-tag model well enough:

- task list markers can render as explicit checked and unchecked marker glyphs,
- alert callouts can use dedicated tags plus an inserted alert title line,
- footnotes can use dedicated inline and block tags with lightweight numbering state.

That keeps the preview cohesive and avoids adding widget lifecycle complexity where GTK text styling is already sufficient.

Alternatives considered:
- Anchored `CheckButton` widgets for task lists would be more visually literal, but they add interactivity expectations and more layout state than this read-only preview needs.
- Anchored alert cards would look richer, but they would overcomplicate a slice whose main value is semantic preservation.

### 3. Delay list-item marker insertion so task list items can override the default bullet or number

The current renderer inserts unordered or ordered list markers immediately when it sees `Tag::Item`. Task lists need one extra beat because the checked or unchecked state arrives as `Event::TaskListMarker(bool)` after the item starts. The renderer will therefore keep a small pending-marker state for the current item and flush it when the first real item content arrives, letting task list items swap in a checkbox-style marker instead of stacking a checkbox after a bullet.

Alternatives considered:
- Leaving the default bullet and appending a checkbox would be simpler, but visually clumsy and not meaningfully better than raw source.
- Rewriting list rendering around a separate list AST would be larger than this change needs.

### 4. Render alert callouts as typed blockquotes with inserted titles and per-kind tags

GitHub alert callouts already arrive from the parser as blockquotes with a specific kind. The preview will treat those as a specialized blockquote rendering path:

- insert a title such as `Note` or `Warning` at the start of the block,
- apply a per-kind body tag for spacing and background treatment,
- keep inline content inside the existing text/tag renderer so emphasis, links, and code continue to work naturally.

This keeps callouts visually distinct without introducing a second block-rendering system.

Alternatives considered:
- Reusing the generic blockquote tag would preserve content but lose the main semantic benefit.
- Building custom widget cards for each alert type would add more structure than this slice needs.

### 5. Number footnotes lazily in preview order and render references and definitions inline

The preview will keep a `label -> ordinal` map and assign numbers the first time a footnote label is seen, whether that happens at a reference or a definition. References render inline with a dedicated tag; definitions render as indented blocks using the same numbering. Definitions stay in source order rather than being relocated to a synthetic section because the preview should preserve document flow unless the source itself moves them.

Alternatives considered:
- A pre-pass over the full document would produce the same numbering but adds complexity without meaningfully improving the result here.
- Moving every definition to a synthesized footer section would feel more browser-like, but it would stop reflecting the source document's actual ordering.

## Risks / Trade-offs

- [Task-list marker flushing could disturb ordinary list rendering] -> Keep the pending-marker state local to list items and protect it with list-specific regression tests.
- [Alert styling may look too close to generic blockquotes or too heavy for the preview] -> Start with light tag-based differentiation and adjust only if tests or screenshots show poor readability.
- [Footnote numbering depends on parser event order] -> Centralize numbering in one helper so references and definitions share the same mapping rules.
- [The preview will still not match every GitHub renderer nuance] -> Keep the slice explicit and narrow so users get clearer semantics without implying full browser parity.

## Migration Plan

No data migration is required. Once shipped, supported GFM content will render automatically in the Markdown preview. Rollback is low risk because the change is localized to preview parsing, text-tag creation, and preview tests.

## Open Questions

None blocking. If later feedback shows alert callouts need richer visual treatment, that can be explored as a follow-up without changing the parser contract from this slice.
