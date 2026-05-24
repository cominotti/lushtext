## Context

LushText's Markdown preview is intentionally GTK-native. Most Markdown renders into a `GtkTextBuffer` with `GtkTextTag`s, while block structures that need native layout, such as tables and local images, use child anchors. Generic blockquotes already arrive from `pulldown-cmark` as `Tag::BlockQuote(None)`, and GitHub alert callouts arrive as typed `Tag::BlockQuote(Some(kind))` events because the preview enables GFM parser support.

The current generic blockquote path applies one fixed `blockquote` tag. That preserves text but loses hierarchy: nested quote depth gets the same margin and styling as a top-level quote, and the rendered preview does not show quote rails. The existing alert callout path is separate and should stay separate because `[!NOTE]`-style callouts have their own semantic title and background treatment.

## Goals / Non-Goals

**Goals:**

- Render generic CommonMark blockquotes with visible quote rails.
- Preserve nested blockquote depth for adjacent marker forms like `>>>` and spaced marker forms like `> > >`.
- Keep raw `>` source markers out of the rendered preview.
- Preserve supported inline formatting inside blockquotes, including emphasis, inline code, strong text, and links.
- Keep GitHub alert callouts distinguishable from generic blockquotes.
- Stay inside the current GTK-native preview architecture and cover the behavior with deterministic widget tests.

**Non-Goals:**

- Introducing WebKit, HTML rendering, CSS Markdown parity, or a second preview backend.
- Adding interactive quote folding, quote navigation, anchors, or source-to-preview synchronization.
- Changing the source editor's Markdown syntax highlighting.
- Changing the existing typed GitHub alert callout contract beyond preserving its separation from generic blockquotes.

## Decisions

### 1. Add a dedicated CommonMark blockquote capability

Create `markdown-preview-blockquotes` instead of extending `markdown-preview-gfm-basics`. Generic blockquotes are part of CommonMark, while alert callouts are a GitHub-flavored specialization that already has its own requirement surface.

Alternatives considered:
- Modifying `markdown-preview-gfm-basics` would blur a CommonMark rendering contract with GFM-only alert behavior.
- Treating this as an implementation-only hardening pass would leave the current "text appears" behavior underspecified and easy to regress.

### 2. Render quote rails as preview glyphs with depth-specific tags

`GtkTextTag` supports margins, spacing, foreground, paragraph background, and font styling, but it does not provide a straightforward per-paragraph left border. The implementation should therefore render quote rails as explicit preview glyphs, using `│` plus spacing at the start of quoted block lines, and apply depth-specific quote tags so each level keeps a stable indentation contract.

The rail glyph is rendered content, not source syntax. It should replace the visual role of the Markdown `>` marker without exposing that raw marker.

Alternatives considered:
- Relying only on indentation would still feel flat and would not match the user's requested visual direction.
- Using paragraph backgrounds would make quotes look like callout cards and compete with alert styling.
- Building anchored quote widgets would create a second block layout path for something that fits the text-buffer renderer well enough.

### 3. Track generic blockquote depth separately from typed alert callouts

The renderer should distinguish `Tag::BlockQuote(None)` from `Tag::BlockQuote(Some(kind))`. Generic quotes increment a generic quote-depth counter and receive rail prefixes. Typed alert callouts continue through the existing alert body/title path and must not be flattened into generic quote rails.

Alternatives considered:
- Treating all blockquotes identically would either make alerts lose semantic styling or make generic quotes look like alerts.
- Counting typed alerts as generic quote depth would make nested callout layouts harder to reason about and could accidentally add rails to alert titles.

### 4. Preserve inline emphasis by avoiding implicit italic for all generic quote body text

The current generic blockquote tag makes all quote text italic. That makes real inline emphasis inside the quote less meaningful. The hardened rendering should keep the quote body visually muted and indented, while leaving italic styling to actual Markdown emphasis events.

Alternatives considered:
- Keeping every quote italic is simple, but it weakens inline formatting fidelity.
- Adding heavier typography or card-like treatments would move generic quotes too close to alert callouts.

### 5. Test rendered structure, not only text presence

Tests should prove more than "quoted text exists." They should verify that rendered output contains visible rail glyphs, raw `>` markers are absent, depth-specific tags are applied to nested quote content, and alert callouts still render through their typed callout path.

Alternatives considered:
- Screenshot-only verification would help visually but would be brittle and harder to run in ordinary CI.
- Checking only tag-table existence would miss the regression where tags exist but rendered text does not carry the right structure.

## Risks / Trade-offs

- [Rail glyphs become selectable rendered text] -> Accept this as a GTK-native preview trade-off and keep them clearly distinct from raw Markdown `>` markers in specs and tests.
- [Line wrapping might make long quoted paragraphs appear rail-less on continuation lines] -> Start with deterministic paragraph-start rails and depth indentation; revisit custom drawing only if visual testing shows wrapped quote paragraphs are misleading.
- [Nested quote spacing can interact with list or footnote margins] -> Derive quote depth from parser events and add focused tests for nested quotes before broader mixed-container refinements.
- [Alert callouts could accidentally inherit generic quote rails] -> Keep typed alert handling separate and add a regression test that `[!NOTE]` does not render as a generic rail-only quote.

## Migration Plan

No user data migration is required. Once implemented, existing Markdown documents render generic blockquotes with stronger hierarchy in preview mode. Rollback is low risk because the change is limited to preview rendering, tests, samples, and documentation; source files and persisted user data are unchanged.

## Open Questions

None blocking.
