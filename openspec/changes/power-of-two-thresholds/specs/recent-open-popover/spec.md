## MODIFIED Requirements

### Requirement: Recent ranking remains deterministic and bounded
The system SHALL preserve newest-first results for an empty trimmed query, the 256-entry recent-history cap (a retention tier rounded up from 200 by `power-of-two-thresholds`, so no stored entry is dropped on upgrade), open-tab exclusion, and no-result behavior. Equal non-empty ranks MUST sort by descending last-opened timestamp and then ascending path so repeated searches return stable ordering.

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
