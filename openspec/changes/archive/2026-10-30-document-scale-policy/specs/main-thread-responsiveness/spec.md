## MODIFIED Requirements

### Requirement: Expensive preview and analysis workflows degrade before blocking input
The system SHALL protect user input from large Markdown preview preprocessing,
minimap marker scans, lossy encoding analysis, GtkSourceView syntax analysis of
long paragraphs, and feature work triggered by edits that grow a paragraph.
Each workflow MUST do one of the following:
- use a bounded/chunked GTK snapshot plus worker processing;
- enforce a documented large-input threshold with a user-visible paused,
  limited or reduced state;
- reuse already available load-time analysis data.

Features whose GTK cost grows with paragraph length MUST be gated by the
document scale policy before the paragraph reaches them, at load and in the
same signal emission as an edit that makes a paragraph exceed the feature's
threshold.

#### Scenario: Markdown preview avoids unbounded preprocessing in one GTK turn
- **WHEN** Markdown preview refresh is requested for a document whose text or
  generated event stream would exceed the configured automatic-preview budget
- **THEN** the app either preprocesses bounded owned input asynchronously or
  shows an explicit paused/limited preview state
- **AND** it does not perform unbounded Markdown preprocessing in one GTK
  callback

#### Scenario: Minimap long-line markers avoid full-buffer blocking scans
- **WHEN** long-line minimap markers are enabled for a document large enough to
  make full-buffer scanning expensive
- **THEN** marker collection uses cached load-time data, a chunked scan, or a
  documented threshold that omits expensive markers
- **AND** scrolling or editing the document is not blocked by a full-buffer scan

#### Scenario: Lossy encoding analysis returns asynchronously for non-small buffers
- **WHEN** the user chooses a save encoding that requires scanning non-small
  buffer contents for lossy conversion
- **THEN** the analysis runs after a bounded snapshot and worker processing step
- **AND** the lossy-save confirmation is shown only for the still-current
  document and encoding request

#### Scenario: Syntax analysis never meets a giant paragraph
- **WHEN** a document whose longest paragraph exceeds the syntax paragraph
  threshold is loaded, or an edit creates such a paragraph
- **THEN** the language is cleared and highlighting is off before any GTK idle
  can run syntax analysis on that paragraph
- **AND** the user sees the reduced-features notice instead of a multi-second
  stall

#### Scenario: Edit-time growth is evaluated within a bounded handler
- **WHEN** an insertion or deletion changes paragraph lengths
- **THEN** the profile update reads only the inserted run and a constant number
  of line lengths whose cost does not grow with line length
- **AND** it never copies or scans the document

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices whose boundaries align to paragraph ends, as GTK defines them: just after `\n`, `\r`, `\r\n` (never between its CR and LF) or U+2029. The paragraph-boundary function MUST be the single shared one whose agreement with `pango_find_paragraph_boundary` is tested. GTK text layout validates whole paragraphs, so a slice that stops inside a paragraph forces later slices to re-lay-out everything already installed in that paragraph. That is quadratic total work and can stall recovery of single-line documents for minutes. A single paragraph longer than the slice byte budget SHALL be installed (and, during clearing, deleted) in one turn, because GTK cannot lay out a partial paragraph incrementally regardless of how the mutation is sliced. The editor SHALL remain non-editable, and projections that would amplify each insertion SHALL remain suspended, until the complete current generation is installed or the operation is cancelled.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded paragraph-aligned slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file contains one paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded paragraph-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: CR-only and U+2029-separated documents are sliced
- **WHEN** an admitted document above the synchronous threshold separates its paragraphs only with `\r`, or only with U+2029
- **THEN** installation uses multiple bounded slices, each ending just after a delimiter
- **AND** the installed buffer is byte-identical to the decoded text

#### Scenario: CRLF pairs survive slicing
- **WHEN** a slice's byte budget ends between the `\r` and `\n` of a CRLF pair
- **THEN** the slice ends after the `\n`, so the pair stays one GTK paragraph delimiter

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation
