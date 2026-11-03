## ADDED Requirements

### Requirement: An inline alert is announced after the opened document takes the keyboard
When a session restore or an activation opens documents in a window, an inline
alert of the selected document SHALL NOT be announced before the opened
document's keyboard handoff has finished (focused, or decided not to). While
the handoff is pending, or the restore or activation that will request it is
still running, the alert SHALL stay due, and it SHALL be announced once the
handoff finishes, or once the restore or activation ends without one. A screen
reader interrupts speech on a focus change, so an announcement made before the
handoff would be cut off. The ordering SHALL NOT depend on how long the
selected document's installation takes.

#### Scenario: A protected document's alert follows the focus handoff
- **WHEN** a command-line open or a startup session restore opens a document
  that enters Protected View
- **THEN** the editor takes the keyboard before its "View-Only" alert is
  announced, whatever the machine's load

#### Scenario: A restore that creates no page still announces
- **WHEN** a session restore ends without creating a page, while the selected
  document has an inline alert that was held back
- **THEN** the alert is announced when the restore ends
