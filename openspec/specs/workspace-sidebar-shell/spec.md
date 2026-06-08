# workspace-sidebar-shell Specification

## Purpose
Define the user-facing workspace sidebar shell, including the fixed scope row, empty-state behavior, ordered folder-set workspace sections, folder-scoped actions, and section-local drill-down navigation.

## Requirements

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

### Requirement: Top-level workspace folder rows expose real folder identity
The system SHALL render each configured workspace folder as a real top-level folder row. A top-level workspace folder row MUST display an actual folder-derived label, MUST expose the folder's configured path through tooltip, accessibility metadata, context targeting, or an equivalent inspection affordance, and MUST NOT be replaced by a synthetic `Files`, `Folders`, or `Files & Folders` tree row.

#### Scenario: One-folder workspace shows the real folder row
- **WHEN** a workspace contains exactly one configured folder `/home/user/novel`
- **THEN** the workspace section renders a top-level row for `/home/user/novel`
- **AND** the visible row label is derived from that actual folder, such as `novel`
- **AND** the row is not labeled `Files`
- **AND** folder-scoped context actions target `/home/user/novel`

#### Scenario: Multi-folder workspace uses the same real-row presentation
- **WHEN** a workspace contains configured folders `/home/user/novel` and `/home/user/research`
- **THEN** the workspace section renders top-level rows for both folders in stored order
- **AND** both rows use actual folder-derived labels
- **AND** the section does not introduce a fake grouping row above them

#### Scenario: Empty workspace does not render a synthetic folder row
- **WHEN** a workspace contains zero folders
- **THEN** the workspace section renders its explicit empty folder-set state
- **AND** it renders no selectable, expandable, draggable, or context-menu-capable synthetic folder row
- **AND** it does not show a `Files`, `Folders`, or `Files & Folders` tree row

#### Scenario: Long or duplicate folder labels remain inspectable
- **WHEN** a workspace has a top-level folder row whose visible label is long or shares a basename with another workspace folder
- **THEN** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** row controls do not overlap the label
- **AND** the user can still inspect the full configured folder path from the row's tooltip, accessibility metadata, context menu target, or equivalent affordance

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
The system SHALL let users reorder folders inside a workspace through drag-and-drop and through a non-pointer control path such as Move Up/Move Down actions. Reordering MUST update the persisted folder order, the sidebar display order, and workspace-aware consumer primary-context tie-breaks. Reordering MUST NOT change folder membership, file contents, notes, bookmarks, or open tabs. During workspace-folder drag-and-drop reorder, drag hover MUST be owned by an inert row-level DnD surface above folder disclosure widgets before those disclosure widgets can react. Reorder drag hover MUST NOT expand folders, collapse folders, flip or flicker disclosure icons, materialize child stores, restart workspace watches, change selection, focus/drill down into folders, show drop-into-folder feedback, or paint any filled row highlight.

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

#### Scenario: Reorder hover does not reach folder disclosure widgets
- **WHEN** the user drags a top-level workspace folder for reorder
- **AND** the pointer moves over another collapsed top-level folder row or over that row's disclosure icon region
- **THEN** the row-level DnD surface owns the drag hover before the folder disclosure widget receives it
- **AND** the disclosure icon remains visually stable
- **AND** the folder row does not emit an expanded-state transition
- **AND** no child model or child rows are materialized solely because of the drag hover
- **AND** workspace watch targets are not restarted solely because of the drag hover
- **AND** the only visible feedback for a valid reorder target is the single rounded insertion line at the before or after edge

#### Scenario: Descendant folder hover is inert during reorder
- **WHEN** the user drags a top-level workspace folder for reorder
- **AND** the pointer moves over a descendant folder row or that descendant row's disclosure icon region
- **THEN** the row-level DnD surface owns the drag hover before the descendant disclosure widget receives it
- **AND** no insertion line is shown for the descendant row
- **AND** dropping on the descendant row is rejected
- **AND** neither the descendant row nor its ancestors expand or collapse because of the hover

#### Scenario: Reorder shield is inactive outside reorder drags
- **WHEN** the user is not actively dragging a top-level workspace folder for reorder
- **THEN** folder disclosure widgets keep their normal pointer and keyboard expansion behavior
- **AND** file activation, file peek, context menus, focus-folder controls, and inline rename flows remain reachable through their existing interactions

#### Scenario: Recycled rows clear reorder hover state
- **WHEN** a file-tree row widget that showed or owned workspace-folder reorder hover is recycled for another folder or file row
- **THEN** the recycled row has no visible insertion indicator
- **AND** the recycled row has no stale valid-drop state
- **AND** ordinary expansion and activation behavior matches the newly bound row

#### Scenario: Reorder shield preserves constrained sidebar geometry
- **WHEN** the sidebar has many workspace folders, long folder names, overlapping folders, or a constrained width
- **AND** the user drags a top-level workspace folder for reorder
- **THEN** the fixed workspace scope row remains visible
- **AND** folder rows do not change height solely because of the reorder shield
- **AND** the sidebar does not gain a horizontal scrollbar
- **AND** folder labels, drag handles, context-menu access, and header controls remain reachable without overlapping

### Requirement: Folder drag-and-drop is visibly reorder-only
The system SHALL present workspace folder drag-and-drop as a reorder-only operation for top-level workspace folder rows. Reorder DnD MUST expose an explicit drag affordance on reorderable top-level folder rows, MUST show a horizontal insertion indicator above or below valid target rows during drag motion, MUST reject invalid targets without showing insertion feedback, MUST NOT expand, collapse, focus, drill down into, or otherwise mutate sidebar tree state during drag hover, and MUST update only workspace folder order when a drop succeeds.

#### Scenario: Reorderable folder rows expose drag affordance
- **WHEN** a workspace section is showing its normal top-level folder list
- **AND** a top-level workspace folder row can be reordered
- **THEN** that row exposes a visible drag handle or equivalent explicit reorder affordance
- **AND** descendant file rows and descendant directory rows do not expose workspace-folder reorder handles

#### Scenario: Valid drag shows insertion line
- **WHEN** the user drags a top-level workspace folder over another top-level workspace folder in the same workspace
- **AND** the pointer is in the target row's upper insertion zone
- **THEN** the sidebar shows a horizontal insertion indicator above the target row
- **AND** it does not show a visual state that implies dropping into the target folder

#### Scenario: Insertion feedback is a single rounded line
- **WHEN** the user drags a top-level workspace folder over a valid before-row or after-row insertion target
- **THEN** the sidebar shows exactly one smooth rounded horizontal insertion line at the target insertion edge
- **AND** it does not show a filled rectangular highlight, duplicate overlapping line, row drop highlight, or centered drop-into-folder cue

#### Scenario: Valid drop reorders folder memberships
- **WHEN** a workspace contains folders A, B, and C
- **AND** the user drops folder C before folder A through the reorder affordance
- **THEN** the sidebar order becomes C, A, B
- **AND** the persisted workspace folder order becomes C, A, B after persistence completes
- **AND** workspace-aware consumers receive a structure update

#### Scenario: Invalid child-row target is rejected without feedback
- **WHEN** the user drags a top-level workspace folder over a descendant file row or descendant directory row
- **THEN** the sidebar shows no insertion indicator for that descendant row
- **AND** dropping there is rejected
- **AND** the workspace folder order remains unchanged

#### Scenario: Drag hover does not expand or collapse folders
- **WHEN** the user is dragging a top-level workspace folder for reorder
- **AND** the pointer moves over another top-level folder row, a descendant folder row, or an expander region
- **THEN** no folder row expands or collapses because of the drag hover
- **AND** no child rows are materialized solely because of the drag hover
- **AND** workspace watch targets are not restarted solely because of drag-hover expansion
- **AND** the only sidebar presentation state that may change during hover is the transient insertion indicator

#### Scenario: Cross-workspace drop is rejected without mutation
- **WHEN** the user drags a workspace folder from Workspace A over a folder row in Workspace B
- **THEN** the sidebar shows no valid insertion indicator for Workspace B
- **AND** dropping there is rejected
- **AND** both workspace folder orders remain unchanged

#### Scenario: Reorder never mutates filesystem content
- **WHEN** the user reorders folders inside a workspace through drag-and-drop or Move Up/Move Down
- **THEN** no file or directory is created, deleted, moved, copied, or renamed on disk
- **AND** only the workspace metadata order changes

#### Scenario: Non-pointer reorder path remains available
- **WHEN** a user cannot or does not use pointer drag-and-drop
- **THEN** the sidebar provides a non-pointer folder reorder path such as Move Up and Move Down
- **AND** that path uses the same persisted workspace-folder reorder behavior as drag-and-drop

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

### Requirement: Workspace creation is name-first and folder-optional
The system SHALL present workspace creation as a name-entry flow rather than a folder chooser. Activating `New Workspace` MUST ask for a workspace name, MUST create a workspace with zero folders after a non-empty trimmed name is confirmed, MUST select that workspace as the current shared workspace scope, and MUST leave folder membership unchanged until the user explicitly adds a folder to that workspace.

#### Scenario: New Workspace opens a name modal
- **WHEN** the user activates the fixed sidebar row's `New Workspace` affordance
- **THEN** the system presents a modal workflow for entering a workspace name
- **AND** the workflow does not open a file or folder chooser
- **AND** no folder path is required before the workspace can be created

#### Scenario: Create named empty workspace
- **WHEN** the user enters `Writing` in the new-workspace name workflow
- **AND** confirms creation
- **THEN** the sidebar creates a workspace named `Writing`
- **AND** the new workspace contains zero workspace folders
- **AND** the new workspace is selected as the current shared workspace scope
- **AND** the workspace section renders with its empty folder-set state and add-folder control reachable

#### Scenario: Empty workspace name is rejected
- **WHEN** the user leaves the new-workspace name empty or enters only whitespace
- **AND** attempts to confirm creation
- **THEN** no workspace is created
- **AND** the current workspace scope is unchanged
- **AND** the workflow keeps the user in a recoverable state where they can enter a valid name or cancel

#### Scenario: Canceling new workspace does not mutate workspace state
- **WHEN** the user opens the new-workspace name workflow
- **AND** cancels it before confirming a valid name
- **THEN** the workspace list is unchanged
- **AND** the current workspace scope is unchanged
- **AND** no workspace persistence write is required for the canceled action

#### Scenario: Add Folder remains the folder picker
- **WHEN** the user creates an empty workspace
- **AND** then activates that workspace section's `Add Folder` affordance
- **AND** selects a folder that is not already in that workspace by canonical identity
- **THEN** the selected folder is appended to that workspace's folder list
- **AND** the flow uses the existing add-folder duplicate and persistence behavior

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

### Requirement: Workspace sections have explicit collapsible bodies
The system SHALL let users collapse and expand each workspace section's folder body from an explicit affordance in the workspace header. Collapsing a workspace section MUST hide that workspace's folder body while keeping the workspace header, workspace label, add-folder control, refresh control, and workspace context menu reachable. Section collapse MUST be separate from individual folder-row expansion, MUST NOT mutate workspace membership or folder order, and MUST NOT change the current shared workspace scope or workspace-aware feature semantics.

#### Scenario: Workspace header exposes collapse affordance
- **WHEN** a workspace section is rendered
- **THEN** the workspace header exposes a chevron, disclosure button, or equivalent explicit collapse/expand affordance near the workspace label
- **AND** the affordance communicates the current expanded or collapsed state visually
- **AND** the affordance has an accessible label or tooltip for collapsing or expanding the workspace section

#### Scenario: Collapsing hides folder body but keeps header controls
- **WHEN** the user collapses a workspace section
- **THEN** the workspace's folder tree, empty-folder-set label, and drill-down body are hidden
- **AND** the workspace header and workspace name remain visible
- **AND** Add Folder, Refresh, and the workspace header context menu remain reachable

#### Scenario: Expanding restores previous section body state
- **WHEN** a workspace section has expanded folder rows or an active drill-down view
- **AND** the user collapses and then expands that workspace section
- **THEN** the workspace section restores the previous folder-body presentation where the underlying rows still exist
- **AND** section collapse does not force every folder row to expand or collapse

#### Scenario: Section collapse does not change workspace semantics
- **WHEN** the user collapses a workspace section
- **THEN** the current shared workspace scope is unchanged
- **AND** search, command palette, notes, bookmarks, Markdown preview, and folder-note flows continue to resolve the same workspace folder coverage
- **AND** no workspace folder is added, removed, reordered, or persisted solely because the section was collapsed

#### Scenario: Collapsed empty workspace keeps Add Folder reachable
- **WHEN** a workspace contains zero folders
- **AND** the user collapses that workspace section
- **THEN** the empty-folder-set body is hidden
- **AND** the Add Folder control remains visible from the workspace header
- **AND** expanding the section shows the empty-folder-set state again

#### Scenario: Many-folder workspace reduces vertical clutter
- **WHEN** a workspace contains enough folder rows or expanded descendants to consume substantial sidebar height
- **AND** the user collapses that workspace section
- **THEN** that workspace consumes only its header area in the workspace-section list
- **AND** other workspace sections become easier to reach without horizontal scrolling or control overlap

#### Scenario: Collapse state survives ordinary section rebuilds
- **WHEN** a workspace section is collapsed
- **AND** the sidebar rebuilds sections for the same workspace ID during the current window session because of add, remove, reorder, refresh, or scope-filter updates
- **THEN** the workspace section remains collapsed after the rebuild
- **AND** the workspace-state JSON payload is not extended solely to persist that collapsed state

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
