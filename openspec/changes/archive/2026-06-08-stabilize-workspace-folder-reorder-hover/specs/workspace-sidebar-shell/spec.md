## MODIFIED Requirements

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
