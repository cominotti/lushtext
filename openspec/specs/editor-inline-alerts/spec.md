# editor-inline-alerts Specification

## Purpose
Define how editor inline alerts present recoverable document messages, workflow actions, dismiss controls, GTK-supported rendering, constrained-width usability, and button contrast inside the editor surface.

## Requirements

### Requirement: Editor inline alerts present persistent recoverable document messages
The system SHALL present editor-scoped warning and error messages as inline alerts above the active editor content. Inline alerts MUST remain visible until the owning workflow resolves them, the user dismisses them, or a newer editor-scoped inline alert replaces them. Inline alerts MUST include a title and body text when provided by the notification payload.

#### Scenario: Show a recoverable load error
- **WHEN** a document load fails with a recoverable error
- **THEN** the editor shows an error inline alert above the editor content
- **AND** the alert title and body describe the load failure
- **AND** the alert remains visible until dismissed, retried, resolved, or replaced

#### Scenario: Show a restored draft warning
- **WHEN** unsaved draft content is restored into an editor
- **THEN** the editor shows a warning inline alert above the editor content
- **AND** the alert remains visible until the draft is saved, discarded, dismissed, resolved, or replaced

### Requirement: Editor inline alerts support the current recovery actions
The system SHALL expose the action buttons provided by the editor-scoped inline notification payload. Error alerts MUST support a primary retry action when the payload provides one. Warning alerts MUST support a primary recovery action and a secondary save action when the payload provides them. Missing action labels MUST hide the corresponding buttons without leaving unusable empty controls.

#### Scenario: Error alert exposes retry
- **WHEN** an error inline alert includes a primary action labeled `_Retry`
- **THEN** the alert shows a visible retry button with that label
- **AND** activating the button invokes the editor retry workflow

#### Scenario: Warning alert exposes discard and save
- **WHEN** a warning inline alert includes primary and secondary action labels
- **THEN** the alert shows both action buttons with those labels
- **AND** activating the primary action invokes the warning recovery workflow
- **AND** activating the secondary action invokes the save workflow

#### Scenario: Warning alert can be informational only
- **WHEN** a warning inline alert includes no primary or secondary action labels
- **THEN** the alert shows the title and body
- **AND** the alert does not show empty action buttons

### Requirement: Dismissing an editor inline alert clears the owning notification
The system SHALL provide an explicit dismiss affordance for editor inline alerts. Activating dismiss MUST clear the visible alert and resolve the editor-scoped notification for that editor without affecting unrelated status-bar notifications or inline alerts owned by other editors.

#### Scenario: Dismiss clears the current editor alert
- **WHEN** an editor inline alert is visible
- **AND** the user activates its dismiss control
- **THEN** the alert is hidden for that editor
- **AND** the editor-scoped inline notification is cleared

#### Scenario: Dismiss does not clear another editor alert
- **WHEN** two editors each own an inline alert
- **AND** the user dismisses the alert in one editor
- **THEN** the dismissed editor no longer shows that alert
- **AND** the other editor's inline alert remains available when that editor is active

### Requirement: Editor inline alerts remain usable in constrained editor widths
The system SHALL keep inline alert titles, body text, and action labels readable when the editor column narrows. Text MUST wrap instead of disappearing, and action buttons MUST remain visible when their corresponding actions are present.

#### Scenario: Warning actions remain visible in a narrow editor
- **WHEN** a warning inline alert with primary and secondary actions is shown in a narrow editor column
- **THEN** both action buttons remain visible
- **AND** their labels wrap instead of forcing the alert off screen

#### Scenario: Error action remains visible in a narrow editor
- **WHEN** an error inline alert with a retry action is shown in a narrow editor column
- **THEN** the retry button remains visible
- **AND** the alert text wraps within the editor column

### Requirement: Editor inline alerts use GTK5-supported widgets
The system SHALL implement editor inline alerts without `GtkInfoBar` or other GTK widgets listed for GTK5 removal. The replacement MUST use supported GTK or Libadwaita widgets while preserving warning and error visual distinction.

#### Scenario: Inline alert template contains no GtkInfoBar
- **WHEN** the editor inline alert UI template is loaded
- **THEN** it does not instantiate `GtkInfoBar`
- **AND** it uses supported GTK or Libadwaita widgets to render the alert content and actions

#### Scenario: Warning and error alerts are visually distinct
- **WHEN** a warning inline alert is shown
- **THEN** it is styled as a warning surface
- **WHEN** an error inline alert is shown
- **THEN** it is styled as an error surface

### Requirement: Inline alert controls share one horizontal action group
The system SHALL present every visible editor inline-alert control in one horizontal action group. The dismiss control MUST appear in the same action group as workflow action buttons and MUST be ordered after retry, discard, save, normalize, or other workflow actions.

#### Scenario: Restored draft alert groups discard save and dismiss
- **WHEN** a "Draft Changes Restored" warning inline alert includes discard and save actions
- **THEN** the alert shows `Discard...`, `Save...`, and dismiss controls in one horizontal action group
- **AND** the dismiss control appears after the save control

#### Scenario: Error alert groups retry and dismiss
- **WHEN** an error inline alert includes a retry action
- **THEN** the alert shows retry and dismiss controls in one horizontal action group
- **AND** the dismiss control appears after the retry control

#### Scenario: Informational warning still exposes dismiss in the action group
- **WHEN** a warning inline alert includes no workflow action labels
- **THEN** the alert shows no empty workflow action buttons
- **AND** the alert still shows the dismiss control in the horizontal action group

### Requirement: Inline alert controls remain horizontally grouped in constrained widths
The system SHALL keep visible editor inline-alert controls adjacent in a horizontal group. The action group MAY wrap onto its own row beneath the alert message when the editor column is too narrow to fit the message and controls on one line, but it MUST wrap as one atomic unit. Alert message text MAY wrap above the controls, and visible controls MUST NOT be split into separate rows or separated into unrelated alert regions.

#### Scenario: Warning controls remain adjacent in a narrow editor
- **WHEN** a warning inline alert with primary, secondary, and dismiss controls is shown in a narrow editor column
- **THEN** all visible controls remain in one horizontal group
- **AND** each visible control receives a positive allocation

#### Scenario: Error controls remain adjacent in a narrow editor
- **WHEN** an error inline alert with retry and dismiss controls is shown in a narrow editor column
- **THEN** retry and dismiss remain in one horizontal group
- **AND** each visible control receives a positive allocation

#### Scenario: Action group wraps as one unit beneath the message
- **WHEN** an inline alert's message and action group cannot fit on one line
- **THEN** the entire action group wraps onto its own row beneath the message
- **AND** the action group's buttons remain in one horizontal group on that row

### Requirement: Editor inline alerts adapt message and action placement to editor width
The system SHALL present the alert message and the action group on a single horizontal line when the editor column is wide enough to fit both, and SHALL wrap the action group onto its own row beneath the message when the column is not wide enough. The adaptive layout MUST be implemented with a supported libadwaita wrapping container and MUST NOT reintroduce `GtkInfoBar`.

#### Scenario: Wide editor shows message and actions on one line
- **WHEN** an inline alert is shown in an editor column wide enough for the message and action group
- **THEN** the message and the action group occupy the same horizontal line
- **AND** the action group is aligned to the trailing edge

#### Scenario: Narrow editor wraps actions beneath the message
- **WHEN** an inline alert is shown in an editor column too narrow for the message and action group on one line
- **THEN** the action group wraps onto its own row beneath the message
- **AND** the message text remains readable and wraps within the column

#### Scenario: Adaptive layout uses a supported wrapping container
- **WHEN** the editor inline alert template is loaded
- **THEN** the message and action group are hosted by a libadwaita `AdwWrapBox`
- **AND** the template does not instantiate `GtkInfoBar`

### Requirement: Inline alert buttons have subtle contrast against alert surfaces
The system SHALL style editor inline-alert buttons so their resting, hover, and active states are discernible against warning and error alert backgrounds. The styling MUST remain scoped to inline-alert buttons and MUST preserve Adwaita-compatible warning and error surfaces.

#### Scenario: Warning alert buttons are distinguishable from the warning background
- **WHEN** a warning inline alert is shown
- **THEN** each visible inline-alert button has a subtle surface or border that distinguishes it from the warning background
- **AND** hovering a button makes that distinction slightly stronger

#### Scenario: Error alert buttons are distinguishable from the error background
- **WHEN** an error inline alert is shown
- **THEN** each visible inline-alert button has a subtle surface or border that distinguishes it from the error background
- **AND** hovering a button makes that distinction slightly stronger

#### Scenario: Alert button styling is scoped
- **WHEN** inline-alert button styling is applied
- **THEN** the styling is scoped to buttons inside editor inline alerts
- **AND** unrelated editor, sidebar, status-bar, dialog, and search-panel buttons are not targeted by the inline-alert contrast rules
