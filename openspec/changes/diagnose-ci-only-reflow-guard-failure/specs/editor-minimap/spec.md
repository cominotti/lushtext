## ADDED Requirements

### Requirement: An editor presented at the top rests exactly at the top
An editor whose document is presented at its first line SHALL rest with its vertical adjustment at `lower` (within the rest state's 0.5 px edge epsilon) once presentation and its passive width bursts have settled, whatever the number of CPUs the process may use. The top-anchor repair MUST therefore be armed for the first reflow after presentation. Any test that relies on the editor resting at the top MUST assert that exact rest state, not a pixel tolerance wider than the epsilon.

#### Scenario: Presented at the top on a four-CPU host
- **WHEN** a 2,000-line document is presented at its first line in the CI image limited to four CPUs
- **AND** presentation and its width bursts have settled
- **THEN** the editor's vertical value equals its `lower` within 0.5 px
- **AND** the first visible line is line one, unclipped

#### Scenario: Presented at the top in a real window
- **WHEN** a document is opened at its top in a `LushtextWindow` in the CI image limited to four CPUs
- **THEN** the editor rests at `lower` within 0.5 px after it settles

#### Scenario: Reflow repair guard starts at rest
- **WHEN** the top-anchor repair guard begins its reflow burst
- **THEN** its precondition has checked that the vertical value equals `lower` within 0.5 px
- **AND** a fixture that leaves the editor off the top fails that precondition, not the repair assertion
