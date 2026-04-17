## ADDED Requirements

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

### Requirement: Missing or unreadable persisted search memory degrades to empty state
The system SHALL treat missing or unreadable `search-history.json` and `saved-searches.json` files as recoverable state loss rather than a panel failure.

#### Scenario: Missing persisted search memory yields empty lists
- **WHEN** the search panel starts without existing persisted search-history or saved-search files
- **THEN** the recent-history and saved-search lists start empty
- **AND** the panel remains usable

#### Scenario: Corrupt persisted search memory does not break the search panel
- **WHEN** the search panel encounters unreadable persisted search-history or saved-search data
- **THEN** the affected list falls back to an empty state
- **AND** the search panel remains usable for new searches and future persistence
