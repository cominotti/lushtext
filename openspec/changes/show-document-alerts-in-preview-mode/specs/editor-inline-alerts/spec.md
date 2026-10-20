## ADDED Requirements

### Requirement: Document-state inline alerts are visible on every surface that shows the document
Every editor inline alert carried on an editor's inline-alert notification surface SHALL be treated as document-state. While Markdown preview-only mode is active, the system SHALL present the selected tab's current inline alert on a preview surface at the top of the Markdown preview. The preview surface MUST show the same title, body, warning or error style, and action buttons as that editor's own inline alert. It MUST read from the same notification state the editor's alert renders from, with no second copy of alert state. The editor's own inline alert widget MUST NOT be moved or reparented. The preview surface MUST be shown whether the preview displays rendered Markdown, the preparing placeholder, or the non-Markdown placeholder.

#### Scenario: External change in preview-only mode shows the alert on the preview surface
- **WHEN** a Markdown document is open in the selected tab
- **AND** preview-only mode is active
- **AND** another process modifies the backing file on disk
- **THEN** the preview surface shows a warning alert titled "File Has Changed on Disk"
- **AND** the alert exposes the "Discard Changes and Reload" action and a dismiss control

#### Scenario: Alert already present when preview-only mode is entered
- **WHEN** the selected tab's editor shows an inline alert
- **AND** the user enters preview-only mode with `Alt+P`
- **THEN** the preview surface shows the same alert title, body, style, and buttons

#### Scenario: Alert on a non-Markdown document in preview-only mode
- **WHEN** preview-only mode is active
- **AND** the selected tab is a non-Markdown document that owns an inline alert
- **THEN** the preview shows its "Not a Markdown file" placeholder
- **AND** the preview surface shows the document's alert above the placeholder

#### Scenario: Leaving preview-only mode hides the preview surface
- **WHEN** the preview surface shows an alert
- **AND** the user leaves preview-only mode
- **THEN** the preview surface is hidden
- **AND** the editor's own inline alert shows the same alert

### Requirement: Exactly one inline alert banner is visible for the selected document in every layout
The system SHALL render the selected document's inline alert on exactly one visible surface. In editor-only and side-by-side layouts that surface MUST be the editor's own inline alert, and the preview surface MUST stay hidden. In preview-only mode it MUST be the preview surface, and the editor's alert MUST be neither visible nor exposed to assistive technology.

#### Scenario: Side-by-side shows exactly one alert
- **WHEN** side-by-side Markdown preview is active
- **AND** the selected document owns an inline alert
- **THEN** the editor's own inline alert is visible
- **AND** the preview surface is not visible
- **AND** exactly one visible widget in the window shows that alert's title

#### Scenario: Preview-only shows exactly one alert
- **WHEN** preview-only mode is active
- **AND** the selected document owns an inline alert
- **THEN** the preview surface is visible
- **AND** the editor's own inline alert is not mapped
- **AND** exactly one accessible object with the alert role and that title is showing

### Requirement: Preview-surface alert actions run the editor's own handlers and update both surfaces
Activating a button on the preview surface SHALL run the same handler as activating the matching button on the selected editor's own inline alert: Retry, the warning-primary action (discard and reload, draft discard, normalize line endings, undo local-history restore, or show in local history), the warning-secondary save action, and dismiss. After either surface's action changes the notification state, both surfaces MUST re-render from that state. A preview-surface action MUST run only if the selected tab and its current alert are still the ones that were projected. Otherwise it MUST run no handler and MUST re-project the current state.

#### Scenario: Reload from the preview surface updates the preview
- **WHEN** preview-only mode shows "File Has Changed on Disk" for a Markdown document
- **AND** the user activates "Discard Changes and Reload" on the preview surface
- **THEN** the editor reloads the current bytes from disk
- **AND** the rendered Markdown preview shows the reloaded content
- **AND** neither the preview surface nor the editor's own inline alert shows the warning

#### Scenario: Dismiss from the preview surface clears the editor alert
- **WHEN** the preview surface shows an alert for the selected document
- **AND** the user activates dismiss on the preview surface
- **THEN** the preview surface is hidden
- **AND** after leaving preview-only mode the editor's own inline alert is hidden
- **AND** the document's buffer content is unchanged

#### Scenario: Acting on the editor alert clears the preview surface
- **WHEN** the editor's own inline alert is dismissed or resolved while the document is selected
- **AND** the user then enters preview-only mode
- **THEN** the preview surface is not shown

#### Scenario: Save from the preview surface on a restored draft
- **WHEN** preview-only mode shows the "Draft Changes Restored" alert
- **AND** the user activates "Save…" on the preview surface
- **THEN** the same save workflow runs as from the editor's own alert

#### Scenario: A stale preview-surface action runs nothing
- **WHEN** the preview surface was projected for one tab's alert
- **AND** the selection changes to another tab before a preview-surface button is activated in the same main-loop turn
- **THEN** no inline-alert handler runs for either tab
- **AND** the preview surface is re-projected for the newly selected tab

#### Scenario: Focus after a preview-surface action resolves the alert
- **WHEN** keyboard focus is on a preview-surface button
- **AND** activating it resolves the alert so the preview surface hides
- **THEN** keyboard focus moves to the Markdown preview content, not to the sidebar

### Requirement: Switching tabs re-projects the preview-surface alert
While preview-only mode is active, the system SHALL re-project the preview surface whenever the selected tab changes, including when the selected tab is closed and when a session-restore batch completes.

#### Scenario: Switching to a tab with an alert shows it
- **WHEN** preview-only mode is active
- **AND** tab A has no alert and tab B owns an inline alert
- **AND** the user selects tab B
- **THEN** the preview surface shows tab B's alert

#### Scenario: Switching to a tab without an alert clears it
- **WHEN** preview-only mode is active and the preview surface shows tab B's alert
- **AND** the user selects tab A, which has no alert
- **THEN** the preview surface is hidden
- **AND** tab B's alert remains owned by tab B and reappears when tab B is selected again

#### Scenario: Closing the selected tab re-projects for the next selection
- **WHEN** preview-only mode is active and the preview surface shows the selected tab's alert
- **AND** the user closes that tab and another tab without an alert becomes selected
- **THEN** the preview surface is hidden

### Requirement: An inline alert is announced once per appearance, not once per surface
The system SHALL announce an inline alert to assistive technology once when it first appears for its editor. The announcement MUST come from the surface the user can currently perceive: the preview surface when preview-only mode is active and the editor is selected, otherwise the editor's own inline alert. Re-rendering an unchanged alert, switching layouts, switching tabs, or another tab's notification activity MUST NOT announce it again. An alert whose title, body, or style changes, or one that reappears after being cleared, MUST be announced again. Error alerts MUST keep the high-priority alert lane, and warnings MUST keep the status-update lane.

#### Scenario: External change in preview-only mode is announced once
- **WHEN** preview-only mode is active
- **AND** "File Has Changed on Disk" appears for the selected document
- **THEN** exactly one announcement is emitted, from the preview surface
- **AND** the editor's own inline alert emits none

#### Scenario: Unrelated notification activity does not re-announce an error
- **WHEN** an error alert "Could Not Open File" has been announced for one editor
- **AND** another tab publishes and dismisses its own inline alert
- **THEN** the error alert is not announced again

#### Scenario: Entering preview-only mode does not re-announce
- **WHEN** an inline alert was announced from the editor's own alert
- **AND** the user enters preview-only mode
- **THEN** the preview surface shows the alert
- **AND** no additional announcement is emitted for it

#### Scenario: A changed alert is announced
- **WHEN** an editor's alert is replaced by an alert with a different title or body
- **THEN** the new alert is announced once

### Requirement: The preview-surface alert remains usable in constrained widths
The preview surface SHALL use the same adaptive layout as the editor's own inline alert. The message and action group share one line when the preview is wide. The action group wraps as one unit beneath the message when the preview is narrow. Titles, bodies, and button labels wrap instead of disappearing.

#### Scenario: Narrow preview-only window wraps the actions beneath the message
- **WHEN** preview-only mode is active in a narrow window
- **AND** the preview surface shows "File Has Changed on Disk"
- **THEN** the action group is allocated on its own row beneath the message
- **AND** "Discard Changes and Reload" and dismiss stay adjacent and fully visible

#### Scenario: Wide preview-only window keeps one line
- **WHEN** preview-only mode is active in a wide window
- **AND** the preview surface shows an alert with short text
- **THEN** the message and the action group are allocated on the same row

### Requirement: A successful load resolves the editor's inline alert through the notification state
When a document load completes and clears the editor's inline alert, the system SHALL resolve that editor's inline-alert notification in the shared notification state, not only hide the widget. A later notification projection MUST NOT bring a resolved alert back on any surface.

#### Scenario: A resolved alert does not reappear after an unrelated projection
- **WHEN** an editor owns an inline alert
- **AND** a load of that document completes successfully through a path that did not dismiss the alert first
- **AND** another tab then publishes an inline alert
- **THEN** the first editor's inline alert stays hidden
- **AND** the preview surface does not show it when that editor is selected in preview-only mode
