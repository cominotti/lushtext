## ADDED Requirements

### Requirement: Notes menu popup activation is stable
The system SHALL open a visible `Notes` menu popup when the user activates the visible header-bar `Notes` button. The system MUST NOT rebuild, replace, or clear the menu model during the popup activation path in a way that prevents GTK from showing the popover.

#### Scenario: Click the Notes button opens the menu
- **WHEN** the window context makes the `Notes` menu button visible
- **AND** the user activates the `Notes` menu button
- **THEN** the `Notes` menu popup becomes open
- **AND** the popup exposes the current note entry points

#### Scenario: Dynamic bookmark label does not cancel popup opening
- **WHEN** the active saved document changes the bookmark-toggle label between `Add Bookmark` and `Remove Bookmark`
- **AND** the user activates the `Notes` menu button after that state refresh
- **THEN** the `Notes` menu popup opens normally
- **AND** the bookmark-toggle label reflects the current cursor state
