## ADDED Requirements

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
