# Split Panes

## Status: Proposed

## Description
View two files side-by-side, or the same file at two different positions, by splitting
the editor area horizontally or vertically. The most requested power-user feature in
lightweight editors — and the most common reason people reluctantly open VS Code for a
"quick edit."

## Current State
- Single `AdwTabView` occupies the entire content area (right side of `GtkPaned`)
- Each tab contains one `LushtextEditorPage` with one `GtkSourceView`
- No mechanism to display multiple editors simultaneously
- The `GtkPaned` is used only for the sidebar/content split

## Motivation
Comparing two files, referencing one while editing another, or viewing the top and
bottom of a long file simultaneously are fundamental editing workflows. GNOME Text
Editor doesn't support splits. Kate does, but Kate is a KDE application. For GTK4
users, this is an unmet need.

## Implementation Plan

### Phase 1: Architecture Design
Two possible approaches:

**Option A: Nested GtkPaned (recommended for MVP)**
1. Replace the content `GtkStack` child with a `GtkPaned` (vertical or horizontal)
2. Each pane contains its own `AdwTabView` + `AdwTabBar`
3. Split action creates the second pane; closing the last tab in a pane removes it
4. Maximum 2 panes for MVP (left/right or top/bottom)

**Option B: Grid-based multi-pane**
1. Replace content area with a `GtkGrid` or recursive `GtkPaned` tree
2. Supports arbitrary split configurations (2x2, 3-way, etc.)
3. More complex, better deferred to a later version

### Phase 2: Core Split Mechanics
1. New actions:
   - `win.split-right` (`Ctrl+\`) — vertical split
   - `win.split-down` (`Ctrl+Shift+\`) — horizontal split
   - `win.close-split` — close the active pane (moves tabs to remaining pane)
   - `win.focus-other-pane` (`Ctrl+Alt+Left/Right`) — switch focus between panes
2. Splitting the current tab:
   - "Split Right with Current File" opens the same `GtkSourceBuffer` in a new
     `GtkSourceView` in the second pane (shared buffer = synchronized edits)
   - "Split Right" creates an empty second pane
3. Active pane tracking: highlight the active pane's tab bar or add a subtle border

### Phase 3: Same-Buffer Splits
1. Two `GtkSourceView` widgets can share one `GtkSourceBuffer` — this is natively
   supported by GtkSourceView
2. Both views show the same content, with independent scroll positions and cursor
3. Edits in one view appear instantly in the other (buffer `changed` signal)
4. Each view maintains its own `GtkSourceSearchContext` for independent search

### Phase 4: Tab Movement Between Panes
1. Drag-and-drop tabs between pane tab bars
2. Context menu "Move to Other Pane" on tab right-click
3. When a pane has no tabs, it shows the `AdwStatusPage` empty state
4. When the last tab is dragged out, the empty pane auto-closes

### Phase 5: Session Persistence
1. Extend `SessionData` to record pane layout: `{ panes: [{ orientation, tabs }] }`
2. Restore split state on session load
3. Each pane's selected tab is tracked independently

## Architecture Considerations
- `AdwTabView` supports a single `AdwTabBar` binding. For two panes, we need two
  `AdwTabView` + `AdwTabBar` pairs. This means the window's `tab_view()` accessor
  needs to become `active_tab_view()` with pane awareness.
- The existing `open_paths: HashSet<PathBuf>` dedup must span both panes — a file open
  in pane 1 should not reopen in pane 2 (instead, focus the existing tab).
- `refresh_status_bar()` must react to the active pane's selected tab, not a single
  global tab view.
- The sidebar's `connect_file_activated` callback needs to open files in the active pane.
- The save-changes dialog on window close must collect unsaved tabs from all panes.
- Buffer eviction must account for the same buffer being visible in multiple views —
  never evict a buffer that any visible view is showing.

## Dependencies
- `GtkSourceView` shared buffer support (built-in)
- `AdwTabView` + `AdwTabBar` (one pair per pane)
- `GtkPaned` for split layout
- Significant refactoring of `LushtextWindow` to support multi-pane tab management

## Risks
- This is the highest-effort feature in the list. The window's tab management assumes a
  single `AdwTabView` throughout — many methods (`new_tab`, `open_document`, `close-tab`,
  session save/restore, draft persistence, status bar refresh) need pane awareness.
- `AdwTabBar` visual design with two tab bars stacked or side-by-side may look cluttered.
  Consider hiding the tab bar in a pane that has only one tab.
- Drag-and-drop between `AdwTabView` instances may not be natively supported — may need
  detach-from-one + attach-to-other logic.
