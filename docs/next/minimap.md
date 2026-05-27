# Minimap with Semantic Regions

## Status: Proposed

## Description
A narrow vertical strip on the right edge of the editor showing a zoomed-out
representation of the entire file, with colored region markers for syntax sections,
bookmarks, search matches, modified-since-last-save ranges, and long-line warnings.
Clicking jumps to that position. Enhanced beyond Sublime's purely visual minimap with
semantic coloring — you can *see* where the changes are, where your bookmarks are,
where the search hits cluster.

## Current State
- No minimap or document overview exists
- GtkSourceView has `GtkSourceMap` — a built-in minimap widget that renders a scaled-down
  view of the buffer content
- The editor overlay (`GtkOverlay`) currently hosts the search bar
- No change tracking (modified-since-last-save regions) exists

## Motivation
For large files (which LushText already handles up to 500MB), navigation by scrolling
is inefficient. A minimap provides spatial orientation — "where am I in the file?" — and
with semantic coloring, it becomes a navigation tool: click on the red region to jump to
the error, click on the yellow marks to visit your bookmarks, see at a glance where your
search matches cluster. Sublime popularized this; LushText can improve on it.

## Implementation Plan

### Phase 1: Basic Minimap via GtkSourceMap
1. Evaluate `GtkSourceMap` (built into GtkSourceView 5):
   - `GtkSourceMap` is a `GtkSourceView` subclass that renders a scaled-down view
   - It connects to a parent `GtkSourceView` and syncs scrolling
   - It's the simplest path to a basic minimap
2. Add `GtkSourceMap` as a child of the editor `GtkOverlay`, positioned right
3. Toggle via GSettings: `show-minimap` (b, default false)
4. Toggle action: `win.toggle-minimap` with keyboard shortcut (e.g., `Ctrl+Shift+M`)
5. Width: ~80-100px, non-interactive text (no cursor, no editing)

### Phase 2: Semantic Markers (if GtkSourceMap is sufficient)
1. Use `GtkSourceMark` + `GtkSourceMarkAttributes` to add colored marks in the gutter
   of the minimap view:
   - Search matches: orange marks
   - Bookmarks: blue marks (requires bookmarks feature)
   - Modified lines: green marks
   - Long lines (>120 chars): subtle red marks
2. The minimap's `GtkSourceView` shares the same `GtkSourceBuffer`, so all marks
   placed by the main editor are automatically visible in the minimap

### Phase 3: Custom Minimap (if GtkSourceMap is insufficient)
If `GtkSourceMap` doesn't support the semantic overlay we need, build a custom widget:
1. New `LushtextMinimap` widget — a `GtkDrawingArea` with custom Cairo rendering
2. Render each line as a thin colored horizontal stripe:
   - Text content: gray stripes proportional to line length
   - Syntax regions: tinted by language category (keywords, strings, comments)
   - Semantic markers: overlay dots/stripes for bookmarks, search matches, changes
3. Viewport indicator: semi-transparent rectangle showing the visible portion
4. Click/drag to scroll the main editor
5. Render asynchronously: compute the minimap bitmap on a background thread when the
   buffer changes, post to main thread via `idle_add_local`

### Phase 4: Change Tracking
1. Track modified-since-last-save line ranges on the `EditorPage`:
   - Use `GtkTextBuffer::connect_insert_text` and `connect_delete_range` to record
     modified line ranges in a `Vec<Range<u32>>`
   - Clear on save (all lines become "clean")
   - Clear on undo past the save point
2. Display as green gutter marks in both the main editor and the minimap
3. This is independently useful even without the minimap (many editors show change
   indicators in the gutter)

### Phase 5: Smooth Interaction
1. Hover over minimap shows a tooltip with the line number and first non-empty content
2. Click scrolls the main editor to that position (animated via existing scroll utilities)
3. Click-and-drag provides live scrolling
4. Minimap auto-hides when the file fits entirely in the viewport (no value added)
5. Minimap respects the current color scheme (dark/light background)

## Architecture Considerations
- **GtkSourceMap vs custom**: `GtkSourceMap` is the path of least resistance — it's
  maintained by the GtkSourceView team, handles font scaling, scroll sync, and click-to-
  scroll automatically. The trade-off is limited customization for semantic overlays.
  Start with `GtkSourceMap` and only build a custom widget if the semantic overlay
  requirements can't be met.
- The minimap shares the same `GtkSourceBuffer` — no duplicate text storage. Marks placed
  by the main editor (bookmarks, search results) are automatically visible. This is a
  major advantage of using `GtkSourceMap` over a custom rendering approach.
- For the custom approach (Phase 3), the minimap rendering should be debounced and
  incremental — only re-render the changed region, not the entire file. For a 500MB file,
  full re-renders would be prohibitively expensive.
- The `GtkOverlay` positioning is tricky with word wrap — the minimap should be anchored
  to the right edge of the `GtkSourceView`, not the right edge of the text content.

## Dependencies
- `GtkSourceMap` (built into GtkSourceView 5) for Phase 1-2
- `GtkSourceMark` for semantic markers
- Bookmarks feature (docs/next/bookmarks.md) for bookmark marks
- Change tracking infrastructure (Phase 4) — useful independently
- GSettings keys: `show-minimap` (b), `minimap-width` (i, default 80)

## Risks
- `GtkSourceMap` may not support custom gutter renderers in the scaled-down view. If
  marks are too small to see at minimap scale, the semantic overlay feature loses value.
  Test with the actual widget before committing to this approach.
- Performance with very large files (>10MB). `GtkSourceMap` creates a second
  `GtkSourceView` which performs its own layout calculations. For huge files, this could
  double the layout cost. May need to disable the minimap above a certain file size
  threshold (aligned with `file_limits.rs` tiers).
- The minimap adds visual width to the editor area, which interacts with the sidebar
  position clamping logic. The `stack_min` calculation in `clamp_sidebar_position` needs
  to account for minimap width.
