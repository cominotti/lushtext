# Workspace Management Dialog

## Status: Next priority

## Description
A dialog for managing workspaces: creating, renaming, switching between workspaces,
and adding/removing root directories and files from a workspace.

## Implementation Plan
1. Create `LushtextWorkspaceDialog` (extends `AdwDialog`)
2. Show list of all workspaces with add/remove/rename controls
3. For each workspace, show its entries (directories and files) with remove buttons
4. Add "Add Folder" and "Add File" buttons per workspace
5. Wire workspace switching to update the sidebar file tree
6. Persist changes via `workspace_manager::save()`
7. Add `win.manage-workspaces` action and menu item
