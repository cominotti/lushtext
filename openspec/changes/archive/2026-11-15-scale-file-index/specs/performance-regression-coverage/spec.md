## MODIFIED Requirements

### Requirement: Bounded palette search has equivalence and scale coverage
The project SHALL prove bounded top-result selection and one-active/one-latest execution against a full-sort reference on representative and generated corpora. Coverage MUST include Unicode normalization, equal scores, empty queries, source deduplication, cancellation, rapid input, and the maximum indexed-file corpus (1,048,576 files).

#### Scenario: Bounded selector is compared with a reference
- **WHEN** generated candidate/query/result-limit combinations run through bounded and full-sort implementations
- **THEN** both produce the same selected identities and deterministic order
- **AND** the bounded implementation retains no more than the configured result count per source

#### Scenario: Maximum index receives rapid queries
- **WHEN** the 1,048,576-file fixture receives more queries than workers can complete
- **THEN** measurements report one active search, at most one pending query, cancellation progress, and final-query latency
- **AND** obsolete full-index jobs do not accumulate in the generic worker FIFO

#### Scenario: Diagnostic privacy regression is exercised
- **WHEN** an invalid Replace Preview range is generated under captured tracing output
- **THEN** typed invalid counts remain correct
- **AND** captured diagnostics contain none of the private source or replacement sentinel text

#### Scenario: The index arena agrees with the per-file reference
- **WHEN** generated trees, symlinks in both traversal orders and mutation sequences (each applied to the filesystem first) run through the arena index and the per-file reference
- **THEN** both list the same files, identities and order, and return the same rows and counts
- **AND** they report the same file-limit and directory-limit truncations, and under a byte limit the arena's files are a prefix of the reference's untruncated list
