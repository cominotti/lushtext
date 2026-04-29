# symbolic-files-root-icon Specification

## Purpose
Define the workspace sidebar icon boundary for the synthetic `Files` root row: the row is a structural landmark and stays symbolic, while actual file-tree directories and files keep regular themed content icons.

## Requirements
### Requirement: Synthetic Files root row uses a symbolic icon
The system SHALL render the normal single-directory workspace root row labeled `Files` with a symbolic workspace/navigation icon. The row MUST preserve the `Files` label and MUST NOT use the regular themed folder icon reserved for actual directory content rows.

#### Scenario: Single-directory workspace root row renders symbolically
- **WHEN** a workspace section renders its normal single-directory root row with the display label `Files`
- **THEN** the row icon is symbolic, preferably `view-list-symbolic`
- **AND** the row label remains `Files`

### Requirement: Actual file-tree content keeps regular themed icons
The system SHALL keep actual filesystem content rows on the regular themed icon rules introduced for file-tree content. Real directory rows MUST use a regular folder icon, and file rows MUST use their regular content-type or fallback file icon.

#### Scenario: Drill-down root keeps regular folder icon
- **WHEN** a workspace section renders a drill-down focused root row that displays the actual focused directory name
- **THEN** the row icon remains the regular themed folder icon
- **AND** the row is not treated as the synthetic `Files` root row

#### Scenario: File rows keep content-type icons
- **WHEN** a workspace section renders a file row whose content type can be inferred from its path
- **THEN** the row icon remains the regular themed icon associated with that content type
- **AND** the row does not use the synthetic root row's symbolic icon

### Requirement: Root icon correction preserves file tree behavior
The system SHALL preserve existing file-tree behavior when changing the synthetic `Files` root row icon. Icon selection MUST NOT alter expansion, selection, refresh reconciliation, file peek, context menus, inline rename, or drill-down behavior.

#### Scenario: Synthetic root row interactions are unchanged
- **WHEN** the user interacts with the synthetic `Files` root row after this correction
- **THEN** expansion, selection, refresh, and available row actions behave as they did before this correction
- **AND** only the icon presentation changes
