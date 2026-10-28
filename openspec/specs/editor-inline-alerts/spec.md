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

### Requirement: Document-state inline alerts are visible on every surface that shows the document
Every editor inline alert carried on an editor's inline-alert notification surface SHALL be treated as document-state. While Markdown preview-only mode is active, the system SHALL present the selected tab's current inline alert on a preview surface at the top of the Markdown preview. The preview surface MUST show the same title, body, warning or error style, and action buttons as that editor's own inline alert. It MUST read from the same notification state the editor's alert renders from, with no second copy of alert state. The editor's own inline alert widget MUST NOT be moved or reparented. The Markdown preview widget MUST NOT host the preview surface, because other workflows reuse that widget. The preview surface MUST be shown whether the preview displays rendered Markdown, the preparing placeholder, or the non-Markdown placeholder.

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
The system SHALL render the selected document's inline alert on exactly one visible surface. In editor-only and side-by-side layouts that surface MUST be the editor's own inline alert, and the preview surface MUST stay hidden. The side-by-side preview MUST stay a pane beside the editor column, never an overlay that covers the editor's alert. In preview-only mode it MUST be the preview surface, and the editor's alert MUST be neither visible nor exposed to assistive technology.

#### Scenario: Side-by-side shows exactly one alert
- **WHEN** side-by-side Markdown preview is active
- **AND** the selected document owns an inline alert
- **THEN** the editor's own inline alert is visible
- **AND** the preview surface is not visible
- **AND** exactly one visible widget in the window shows that alert's title

#### Scenario: A narrow side-by-side window keeps the editor alert uncovered
- **WHEN** side-by-side Markdown preview is active in the narrowest supported window
- **AND** the selected document owns an inline alert
- **THEN** the editor's own inline alert is allocated inside the window and does not intersect the preview pane's allocation
- **AND** the preview surface is not visible

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
- **AND** the selection changes to another tab while tab-derived projections are deferred
- **AND** a preview-surface button is activated before the deferred projections are rebuilt
- **THEN** no inline-alert handler runs for either tab
- **AND** the preview surface is re-projected for the newly selected tab

#### Scenario: An action after an ordinary tab switch acts on the newly shown alert
- **WHEN** preview-only mode is active and the user selects another tab that owns an alert
- **AND** the user activates a button on the preview surface
- **THEN** the handler runs for the newly selected tab's alert, which is the one the preview surface shows

#### Scenario: Focus after a preview-surface action resolves the alert
- **WHEN** keyboard focus is on a preview-surface button
- **AND** activating it resolves the alert so the preview surface hides while preview-only mode stays active
- **THEN** keyboard focus moves to the Markdown preview text when it is visible
- **AND** otherwise the window has no focus widget
- **AND** in neither case does focus move to the workspace sidebar

#### Scenario: Focus when preview-only mode ends on a preview-surface button
- **WHEN** keyboard focus is on a preview-surface button
- **AND** the user leaves preview-only mode, so the preview surface hides and the editor's own bar shows the alert
- **THEN** keyboard focus moves to the selected document's editor
- **AND** it does not stay on the hidden preview-surface button

#### Scenario: A resolve not started from the preview surface moves no focus
- **WHEN** preview-only mode is active, the preview surface shows an alert, and the window has no focus widget
- **AND** the alert is resolved by something other than a preview-surface button
- **THEN** the window still has no focus widget

#### Scenario: Focus after a dialog-routed action resolves the alert
- **WHEN** the user activates "Normalize…" on the preview surface and chooses a line ending in the dialog
- **AND** the dialog closes and the alert is resolved
- **THEN** keyboard focus is not on the workspace sidebar

#### Scenario: Escape does not dismiss the preview-surface alert
- **WHEN** the preview surface shows an alert and no transient surface is open
- **AND** the user presses Escape
- **THEN** the alert remains shown

### Requirement: The warning-primary action stays bound to its alert until the alert changes
Activating the warning-primary button on either surface SHALL NOT consume the editor's pending warning action. The pending action MUST stay bound to the editor's current alert until another alert is emitted for that editor. A warning-primary action whose dialog is cancelled MUST leave the same button running the same action again.

#### Scenario: Cancelling the line-ending dialog keeps Normalize bound
- **WHEN** an editor shows "Mixed Line Endings Detected"
- **AND** the user has unsaved edits
- **AND** the user activates "Normalize…" and cancels the line-ending dialog
- **AND** the user activates "Normalize…" again
- **THEN** the line-ending dialog opens again
- **AND** the document is not reloaded and the unsaved edits remain

### Requirement: Each window projects only its own selected tab
Every application window SHALL project its own selected tab's alert onto its own preview surface, from its own notification state, and SHALL announce only its own alerts.

#### Scenario: Two windows in different modes
- **WHEN** window A is in preview-only mode with a selected document that owns an alert
- **AND** window B is in editor-only mode with a selected document that owns a different alert
- **THEN** window A's preview surface shows window A's alert
- **AND** window B's preview surface is hidden and window B's editor alert shows window B's alert

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

### Requirement: An inline alert is announced once, when it first becomes perceivable
The system SHALL announce an inline alert to assistive technology exactly once, at the first moment it is perceivable. An alert is perceivable on the preview surface when its editor is the window's selected tab and preview-only mode is active. It is perceivable on the editor's own inline alert when its editor is selected and preview-only mode is not active. It is perceivable on no surface when its editor is not selected. The announcement MUST come from the surface where the alert is perceivable. An alert raised on a non-selected tab MUST NOT be announced when it is raised. It MUST be announced when its tab is first selected. Re-rendering an unchanged alert, switching layouts or tabs after it was announced, and another tab's notification activity MUST NOT announce it again. An alert whose title, body, or style changes, or one that reappears after being cleared, MUST be announced again. Error alerts MUST keep the high-priority alert lane, and warnings MUST keep the status-update lane.

#### Scenario: External change in preview-only mode is announced once
- **WHEN** preview-only mode is active
- **AND** "File Has Changed on Disk" appears for the selected document
- **THEN** exactly one announcement is emitted, from the preview surface
- **AND** the editor's own inline alert emits none

#### Scenario: Unrelated notification activity does not re-announce an error
- **WHEN** an error alert "Could Not Open File" has been announced for the selected editor
- **AND** another tab publishes and dismisses its own inline alert, and a status message is published
- **THEN** the error alert is not announced again

#### Scenario: Entering preview-only mode does not re-announce
- **WHEN** an inline alert was announced from the editor's own alert
- **AND** the user enters preview-only mode
- **THEN** the preview surface shows the alert
- **AND** no additional announcement is emitted for it

#### Scenario: A background-tab alert is announced when its tab is selected
- **WHEN** an alert is raised for a tab that is not selected
- **THEN** no announcement is emitted when it is raised
- **AND** when the user then selects that tab, exactly one announcement is emitted for it, from the surface the current layout shows

#### Scenario: Switching back to an announced alert does not re-announce
- **WHEN** a tab's alert was announced while the tab was selected
- **AND** the user selects another tab and then selects that tab again
- **THEN** no additional announcement is emitted for that alert

#### Scenario: A changed alert is announced
- **WHEN** the selected editor's alert is replaced by an alert with a different title or body
- **THEN** the new alert is announced once

### Requirement: A status message is announced when it is published, not when it is re-rendered
The status lane SHALL announce a warning or error status message only when the message is published or updated and wins the lane, using its existing lane and throttling. Re-rendering the lane during an expiry sweep, a resolve, a heartbeat, an inline-alert publish, a tab switch, or a layout change MUST NOT announce the visible status message again.

#### Scenario: A visible error status is not re-announced by unrelated passes
- **WHEN** an error status message has been published and announced
- **AND** an inline alert is published on another tab and the user switches tabs before the status message expires
- **THEN** the error status message is not announced again

### Requirement: The preview-surface alert remains usable in constrained widths
The preview surface SHALL use the same adaptive layout as the editor's own inline alert. The message and action group share one line when the preview is wide. The action group wraps as one unit beneath the message when the preview is narrow. Titles, bodies, and button labels wrap instead of disappearing.

#### Scenario: Enlarged text in the narrowest preview-only window wraps the actions beneath the message
- **WHEN** preview-only mode is active in the narrowest supported window
- **AND** text is scaled to twice its default size
- **AND** the preview surface shows "Draft Changes Restored" with its Discard and Save actions
- **THEN** the action group is allocated on its own row beneath the message
- **AND** the action buttons and dismiss stay adjacent on one row and fully visible

#### Scenario: Default-size text in the narrowest preview-only window keeps the actions usable
- **WHEN** preview-only mode is active in the narrowest supported window at the default text size
- **AND** the preview surface shows an alert
- **THEN** the message wraps its own text before the action group is pushed off its row
- **AND** every action button and dismiss stays fully visible

#### Scenario: Wide preview-only window keeps one line
- **WHEN** preview-only mode is active in a wide window
- **AND** the preview surface shows an alert with short text
- **THEN** the message and the action group are allocated on the same row

### Requirement: A successful load resolves the alerts it supersedes through the notification state
When a document load completes successfully, the system SHALL resolve that editor's inline alert in the shared notification state if the alert was emitted as superseded by a load. That applies to load failures, cancelled loads, external-change warnings, line-ending warnings, restored-draft alerts, local-history restore alerts, and the automatic-recovery limit warning. Recovery alerts that point at a preserved draft ("Draft Not Restored", "Draft Restore Failed") MUST outlive a successful load. A later notification projection MUST NOT bring a resolved alert back on any surface.

#### Scenario: A superseded alert does not reappear after an unrelated projection
- **WHEN** an unmodified open document shows "File Has Changed on Disk"
- **AND** the document reloads successfully through a path that does not dismiss the alert first, such as a Replace All target reload, an eviction reload, or reopening with an encoding
- **AND** a status message or another tab's inline alert is then published
- **THEN** the editor's inline alert stays hidden
- **AND** the preview surface does not show it when that editor is selected in preview-only mode

#### Scenario: A recovery alert survives an eviction reload
- **WHEN** an unmodified document shows "Draft Not Restored"
- **AND** its tab is evicted for memory and then selected again, so it reloads
- **THEN** the "Draft Not Restored" alert is still shown after the reload completes

#### Scenario: A recovery alert is not replaced by a reload's line-ending warning
- **WHEN** an unmodified document with mixed line endings shows "Draft Not Restored"
- **AND** its tab reloads after an eviction
- **THEN** the "Draft Not Restored" alert is still shown after the reload completes
- **AND** "Mixed Line Endings Detected" is not published over it

