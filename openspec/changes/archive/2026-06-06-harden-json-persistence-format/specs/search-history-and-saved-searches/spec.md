## ADDED Requirements

### Requirement: Saved searches use recovery-aware v1 JSON persistence
The system SHALL persist user-managed saved searches as a supported v1 app-owned JSON envelope. Saved-search loading MUST preserve unsupported or malformed existing metadata before replacing it with default saved-search state.

#### Scenario: Save saved searches as v1
- **WHEN** the user creates, updates, or deletes a saved search
- **THEN** `saved-searches.json` is written as a pretty JSON envelope with the saved-searches document kind
- **AND** each saved search preserves its display name and full query state in the payload

#### Scenario: Unsupported saved searches are preserved
- **WHEN** the search panel loads unsupported or malformed saved-search metadata
- **THEN** the original metadata is quarantined or preserved in place when replacement is unsafe
- **AND** the panel remains usable for new searches while reporting a grouped recovery diagnostic

## MODIFIED Requirements

### Requirement: Missing or unreadable persisted search memory degrades to empty state
The system SHALL treat missing `search-history.json` and missing `saved-searches.json` files as ordinary empty state. Recent search history remains low-value ephemeral state: unreadable, malformed, or unsupported recent-history data MAY degrade to an empty recent-history list with a diagnostic. Saved searches are user-managed state: unreadable, malformed, or unsupported `saved-searches.json` data MUST be preserved before replacement and MUST produce recovery diagnostics rather than being silently discarded.

#### Scenario: Missing persisted search memory yields empty lists
- **WHEN** the search panel starts without existing persisted search-history or saved-search files
- **THEN** the recent-history and saved-search lists start empty
- **AND** the panel remains usable

#### Scenario: Corrupt recent search memory does not break the search panel
- **WHEN** the search panel encounters unreadable, malformed, or unsupported persisted recent-history data
- **THEN** the recent-history list falls back to an empty state with a diagnostic
- **AND** the search panel remains usable for new searches and future persistence

#### Scenario: Corrupt saved-search memory is preserved
- **WHEN** the search panel encounters unreadable, malformed, or unsupported saved-search data
- **THEN** the affected saved-search metadata is preserved or left untouched when preservation fails
- **AND** the saved-search list may start empty only after replacement is safe
- **AND** the user receives grouped recovery feedback rather than silent loss
