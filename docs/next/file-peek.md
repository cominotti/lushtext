# Smart File Peek

## Status: Proposed

## Description
Preview a file's contents from the sidebar without opening a tab. Press `Space` on a
selected sidebar file to show a floating preview popover with syntax-highlighted content,
file metadata, and a quick action to promote the preview to a full tab. Solves the
universal "I need to glance at 10 files to find the right one" problem without tab
pollution.

## Current State
- Sidebar file activation (double-click or Enter) opens the file as a full tab via
  `open_document()`
- No preview mechanism exists
- File metadata (size, encoding) is only available after opening a file in a tab
- `file_limits.rs` already classifies file sizes for graceful degradation

## Motivation
VS Code's "preview tabs" (italic title, silently replaced by the next opened file) are
confusing — users don't understand why their tab disappeared. A deliberate peek/open
distinction is cleaner: peek to glance, Enter to commit. This is especially valuable
when navigating unfamiliar codebases or large document collections where you need to
inspect many files before finding the right one.

## Implementation Plan

### Phase 1: Peek Popover Widget (ui/file_peek/)
1. New `LushtextFilePeek` widget — a `GtkPopover` containing:
   - Header: file name, relative path, file size, last modified date
   - Body: `GtkSourceView` (read-only, no line numbers, no gutter) showing first ~60
     lines with syntax highlighting
   - Footer: "Open" button (promotes to tab), "Copy Path" button
2. Size: ~500px wide, ~400px tall (responsive to window size)
3. Positioned relative to the selected sidebar row

### Phase 2: Keyboard + Mouse Triggers
1. `Space` key on selected sidebar item opens peek popover
2. Second `Space` or `Escape` closes it
3. `Enter` while peek is open promotes to a full tab and closes peek
4. Optional: hover with delay (500ms) opens peek on mouse — configurable, default off
5. Arrow keys while peek is open navigate sidebar selection and update peek content

### Phase 3: Content Loading
1. Read first 8KB of file (enough for ~60 lines of typical source code)
2. Use existing `file_limits.rs` size classification — skip preview for files >50MB
3. Load on background thread via `spawn_blocking_then`
4. Detect language via `sourceview5::LanguageManager` (same as `open_document`)
5. Apply current style scheme (light/dark aware)
6. For binary files: show "Binary file — N bytes" instead of content
7. For images: show a `GtkPicture` thumbnail (stretch goal)

### Phase 4: Metadata Display
1. File size (human-readable: "12.3 KB")
2. Last modified (relative: "2 hours ago", absolute on hover)
3. Line count (from the partial read, with "~" prefix if file was truncated)
4. Detected language/syntax
5. Encoding (UTF-8 assumed for now; update when encoding support lands)

## Architecture Considerations
- The peek popover should be a single instance owned by `LushtextSidebar` (or per
  `WorkspaceSection`), repositioned and updated on each peek request. Creating/destroying
  popovers per peek would be wasteful.
- The read-only `GtkSourceView` in the peek shares the same style scheme and font settings
  as the main editor (via the existing `.monospace` CSS class and GSettings bindings).
- Partial file reads (8KB) avoid loading huge files into memory just for a preview. The
  buffer is not kept — closing the peek frees the memory.
- The `GtkPopover` approach is preferred over a separate panel because it's ephemeral and
  doesn't change the window layout. It disappears as soon as the user moves on.
- Sidebar keyboard navigation while peek is open requires intercepting arrow key events
  on the popover and forwarding them to the `GtkListView` selection model.

## Dependencies
- `GtkSourceView` for syntax-highlighted preview
- Existing `GtkSourceLanguageManager` and style scheme infrastructure
- `spawn_blocking_then` for async file loading
- `file_limits.rs` for size classification
- Sidebar `GtkListView` selection model for keyboard navigation

## Risks
- `GtkPopover` positioning near the edge of the screen may cause the popover to flip
  or clip awkwardly. Need to test with sidebar at various widths.
- Loading many files in rapid succession (holding arrow key with peek open) could
  overwhelm the I/O system. Debounce content loading at 150ms and cancel in-flight
  loads when selection changes.
- The `GtkSourceView` inside a popover may have focus issues — the popover grabs focus
  by default, which would break arrow key navigation. May need `can-focus=false` on the
  preview source view.
