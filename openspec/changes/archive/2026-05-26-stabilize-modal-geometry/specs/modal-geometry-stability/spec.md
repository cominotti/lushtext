## ADDED Requirements

### Requirement: Modal interactions preserve outer geometry
The system SHALL keep every presented modal dialog, popup, and modal browser surface visually stable while the user interacts inside it. A modal MUST NOT shrink, grow, or shift unexpectedly when its content changes, including mode switches, placeholder-to-content swaps, sidebar selection changes, filtering, async preview results, or validation and warning states.

#### Scenario: Dynamic content updates inside a modal
- **WHEN** the user performs an in-modal action that replaces placeholder, loading, empty, preview, edit, render, warning, or result content
- **THEN** the modal keeps the same outer size and position
- **AND** the updated content remains usable within the existing modal bounds

#### Scenario: Modal browser selection changes preview content
- **WHEN** the user changes the selected row inside a modal browser such as Notes or Local History
- **THEN** the browser modal keeps the same outer size and position
- **AND** the preview content change does not cause visible shell drift

#### Scenario: Modal filtering changes result state
- **WHEN** the user types into a modal search or filter field and the modal switches between populated and empty states
- **THEN** the modal keeps the same outer size and position
- **AND** the result state remains visually aligned within the existing modal frame

### Requirement: Dynamic modal pages declare stable geometry before presentation
The system SHALL ensure modal pages that can change after presentation reserve their final geometry before the user can trigger the change. Dynamic modal pages MUST use fixed modal dimensions, stable min and max content dimensions, or prewarmed hidden content so their first visible activation cannot remeasure the modal shell.

#### Scenario: Hidden page becomes visible for the first time
- **WHEN** a modal contains a hidden page that will become visible through a user action
- **THEN** the hidden page already advertises the same geometry as its final visible state
- **AND** showing the page for the first time does not resize the modal

#### Scenario: Placeholder content is replaced by rendered content
- **WHEN** a modal replaces a placeholder with rendered content after the modal is already presented
- **THEN** the placeholder and rendered content expose the same modal geometry contract
- **AND** the replacement does not resize the modal

### Requirement: Representation switches preserve text origin
The system SHALL keep matching text origins when a modal switches between two representations of the same user text. Edit and Render representations MUST align the first visible text position both horizontally and vertically.

#### Scenario: Switch between editable and rendered text
- **WHEN** the user switches a modal surface from editable text to rendered text for the same content
- **THEN** the first rendered text starts at the same visual origin as the editable text
- **AND** the modal keeps the same outer size and position
