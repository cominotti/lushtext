## Why

LushText's Markdown preview currently renders headings, lists, blockquotes, and code, but it drops table structure entirely. That makes common README and notes content misleading in preview mode because rows, columns, and header relationships disappear instead of staying readable.

## What Changes

- Extend the Markdown preview parser configuration so table syntax is recognized instead of skipped.
- Render Markdown tables in the native preview widget with readable row and column structure, including header rows and separator lines.
- Preserve table cell content order and line-level readability when tables include uneven column widths or blank cells.
- Add focused preview tests that cover table parsing, layout, and regression cases alongside the existing Markdown rendering coverage.

## Capabilities

### New Capabilities
- `markdown-preview-tables`: Render Markdown table syntax in the read-only preview pane with readable headers, rows, and cell alignment.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/markdown_preview`, preview refresh wiring in `ui/window/preview.rs`, and Markdown preview tests.
- Affected systems: `pulldown-cmark` parser options, preview text/tag rendering, and any helper logic needed to measure and align table columns.
- Dependencies and APIs: no new external dependency is expected; the existing `pulldown-cmark` integration will likely enable table support and add internal rendering helpers.
