## 1. Parser and preview-table foundations

- [x] 1.1 Enable Markdown table parsing in `ui/markdown_preview` and add the small helper state needed to buffer table alignments, rows, and cells until a full table is available.
- [x] 1.2 Add preview state for anchored table widgets so rerendering, clearing, and placeholder transitions remove stale table grids before rebuilding content.
- [x] 1.3 Keep table rendering isolated behind focused helper functions or types so normal paragraph, list, and code rendering paths stay easy to reason about.

## 2. Native table rendering

- [x] 2.1 Build a `GtkGrid` for each buffered Markdown table and insert it into the `GtkTextView` flow with `GtkTextChildAnchor` at the correct buffer position.
- [x] 2.2 Render header and body cells with `GtkLabel`, mapping Markdown column alignment to native GTK alignment properties, preserving blank cells, and applying the agreed inline-markup subset for bold, italic, strikethrough, inline code, and line breaks.
- [x] 2.3 Add the minimal styling and spacing needed for readable native tables, explicitly defer clickable link activation and full text-tag parity in cells, and insert the finished grid without breaking surrounding paragraph, list, and code-block ordering in the preview.

## 3. Verification and regression coverage

- [x] 3.1 Add deterministic tests for the buffered table model and for preview documents that mix text-rendered blocks with one or more anchored table widgets.
- [x] 3.2 Add widget-aware tests that verify row and column structure, header distinction, alignment properties, rerender cleanup, and the supported inline-markup subset inside anchored table cells.
- [x] 3.3 Run the relevant Rust test target(s) for the Markdown preview renderer and update any nearby documentation or comments needed to explain the new table-rendering path.
