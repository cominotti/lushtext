## ADDED Requirements

### Requirement: Lossy-encoding analysis is exact and allocation-bounded
The system SHALL determine representability for a selected save encoding with a reusable whole-input analyzer rather than allocating temporary strings or initializing an encoder for each Unicode scalar. The result MUST preserve the exact total issue count and the first eight issue positions with their original line, column, and Unicode-scalar identity.

#### Scenario: UTF-16 is selected for valid editor text
- **WHEN** the user selects UTF-16LE or UTF-16BE for a Rust string held by the editor
- **THEN** representability analysis returns lossless without scanning each scalar for encoder failure
- **AND** the normal save encoder still emits the selected UTF-16 byte order

#### Scenario: Legacy encoding cannot represent several scalars
- **WHEN** Windows-1252 or Shift_JIS cannot represent multiple characters in the document
- **THEN** analysis reports the exact issue count and first eight original source positions
- **AND** it does so without one temporary `String` or encoder construction per scalar

#### Scenario: Optimized analysis is compared with actual encoding
- **WHEN** property and boundary fixtures analyze arbitrary valid Unicode text for each supported save encoding
- **THEN** lossless/lossy classification agrees with actual no-replacement encoding behavior
- **AND** diagnostic positions identify precisely the unrepresentable source scalars
