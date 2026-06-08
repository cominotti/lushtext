## ADDED Requirements

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
