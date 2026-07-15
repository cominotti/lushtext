# encoding-toolkit Specification

## Purpose
Define the document-local encoding, line-ending, and file-health workflows so quick state stays accessible from the bottom bar while slower inspection and health details live in document properties.

## Requirements

### Requirement: File-health details live in the document properties surface
The system SHALL expose file-health details through the document properties surface for the active document rather than through the bottom bar's quick editor-state strip. When an active document has encoding-adjacent health findings, the document properties surface MUST provide the detailed findings and any slower follow-up actions.

#### Scenario: Review file-health details from document properties
- **WHEN** the active document has one or more file-health findings and the user opens document properties
- **THEN** the document properties surface shows the recorded findings for that document
- **AND** the user can inspect any available next actions from that surface

### Requirement: LushText detects and surfaces document encoding state on open
The system SHALL open file-backed documents through an encoding-aware load pipeline that records the decoding used for the active buffer, the presence of a BOM, decode confidence, and the document's line-ending state. The system MUST surface the current encoding and line-ending state in the bottom bar for file-backed tabs using compact, quick editor-state controls that remain secondary to the editor content.

#### Scenario: Open a UTF-8 file with uniform line endings
- **WHEN** the user opens a UTF-8 text file whose contents use only LF line endings
- **THEN** the document opens without conversion prompts
- **AND** the bottom bar shows `UTF-8` and `LF` for that tab

#### Scenario: Open a non-UTF-8 file with mixed line endings
- **WHEN** the user opens a text file whose bytes are decoded with a non-UTF-8 encoding and whose contents contain mixed line endings
- **THEN** the system opens the document using the detected decoding
- **AND** the bottom bar shows the detected encoding and a mixed line-ending state
- **AND** the document records file-health findings for follow-up actions

### Requirement: Bottom-bar encoding controls stay lightweight and document-local
The system SHALL expose encoding and line-ending state through compact bottom-bar entry points tied to the active document. Inline menus or popovers opened from those entry points MUST stay concise and focus on current state plus immediate next actions. When a task requires browsing a broader encoding list, reading explanatory text, or confirming consequences, the system MUST open a dedicated document-modal flow from the bottom-bar affordance instead of overloading the transient surface.

#### Scenario: Review encoding state from the bottom bar
- **WHEN** the user activates the encoding entry point for a file-backed document
- **THEN** the revealed surface shows the document's current opened encoding and next-save encoding
- **AND** the same surface exposes immediate actions to reopen or save using another encoding

#### Scenario: Open a full encoding chooser from a compact bottom-bar entry point
- **WHEN** the user requests an encoding that is not part of the compact inline choices
- **THEN** the system opens a dedicated document-modal chooser from the bottom-bar flow
- **AND** the bottom-bar surface itself remains a lightweight entry point rather than a large picker

#### Scenario: Choose a line-ending policy from a single-choice control
- **WHEN** the user activates the line-ending entry point for a file-backed document
- **THEN** the system presents `LF`, `CRLF`, and `CR` as mutually exclusive choices with the current write policy selected
- **AND** choosing another option updates the future save policy without opening a separate tool window

### Requirement: Users can reopen documents with a different encoding without silently losing work
The system SHALL provide a `Reopen with Encoding…` flow for saved documents that rereads the on-disk bytes with a user-selected decoder. The system MUST not discard unsaved edits during that flow without explicit user confirmation in a document-modal dialog that preserves the existing buffer until the user chooses to continue.

#### Scenario: Reopen a clean document with another encoding
- **WHEN** the user invokes `Reopen with Encoding…` for an unmodified file-backed document and selects a different supported encoding
- **THEN** the system rereads the current on-disk bytes using that encoding
- **AND** the editor buffer and bottom bar update to reflect the reopened interpretation

#### Scenario: Reopen a modified document with another encoding
- **WHEN** the user invokes `Reopen with Encoding…` for a modified file-backed document
- **THEN** the system asks the user to confirm discarding unsaved edits before rereading the file
- **AND** the confirmation flow presents explicit `Cancel` and `Reopen` actions
- **AND** the existing buffer remains unchanged unless the user confirms

### Requirement: Users can choose how future saves encode and normalize a document
The system SHALL let users choose the encoding and line-ending style used for future saves independently from the current buffer's in-memory Unicode representation. The system MUST preview or warn before writing a lossy encoding conversion, and MUST route irreversible save-conversion confirmation through a dedicated document-modal flow rather than a crowded inline chooser.

#### Scenario: Save a document using a different line-ending style
- **WHEN** the user selects `CRLF` for a file-backed document that is currently configured to save with `LF`
- **THEN** the bottom bar reflects the new save policy before the next write
- **AND** the next save writes the document using `CRLF` line endings

#### Scenario: Save a document using a lossy encoding
- **WHEN** the user chooses a save encoding that cannot represent one or more characters in the current buffer
- **THEN** the system shows a bounded preview or warning describing the affected content before saving
- **AND** the file is not written until the user explicitly confirms the lossy conversion
- **AND** the confirmation flow offers explicit `Cancel` and affirmative save actions

### Requirement: LushText reports encoding-related file health issues
The system SHALL collect per-document file-health findings for encoding-adjacent issues such as mixed line endings, BOM presence, low-confidence decode results, and binary-like content. The system MUST expose those findings from the active document's document properties surface and raise document-scoped persistent warnings when an immediate fix is available. Non-destructive findings MUST default to the document properties surface or persistent warnings instead of interrupting file open with a blocking modal dialog.

#### Scenario: Review file-health details from the document properties surface
- **WHEN** the active document has one or more file-health findings
- **THEN** the document properties surface shows a visible health entry for that document
- **AND** activating that entry reveals the recorded findings and any available next actions

#### Scenario: Normalize mixed line endings from an open warning
- **WHEN** the active document is opened with mixed line endings and the system offers a normalization action
- **THEN** the warning exposes a `Normalize…` action that leads to choosing a target line-ending style
- **AND** the document keeps the same textual content while updating its future save policy to the chosen line-ending style

#### Scenario: Low-confidence decoding does not block document open
- **WHEN** the system opens a document with a low-confidence decode result but no immediate data-loss action is required
- **THEN** the document still opens into the editor
- **AND** the system surfaces the uncertainty through the document properties surface and any persistent document warning
- **AND** the system does not interrupt open with a blocking modal dialog

### Requirement: Encoding metadata remains accessible at narrow window widths
The system SHALL preserve access to encoding, line-ending, and file-health workflows across supported window sizes without clipping labels or moving the workflow into a separate preferences-style surface. When horizontal space is insufficient for the spacious layout, the system SHALL keep encoding and line-ending actions reachable from the bottom bar while keeping file-health details reachable through the document properties surface's compact presentation.

#### Scenario: Narrow windows retain access to encoding controls
- **WHEN** a file-backed document is active in a narrow window where the side-by-side document-properties pane cannot fit comfortably
- **THEN** the encoding and line-ending actions remain reachable through the bottom bar
- **AND** file-health details remain reachable through the document properties bottom-sheet presentation
- **AND** the controls do not disappear or require a separate preferences-style surface

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
