## ADDED Requirements

### Requirement: Editor inline alerts use compact balanced vertical rhythm
The system SHALL render editor inline alerts with balanced compact vertical padding. The default alert surface MUST use equal top and bottom padding no greater than 6 CSS pixels, MUST preserve the existing horizontal inset, and MUST NOT use asymmetric padding to force one-off alignment. This compact rhythm MUST preserve existing warning/error styling, message/action placement, grouped controls, dismiss affordance, and accessibility behavior.

#### Scenario: Restored draft warning uses balanced compact padding
- **WHEN** a restored draft warning with discard, save, and dismiss controls is shown in a wide editor column
- **THEN** the alert surface uses equal top and bottom padding no greater than 6 CSS pixels
- **AND** the message and action group remain on one row when they fit
- **AND** the discard, save, and dismiss controls remain fully visible and reachable

#### Scenario: Retryable error alert uses balanced compact padding
- **WHEN** a retryable error inline alert is shown
- **THEN** the alert surface uses the same compact balanced padding as warning alerts
- **AND** the retry and dismiss controls remain fully visible and reachable
- **AND** the alert remains visually distinct as an error surface

#### Scenario: Informational warning keeps dismiss with compact padding
- **WHEN** a warning inline alert has no workflow action labels
- **THEN** the alert surface still uses balanced compact padding
- **AND** the dismiss control remains visible in the action group
- **AND** no empty workflow action buttons are shown

#### Scenario: Narrow wrapped alert remains readable
- **WHEN** an inline alert's message and action group cannot fit on one line in a constrained editor column
- **THEN** the message text remains readable within the editor column
- **AND** the entire action group wraps as one unit beneath the message
- **AND** the compact padding does not cause visible controls to overlap or lose positive allocation
