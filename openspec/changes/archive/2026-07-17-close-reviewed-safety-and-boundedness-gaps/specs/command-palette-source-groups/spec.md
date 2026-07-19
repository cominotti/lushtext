## ADDED Requirements

### Requirement: Note scoring prunes only non-contributing body work
Command-palette note scoring SHALL preserve the current eligible rows, fuzzy scores, deterministic source-ordinal tie order, category grouping, Unicode behavior, and cancellation semantics. The scorer MAY skip a note body only after another searchable field has established eligibility and the scoring policy proves that the body cannot improve that row's result. A body MUST still be searched when it can establish eligibility or improve ordering.

#### Scenario: Metadata match already dominates a large body
- **WHEN** note title, path, workspace, line metadata, or another searchable metadata field establishes the row's best score and the body cannot exceed it
- **THEN** the scorer does not scan the body
- **AND** the row keeps the same score, identity, group, and deterministic position as the unpruned reference

#### Scenario: Query matches only the note body
- **WHEN** no searchable metadata field matches but the query appears in the note body
- **THEN** the body remains eligible for bounded scoring
- **AND** the matching row is neither pruned nor reordered solely by the optimization

#### Scenario: Unicode and equal scores remain equivalent
- **WHEN** generated notes contain Unicode text, empty fields, equal metadata and body scores, and source-order ties
- **THEN** optimized and unpruned reference scoring publish identical selected identities and order
- **AND** per-source result retention remains within the configured top-result bound

#### Scenario: New query cancels a pruned or body-scanning pass
- **WHEN** a newer palette query supersedes scoring during metadata or body evaluation
- **THEN** the active scorer stops at the existing bounded cancellation checkpoint
- **AND** only the latest query may publish note rows or searching state
