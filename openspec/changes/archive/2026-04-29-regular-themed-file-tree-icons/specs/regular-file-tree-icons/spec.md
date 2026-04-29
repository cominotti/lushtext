## ADDED Requirements

### Requirement: File tree content rows use regular themed icons
The system SHALL render regular themed icons for actual filesystem content rows in each workspace section's file tree. Directory rows MUST use a regular folder icon, and file rows MUST use a regular file icon rather than the current symbolic-only generic file icon.

#### Scenario: Directory rows use regular folder icons
- **WHEN** a workspace section renders a directory row in the file tree
- **THEN** the row icon is resolved from the regular themed folder icon
- **AND** the row remains expandable or non-expandable according to the existing tree rules

#### Scenario: File rows do not use the symbolic generic file icon
- **WHEN** a workspace section renders a file row in the file tree
- **THEN** the row icon is resolved from a regular themed file icon
- **AND** the row does not use `text-x-generic-symbolic` as its normal file icon

### Requirement: File icons follow platform content types with fallbacks
The system SHALL derive file-row icons from platform content-type metadata when a content type can be inferred from the file path. If the content type cannot be inferred or no usable regular themed icon is available, the system MUST fall back to a regular generic text/file icon without showing a missing-icon placeholder.

#### Scenario: Known file type uses content-type icon
- **WHEN** the file tree renders a file path whose content type can be inferred from its name or extension
- **THEN** the row icon uses the regular themed icon associated with that content type
- **AND** no file contents are read from disk solely to choose the row icon

#### Scenario: Unknown file type uses regular fallback
- **WHEN** the file tree renders a file path whose content type is unknown or whose themed icon cannot be used
- **THEN** the row icon falls back to a regular generic file icon
- **AND** the row does not render GTK's missing-icon placeholder

### Requirement: Non-content sidebar affordances remain symbolic
The system SHALL keep symbolic icons for sidebar controls and non-content status rows. The regular themed icon behavior MUST apply only to actual filesystem content rows in the file tree.

#### Scenario: Sidebar controls remain symbolic
- **WHEN** the sidebar renders controls such as New Workspace, Refresh, Replace Workspace Root, drill-down back, or Focus Folder
- **THEN** those controls continue to use symbolic icons
- **AND** their actions, tooltips, visibility, and placement remain unchanged

#### Scenario: Placeholder rows remain symbolic status rows
- **WHEN** the file tree renders a synthetic placeholder or informational row instead of a real filesystem path
- **THEN** that row continues to use a symbolic status/information icon
- **AND** it is not treated as a file or directory content row for regular icon selection

### Requirement: Icon changes preserve file tree behavior
The system SHALL preserve existing file tree interaction behavior when row icons change. Icon selection MUST NOT alter row expansion, selection, sorting, inline rename, file peek, context menus, refresh reconciliation, or workspace filtering.

#### Scenario: Row interactions survive regular icon binding
- **WHEN** the user interacts with a file-tree row after regular icons are enabled
- **THEN** selection, activation, context menu actions, file peek, and inline rename behave as they did before this change
- **AND** the icon does not consume input or change the row's layout contract beyond the existing icon slot

#### Scenario: Refresh keeps regular icon presentation
- **WHEN** a workspace section refreshes visible file-tree rows after file or directory changes
- **THEN** refreshed directory and file rows continue to use the regular themed icon rules
- **AND** unchanged row expansion and selection restoration behavior remains governed by the existing refresh contract
