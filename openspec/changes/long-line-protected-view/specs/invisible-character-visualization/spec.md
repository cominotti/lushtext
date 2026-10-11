## ADDED Requirements

### Requirement: Invisible characters are off while a Protected View fold exists
While any Protected View fold exists in a document, its invisible-character mode SHALL be forced to off regardless of the user's choice. `win.cycle-invisible-characters` SHALL publish the Protected View notice instead of changing the drawer. The user's stored preference SHALL be left untouched.

#### Scenario: Cycling invisibles on a protected document
- **WHEN** the user presses Ctrl+Shift+I on a protected document
- **THEN** the space drawer stays disabled, the notice is shown, and other documents keep the user's mode
