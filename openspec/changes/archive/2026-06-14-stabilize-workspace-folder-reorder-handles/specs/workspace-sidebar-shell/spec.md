## ADDED Requirements

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
