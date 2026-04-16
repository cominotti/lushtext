## MODIFIED Requirements

### Requirement: LushText detects and surfaces document encoding state on open
The system SHALL open file-backed documents through an encoding-aware load pipeline that records the decoding used for the active buffer, the presence of a BOM, decode confidence, and the document's line-ending state. The system MUST surface the current encoding and line-ending state in the status bar for file-backed tabs using compact document-local metadata controls that remain secondary to the editor content.

#### Scenario: Open a UTF-8 file with uniform line endings
- **WHEN** the user opens a UTF-8 text file whose contents use only LF line endings
- **THEN** the document opens without conversion prompts
- **AND** the status bar shows `UTF-8` and `LF` for that tab

#### Scenario: Open a non-UTF-8 file with mixed line endings
- **WHEN** the user opens a text file whose bytes are decoded with a non-UTF-8 encoding and whose contents contain mixed line endings
- **THEN** the system opens the document using the detected decoding
- **AND** the status bar shows the detected encoding and a mixed line-ending state
- **AND** the document records file-health findings for follow-up actions

### Requirement: Status-bar encoding controls stay lightweight and document-local
The system SHALL expose encoding, line-ending, and file-health state through compact status-bar entry points tied to the active document. Inline menus or popovers opened from those entry points MUST stay concise and focus on current state plus immediate next actions. When a task requires browsing a broader encoding list, reading explanatory text, or confirming consequences, the system MUST open a dedicated document-modal flow from the status-bar affordance instead of overloading the transient surface.

#### Scenario: Review encoding state from the status bar
- **WHEN** the user activates the encoding entry point for a file-backed document
- **THEN** the revealed surface shows the document's current opened encoding and next-save encoding
- **AND** the same surface exposes immediate actions to reopen or save using another encoding

#### Scenario: Open a full encoding chooser from a compact entry point
- **WHEN** the user requests an encoding that is not part of the compact inline choices
- **THEN** the system opens a dedicated document-modal chooser from the status-bar flow
- **AND** the status-bar surface itself remains a lightweight entry point rather than a large picker

#### Scenario: Choose a line-ending policy from a single-choice control
- **WHEN** the user activates the line-ending entry point for a file-backed document
- **THEN** the system presents `LF`, `CRLF`, and `CR` as mutually exclusive choices with the current write policy selected
- **AND** choosing another option updates the future save policy without opening a separate tool window

### Requirement: Users can reopen documents with a different encoding without silently losing work
The system SHALL provide a `Reopen with Encoding…` flow for saved documents that rereads the on-disk bytes with a user-selected decoder. The system MUST not discard unsaved edits during that flow without explicit user confirmation in a document-modal dialog that preserves the existing buffer until the user chooses to continue.

#### Scenario: Reopen a modified document with another encoding
- **WHEN** the user invokes `Reopen with Encoding…` for a modified file-backed document
- **THEN** the system asks the user to confirm discarding unsaved edits before rereading the file
- **AND** the confirmation flow presents explicit `Cancel` and `Reopen` actions
- **AND** the existing buffer remains unchanged unless the user confirms

### Requirement: Users can choose how future saves encode and normalize a document
The system SHALL let users choose the encoding and line-ending style used for future saves independently from the current buffer's in-memory Unicode representation. The system MUST preview or warn before writing a lossy encoding conversion, and MUST route irreversible save-conversion confirmation through a dedicated document-modal flow rather than a crowded status-bar popover.

#### Scenario: Save a document using a lossy encoding
- **WHEN** the user chooses a save encoding that cannot represent one or more characters in the current buffer
- **THEN** the system shows a bounded preview or warning describing the affected content before saving
- **AND** the file is not written until the user explicitly confirms the lossy conversion
- **AND** the confirmation flow offers explicit `Cancel` and affirmative save actions

### Requirement: LushText reports encoding-related file health issues
The system SHALL collect per-document file-health findings for encoding-adjacent issues such as mixed line endings, BOM presence, low-confidence decode results, and binary-like content. The system MUST expose those findings from the active document's status-bar metadata and raise document-scoped persistent warnings when an immediate fix is available. Non-destructive findings MUST default to document-local health surfaces or persistent warnings instead of interrupting file open with a blocking modal dialog.

#### Scenario: Normalize mixed line endings from an open warning
- **WHEN** the active document is opened with mixed line endings and the system offers a normalization action
- **THEN** the warning exposes a `Normalize…` action that leads to choosing a target line-ending style
- **AND** the document keeps the same textual content while updating its future save policy to the chosen line-ending style

#### Scenario: Low-confidence decoding does not block document open
- **WHEN** the system opens a document with a low-confidence decode result but no immediate data-loss action is required
- **THEN** the document still opens into the editor
- **AND** the system surfaces the uncertainty through the status-bar health indicator and any persistent document warning
- **AND** the system does not interrupt open with a blocking modal dialog

### Requirement: Encoding metadata remains accessible at narrow window widths
The system SHALL preserve access to encoding, line-ending, and file-health controls across supported window sizes without clipping labels or moving the workflow into a separate tool window. When horizontal space is insufficient for the full metadata cluster, the system MAY collapse those controls into a compact grouped entry point as long as the same document-local actions remain available.

#### Scenario: Narrow windows retain access to encoding controls
- **WHEN** a file-backed document is active in a narrow window where the full metadata cluster cannot fit comfortably
- **THEN** the encoding, line-ending, and file-health actions remain reachable through a compact grouped status-bar entry point
- **AND** the controls do not disappear or require a separate preferences-style surface

### Requirement: User-visible encoding actions follow GNOME writing conventions
The system SHALL use concise user-visible labels for encoding and line-ending workflows. Actions which require further input or confirmation MUST use an ellipsis character (`…`), and warning copy MUST describe document state and consequences in plain language rather than internal implementation terms.

#### Scenario: Action labels communicate additional steps
- **WHEN** the user opens an encoding action menu or dialog launcher
- **THEN** labels such as `Reopen with Encoding…` and `Save Using Encoding…` use an ellipsis to indicate that more input or confirmation follows
- **AND** compact state labels such as `UTF-8`, `LF`, and `Issues` remain short and scannable
