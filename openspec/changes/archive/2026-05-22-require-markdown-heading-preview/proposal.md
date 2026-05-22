## Why

Markdown headings are mandatory preview behavior because README-style documents, notes, and project docs become misleading when heading hierarchy is visible only as raw source syntax. LushText already has native heading-rendering code, but the behavior is not protected by an explicit OpenSpec contract and the rendered preview is easy to confuse with the source editor.

## What Changes

- Add a dedicated Markdown preview heading capability that makes ATX headings (`#` through `######`) and Setext headings (`===` and `---`) mandatory rendered-preview behavior.
- Require heading preview to remove raw heading marker syntax from rendered output while preserving the heading text and visible hierarchy.
- Require every supported heading level to receive distinct visual treatment in rendered preview, with H1 through H6 remaining distinguishable from body text and from each other.
- Make Markdown heading lines visually stand out in the source editor while preserving the raw editable marker syntax.
- Require rendered Markdown preview to be reachable for Markdown documents through a visible application action in addition to the existing keyboard shortcut, so users can discover the heading renderer without guessing hidden behavior.
- Strengthen preview tests so they verify heading tags are applied to rendered heading text, not merely present in the tag table.

## Capabilities

### New Capabilities
- `markdown-preview-headings`: Mandatory rendered-preview behavior and discoverability for Markdown heading hierarchy.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, preview actions and UI wiring in `crates/lushtext-core/src/ui/window` and `resources/ui/window.ui`, bundled GtkSourceView style schemes, preview styling, and Markdown/widget tests.
- Affected systems: `pulldown-cmark` heading event handling, `GtkTextTag` application for heading levels, source-editor heading style, preview action discoverability, and Markdown preview regression coverage.
- Dependencies and APIs: no new external dependency is expected; the change formalizes and hardens the existing native `pulldown-cmark` plus GTK preview path.
