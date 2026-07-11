## ADDED Requirements

### Requirement: Recent Open uses shared fuzzy scoring within explicit tiers
The system SHALL rank non-empty Recent Open queries through case-insensitive Prefix, Substring, and Fuzzy tiers in that order. Fuzzy-tier matches MUST use the same GTK-free nucleo query configuration as the command palette and MUST sort by descending nucleo score before recency and path tie-breaks.

#### Scenario: Prefix outranks stronger fuzzy candidate
- **WHEN** one recent row has a case-insensitive field prefix match and another has only a high-scoring fuzzy match
- **THEN** the prefix row appears first
- **AND** nucleo score does not cross the explicit tier boundary

#### Scenario: Substring outranks fuzzy candidate
- **WHEN** one recent row contains the query as a case-insensitive substring and another matches only as a subsequence
- **THEN** the substring row appears first

#### Scenario: Better fuzzy score outranks newer weak fuzzy match
- **WHEN** two recent rows match only in the fuzzy tier and one receives a higher nucleo score
- **THEN** the higher-scoring row appears first
- **AND** recency is consulted only after equal fuzzy score

#### Scenario: Best matching field determines the row tier
- **WHEN** a row matches fuzzily by title but as a substring in its subtitle or path
- **THEN** the row receives the Substring tier
- **AND** it is not ranked only from its weaker title match

### Requirement: Recent ranking remains deterministic and bounded
The system SHALL preserve newest-first results for an empty trimmed query, the 200-entry recent-history cap, open-tab exclusion, and no-result behavior. Equal non-empty ranks MUST sort by descending last-opened timestamp and then ascending path so repeated searches return stable ordering.

#### Scenario: Empty query stays newest-first
- **WHEN** the Open popover query is empty or whitespace-only
- **THEN** eligible recent rows are ordered newest first
- **AND** equal timestamps are ordered deterministically by path
- **AND** no fuzzy scorer is required to produce the list

#### Scenario: Equal fuzzy ranks use recency
- **WHEN** two rows have the same fuzzy tier and nucleo score
- **THEN** the more recently opened row appears first

#### Scenario: Equal scores and timestamps use path
- **WHEN** two rows have equal tier, fuzzy score, and last-opened timestamp
- **THEN** ascending path order determines their stable order

#### Scenario: No candidate matches
- **WHEN** prefix, substring, and shared fuzzy scoring reject every eligible recent row
- **THEN** the Open popover shows its existing no-results state
- **AND** the file chooser and search controls remain reachable

### Requirement: Shared fuzzy abstraction remains GTK-free and concrete
The project SHALL keep reusable nucleo query state in a GTK-free service module as one concrete helper. Palette and Recent Open MAY apply different higher-level tier or grouping policies, but MUST use the same case and normalization configuration for true fuzzy score calculation. The change MUST NOT introduce a generic matcher trait, global mutable matcher, or UI dependency.

#### Scenario: Palette and recents score the same candidate
- **WHEN** a cross-surface fixture passes the same non-empty query and candidate to palette fuzzy scoring and Recent Open's fuzzy tier
- **THEN** both receive the same nucleo match acceptance and score
- **AND** each surface may still apply its own surrounding ordering policy

#### Scenario: Independent queries do not share mutable state
- **WHEN** palette and Recent Open searches run with different queries
- **THEN** each query owns its matcher and conversion buffer
- **AND** one search cannot change another search's score results

### Requirement: Fuzzy ranking coverage includes realistic state extremes
The project SHALL add pure service and Open-popover regression tests for empty, one-row, representative, many-row, no-result, Unicode, composed/decomposed text, mixed case, deep and awkward paths, equal timestamps, equal scores, and all-recents-open states.

#### Scenario: Unicode and awkward paths remain searchable
- **WHEN** recent titles or paths contain accented Unicode, spaces, symbols, or deep components
- **THEN** matching follows the shared case and normalization policy
- **AND** result rows preserve their existing readable, ellipsized presentation

#### Scenario: All matching recents are already open
- **WHEN** ranking would match rows but open-tab exclusion removes every one
- **THEN** the popover shows its established empty eligible state
- **AND** ranking does not reintroduce open documents as fake recent rows
