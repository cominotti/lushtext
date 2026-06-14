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

### Requirement: Workspace folder reorder affordances stay synchronized with live folder membership
The system SHALL keep workspace folder reorder affordances synchronized with the current visible workspace folder set. A top-level persisted workspace folder row MUST expose the reorder affordance only when its workspace section is showing the normal top-level folder list and that workspace has at least two configured folders. The affordance state MUST update immediately when folders are added, removed, reordered, refreshed, hidden, shown, collapsed, expanded, or rebound through row recycling. Descendant file rows, descendant directory rows, focused drill-down rows, empty-folder-set states, and one-folder workspaces MUST NOT expose workspace-folder reorder affordances.

#### Scenario: Empty workspace has no reorder affordance
- **WHEN** a workspace section contains zero configured folders
- **THEN** the section shows its empty folder-set state
- **AND** no workspace-folder reorder handle is visible
- **AND** the workspace header actions remain reachable

#### Scenario: First folder remains non-reorderable
- **WHEN** a user adds the first folder to an empty workspace during the current window session
- **THEN** that folder row appears in the normal top-level folder list
- **AND** it does not expose a workspace-folder reorder handle
- **AND** disclosure, context-menu, file activation, file peek, focus-folder, and inline-rename interactions keep their normal behavior

#### Scenario: Adding a second folder refreshes existing rows
- **WHEN** a workspace section already shows one top-level workspace folder
- **AND** the user adds a second folder to the same workspace
- **THEN** both top-level workspace folder rows expose visible reorder handles without requiring a sidebar hide/show, scope change, manual refresh, or app restart
- **AND** descendant file and directory rows still do not expose workspace-folder reorder handles

#### Scenario: Removing back to one folder hides stale handles
- **WHEN** a workspace section shows two top-level workspace folders with reorder handles
- **AND** the user removes either folder from that workspace
- **THEN** the remaining top-level workspace folder does not expose a reorder handle
- **AND** no stale drag shield, insertion indicator, or valid-drop state remains on the row

#### Scenario: Removing all folders returns to empty state
- **WHEN** a workspace section shows one or more top-level workspace folders
- **AND** the user removes every folder from that workspace
- **THEN** the section shows its empty folder-set state
- **AND** no reorder handle, insertion indicator, or fake folder row remains visible
- **AND** the workspace header actions remain reachable

#### Scenario: Reordering keeps all top-level handles synchronized
- **WHEN** a workspace contains folders A, B, and C
- **AND** the user reorders them through drag-and-drop or the non-pointer Move Up/Move Down path
- **THEN** the visible folder order matches the persisted workspace folder order after the reorder completes
- **AND** every top-level workspace folder row still exposes a reorder handle
- **AND** no descendant row exposes a workspace-folder reorder handle

#### Scenario: Row recycling does not leak reorder state
- **WHEN** a realized row that previously represented a reorderable top-level workspace folder is rebound to a descendant file row, descendant directory row, one-folder workspace row, or empty-state-adjacent row
- **THEN** the rebound row does not expose a workspace-folder reorder handle
- **AND** it has no visible insertion indicator or stale valid-drop state
- **AND** ordinary row activation, disclosure, peek, context-menu, focus-folder, and inline-rename interactions match the newly bound row

#### Scenario: Collapsed section reflects membership changes when expanded
- **WHEN** a workspace section is collapsed
- **AND** folders are added, removed, or reordered in that workspace
- **AND** the user expands the section
- **THEN** the visible folder order and reorder handles match the current configured folder set immediately
- **AND** the section collapse state does not mutate folder membership or folder order

#### Scenario: Scope filtering preserves synchronized handles
- **WHEN** the user switches between `All workspaces` and a concrete workspace scope
- **AND** a hidden workspace section becomes visible again
- **THEN** each visible top-level workspace folder row exposes a reorder handle exactly when that workspace has at least two configured folders
- **AND** one-folder and zero-folder workspaces remain free of reorder handles

#### Scenario: Dense and awkward folder sets keep controls reachable
- **WHEN** a workspace contains many folders, long folder names, overlapping folder paths, or is shown at a constrained sidebar width
- **THEN** reorder handles, folder labels, disclosure controls, context menus, add-folder, refresh, and workspace header actions remain reachable without overlapping
- **AND** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** dense folder rows scroll only in the workspace-section list area below the fixed workspace selector row

#### Scenario: Invalid reorder targets stay rejected after synchronization
- **WHEN** the user drags a top-level workspace folder over a descendant file row, descendant directory row, focused drill-down row, one-folder workspace row, empty workspace section, or a row in another workspace
- **THEN** the sidebar shows no valid insertion indicator for that target
- **AND** dropping there is rejected
- **AND** folder membership, folder order, and filesystem contents remain unchanged

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

### Requirement: Workspace sidebar shares the side-rail tone while preserving navigation behavior
The system SHALL render the workspace sidebar on the same coordinated side-rail tone used by the document properties surface. The sidebar MUST keep its navigation-tree presentation: the fixed workspace selector row remains visible, workspace section headers remain the workspace-level control surface, file-tree rows keep their navigation sidebar behavior, dense folder lists scroll only below the fixed selector, and the sidebar MUST NOT gain horizontal scrolling or inspector-style grouped cards as part of this styling change.

#### Scenario: No-workspace and empty-workspace states keep controls reachable
- **WHEN** the app starts with no workspaces or with a workspace that has zero folders
- **THEN** the workspace sidebar uses the shared side-rail tone
- **AND** the fixed workspace selector and new-workspace control remain visible
- **AND** any empty folder-set state remains readable without introducing a fake folder row

#### Scenario: Representative workspace tree keeps navigation styling
- **WHEN** the sidebar shows a workspace with one or more configured folders
- **THEN** top-level folder rows and descendant file rows keep their existing navigation sidebar presentation
- **AND** disclosure, selection, context-menu, file activation, file peek, focus-folder, and inline rename interactions remain visually reachable
- **AND** the rows are not restyled as document-properties inspector cards

#### Scenario: Dense and awkward workspace names preserve constrained geometry
- **WHEN** the sidebar contains many folders, long workspace names, long folder names, duplicate basenames, or overlapping folder paths
- **THEN** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** dense rows scroll only in the workspace-section list area below the fixed selector
- **AND** labels, drag handles, disclosure controls, add-folder, refresh, and workspace header actions do not overlap

#### Scenario: Reorder and hover states remain legible
- **WHEN** workspace folder reorder handles, insertion indicators, row hover states, row selection states, or drop shields are visible
- **THEN** those states remain legible against the shared side-rail tone
- **AND** the styling change does not introduce filled row drop highlights, duplicate insertion feedback, or drop-into-folder cues

#### Scenario: Sidebar and document properties feel coordinated when both are visible
- **WHEN** the window is spacious enough to show the workspace sidebar, editor content, and document properties pane at the same time
- **THEN** the two side surfaces use coordinated side-rail styling
- **AND** the editor content remains visually distinct from the side rails
- **AND** each side surface keeps its own content idiom: workspace as navigation, document properties as inspector

### Requirement: Workspace sidebar toggles visibly animate across adaptive widths
The system SHALL show an observable workspace-sidebar transition whenever the user toggles the workspace panel between shown and hidden states. The transition MUST remain visible at collapsed overlay widths, intermediate desktop widths where adaptive secondary-surface decisions can change, and wide desktop widths where the sidebar consumes layout width. Requested visibility, rendered visibility, action state, focus restoration, minimap protection, and persisted settings MUST remain consistent before, during, and after the transition.

#### Scenario: Intermediate desktop show transition animates
- **WHEN** the window is approximately `1100sp` wide, the workspace sidebar is hidden, the selected workspace sidebar preset is `Comfy`, and the user toggles the workspace panel on
- **THEN** the workspace sidebar becomes requested visible
- **AND** the user can observe the sidebar moving from hidden toward fully visible rather than appearing only at the final `x == 0` position
- **AND** final geometry settles with the workspace sidebar fully visible at the expected `Comfy` width for that window
- **AND** document-properties presentation reconciliation does not collapse the workspace-sidebar transition into an immediate jump

#### Scenario: Intermediate desktop hide transition animates
- **WHEN** the window is approximately `1100sp` wide, the workspace sidebar is shown, the selected workspace sidebar preset is `Comfy`, and the user toggles the workspace panel off
- **THEN** the workspace sidebar becomes requested hidden
- **AND** the user can observe the sidebar moving from fully visible toward hidden rather than disappearing only after an immediate endpoint jump
- **AND** final geometry settles with the workspace sidebar hidden and the editor/content area in its expected final allocation

#### Scenario: Collapsed overlay transition remains visible
- **WHEN** the window is at or below the workspace collapsed-width breakpoint and the user explicitly toggles the workspace panel
- **THEN** the workspace panel uses the collapsed overlay presentation
- **AND** the show or hide transition remains visible
- **AND** persistent chrome, the status-bar toggle, and any focused editor content remain reachable after the transition settles

#### Scenario: Wide desktop transition remains visible
- **WHEN** the window is wide enough for both the workspace sidebar and document properties to use side-by-side desktop presentation
- **AND** the user toggles the workspace panel
- **THEN** the workspace sidebar transition remains visible while layout width is consumed or released
- **AND** the document-properties pane keeps its requested visibility and final allocation policy
- **AND** no unrelated secondary surface opens or closes solely to make the workspace animation possible

#### Scenario: Sidebar content extremes do not suppress animation
- **WHEN** the workspace sidebar contains no workspaces, one representative workspace, or many workspaces with long folder names and deep visible trees
- **AND** the user toggles the workspace panel
- **THEN** the show or hide transition remains visible
- **AND** the fixed workspace scope row, workspace creation affordance, section headers, and item-region scrolling contracts remain intact after the transition settles
- **AND** the sidebar does not gain an unintended horizontal scrollbar, fake row, or overlapping controls because of the animation path

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
