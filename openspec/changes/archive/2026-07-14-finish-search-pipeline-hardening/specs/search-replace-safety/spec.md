## ADDED Requirements

### Requirement: Search diagnostics never contain private document text
Search and Replace Preview diagnostics, warnings, automation state, and typed failures MUST NOT include document substrings, complete match text, replacement expansions, or other buffer contents. Invalid preview rows SHALL be represented by bounded counts and non-content reason classes.

#### Scenario: Regex no longer matches an extracted range
- **WHEN** Replace Preview cannot re-match a recorded range
- **THEN** the outcome increments a typed invalid-row reason without logging the original substring
- **AND** UI feedback may report only bounded counts and non-private metadata

#### Scenario: Diagnostic logging is enabled at default level
- **WHEN** search warnings are written to stderr or the user-session journal
- **THEN** messages contain no matched or surrounding document text
- **AND** file paths or line numbers are included only when required by the existing diagnostic policy

#### Scenario: Invalid rows coexist with valid preview rows
- **WHEN** a preview contains both valid replacements and invalid stale ranges
- **THEN** confirmation still includes only valid current rows
- **AND** the invalid summary reveals no private source or replacement contents
