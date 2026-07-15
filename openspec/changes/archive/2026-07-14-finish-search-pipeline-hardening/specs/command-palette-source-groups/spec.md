## ADDED Requirements

### Requirement: Command palette retains only bounded top results while scoring
For each command-palette source, the system SHALL retain at most the configured result limit while scoring candidates. Results MUST be ordered by descending fuzzy score with source ordinal as the deterministic equal-score tie-break, and grouped output MUST preserve existing source priority and canonical-file deduplication.

#### Scenario: Many candidates match one source
- **WHEN** a query matches more source items than the configured result limit
- **THEN** scoring retains only a bounded top-result structure
- **AND** the final rows equal the highest-ranked results under the defined score and tie order

#### Scenario: Equal-score candidates are repeated
- **WHEN** several candidates receive the same fuzzy score
- **THEN** their relative order follows source ordinal deterministically
- **AND** repeated identical queries return the same rows and order

#### Scenario: Empty query uses source order
- **WHEN** the query is empty
- **THEN** each source returns its first bounded items in source order
- **AND** no full-source result collection is materialized

### Requirement: Palette source behavior survives bounded selection
Bounded selection MUST preserve open-tab precedence, workspace-scope labels, note-category order, command grouping, and duplicate suppression across sources.

#### Scenario: Open tab and workspace file both match
- **WHEN** the same canonical file is selected into both source-local top sets
- **THEN** grouped output retains only the `Open Tabs` row
- **AND** bounded selection does not reintroduce the workspace duplicate

#### Scenario: Mixed All-mode sources exceed their limits
- **WHEN** open tabs, workspace files, notes, and commands all have more matches than their per-source limits
- **THEN** each group remains individually bounded
- **AND** group ordering and category labels remain unchanged
