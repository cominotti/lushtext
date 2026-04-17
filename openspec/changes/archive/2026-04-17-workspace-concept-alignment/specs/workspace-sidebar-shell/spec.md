## ADDED Requirements

### Requirement: Sidebar keeps a fixed workspace scope row
The system SHALL render a fixed top sidebar row above the scrollable workspace-section list. That row MUST contain the workspace scope selector and the `New Workspace` affordance, and it MUST remain visible while workspace sections scroll. The selector MUST offer the explicit aggregate scope `All workspaces` plus one item per restored workspace.

#### Scenario: Populated sidebar keeps the scope row pinned
- **WHEN** one or more workspaces are restored and the workspace-section list becomes vertically scrollable
- **THEN** the scope selector row remains visible above the scroll area
- **AND** scrolling the workspace sections does not scroll that top row away

#### Scenario: Scope selector lists aggregate and concrete workspace scopes
- **WHEN** the sidebar renders with restored workspaces
- **THEN** the scope selector includes `All workspaces`
- **AND** it includes one additional option for each restored workspace section

### Requirement: Empty workspace state remains an explicit shell
The system SHALL treat the no-workspace state as an intentional empty sidebar shell. When no workspaces exist, the sidebar MUST render no workspace sections and MUST NOT create a visible placeholder workspace section solely to satisfy model defaults.

#### Scenario: First launch with no workspaces shows an empty shell
- **WHEN** the app launches without any persisted workspaces
- **THEN** the sidebar shows the fixed top scope row
- **AND** the sidebar renders zero workspace sections below it

#### Scenario: Removing the last workspace returns to the empty shell
- **WHEN** the user unlists the last remaining workspace
- **THEN** the sidebar returns to the fixed top scope row with no workspace sections
- **AND** no placeholder `New Workspace` section is inserted automatically

### Requirement: Each workspace section owns one root directory
The system SHALL render one sidebar section per workspace, and each section MUST correspond to exactly one persisted root directory. Each workspace-section header MUST expose `Refresh` immediately to the left of `Replace Workspace Root`, and the workspace header context menu MUST continue to expose rename and unlist actions for that one workspace.

#### Scenario: Restored workspaces create one section each
- **WHEN** the app restores multiple persisted workspaces
- **THEN** the sidebar renders one section per workspace
- **AND** each section owns one persisted root directory rather than a mixed set of roots

#### Scenario: Workspace header keeps section-local controls
- **WHEN** a workspace section header is rendered
- **THEN** it shows `Refresh` immediately to the left of `Replace Workspace Root`
- **AND** its header context menu exposes `Rename Workspace` and `Unlist Workspace`

### Requirement: Drill-down navigation stays local to the current workspace section
The system SHALL keep deep folder drill-down as temporary section-local navigation state. Focusing a descendant folder MUST re-root only that workspace section, MUST reveal a back affordance for the focused lineage, and MUST NOT mutate the workspace's persisted root directory.

#### Scenario: Focus Folder re-roots one section temporarily
- **WHEN** the user focuses a nested folder inside a workspace section
- **THEN** only that workspace section re-roots itself to the focused folder
- **AND** the sidebar reveals a back affordance showing the focused drill-down lineage
- **AND** the workspace's persisted root directory remains unchanged

#### Scenario: Navigating back restores the previous workspace view
- **WHEN** the user activates the drill-down back affordance
- **THEN** the workspace section returns to the previous focused folder or original workspace root
- **AND** the section restores the broader workspace tree without redefining the workspace itself
