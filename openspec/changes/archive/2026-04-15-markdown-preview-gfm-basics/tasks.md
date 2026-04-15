## 1. Parser and preview-state updates

- [x] 1.1 Enable the `pulldown-cmark` task-list, footnote, and GFM alert options in `ui/markdown_preview` and add the small preview-local state needed to coordinate list markers and footnote numbering.
- [x] 1.2 Extend preview tag creation and helper functions so task list markers, alert callouts, and footnote references and definitions have clear native styling without introducing a second rendering engine.

## 2. Native preview rendering

- [x] 2.1 Update list-item rendering so checked and unchecked task list items replace the default bullet or number marker with a distinct task-state marker.
- [x] 2.2 Render GitHub alert callouts as typed callout blocks with inserted titles and preserved inline body content.
- [x] 2.3 Render footnote references and definitions with shared numbering, readable inline markers, and definition blocks that keep the existing document flow intact.

## 3. Verification and regression coverage

- [x] 3.1 Add deterministic Markdown preview tests that cover task lists, alert callouts, and footnotes without leaking raw source markers into the rendered text.
- [x] 3.2 Run the relevant Markdown preview test targets and update any nearby comments needed to explain the new preview paths.
