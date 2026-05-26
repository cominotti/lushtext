## MODIFIED Requirements

### Requirement: Each workspace section owns one root directory
The system SHALL render one sidebar section per workspace, and each section MUST correspond to exactly one persisted root directory. Each workspace-section header MUST expose a single section-local `Refresh` control in the rightmost header-control position, and the workspace header context menu MUST continue to expose rename and remove actions for that one workspace. The system MUST NOT expose a workspace-section control that replaces the persisted root directory in place.

#### Scenario: Restored workspaces create one section each
- **WHEN** the app restores multiple persisted workspaces
- **THEN** the sidebar renders one section per workspace
- **AND** each section owns one persisted root directory rather than a mixed set of roots

#### Scenario: Workspace header keeps section-local controls
- **WHEN** a workspace section header is rendered
- **THEN** it shows `Refresh` as the rightmost header-control button
- **AND** it does not show a `Replace Workspace Root` control
- **AND** its header context menu exposes `Rename Workspace` and `Remove Workspace`

#### Scenario: Changing to a different folder uses workspace remove and add
- **WHEN** the user wants to use a different folder instead of an existing workspace root
- **THEN** the sidebar provides that path through removing the existing workspace and creating a new workspace
- **AND** the existing workspace's persisted root is not mutated in place
