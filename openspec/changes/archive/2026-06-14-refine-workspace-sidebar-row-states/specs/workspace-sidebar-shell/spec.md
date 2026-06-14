## ADDED Requirements

### Requirement: Workspace file-tree rows separate interaction feedback from app state
The system SHALL render workspace file-tree row interaction feedback as transient hover, press, and keyboard-focus affordances rather than as a persistent selected-row fill after ordinary pointer clicks. The file tree MAY keep internal selection state for keyboard navigation, activation, file peek, refresh preservation, and accessibility, but that internal selection MUST NOT make a clicked row look like the active document or current application state after the pointer interaction has ended.

#### Scenario: Pointer click does not leave sticky row emphasis
- **WHEN** the user clicks a workspace folder row or descendant file-tree row
- **AND** the pointer is no longer hovering or pressing that row
- **THEN** the row does not retain a persistent selected-row fill solely because it was clicked
- **AND** rows not representing an open or active tab return to the ordinary file-tree presentation
- **AND** directory disclosure, file activation, context-menu targeting, inline rename, and focus-folder behavior remain unchanged

#### Scenario: Hover and press remain visible but temporary
- **WHEN** the pointer hovers over or presses a workspace file-tree row
- **THEN** the row shows an appropriate transient hover or press affordance
- **AND** the affordance clears when the pointer leaves or the press completes
- **AND** the row height, disclosure icon position, label position, and context-menu target do not shift because of the transient affordance

#### Scenario: Keyboard focus remains visible for peek and navigation
- **WHEN** keyboard navigation moves the file-tree focus target to a file row
- **THEN** the row remains visibly focusable enough for keyboard users to understand where Space-to-peek and activation will apply
- **AND** pressing Space on that focused or internally selected file row opens the existing read-only peek surface for that file
- **AND** Escape, repeated Space, non-file selection, section rebuild, and workspace-filter hide still dismiss peek through the existing behavior

#### Scenario: Empty and no-required-context states avoid fake row emphasis
- **WHEN** the sidebar has no workspaces or a visible workspace section has zero folders
- **THEN** the fixed workspace selector and workspace header actions remain reachable
- **AND** the empty state remains readable
- **AND** the sidebar does not render a fake selected, open, active, selectable, or expandable file-tree row to represent missing content

### Requirement: Workspace file-tree rows indicate open and active tab files
The system SHALL show a persistent but restrained indicator on file rows whose file identity matches an open tab, and SHALL show a slightly stronger restrained indicator on file rows whose file identity matches the currently active tab. These indicators MUST apply only to file rows, MUST be derived from the window-owned open-tab state, MUST update without requiring sidebar hide/show, manual refresh, scope changes, or app restart, and MUST NOT replace the tab strip as the primary active-document surface.

#### Scenario: Opening a file marks matching file rows
- **WHEN** the user opens a file from the workspace file tree
- **THEN** every visible file row that resolves to that open file identity shows the open-file indicator
- **AND** the file row matching the active tab shows the active-file indicator
- **AND** parent folders, sibling files, descendant directories, placeholders, and empty-folder-set states do not show an open-file indicator

#### Scenario: Switching tabs updates active row indication
- **WHEN** two saved files are open in tabs
- **AND** both files are visible in the workspace file tree
- **AND** the user switches the active tab from file A to file B
- **THEN** file A keeps the open-file indicator and loses the active-file indicator
- **AND** file B shows the active-file indicator
- **AND** the update appears on realized rows without requiring the file tree to rebind or refresh

#### Scenario: Closing or failed-opening a tab removes stale open indication
- **WHEN** a file row shows an open-file or active-file indicator
- **AND** the corresponding tab is closed or the first load for that file fails and the path is removed from the open-tab set
- **THEN** the matching file row no longer shows open-file or active-file indication
- **AND** no stale indicator remains on recycled row widgets
- **AND** the row's ordinary activation, disclosure, peek, context-menu, and inline-rename behavior still matches the newly bound row

#### Scenario: Save As and sidebar rename move the indicator
- **WHEN** a file-backed tab changes path through Save As or an in-app sidebar rename
- **THEN** the old file path row no longer shows open-file or active-file indication
- **AND** the new file path row shows the appropriate open-file or active-file indication when visible
- **AND** the update uses the same path and canonical identity semantics as duplicate-tab detection where those identities are available

#### Scenario: Deleting a file or folder closes affected tabs and clears indicators
- **WHEN** the user deletes a file or directory from the workspace file tree
- **AND** the delete workflow closes tabs whose paths match that file or live under that directory
- **THEN** rows for those closed file identities no longer show open-file or active-file indicators
- **AND** unaffected open file rows keep their indicators

#### Scenario: Session restore marks restored file tabs
- **WHEN** the app restores saved file-backed tabs from session data
- **AND** the workspace sidebar later materializes matching file rows
- **THEN** restored open files show open-file indicators
- **AND** the restored active tab's file row shows the active-file indicator when visible
- **AND** indicators do not appear for untitled tabs or failed restored file loads

#### Scenario: Overlapping folder trees mark every matching visible row
- **WHEN** a workspace contains overlapping folders that can show the same open file in more than one visible file-tree row
- **THEN** every visible row that resolves to the open file identity shows the open-file indicator
- **AND** every visible row that resolves to the active file identity shows the active-file indicator
- **AND** the sidebar does not hide duplicate file rows merely to simplify indicator presentation

#### Scenario: Dense and constrained trees keep indicators readable
- **WHEN** the sidebar contains many files, long file names, deep paths, overlapping folders, or is shown at a constrained width
- **THEN** open-file and active-file indicators remain visible without overlapping labels, disclosure icons, drag handles, focus-folder controls, or context-menu targets
- **AND** row height remains stable across hover, focus, open, active, and recycled states
- **AND** the sidebar preserves its no-horizontal-scrollbar contract
- **AND** dense file rows scroll only in the workspace-section list area below the fixed workspace selector row

#### Scenario: Row recycling clears old tab-state presentation
- **WHEN** a realized row widget that previously represented an open or active file is rebound to a closed file, folder, placeholder, empty-state-adjacent row, or a file from another workspace section
- **THEN** the rebound row does not keep stale open-file or active-file styling
- **AND** if the rebound row represents a different open or active file, it shows only the indicator appropriate for that new file identity
- **AND** ordinary row hover, press, focus, activation, disclosure, peek, context-menu, and inline-rename behavior match the newly bound row
