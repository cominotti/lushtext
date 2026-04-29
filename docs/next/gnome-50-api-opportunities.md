# GNOME 50 API Opportunities

## Status: Proposed

## Scope

This note covers the two follow-up areas that need more investigation before
implementation:

- **4. AdwSidebar / AdwViewSwitcherSidebar**
- **6. pulldown-cmark 0.13 Markdown extensions**

The goal is not to adopt every new API. The goal is to pick the places where
new platform support makes LushText simpler, more native, or more capable
without destabilizing the editor shell.

## 4. AdwSidebar / AdwViewSwitcherSidebar

### Current Fit

`AdwSidebar`, `AdwSidebarSection`, and `AdwSidebarItem` are useful for
application navigation lists. `AdwViewSwitcherSidebar` is useful when a
surface is already modeled as an `AdwViewStack`. They are not a drop-in
replacement for LushText's current workspace file sidebar.

The current `LushtextSidebar` is more than navigation:

- It owns a persisted set of workspace roots and the `All workspaces` aggregate
  scope.
- Each workspace section uses `GtkTreeListModel` plus `GtkTreeExpander` for a
  real file tree.
- Rows carry file operations, right-click menus, inline rename, delete
  confirmation, new file/folder flows, file peek, async scans, and watcher
  reconciliation.
- Deep directory labels are deliberately clipped inside the sidebar width
  contract instead of widening the paned layout.

Replacing that file tree with `AdwSidebar` would likely remove behavior rather
than simplify it.

### Concrete Proposal

Do **not** rewrite the workspace file tree around `AdwSidebar` now.

Instead, use the new Adwaita sidebar family for a separate navigation surface
where the data is naturally sectioned, shallow, and action-oriented:

1. Prototype a **Workspace Activity** surface inside the document-properties
   area or notes browser.
2. Model sections as:
   - `Bookmarks`
   - `Range Notes`
   - `Document Note`
   - `Workspace Note`
   - `Local History`
3. Prefer `AdwViewSwitcherSidebar` if the destination pages already live in an
   `AdwViewStack`; prefer `AdwSidebar` if the surface wants explicit model
   rows independent from a stack.
4. Keep the existing file sidebar on `GtkListView` / `GtkTreeListModel`.

### Acceptance Criteria

- The prototype preserves keyboard navigation and activation without custom
  focus workarounds.
- Compact layouts keep the current one-secondary-surface rule intact.
- Widget tests cover selecting every activity item, compact-width allocation,
  and the active item after page changes.
- The prototype does not regress existing bookmark, annotation, document-note,
  workspace-note, or local-history flows.

### Risks

- `AdwSidebar` is navigation-first, not tree-first. It should not own file
  hierarchy, file-system mutation, or async directory loading.
- Rehosting the current workspace file tree would put custom row factories and
  context menus back into a surface designed for simpler navigation rows.
- A narrow prototype is safer than a broad shell rewrite because the current
  sidebar has a lot of stateful behavior.

## 6. pulldown-cmark 0.13 Markdown Extensions

### Current Fit

The Markdown preview already enables tables, task lists, footnotes,
strikethrough, and GFM blockquote kinds. The current renderer is deliberately
GTK-native: most content goes through `GtkTextBuffer` tags, while tables and
local images use child anchors.

`pulldown-cmark` 0.13 exposes more parser options that can improve preview
fidelity:

- `ENABLE_SMART_PUNCTUATION`
- `ENABLE_DEFINITION_LIST`
- `ENABLE_SUPERSCRIPT`
- `ENABLE_SUBSCRIPT`
- `ENABLE_HEADING_ATTRIBUTES`
- `ENABLE_YAML_STYLE_METADATA_BLOCKS`
- `ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS`
- `ENABLE_MATH`
- `ENABLE_WIKILINKS`

### Concrete Proposal

Implement the parser extensions in phases, starting with the options that fit
the current renderer's text-and-tag model.

#### Phase 1: Text-Tag Extensions

Add:

- `ENABLE_SMART_PUNCTUATION`
- `ENABLE_DEFINITION_LIST`
- `ENABLE_SUPERSCRIPT`
- `ENABLE_SUBSCRIPT`

Rendering strategy:

- Smart punctuation needs no renderer state beyond enabling the option.
- Definition lists map to a term line plus an indented definition line.
- Superscript and subscript use dedicated text tags with smaller scale and
  baseline offset.

Acceptance:

- Add event-stream tests before renderer changes so the exact 0.13 event shape
  is locked down.
- Add unit tests for definition-list text order and nested inline markup.
- Add widget tests for readable superscript/subscript in normal paragraphs and
  table cells if table-cell parsing receives the same inline subset.

#### Phase 2: Footnote Polish

Footnotes are already enabled, but the preview can make them clearer:

- Render references with a small raised marker.
- Render collected definitions in an end-of-preview "Footnotes" section.
- Keep link activation local to the preview buffer; no remote or browser
  behavior is required.

Acceptance:

- Definitions are stable even when the parser emits them after the referring
  paragraph.
- Duplicate or missing references degrade to readable text.

#### Phase 3: Workspace Wikilinks

Treat `ENABLE_WIKILINKS` as a product feature, not just a parser toggle.

Proposed behavior:

- Resolve `[[Name]]` against current workspace roots.
- Prefer exact Markdown filename matches such as `Name.md`.
- Never fetch remote content.
- Reuse the existing Markdown local-path resolver where possible.

Acceptance:

- Ambiguous matches show a non-destructive fallback instead of opening the
  wrong file.
- Untitled documents without a workspace context render wikilinks as plain
  readable text.

#### Phase 4: Defer Math and Metadata Rendering

Do not implement math rendering yet. `ENABLE_MATH` identifies inline and
display math, but good TeX rendering needs a renderer decision outside
`pulldown-cmark`.

Metadata blocks and heading attributes can be parsed later if LushText adds
document outline, export, or heading anchor behavior. For now, they should not
drive visible UI.

### Verification Plan

- Start every parser-option change with a tiny event-stream fixture. Previous
  table work proved that assuming the event shape is easy to get wrong.
- Keep all rendering native to GTK widgets and text tags.
- Preserve the existing local-image sandbox and do not add remote fetch paths.
- Run Markdown widget tests under the headless GTK runner after every renderer
  phase.

## Official References

- GTK 4.22 docs: https://docs.gtk.org/gtk4/
- Libadwaita 1.9 docs: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/
- GtkSourceView 5 docs: https://gnome.pages.gitlab.gnome.org/gtksourceview/gtksourceview5/
- pulldown-cmark options: https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.Options.html
