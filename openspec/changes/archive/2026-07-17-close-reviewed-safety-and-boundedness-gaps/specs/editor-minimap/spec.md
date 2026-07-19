## ADDED Requirements

### Requirement: Large minimap analysis yields and rejects stale generations
For documents within the existing minimap-supported tier, wrapped-layout and long-line analysis SHALL use cached evidence or bounded GTK snapshot or iterator slices. No scheduled turn may inspect more than the configured character or item budget, and every analysis session MUST carry editor lifetime and minimap-analysis generation. Buffer edits, wrap changes, marker preferences, file replacement, or page teardown MUST invalidate stale analysis before another slice or projection is accepted.

#### Scenario: Wrapped many-short-line document exceeds one slice
- **WHEN** the minimap is enabled with wrapping for a supported multi-megabyte document containing many short lines
- **THEN** layout analysis runs over bounded GTK turns rather than scanning the complete buffer in one callback
- **AND** the minimap remains visible while supported analysis reaches its current terminal state

#### Scenario: Edit supersedes active analysis
- **WHEN** the document changes after one or more analysis slices but before terminal publication
- **THEN** the stale generation stops before applying layout or marker results
- **AND** only the latest generation may update minimap availability, cache, or warnings

#### Scenario: Long-line markers reuse bounded current evidence
- **WHEN** optional long-line warnings and wrapped-layout availability require overlapping document analysis
- **THEN** they reuse current cached or sliced evidence instead of performing separate full-buffer scans or copies
- **AND** disabling markers releases marker-only state without invalidating unrelated minimap features

#### Scenario: Unsupported size tier remains explicit
- **WHEN** a document exceeds the existing minimap-supported file-size tier
- **THEN** the editor keeps the saved minimap preference and shows the existing unavailable feedback
- **AND** the bounded-analysis workflow does not introduce a new lower byte-only hide threshold
