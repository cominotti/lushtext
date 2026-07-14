## ADDED Requirements

### Requirement: Bounded palette search has equivalence and scale coverage
The project SHALL prove bounded top-result selection and one-active/one-latest execution against a full-sort reference on representative and generated corpora. Coverage MUST include Unicode normalization, equal scores, empty queries, source deduplication, cancellation, rapid input, and the maximum indexed-file corpus.

#### Scenario: Bounded selector is compared with a reference
- **WHEN** generated candidate/query/result-limit combinations run through bounded and full-sort implementations
- **THEN** both produce the same selected identities and deterministic order
- **AND** the bounded implementation retains no more than the configured result count per source

#### Scenario: Maximum index receives rapid queries
- **WHEN** the 100,000-file fixture receives more queries than workers can complete
- **THEN** measurements report one active search, at most one pending query, cancellation progress, and final-query latency
- **AND** obsolete full-index jobs do not accumulate in the generic worker FIFO

#### Scenario: Diagnostic privacy regression is exercised
- **WHEN** an invalid Replace Preview range is generated under captured tracing output
- **THEN** typed invalid counts remain correct
- **AND** captured diagnostics contain none of the private source or replacement sentinel text
