# Main Thread Responsiveness — Delta

## MODIFIED Requirements

### Requirement: Large file installation yields in bounded GTK slices
Installing a decoded document above the synchronous installation threshold SHALL use bounded main-loop slices whose boundaries align to paragraph ends (just after a newline). GTK text layout validates whole paragraphs, so a slice that stops inside a paragraph forces later slices to re-lay-out everything already installed in that paragraph — quadratic total work that can stall recovery of single-line documents for minutes. A single paragraph longer than the slice byte budget SHALL be installed (and, during clearing, deleted) in one turn, because GTK cannot lay out a partial paragraph incrementally regardless of how the mutation is sliced. The editor SHALL remain non-editable and projections that would amplify each insertion SHALL remain suspended until the complete current generation is installed or the operation is cancelled.

#### Scenario: Large decoded text is installed
- **WHEN** an admitted load returns text above the synchronous installation threshold
- **THEN** GTK inserts the text in bounded paragraph-aligned slices with scheduling points between them
- **AND** syntax, minimap, history, draft, monitor, and modified-state finalization run only after the complete current generation is present

#### Scenario: Giant single-paragraph content avoids quadratic re-layout
- **WHEN** a recovered draft or loaded file contains one paragraph larger than the slice byte budget
- **THEN** that paragraph is installed in a single turn while any multi-paragraph remainder keeps bounded newline-aligned slices
- **AND** previously installed paragraphs are not re-validated by later slices

#### Scenario: Load is cancelled during installation
- **WHEN** the tab closes, reloads, or advances generation between installation slices
- **THEN** remaining slices stop without applying final loaded state
- **AND** admission ownership and retained decoded text are released

#### Scenario: Small load remains direct
- **WHEN** decoded text is below the synchronous installation threshold
- **THEN** the existing direct installation path may run in one GTK turn
- **AND** it observes the same generation and finalization rules as chunked installation
