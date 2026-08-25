## ADDED Requirements

### Requirement: Footnote numbering is stable across Markdown projection turns
Footnote numbering SHALL be owned by the preview render generation rather than by one projection batch. A rendered reference marker MUST match the number shown by its rendered definition even when the reference and the definition are projected in different GTK turns, and numbering MUST NOT restart at each turn.

#### Scenario: Reference and definition are projected in different turns
- **WHEN** a document is large enough that a footnote reference and its definition land in different projection batches
- **THEN** the rendered reference marker and its rendered definition show the same number
- **AND** later footnotes continue the numbering instead of restarting

#### Scenario: Many footnotes across many turns
- **WHEN** a document contains several inline and reference-style footnotes spread across several projection batches
- **THEN** each marker is numbered once and matches exactly one rendered definition
- **AND** no two distinct footnotes share a number
