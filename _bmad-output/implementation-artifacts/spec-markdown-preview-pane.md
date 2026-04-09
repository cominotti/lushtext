---
title: 'Markdown Preview Pane'
type: 'feature'
created: '2026-04-06'
status: 'done'
baseline_commit: '359b6a3'
context: ['.agents/rules/ui.md', '.agents/rules/widget-wiring.md']
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** LushText has no Markdown preview. Users editing `.md` files must switch to another application to see rendered output.

**Approach:** Add a `LushtextMarkdownPreview` widget using a read-only `GtkTextView` with `pulldown-cmark` event stream → `GtkTextTag` rendering. The preview lives as the end-child of a new inner `GtkPaned` inside the "tabs" stack page. Three viewing states via the same paned: editor-only (default), side-by-side (right pane visible, capped at 1/3 window width like sidebar), and preview-only (Alt+P toggle, only when side pane is hidden). Side-by-side uses the established `AdwTimedAnimation` + pixman-safe animation pattern. Preview refreshes on tab switch and on buffer changes (debounced 300ms, generation counter).

## Boundaries & Constraints

**Always:**
- Reuse sidebar animation pattern exactly: `AdwTimedAnimation`, `EaseOutCubic`, 250ms, 1px minimum (not 0), `shrink-*-child` toggle, `connect_done` visibility snap
- Clamp preview pane position symmetrically to sidebar: max 1/3 window width from the right edge
- Read-only `GtkTextView` (`editable=false`, `cursor-visible=false`, `wrap-mode=word`)
- Dark/light mode via Adwaita semantic color tokens for text tags (auto-switch, no manual handling)
- Debounce rendering at 300ms using generation counter pattern
- GSettings keys: `preview-pane-position` (i), `preview-pane-visible` (b), matching sidebar key pattern
- Auto-detect Markdown via GtkSourceView language ID (`"markdown"`)
- Disable both preview actions when no tabs are open

**Ask First:**
- Shortcut key for side-by-side toggle (Alt+P is reserved for preview-only mode)
- Adding preview toggle to the primary/hamburger menu
- Supporting GFM extensions (tables, strikethrough, task lists) beyond CommonMark

**Never:**
- WebKitGTK or any browser engine
- Editing in the preview pane
- Image rendering (deferred)
- Automatic preview activation on app launch

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Side-by-side on .md | Toggle action on Markdown tab | Preview pane animates in from right with rendered content | N/A |
| Alt+P (side pane hidden) | Alt+P on .md tab | Editor replaced by full-width preview; Alt+P again restores editor | N/A |
| Alt+P (side pane visible) | Alt+P while side-by-side active | No-op — action only works when side pane is hidden | N/A |
| Non-Markdown file | Preview visible, switch to .py tab | Preview shows centered "Not a Markdown file" placeholder | N/A |
| No tabs | Any preview toggle with 0 tabs | No-op (actions disabled) | N/A |
| Rapid typing | Fast edits in .md buffer | Debounce coalesces; single re-render per 300ms window | N/A |

</frozen-after-approval>

## Code Map

- `src/ui/markdown_preview/` — New widget: `imp.rs` (template, TextTag table), `mod.rs` (public API, `pulldown-cmark` → TextTag rendering)
- `resources/ui/markdown-preview.ui` — Template: `GtkScrolledWindow` > `GtkTextView`
- `resources/ui/window.ui` — Restructure "tabs" page: wrap `AdwTabView` in `GtkPaned[preview_paned]` with preview as end-child
- `src/ui/window/imp.rs` — Add `preview_paned` TemplateChild, animation fields, extend `clamp_sidebar_position` to also clamp preview, extend `size_allocate`
- `src/ui/window/mod.rs` — Add `toggle-preview-pane` and `toggle-preview-mode` actions, `animate_preview()`, wire buffer `changed` with 300ms debounce, wire `selected-page` notify for preview update
- `resources/dev.cominotti.lushtext.gresource.xml` — Register `markdown-preview.ui`
- `data/dev.cominotti.lushtext.gschema.xml` — Add `preview-pane-position` (i, default 300) and `preview-pane-visible` (b, default false)
- `Cargo.toml` (workspace + core) — Add `pulldown-cmark` dependency

## Tasks & Acceptance

**Execution:**
- [x] `Cargo.toml` (workspace + core) — Add `pulldown-cmark = "0.12"` workspace dep, reference in core crate
- [x] `data/dev.cominotti.lushtext.gschema.xml` — Add `preview-pane-position` and `preview-pane-visible` keys
- [x] `resources/ui/markdown-preview.ui` — Create template: `GtkScrolledWindow` > `GtkTextView` (editable=false, cursor-visible=false, wrap-mode=word, vexpand=true)
- [x] `src/ui/markdown_preview/imp.rs` — CompositeTemplate struct with `text_view` and `scrolled_window` children; create TextTag table in `constructed()` (heading1-6, bold, italic, code, code-block, link, blockquote, list-item, horizontal-rule)
- [x] `src/ui/markdown_preview/mod.rs` — `render_markdown(&str)`: walk `pulldown-cmark` events, apply TextTags to buffer; `clear()`; `show_placeholder(&str)` for non-MD state
- [x] `resources/ui/window.ui` — Replace "tabs" GtkBox with `GtkPaned[preview_paned]` (horizontal): start=GtkBox>AdwTabView, end=LushtextMarkdownPreview; set shrink-end-child=false, resize-end-child=false
- [x] `resources/dev.cominotti.lushtext.gresource.xml` — Add `ui/markdown-preview.ui` entry
- [x] `src/ui/mod.rs` — Add `pub mod markdown_preview;`
- [x] `src/ui/window/imp.rs` — Add `preview_paned` TemplateChild, `preview_visible`/`preview_mode`/`saved_preview_pos`/`preview_animation`/`preview_render_gen` Cell fields; register `LushtextMarkdownPreview` in `class_init`; extend `size_allocate` to clamp preview position (mirror sidebar logic for right side)
- [x] `src/ui/window/mod.rs` + `src/ui/window/preview.rs` — `setup_preview_actions`: add `toggle-preview-pane` (stateful bool) and `toggle-preview-mode` (stateful bool); `setup_shortcuts`: `<Alt>p` → `win.toggle-preview-mode`; `animate_preview_pane(bool)` and `animate_preview_mode(bool)` mirroring `animate_sidebar`; wire `tab_view.connect_notify_local("selected-page")` to refresh preview; wire active buffer's `connect_changed` with 300ms debounce to re-render; `update_content_stack` disables preview actions when no tabs
- [x] Widget tests — 19 tests: preview construction, content/placeholder mode switching, TextTag rendering for headings/bold/italic/code/links/lists/blockquotes/hrule, tag existence check, clear, re-render replacement, read-only and cursor-hidden assertions
- [x] Documentation — Updated AGENTS.md (module layout, widget hierarchy, design decision), README.md (features), ui.md (widget hierarchy)

**Acceptance Criteria:**
- Given a .md file open, when user triggers side-by-side toggle, then preview pane animates in from right showing rendered Markdown
- Given side-by-side visible, when user edits buffer, then preview updates after 300ms debounce
- Given side-by-side hidden and .md tab active, when Alt+P pressed, then editor replaced by full-width rendered preview; Alt+P again restores editor
- Given preview visible, when switching to non-Markdown tab, then preview shows placeholder message
- Given no tabs, then both preview actions are disabled
- Given preview pane visible during window resize, then pane stays clamped to max 1/3 window width

## Design Notes

**TextTag color scheme** (Adwaita semantic tokens, auto-switch dark/light):
- Headings: `@accent_color` foreground, scaled sizes (h1=1.6em → h6=1.05em), bold weight
- Code spans: `@card_bg_color` background, monospace font family
- Code blocks: same as spans but full-width, with left/right paragraph margins
- Links: `@accent_color` foreground, `PANGO_UNDERLINE_SINGLE`
- Blockquotes: `@dim_label_color` foreground, increased left margin
- Horizontal rules: center-aligned "---" in `@dim_label_color`

**Preview-only mode (Alt+P)** reuses the same `preview_paned`:
- Enter: animate paned position to 1px (editor shrinks to nothing), then `tab_view_box.set_visible(false)`
- Exit: `tab_view_box.set_visible(true)`, animate from 1px back to `allocated_width`
- `shrink-start-child` temporarily `true` during animation (mirror of sidebar's `shrink-start-child` toggle)

## Verification

**Commands:**
- `make check` — expected: clippy + fmt pass with no warnings
- `make test` — expected: all existing + new tests pass
- `make build-debug` — expected: clean compilation

**Manual checks:**
- Toggle side-by-side on .md file — preview renders headings, bold, italic, code, lists, links
- Resize window with preview visible — pane stays within 1/3 width
- Alt+P with side pane hidden — full-width preview replaces editor, Alt+P restores
- Switch light/dark theme — preview tag colors adapt automatically
- Tab switch between .md and .rs — preview content updates correctly
- `make run` stderr — no GTK/pixman warnings during toggle animations

## Suggested Review Order

**Widget: pulldown-cmark → TextTag rendering**

- Entry point: event loop that maps CommonMark events to GtkTextTags
  [`mod.rs:57`](../../crates/lushtext-core/src/ui/markdown_preview/mod.rs#L57)

- TextTag table construction and Adwaita color matching for dark/light
  [`imp.rs:96`](../../crates/lushtext-core/src/ui/markdown_preview/imp.rs#L96)

**Window integration: actions, animation, clamping**

- Two-action design: side-by-side toggle + Alt+P preview-only mode
  [`preview.rs:28`](../../crates/lushtext-core/src/ui/window/preview.rs#L28)

- Side-by-side animation mirroring sidebar pattern (shrink-end-child, 1px target)
  [`preview.rs:102`](../../crates/lushtext-core/src/ui/window/preview.rs#L102)

- Preview-only animation: editor hidden, preview full-width
  [`preview.rs:161`](../../crates/lushtext-core/src/ui/window/preview.rs#L161)

- Right-side pane clamping (max 1/3 width, debounced GSettings persist)
  [`preview.rs:258`](../../crates/lushtext-core/src/ui/window/preview.rs#L258)

- Preview state fields and type registration on window imp
  [`imp.rs:57`](../../crates/lushtext-core/src/ui/window/imp.rs#L57)

- Buffer changed signal wiring with stored handler ID for cleanup
  [`mod.rs:269`](../../crates/lushtext-core/src/ui/window/mod.rs#L269)

- Preview mode reset on last-tab-close (review fix)
  [`mod.rs:341`](../../crates/lushtext-core/src/ui/window/mod.rs#L341)

**Template and schema changes**

- window.ui: nested GtkPaned inside "tabs" stack page
  [`window.ui:82`](../../resources/ui/window.ui#L82)

- Preview widget template: read-only GtkTextView + AdwStatusPage placeholder
  [`markdown-preview.ui:1`](../../resources/ui/markdown-preview.ui#L1)

- GSettings keys: preview-pane-position, preview-pane-visible
  [`gschema.xml:82`](../../data/dev.cominotti.lushtext.gschema.xml#L82)

**Tests**

- 19 widget tests for rendering, mode switching, read-only assertions
  [`markdown_preview.rs:1`](../../crates/lushtext/tests/widget/markdown_preview.rs#L1)

- 4 regression tests: action lifecycle + Alt+P no-op when side pane visible
  [`window.rs:2100`](../../crates/lushtext/tests/widget/window.rs#L2100)
