## Context

LushText has two Markdown-facing surfaces that intentionally do different jobs. The editor tab is a `GtkSourceView` source editor, so it shows raw Markdown syntax with syntax highlighting. The rendered preview is `LushtextMarkdownPreview`, a GTK-native read-only surface that streams `pulldown-cmark` events into a `GtkTextBuffer` with `GtkTextTag`s and anchored widgets where needed.

The heading renderer already handles `Tag::Heading` and creates `heading1` through `heading6` tags, but the OpenSpec surface does not explicitly require heading behavior. Existing widget tests only prove that heading text appears and that a heading tag exists somewhere in the tag table. They do not prove that each rendered heading receives its matching heading tag or that Setext headings are covered. The preview action is also discoverability-poor: `Alt+P` exists, but the primary menu does not expose a Markdown Preview action.

## Goals / Non-Goals

**Goals:**
- Make rendered Markdown heading support an explicit, testable OpenSpec capability.
- Cover ATX H1-H6 and Setext H1-H2 heading syntax.
- Verify that heading-level tags are applied to the actual rendered heading text.
- Preserve the source editor as raw Markdown editing while making heading lines visually obvious through syntax styling.
- Add a visible primary-menu action for Markdown Preview so users can discover the renderer without knowing `Alt+P`.

**Non-Goals:**
- Automatically opening preview for every Markdown file on launch or file open.
- Hiding Markdown syntax markers or turning the editable source view into a rendered Markdown surface.
- Adding a web renderer, HTML/CSS Markdown engine, or second preview backend.
- Implementing heading anchors, generated table of contents, document outline navigation, or GitHub heading-link behavior.

## Decisions

### 1. Treat heading support as a base preview capability

Create a new `markdown-preview-headings` capability instead of attaching this contract to `markdown-preview-gfm-basics`. Headings are CommonMark basics, not a GitHub-flavored extension, and they are important enough to stand on their own.

Alternative considered: modify `markdown-preview-gfm-basics`. That would mix CommonMark heading hierarchy with GFM-only task lists, alerts, and footnotes, making the spec harder to reason about.

### 2. Keep the renderer on the existing `pulldown-cmark` and `GtkTextTag` path

The implementation should continue to rely on `pulldown-cmark` heading events and the existing `heading1` through `heading6` tag naming scheme. The work is primarily hardening: confirm all heading syntaxes map to the correct tag, preserve block spacing, and prove raw marker syntax is not emitted in rendered preview.

Alternative considered: post-process source text for heading markers. That would duplicate parser behavior and would be weaker than the structured event stream the preview already receives.

### 3. Test the tag on the rendered text, not only the tag table

Widget tests should inspect the tags at the heading text offsets for all ATX levels and Setext levels. A tag existing in the table is not enough; the regression we care about is the rendered document losing heading hierarchy even though the tag definitions remain present.

Alternative considered: screenshot-only verification. That is useful for manual confidence, but tag-level widget tests are deterministic and catch regressions closer to the renderer.

### 4. Add a primary-menu Markdown Preview action bound to preview-only mode

Add a visible primary-menu item labeled `Markdown Preview` that invokes the existing `win.toggle-preview-mode` action. This keeps one source of truth for preview-only behavior, preserves `Alt+P`, and avoids adding a new preview state machine.

Alternative considered: automatically enabling preview whenever a Markdown file is active. That would surprise users who opened a file to edit source and would conflict with the existing rule that preview starts hidden unless requested.

### 5. Preserve source editing while making headings visible

The source editor should continue to show raw heading markers, but heading lines must be visually stronger than body text. The bundled GtkSourceView schemes should scale the shared `def:heading` style so Markdown headings stand out while cursor movement, selection, and source syntax remain normal editor behavior.

Alternative considered: hide the marker text and turn heading lines into rendered blocks inside `GtkSourceView`. That would make editing behavior harder to predict, especially around cursor movement, line height, selection, and source syntax visibility.

## Risks / Trade-offs

- [Users may still expect Markdown files to open directly in preview] -> Add the visible menu action and keep the default editor behavior explicit in tests.
- [A future renderer refactor could leave heading tags defined but unused] -> Assert tags on the rendered heading text offsets for each level.
- [Setext `---` can also mean a horizontal rule depending on context] -> Rely on `pulldown-cmark` event classification and test only valid Setext heading source.
- [Preview-only mode is hidden while side-by-side preview is visible] -> Bind the menu action to the existing preview-only action first; side-by-side preview remains an existing internal action and can get its own visible control later if product direction calls for it.

## Migration Plan

No user data migration is required. The change hardens existing preview behavior and adds a visible invocation path. Rollback is low risk: removing the menu item and tests returns the app to the previous hidden-shortcut behavior without touching documents or persisted user content.

## Open Questions

None blocking.
