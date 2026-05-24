## Why

LushText's Markdown preview currently preserves generic blockquote text, but it flattens quote depth into a single muted paragraph style. Nested blockquotes therefore lose the visual structure users expect from rendered Markdown, especially in README-style prose and notes.

## What Changes

- Add an explicit CommonMark blockquote preview capability.
- Render generic blockquotes with visible quote rails and depth-aware indentation instead of a single flat margin.
- Preserve nested blockquote hierarchy for adjacent `>` markers and spaced `> > >` source forms.
- Keep raw `>` source markers out of rendered preview text.
- Preserve supported inline Markdown formatting inside blockquotes.
- Keep GitHub alert callouts on their existing typed callout path so `[!NOTE]`-style blocks remain visually distinct from generic blockquotes.

## Capabilities

### New Capabilities

- `markdown-preview-blockquotes`: Render generic CommonMark blockquotes, including nested quote depth, as readable native preview content with visible quote rails.

### Modified Capabilities

- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, Markdown preview widget tests, `samples/markdown-test.md`, README, and agent documentation that describes preview support.
- Affected systems: `pulldown-cmark` blockquote event handling, preview text-tag creation, and deterministic widget coverage for rendered blockquote structure.
- Dependencies and APIs: no new external dependency is expected; the implementation should stay on the existing GTK-native `GtkTextBuffer`/`GtkTextTag` renderer path.
