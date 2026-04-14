## Context

LushText already ships a native Markdown preview widget that streams `pulldown-cmark` events into a `GtkTextBuffer` and applies `GtkTextTag`s for headings, links, lists, quotes, and code. Today that renderer builds its parser with `Options::empty()` and intentionally falls through on table-related events, so Markdown tables vanish from preview instead of being rendered as structured content.

This change stays inside the existing preview architecture: a `GtkTextView` inside the preview pane, theme-aware `GtkTextTag`s, and no embedded web renderer. The main implementation work is therefore in `ui/markdown_preview`, with only light touchpoints in preview refresh wiring and tests.

`pulldown-cmark 0.12` already exposes the pieces we need for native table support: `Options::ENABLE_TABLES`, `Tag::Table(Vec<Alignment>)`, `Tag::TableHead`, `Tag::TableRow`, and `Tag::TableCell`. The design problem is not parsing alone; it is turning those events into a readable table presentation while keeping the current text-flow preview shell.

## Goals / Non-Goals

**Goals:**
- Parse Markdown table syntax in the existing preview renderer instead of dropping it.
- Render tables as readable native preview content with visible headers, rows, and column boundaries.
- Preserve surrounding Markdown flow so tables appear in the right place between paragraphs, lists, and other blocks.
- Keep the implementation deterministic and testable without introducing a separate HTML or WebKit rendering path.
- Preserve inline cell content and alignment cues well enough that common README tables stay understandable in preview mode.

**Non-Goals:**
- Full HTML/CSS table fidelity or pixel-perfect browser rendering.
- Interactive tables, sortable columns, selection affordances, or copy-as-table workflows.
- Support for raw HTML `<table>` blocks or every possible nested block element inside cells in the first iteration.
- Changes to Markdown editing, export, or non-preview rendering paths.

## Decisions

### 1. Enable the table parser extension and buffer each table before rendering it

The preview renderer will enable `Options::ENABLE_TABLES` and treat each table as a buffered block rather than a purely streaming structure. As soon as a `Tag::Table(...)` starts, the renderer will collect alignments, header/body row boundaries, and cell contents into a small in-memory table model. The renderer will emit the final text only when the matching `TagEnd::Table` arrives.

This keeps the surrounding Markdown renderer unchanged while giving table rendering the information it needs up front. Column widths cannot be computed correctly if cells are written to the buffer one event at a time.

Alternatives considered:
- Streaming each cell directly into the `TextBuffer`: simpler at first glance, but it cannot produce stable column widths because later rows may be wider.
- A second parse pass just for tables: possible, but it duplicates work and complicates synchronization with the existing block separator logic.

### 2. Render each completed table as an anchored `GtkGrid` inside the existing `GtkTextView`

When a buffered table is complete, the preview renderer will create a `GtkTextChildAnchor` at the current buffer position and attach a `GtkGrid` there with `GtkTextView::add_child_at_anchor()`. Paragraphs, headings, lists, and code blocks will continue to use the current text-and-tag path; only tables become embedded widgets.

This is the most GTK-native path available without replacing the preview architecture. GTK already models multiline text flow and embedded child widgets this way, and `GtkGrid` gives us row-and-column layout without manually simulating a table in padded text.

Alternatives considered:
- Rendering padded pipe tables back into the `GtkTextBuffer`: less code churn, but it pushes layout and alignment work into custom string formatting rather than GTK layout.
- Embedding a WebView or HTML renderer: much heavier than the current native preview path and inconsistent with the rest of the preview implementation.
- Replacing the whole preview with a widget tree for every Markdown block: much larger architectural churn than this feature needs.

### 3. Use `GtkLabel` cells and GTK alignment properties instead of custom text-width math

Each table cell will render as a `GtkLabel` attached into the `GtkGrid`. Header cells can use a dedicated CSS class or stronger label styling, while body cells stay lightweight. Column alignment from Markdown syntax maps naturally to label alignment and expansion behavior, which lets GTK handle positioning rather than relying on manually padded text.

This keeps the implementation simple and idiomatic in Rust and GTK: a small buffered table model turns into a small widget subtree. It also keeps table styling in GTK/CSS territory rather than inventing a parallel mini layout engine.

Alternatives considered:
- Computing monospace widths and padding every cell manually: workable, but less GTK-native and more fragile around content variation.
- Using tab stops on `GtkTextTag`: still fundamentally text-emulation, and more awkward than a real grid once header styling and column alignment are involved.

### 4. Render a narrow inline-markup subset inside `GtkLabel` cells

The first table iteration will prioritize correct table structure, alignment, and readable text, but it will not reduce every cell to plain text. Cell content will be buffered into escaped label text plus a small supported markup subset that maps cleanly onto `GtkLabel` and Pango markup: bold, italic, strikethrough, inline code, and line breaks. This gives common README-style tables a meaningful amount of inline fidelity without recreating the full text-buffer renderer inside each cell.

Links inside cells will remain presentation-only in the first iteration: the implementation may style them as links if that stays simple, but it will not introduce clickable link activation just for table cells while the rest of the preview remains non-interactive. Likewise, code-span background parity and full nested inline feature parity are intentionally deferred.

This matches the scope of the spec and the user request: tables should render correctly, readably, and natively, while the first version stays simple enough to ship confidently.

Alternatives considered:
- Plain text only: smallest implementation, but too lossy for common Markdown tables that use emphasis or code spans.
- Recreating the full `GtkTextTag` renderer inside every cell: possible, but it substantially increases complexity and works against the goal of a simple native solution.
- Flattening the table into one plain text block: simpler, but not the most GTK-native answer and less faithful to table structure.

### 5. Treat table widgets as explicit preview state with clear rerender cleanup

Because embedded table widgets are not just buffer text, the preview widget will need explicit lifecycle management for them. Rerendering or clearing the preview should remove any previously anchored table widgets before inserting new ones, so the widget tree stays in sync with the current Markdown buffer and no stale grids accumulate.

This is the key operational cost of the widget-anchor approach, but it is localized and straightforward once made explicit in the preview state.

Alternatives considered:
- Relying on buffer replacement alone to clean up table widgets: too implicit, and likely to leave cleanup semantics unclear.
- Avoiding widget state by forcing everything back into text: simpler operationally, but it gives up the GTK-native rendering path the user asked for.

### 6. Add widget-aware tests around table structure, alignment, and rerender behavior

The current preview tests mostly assert against `buffer_text()`, which works well for text-rendered Markdown blocks but is not sufficient once tables become anchored widgets. The implementation should therefore keep a small testable table-buffering helper and add widget-level assertions for table presence, row and column counts, header styling, alignment properties, and rerender cleanup.

This keeps the GTK-native approach verifiable without overrelying on brittle end-to-end visual tests.

Alternatives considered:
- Continuing to test tables only through buffer text: insufficient because child-anchor widgets are not the same as inline text.
- Manual-only verification: too risky for layout and lifecycle behavior.

## Risks / Trade-offs

- [Anchored table widgets require explicit cleanup on rerender] -> Track inserted table widgets in preview state and remove them whenever the preview buffer is cleared or rebuilt.
- [Wide tables may pressure the preview layout] -> Let `GtkGrid` and the surrounding scrolled preview container handle natural sizing first, and only add width constraints if real content proves necessary.
- [Table cells may not immediately match every inline text style used elsewhere in preview] -> Support a narrow high-value inline subset first, then expand only if real examples justify it.
- [Buffered table rendering still adds parser state to the current preview loop] -> Isolate the state in a small helper model and cover it with focused tests.

## Migration Plan

No user data migration is required. Once shipped, Markdown previews start rendering tables automatically for supported Markdown files. Rollback is low risk because the change is confined to preview parsing and rendering; disabling the feature would return the app to the current table-skipping behavior without touching user content or persisted state.

## Open Questions

None blocking. The implementation can decide whether header distinction comes primarily from CSS classes, bold label styling, separator rows, or a small combination of those, as long as the rendered table remains clearly readable.
