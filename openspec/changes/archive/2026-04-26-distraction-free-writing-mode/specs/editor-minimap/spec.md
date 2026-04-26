## ADDED Requirements

### Requirement: Focus Mode temporarily hides the minimap
The system SHALL suppress editor minimap rendering while Focus Mode is active, regardless of the user's saved minimap preference. Focus Mode MUST NOT change the saved minimap preference, and normal minimap availability MUST resume when Focus Mode exits.

#### Scenario: Enabled minimap hides while focused
- **WHEN** the user's minimap preference is enabled and a supported editor page shows the minimap
- **AND** the user enters Focus Mode
- **THEN** the minimap is hidden
- **AND** the saved minimap preference remains enabled

#### Scenario: Minimap restores after focus
- **WHEN** Focus Mode is active after hiding an enabled minimap
- **AND** the user exits Focus Mode
- **THEN** the minimap renders again for supported editor pages
- **AND** unsupported editor pages continue to follow normal minimap availability rules

#### Scenario: Disabled minimap remains disabled after focus
- **WHEN** the user's minimap preference is disabled
- **AND** the user enters and exits Focus Mode
- **THEN** the minimap remains disabled
