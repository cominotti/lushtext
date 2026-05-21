## MODIFIED Requirements

### Requirement: Workspace sections expose a manual refresh control
The system SHALL show a `Refresh` button in each workspace-section header as the rightmost header-control button, and invoking it MUST refresh that workspace section using the same tree-reload behavior as automatic refresh. The refresh control MUST remain available without any adjacent replace-root control.

#### Scenario: Refresh button placement in the header
- **WHEN** a workspace section header is rendered
- **THEN** it shows a `Refresh` control in the rightmost header-control position
- **AND** no replace-root control appears to the right of it

#### Scenario: Manual refresh reloads stale content
- **WHEN** the user activates the `Refresh` control for a workspace section whose tree is stale
- **THEN** that workspace section reloads its tree from disk
- **AND** newly added, removed, or renamed paths appear in the refreshed result
