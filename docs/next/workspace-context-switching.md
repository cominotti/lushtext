# Workspace Context Switching

## Status: Proposed

## Description
Extend the existing multi-workspace sidebar into full workspace context switching.
Each workspace remembers its own open tabs, scroll positions, sidebar expansion state,
and active file. Switching workspaces instantly swaps the entire editing context —
like virtual desktops for editing sessions.

## Current State
- `WorkspacesFile` persists workspace definitions (name + root directories) to
  `workspaces.json`
- Session persistence (`session.json`) is global — all tabs share one `AdwTabView`
  and one session file regardless of workspace
- Sidebar expansion state is not persisted
- No concept of "active workspace" beyond which section is visible in the sidebar

## Motivation
Users who work across multiple projects (sysadmins managing servers, writers
switching between books, developers juggling repos) currently see all tabs from
all contexts mixed together. Switching context means mentally filtering irrelevant
tabs and manually closing/reopening files. This is the most common pain point in
lightweight editors that support workspaces.

## Implementation Plan

### Phase 1: Per-Workspace Session Storage
1. Extend `SessionData` with a `workspace_id: Option<WorkspaceId>` field per tab
2. Add `WorkspaceSessionData` struct: `{ active_tab_index, tab_order, sidebar_expansion }`
3. Store per-workspace session data alongside the global session in
   `$XDG_DATA_HOME/lushtext/workspace-sessions/<workspace_id>.json`
4. On workspace switch, serialize current workspace's tabs and restore target's tabs

### Phase 2: Tab Filtering
1. Add an "active workspace" concept to `LushtextWindow` (`Cell<Option<WorkspaceId>>`)
2. When a workspace is active, `AdwTabView` shows only tabs belonging to that workspace
3. Unaffiliated tabs (opened via CLI, drag-and-drop) remain visible in all contexts
4. Tab affiliation is determined by `Path::starts_with` against workspace roots

### Phase 3: Sidebar Expansion Persistence
1. Track expanded directories per workspace section as `HashSet<PathBuf>` in
   `WorkspaceSessionData`
2. On `TreeListRow` expand/collapse, update the set and mark persistence dirty
3. On workspace restore, expand rows matching persisted paths after tree model loads

### Phase 4: Keyboard-Driven Switching
1. `Ctrl+Alt+1..9` switches to workspace by index
2. Command palette "Switch Workspace" command with fuzzy matching on workspace names
3. Optional: workspace indicator in the header bar subtitle

## Architecture Considerations
- Tab hiding could use `AdwTabView` page visibility or a filtered model. The
  `AdwTabView` API does not natively support hiding pages — may need to detach/reattach
  pages, which interacts with session save triggers. Investigate `AdwTabPage::set_pinned`
  and custom filtering before committing to detach/reattach.
- Per-workspace session files avoid bloating the global session and allow independent
  lifecycle (deleting a workspace deletes its session).
- The draft system needs workspace awareness — drafts for workspace-affiliated untitled
  tabs should restore into the correct workspace context.

## Dependencies
- Existing `WorkspacesFile` and `WorkspaceId` types (model/workspace.rs)
- Existing `SessionData` and session persistence (services/session_service.rs)
- `AdwTabView` page management (ui/window/)

## Risks
- `AdwTabView` + `AdwTabBar` may not support filtered views cleanly — GTK4's tab
  widgets assume all pages are always visible. May need a per-workspace `AdwTabView`
  with a `GtkStack` to swap between them, which changes the window architecture
  significantly.
- Session restore ordering becomes complex: global session loads first, then workspace
  sessions overlay. Race conditions with draft loading need careful sequencing.
