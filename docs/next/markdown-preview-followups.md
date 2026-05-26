# Markdown Preview Follow-Ups

## Status: Proposed

## Description

LushText's native Markdown preview now covers the highest-value GitHub-flavored
basics that fit the existing renderer cleanly: task lists, alert callouts,
footnotes, definition lists, tables, headings, lists, links, blockquotes,
rules, and fenced and inline code.

This note captures the next follow-up items that still fit the same guiding
constraint:

- **GTK-native**
- **simple to maintain**
- **no HTML or WebKit rendering path**
- **no GitHub-context-dependent behavior**

## Follow-Up Candidates

### 1. Clickable links in preview

The preview already styles links, but they remain presentation-only. The next
simple step is to make supported links open externally when activated from the
read-only preview.

Why this still fits:
- The rendered content can stay inside the current `GtkTextView` path
- The behavior is useful across normal Markdown prose, footnotes, and other
  already-supported inline contexts
- It improves fidelity without pulling in browser semantics

### 2. Links inside table cells

Table cells currently preserve a small inline subset, but links inside those
cells do not get dedicated link styling or activation behavior. A follow-up can
extend the existing table-cell markup subset so common README tables read more
faithfully.

Why this still fits:
- The table implementation already has a focused cell-markup builder
- This is a local extension of the existing native table path
- It avoids inventing a second table renderer

### 3. Local image rendering for Markdown files

The first image-oriented step should be limited to simple Markdown image syntax
for local files and workspace-relative paths. The preview can show images as
read-only content blocks without aiming for browser-level layout fidelity.

Why this still fits:
- GTK already has native image widgets
- The value is high for README-style documents
- The scope can stay narrow by explicitly excluding raw HTML image handling,
  responsive layout, captions, and remote-fetch complexity

### 4. Advanced list fidelity

The current list rendering covers tight and loose list row flow, offset ordered
markers, task-list markers, and nested hanging indents. Future refinements
should focus on rarer mixed CommonMark structures and visual polish rather than
rebuilding the renderer.

Why this still fits:
- It stays on the existing text-and-tag path
- It improves readability without introducing a new rendering system
- It is mostly a refinement of current renderer state management

## Explicit Non-Goals For This Follow-Up Track

These items do **not** currently fit the "simple + GTK-native" bar and should
remain out of scope unless the renderer strategy changes:

- Raw HTML Markdown blocks such as `<details>` and custom embedded HTML
- GitHub repo-aware autolinks such as `#123` or `owner/repo#123`
- Bare-URL GitHub-style linkification beyond what the parser already provides
- Browser-parity math rendering
- Full browser-level Markdown/CSS fidelity

## Suggested Order

1. Clickable links in preview
2. Links inside table cells
3. Local image rendering
4. Nested list fidelity refinements

This order keeps the next work close to the current renderer shape and avoids
jumping into heavier block-layout work too early.
