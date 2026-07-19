## MODIFIED Requirements

### Requirement: Replace All builds changed text without full line-vector amplification
The system SHALL avoid constructing a full per-line collection of owned strings, byte ranges, or equivalent metadata during Replace All. Before line discovery, it MUST validate replacement-count and recorded-range bounds. For accepted files, it MUST validate UTF-8 using the project's established large-file validation approach and build replacement output in one source-order pass from sorted recorded replacements. Retained edit metadata MUST remain proportional to accepted replacements rather than total source-line count, while original bytes, output bytes, and durable undo bytes remain governed by their existing caps.

#### Scenario: Large accepted file avoids line-vector allocation
- **WHEN** Replace All processes a file within the per-file cap but large enough to stress allocation
- **THEN** it does not split or index the entire file into a per-line vector
- **AND** it still validates stale search results before writing

#### Scenario: Dense short-line file stays within retained metadata bounds
- **WHEN** an accepted file near the byte cap contains millions of short lines but no more than the configured replacement-count limit
- **THEN** line discovery streams only the boundaries needed by the sorted replacements
- **AND** retained line or edit metadata remains bounded by accepted replacement count rather than source-line count

#### Scenario: Streaming construction preserves line semantics
- **WHEN** replacements target LF, CRLF, final unterminated, Unicode, and empty lines
- **THEN** streaming construction produces the same changed bytes and stale-line decisions as the reference behavior
- **AND** durable journal-before-mutation and cancellation ordering remain unchanged
