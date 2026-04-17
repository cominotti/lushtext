## ADDED Requirements

### Requirement: Transparency-derived style schemes are cached under app data and regenerated safely
The system SHALL cache derived opacity-aware GtkSourceView style schemes under `$XDG_DATA_HOME/lushtext/style-schemes/` when non-opaque tab-content transparency needs a scheme variant that does not already exist in the active style-scheme search path. Missing cached derived schemes MUST regenerate automatically instead of disabling the transparency feature.

#### Scenario: First non-opaque use creates or reuses a derived cached scheme
- **WHEN** the user applies a transparency value below `100%` for a base style scheme that does not already have the required derived opacity variant loaded
- **THEN** the system creates or reuses a derived style-scheme file under the app data directory
- **AND** the editor can apply that derived scheme for the current tab-content transparency value

#### Scenario: Deleted derived cache is regenerated on demand
- **WHEN** a previously used derived transparency style-scheme file is missing from the app data cache
- **THEN** the system regenerates the needed derived scheme automatically the next time that opacity variant is required
- **AND** tab-content transparency continues to work without requiring the user to reset the preference
