## ADDED Requirements

### Requirement: Notes source construction scratch is byte-bounded
The unified Notes source loader SHALL enforce explicit conservative byte ceilings for concurrently retained construction scratch and sidecar traversal paths in addition to existing entry, searchable-text, final retained-source, sidecar-count, open-editor, and diagnostic limits. Accounting MUST use saturating arithmetic and MUST include the current recovery-aware sidecar input, retained path batch, canonical identity copies, diagnostic storage, temporary category/capacity ownership, and other construction allocations that overlap the final source. Reaching a construction or path ceiling MUST stop at a deterministic complete boundary and publish a distinct typed truncation reason with compact current/peak metrics.

#### Scenario: Sidecar directory contains long Unicode paths
- **WHEN** fewer than the sidecar entry cap would nevertheless exceed the traversal path-byte ceiling
- **THEN** the byte-bounded scanner retains only complete entries within both limits
- **AND** source feedback distinguishes path-byte truncation from sidecar-count truncation

#### Scenario: One near-limit sidecar overlaps admitted rows
- **WHEN** recovery-aware loading holds a sidecar input near its metadata byte limit while final rows and construction scratch already exist
- **THEN** measured peak construction ownership remains within the documented scratch ceiling
- **AND** the loader stops before another complete allocation would exceed that ceiling

#### Scenario: Diagnostics and canonicalization consume scratch
- **WHEN** malformed sidecars and many folder identities produce bounded recovery diagnostics and canonical path copies
- **THEN** those allocations contribute to construction metrics rather than bypassing admission
- **AND** valid rows admitted before the deterministic boundary remain ordered, browsable, and activatable

#### Scenario: Construction is cancelled
- **WHEN** source generation is superseded or the Notes browser closes during sidecar traversal or parsing
- **THEN** cancellation releases path, sidecar, diagnostic, and category scratch on the worker
- **AND** no large construction allocation crosses to GTK in the cancelled outcome

#### Scenario: Final source reaches GTK
- **WHEN** bounded construction completes with admitted rows
- **THEN** only the final measured retained source and compact metrics cross to GTK under the existing progress reservation
- **AND** construction scratch has already been released and is not hidden inside diagnostic payloads
