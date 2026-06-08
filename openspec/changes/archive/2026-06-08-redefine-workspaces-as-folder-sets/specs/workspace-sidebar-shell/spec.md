## ADDED Requirements

### Requirement: Workspace sections render ordered folder trees
The system SHALL render one workspace section per workspace, and each workspace section SHALL render one top-level folder tree for each persisted workspace folder in that workspace's stored order. A workspace section MAY contain zero folder trees. The workspace section header MUST remain the workspace-level control surface for renaming/removing the workspace, adding folders to that workspace, refreshing visible folder trees, and accessing deterministic folder-note flows.

#### Scenario: Workspace with multiple folders renders ordered folder trees
- **WHEN** the app restores one workspace with folders `/repo`, `/docs`, and `/tmp/notes` in that order
- **THEN** the sidebar renders one workspace section for that workspace
- **AND** the section renders top-level folder trees for `/repo`, `/docs`, and `/tmp/notes` in that order
- **AND** the section does not split those folders into separate workspace sections

#### Scenario: Zero-folder workspace renders explicit empty state
- **WHEN** the app restores a workspace with no folders
- **THEN** the sidebar renders that workspace section with its header controls available
- **AND** the section shows an explicit empty folder-set state rather than a fake folder row
- **AND** the empty state keeps the add-folder action reachable

#### Scenario: Same file can appear in overlapping folder trees
- **WHEN** a workspace contains folders `/repo` and `/repo/src`
- **AND** `/repo/src/main.rs` exists on disk
- **THEN** the `/repo` folder tree may show `src/main.rs`
- **AND** the `/repo/src` folder tree may show `main.rs`
- **AND** the sidebar does not hide either row merely because both rows refer to the same file

### Requirement: Users can add and remove folders inside a workspace
The system SHALL let users add folders to an existing workspace and remove individual folders from a workspace without deleting files on disk. Folder add operations MUST reject duplicate canonical folders only inside the target workspace. Removing a folder MUST remove only that folder's workspace membership and MUST NOT delete its files, delete its folder note, or remove the workspace unless the user explicitly removes the workspace.

#### Scenario: Add folder to existing workspace
- **WHEN** the user activates the add-folder affordance for a workspace section
- **AND** selects a folder that is not already in that workspace by canonical identity
- **THEN** the folder is appended to that workspace's folder list
- **AND** the sidebar shows a new top-level folder tree for that folder
- **AND** workspace-aware consumers refresh for the updated folder set

#### Scenario: Remove folder from workspace
- **WHEN** the user removes one folder from a workspace section
- **THEN** that folder tree is removed from the sidebar
- **AND** files on disk are not deleted
- **AND** the workspace remains present even if it now has zero folders
- **AND** any folder-note sidecar for that folder is preserved for future re-add

#### Scenario: Duplicate folder add gives feedback
- **WHEN** the user tries to add a folder already present in the same workspace by canonical identity
- **THEN** the workspace folder list is unchanged
- **AND** the sidebar surfaces feedback that the folder already belongs to that workspace

### Requirement: Users can reorder workspace folders
The system SHALL let users reorder folders inside a workspace through drag-and-drop and through a non-pointer control path such as Move Up/Move Down actions. Reordering MUST update the persisted folder order, the sidebar display order, and workspace-aware consumer primary-context tie-breaks. Reordering MUST NOT change folder membership, file contents, notes, bookmarks, or open tabs.

#### Scenario: Drag-and-drop reorders folders
- **WHEN** a workspace section contains folders A, B, and C
- **AND** the user drags folder C before folder A
- **THEN** the sidebar order becomes C, A, B
- **AND** the persisted folder order becomes C, A, B after persistence completes
- **AND** workspace-aware consumers receive a structure update

#### Scenario: Keyboard-accessible reorder works
- **WHEN** a workspace section contains folders A, B, and C
- **AND** the user invokes a non-pointer Move Up action for folder C
- **THEN** the sidebar order becomes A, C, B
- **AND** the same persisted folder-order update path is used as drag-and-drop

#### Scenario: Reorder does not rebuild unrelated sections
- **WHEN** the user reorders folders inside Workspace A
- **THEN** Workspace B sections remain mounted and unchanged
- **AND** no unrelated workspace selection, folder note, document tab, or sidebar filter state is reset

### Requirement: Folder context menus expose folder-scoped actions
The system SHALL expose folder-scoped actions from the context menu of a top-level workspace folder row. Folder-scoped actions MUST have a clear folder target and MUST use folder terminology. File and directory rows below a workspace folder MUST keep existing file operations and document-note/local-history actions.

#### Scenario: Folder row context menu exposes folder note
- **WHEN** the user opens the context menu for a top-level workspace folder row
- **THEN** the menu offers `Open Folder Note...`
- **AND** activating it opens the note for that exact folder
- **AND** the action does not depend on the current active editor tab

#### Scenario: Folder row context menu exposes remove folder
- **WHEN** the user opens the context menu for a top-level workspace folder row
- **THEN** the menu offers a remove-from-workspace action using folder terminology
- **AND** activating it removes that folder from the workspace after confirmation
- **AND** it does not delete files from disk

### Requirement: Sidebar geometry remains stable across folder-set states
The system SHALL keep the fixed workspace selector row visible, avoid horizontal scrollbars, and keep commands reachable across zero folders, representative folder sets, many folders, long folder names, overlapping folders, and constrained sidebar widths. Dense folder lists MUST scroll only in the workspace-section list area below the fixed selector row.

#### Scenario: Many folders scroll below fixed selector
- **WHEN** a workspace contains enough folders for the sidebar content to exceed the visible height
- **THEN** the top workspace selector row remains fixed and visible
- **AND** only the workspace-section list scrolls
- **AND** add-folder, refresh, reorder, and context-menu actions remain reachable

#### Scenario: Long folder names do not create horizontal scrollbar
- **WHEN** a workspace folder has a long path or display name
- **THEN** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** folder labels ellipsize or wrap according to the sidebar design
- **AND** controls remain visible without overlapping text

## MODIFIED Requirements

### Requirement: Sidebar keeps a fixed workspace scope row
The system SHALL render a fixed top sidebar row above the scrollable workspace-section list. That row MUST contain the workspace scope selector and a workspace creation affordance, and it MUST remain visible while workspace sections and folder trees scroll. The selector MUST offer the explicit aggregate scope `All workspaces` plus one item per restored workspace, regardless of how many folders each workspace contains.

#### Scenario: Populated sidebar keeps the scope row pinned
- **WHEN** one or more workspaces are restored and the workspace-section list becomes vertically scrollable
- **THEN** the scope selector row remains visible above the scroll area
- **AND** scrolling the workspace sections or folder trees does not scroll that top row away

#### Scenario: Scope selector lists aggregate and concrete workspace scopes
- **WHEN** the sidebar renders with restored workspaces
- **THEN** the scope selector includes `All workspaces`
- **AND** it includes one additional option for each restored workspace
- **AND** it does not list every folder as a separate workspace scope

### Requirement: Empty workspace state remains an explicit shell
The system SHALL treat the no-workspace state as an intentional empty sidebar shell. When no workspaces exist, the sidebar MUST render no workspace sections and MUST NOT create a visible placeholder workspace section solely to satisfy model defaults. This no-workspace shell is distinct from a real workspace that contains zero folders.

#### Scenario: First launch with no workspaces shows an empty shell
- **WHEN** the app launches without any persisted workspaces
- **THEN** the sidebar shows the fixed top scope row
- **AND** the sidebar renders zero workspace sections below it

#### Scenario: Removing the last workspace returns to the empty shell
- **WHEN** the user removes the last remaining workspace
- **THEN** the sidebar returns to the fixed top scope row with no workspace sections
- **AND** no placeholder `New Workspace` section is inserted automatically

#### Scenario: Zero-folder workspace is not the no-workspace shell
- **WHEN** one workspace exists but its folder list is empty
- **THEN** the sidebar renders that workspace section
- **AND** the app does not treat the window as having no workspace scope

### Requirement: Drill-down navigation stays local to the current workspace section
The system SHALL keep deep folder drill-down as temporary section-local navigation state. Focusing a descendant directory MUST temporarily show that subtree only inside the current workspace section, MUST reveal a back affordance for the focused lineage, and MUST NOT mutate the workspace's persisted folder list or folder order.

#### Scenario: Focus Folder narrows one section temporarily
- **WHEN** the user focuses a nested folder inside one workspace folder tree
- **THEN** only that workspace section narrows its tree view to the focused folder
- **AND** the sidebar reveals a back affordance showing the focused drill-down lineage
- **AND** the workspace's persisted folder set remains unchanged

#### Scenario: Navigating back restores the previous workspace view
- **WHEN** the user activates the drill-down back affordance
- **THEN** the workspace section returns to the previous focused folder or configured folder tree list
- **AND** the section restores the broader workspace tree without redefining the workspace's folder set

## REMOVED Requirements

### Requirement: Each workspace section owns one root directory
**Reason**: A workspace section now owns an ordered set of zero or more workspace folders, not exactly one root directory.
**Migration**: Existing one-root workspaces migrate to one-folder workspace sections; section-level refresh remains but applies to the section's folder trees.
