# search-history-and-saved-searches Specification

## Purpose
TBD - created by archiving change data-home-persistence-contracts. Update Purpose after archive.
## Requirements
### Requirement: Successful searches persist bounded recent-history entries
The system SHALL persist recent search history to `$XDG_DATA_HOME/lushtext/search-history.json`. Recent history MUST store only non-empty successful searches, MUST deduplicate identical query-state entries by moving them to the front, and MUST retain at most 20 entries.

#### Scenario: Successful search adds a recent-history entry
- **WHEN** a non-empty search completes successfully with one or more results
- **THEN** the current query state is added to recent search history
- **AND** the newest entry appears at the front of the recent-history list

#### Scenario: Repeating the same search does not create a duplicate recent-history row
- **WHEN** the user repeats a search whose full query state already exists in recent history
- **THEN** the existing history entry moves to the front
- **AND** the recent-history list does not gain a duplicate entry

#### Scenario: Empty or unsuccessful searches are not persisted as recent history
- **WHEN** the query is empty or the search completes without successful result state to persist
- **THEN** the system does not add a new recent-history entry

### Requirement: Recent-history entries restore the full query state
The system SHALL persist enough recent-history state to reconstruct the query text, search toggles, and optional glob filter.

#### Scenario: Restoring from recent history rehydrates the search state
- **WHEN** the user restores a search from the recent-history list
- **THEN** the search entry, toggle states, and glob filter are restored to the persisted values
- **AND** the panel can rerun that query using the restored state

### Requirement: Saved searches persist until explicitly deleted
The system SHALL persist user-managed saved searches to `$XDG_DATA_HOME/lushtext/saved-searches.json` until the user explicitly deletes them. Saved searches MUST preserve their display name and full query state across app restarts.

#### Scenario: Saved search survives restart
- **WHEN** the user names and saves a search and later restarts the app
- **THEN** the saved search still appears in the saved-search list
- **AND** restoring it rehydrates the saved query state

#### Scenario: Deleting a saved search removes it from persisted state
- **WHEN** the user deletes a saved search
- **THEN** that saved search is removed from the saved-search list
- **AND** it does not reappear after restart

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

### Requirement: Missing or unreadable persisted search memory degrades to empty state
The system SHALL treat missing `search-history.json` and missing `saved-searches.json` files as ordinary empty state. Recent search history remains low-value ephemeral state: unreadable, malformed, or unsupported recent-history data MAY degrade to an empty recent-history list with a diagnostic. Saved searches are user-managed state: unreadable, malformed, or unsupported `saved-searches.json` data MUST be preserved before replacement and MUST produce recovery diagnostics rather than being silently discarded.

#### Scenario: Missing persisted search memory yields empty lists
- **WHEN** the search panel starts without existing persisted search-history or saved-search files
- **THEN** the recent-history and saved-search lists start empty
- **AND** the panel remains usable

#### Scenario: Corrupt persisted search memory does not break the search panel
- **WHEN** the search panel encounters unreadable, malformed, or unsupported persisted recent-history data
- **THEN** the recent-history list falls back to an empty state with a diagnostic
- **AND** the search panel remains usable for new searches and future persistence

#### Scenario: Corrupt saved-search memory is preserved
- **WHEN** the search panel encounters unreadable, malformed, or unsupported saved-search data
- **THEN** the affected saved-search metadata is preserved or left untouched when preservation fails
- **AND** the saved-search list may start empty only after replacement is safe
- **AND** the user receives grouped recovery feedback rather than silent loss
