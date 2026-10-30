## ADDED Requirements

### Requirement: Protected View preserves file bytes on save
A document installed in Protected View SHALL never become modified through LushText interaction, so Save SHALL write nothing for it (the Save action is disabled while it is unmodified). Every save of a protected document (Save, Save As, and the close-with-changes save) SHALL write the document's `ProtectedTextSource` on the worker, verbatim in the document's detected encoding and BOM state, through the existing durable-write path. The source equals the buffer's full text, folded tails included (`text(..., true)`), because the buffer is immutable from entry and the source is the installed body or a single capture taken after entry; widget tests SHALL assert that equality on every protected fixture. Every save-time transformation SHALL be suppressed for a protected document, so that no save path changes the buffer and an unmodified document's Save As output is byte-identical to its source file:
- EditorConfig-driven formatting and its mirror-back into the buffer;
- line-ending normalization. The buffer already holds the original separators, so a document whose line endings are `Mixed` SHALL be written as it is, instead of being refused with `EditorSaveError::MixedLineEndings`, because normalization is unavailable in Protected View and refusing would leave a recovered or edit-growth document unsaveable.

The loader already refuses decodes with errors. When the document's encoding can still decode without errors yet not round-trip exactly, Save As of an unmodified protected document SHALL be refused with a notice instead of writing different bytes. A document that entered Protected View through an edit or through draft recovery SHALL keep its modified state, and Save, Save As and close-with-changes SHALL keep working, without transformations.

#### Scenario: Byte-identity round trip
- **WHEN** a synthetic protected file (UTF-8 with and without BOM, CRLF, CR-only, mixed line endings, U+2029 separators, and a 1.9M-character paragraph) is opened and saved with Save As to a new path
- **THEN** the new file's bytes equal the source file's bytes

#### Scenario: Save-time formatting does not apply
- **WHEN** an `.editorconfig` requests trailing-whitespace trimming and a final newline, and a protected document is saved with Save As
- **THEN** no formatting is applied, the buffer is not mirrored back, and the bytes are identical

#### Scenario: Edit-time entry keeps unsaved work safe
- **WHEN** a paste makes a modified document protected and the user then closes the tab
- **THEN** the save-changes dialog appears and saving writes the buffer's text, including the pasted paragraph, durably from the source captured after entry, with no transformation and no mirror-back into the buffer

#### Scenario: Mixed line endings stay saveable
- **WHEN** a protected document with mixed line endings that entered Protected View through an edit is saved
- **THEN** the save succeeds and writes the buffer's separators unchanged
