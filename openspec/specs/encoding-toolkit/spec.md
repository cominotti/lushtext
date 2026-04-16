# encoding-toolkit Specification

## Purpose
TBD - created by archiving change encoding-toolkit. Update Purpose after archive.
## Requirements
### Requirement: LushText detects and surfaces document encoding state on open
The system SHALL open file-backed documents through an encoding-aware load pipeline that records the decoding used for the active buffer, the presence of a BOM, decode confidence, and the document's line-ending state. The system MUST surface the current encoding and line-ending state in the status bar for file-backed tabs.

#### Scenario: Open a UTF-8 file with uniform line endings
- **WHEN** the user opens a UTF-8 text file whose contents use only LF line endings
- **THEN** the document opens without conversion prompts
- **AND** the status bar shows `UTF-8` and `LF` for that tab

#### Scenario: Open a non-UTF-8 file with mixed line endings
- **WHEN** the user opens a text file whose bytes are decoded with a non-UTF-8 encoding and whose contents contain mixed line endings
- **THEN** the system opens the document using the detected decoding
- **AND** the status bar shows the detected encoding and a mixed line-ending state
- **AND** the document records file-health findings for follow-up actions

### Requirement: Users can reopen documents with a different encoding without silently losing work
The system SHALL provide a `Reopen with Encoding...` flow for saved documents that rereads the on-disk bytes with a user-selected decoder. The system MUST not discard unsaved edits during that flow without explicit user confirmation.

#### Scenario: Reopen a clean document with another encoding
- **WHEN** the user invokes `Reopen with Encoding...` for an unmodified file-backed document and selects a different supported encoding
- **THEN** the system rereads the current on-disk bytes using that encoding
- **AND** the editor buffer and status bar update to reflect the reopened interpretation

#### Scenario: Reopen a modified document with another encoding
- **WHEN** the user invokes `Reopen with Encoding...` for a modified file-backed document
- **THEN** the system asks the user to confirm discarding unsaved edits before rereading the file
- **AND** the existing buffer remains unchanged unless the user confirms

### Requirement: Users can choose how future saves encode and normalize a document
The system SHALL let users choose the encoding and line-ending style used for future saves independently from the current buffer's in-memory Unicode representation. The system MUST preview or warn before writing a lossy encoding conversion.

#### Scenario: Save a document using a different line-ending style
- **WHEN** the user selects `CRLF` for a file-backed document that is currently configured to save with `LF`
- **THEN** the status bar reflects the new save policy before the next write
- **AND** the next save writes the document using `CRLF` line endings

#### Scenario: Save a document using a lossy encoding
- **WHEN** the user chooses a save encoding that cannot represent one or more characters in the current buffer
- **THEN** the system shows a bounded preview or warning describing the affected content before saving
- **AND** the file is not written until the user explicitly confirms the lossy conversion

### Requirement: LushText reports encoding-related file health issues
The system SHALL collect per-document file-health findings for encoding-adjacent issues such as mixed line endings, BOM presence, low-confidence decode results, and binary-like content. The system MUST expose those findings from the active document's status-bar metadata and raise document-scoped warnings when an immediate fix is available.

#### Scenario: Review file-health details from the status bar
- **WHEN** the active document has one or more file-health findings
- **THEN** the status bar shows a visible health indicator for that document
- **AND** activating that indicator reveals the recorded findings and any available next actions

#### Scenario: Normalize mixed line endings from an open warning
- **WHEN** the active document is opened with mixed line endings and the system offers a normalization action
- **THEN** the user can choose a target line-ending style from that warning flow
- **AND** the document keeps the same textual content while updating its future save policy to the chosen line-ending style

