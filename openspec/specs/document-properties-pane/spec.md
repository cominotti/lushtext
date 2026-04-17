## ADDED Requirements

### Requirement: Header bar provides the document properties toggle
The system SHALL expose document properties through a toggle button at the end of the main header bar using the `info-outline-symbolic` icon. The toggle MUST reflect whether the document properties surface is open, MUST use the tooltip `Document Properties`, and MUST own the `F9` shortcut.

#### Scenario: Open document properties from the header bar
- **WHEN** a document tab is active and the user activates the header-bar info button
- **THEN** the document properties surface opens for the active document
- **AND** the toggle enters its active state

#### Scenario: Toggle document properties with F9
- **WHEN** the main window has focus and the user presses `F9`
- **THEN** the system toggles the same document properties surface controlled by the header-bar button
- **AND** the shortcut does not toggle the workspace sidebar instead

### Requirement: Document properties surface owns current-document metadata and controls
The system SHALL use the document properties surface as the home for slower, inspectable document information rather than for the bottom bar's quick editor-state controls. The surface MUST present the active document's metadata when a file-backed tab is selected and MUST present a graceful empty or unavailable state when no active document can supply a field.

#### Scenario: File-backed document shows metadata in the document properties surface
- **WHEN** a file-backed document is active and the document properties surface is opened
- **THEN** the surface shows slower document-inspection fields such as path or location, file size, formatting source, statistics, and file-health details
- **AND** those values correspond to the active tab

#### Scenario: Untitled or empty states stay explicit
- **WHEN** the active tab is untitled or no document is selected
- **THEN** the document properties surface keeps its structure visible
- **AND** unavailable fields use explicit empty-state copy instead of stale data from a previously selected document

### Requirement: Document properties surface does not replace quick bottom-bar editor state
The system SHALL keep the document properties surface distinct from the bottom bar's quick editor-state strip. The surface MUST NOT duplicate the bottom bar's encoding or line-ending controls as primary controls for the same active document.

#### Scenario: Opening document properties does not duplicate quick encoding controls
- **WHEN** a file-backed document is active and the user opens document properties
- **THEN** the bottom bar remains the primary home for quick encoding and line-ending controls
- **AND** the document properties surface does not present duplicate primary controls for those same quick editor-state actions

### Requirement: Bottom bar keeps quick editor state while avoiding slower document-detail duplication
The system SHALL keep the bottom bar focused on shell feedback and quick, glanceable editor state. The bottom bar MUST continue to host the workspace toggle, status or feedback messaging, the active document's encoding and line-ending state, and a terse `EditorConfig` badge when per-file overrides are active. It MUST NOT expose a separate document-properties toggle or duplicate slower document-inspection fields that are owned by the document properties surface.

#### Scenario: Bottom bar keeps shell controls and quick editor state
- **WHEN** the main window is rendered with one or more open tabs
- **THEN** the bottom bar still exposes the workspace toggle, status or feedback area, quick encoding and line-ending state, and any active `EditorConfig` badge
- **AND** those controls remain usable whether or not the document properties surface is open

#### Scenario: Bottom bar does not duplicate slower document properties
- **WHEN** a file-backed document is active after this change
- **THEN** the bottom bar does not show a separate properties toggle
- **AND** it does not duplicate slower document-inspection fields such as path, file size, formatting-source summary, or statistics that belong to the document properties surface

### Requirement: Preferences remain the home for app-wide editor defaults
The system SHALL keep app-wide editor defaults in Preferences instead of in the document properties surface. Controls such as `Use EditorConfig`, word wrap, line numbers, current-line highlight, and default tabs or spaces behavior MUST remain accessible through Preferences rather than being redefined as per-document properties by this change.

#### Scenario: Document properties does not become a second preferences surface
- **WHEN** the user opens the document properties surface after this change
- **THEN** app-wide editor defaults remain absent from that surface
- **AND** those defaults continue to be configured from Preferences

### Requirement: EditorConfig state is split by surface role
The system SHALL present `EditorConfig` through three different surfaces according to intent. The bottom bar MUST provide a terse badge when per-file overrides are active, the document properties surface MUST present a richer formatting-source explanation, and Preferences MUST remain the only place where the global `Use EditorConfig` toggle is changed.

#### Scenario: EditorConfig override is glanceable and explainable
- **WHEN** a file-backed document is active, `Use EditorConfig` is enabled, and that file has one or more resolved formatting overrides
- **THEN** the bottom bar shows an `EditorConfig` badge
- **AND** the document properties surface shows formatting-source copy indicating that an `EditorConfig` override is active

#### Scenario: Global EditorConfig toggle stays in Preferences
- **WHEN** the user wants to enable or disable `EditorConfig` globally after this change
- **THEN** the control is available in Preferences
- **AND** the document properties surface does not expose a second global `Use EditorConfig` toggle

### Requirement: Document properties uses adaptive GNOME-like presentations
The system SHALL present document properties as a right-side utility pane on spacious layouts and as a bottom sheet on compact layouts, while preserving the same content contract and header-bar toggle.

#### Scenario: Spacious layout uses a right-side pane
- **WHEN** the window is wide enough to support the workspace sidebar, editor content, and document properties side by side
- **THEN** opening document properties shows them in a right-side pane
- **AND** the main editor content remains visible alongside both panes

#### Scenario: Compact layout uses a bottom sheet
- **WHEN** the window is too narrow to keep document properties beside the editor without crowding the content
- **THEN** the same header-bar toggle opens document properties as a bottom sheet
- **AND** the bottom sheet shows the same document-properties content categories instead of a reduced alternative surface

### Requirement: Pane-to-sheet switching uses the existing dynamic editor-width guard
The system SHALL switch the document properties surface from right-side pane mode to bottom-sheet mode using the existing editor-width guard that already accounts for whether the workspace sidebar is consuming width and how wide that sidebar is. The system MUST NOT replace that behavior with a new fixed magic breakpoint.

#### Scenario: No workspace-width consumption keeps the narrower guard
- **WHEN** the workspace sidebar is not consuming layout width
- **AND** the total window width is `912sp` or narrower
- **THEN** document properties use the compact bottom-sheet presentation instead of a right-side pane

#### Scenario: Default workspace layout widens the guard
- **WHEN** the workspace sidebar is consuming width using the default `Comfy` preset
- **AND** the total window width is `1350sp` or narrower
- **THEN** document properties use the compact bottom-sheet presentation instead of a right-side pane

### Requirement: Compact layouts show only one secondary surface at a time
The system SHALL prevent the workspace sidebar and the document properties surface from remaining visible together in compact layouts. The workspace sidebar MUST yield to document properties when document properties are opened, and document properties MUST close if the workspace sidebar is explicitly opened instead.

#### Scenario: Opening document properties closes the workspace sidebar in compact mode
- **WHEN** the window is in a compact layout, the workspace sidebar is visible, and the user opens document properties
- **THEN** the workspace sidebar closes
- **AND** document properties open in their compact presentation

#### Scenario: Opening the workspace sidebar closes document properties in compact mode
- **WHEN** the window is in a compact layout, document properties are visible, and the user opens the workspace sidebar
- **THEN** document properties close
- **AND** the workspace sidebar becomes the active secondary surface

### Requirement: Temporary compact suppression does not discard desktop visibility intent
The system SHALL treat compact mutual exclusion as layout arbitration rather than as an implicit permanent preference change. When the window returns to a spacious layout, each surface's visibility MUST reflect the most recent explicit user choice instead of the temporary suppression needed during compact mode.

#### Scenario: Widening restores both explicitly open surfaces
- **WHEN** the workspace sidebar and document properties were both explicitly open on a spacious layout
- **AND** the window shrinks into compact mode so the workspace sidebar is temporarily suppressed while document properties stay open
- **AND** the window widens back to a spacious layout without any new explicit pane toggle
- **THEN** both surfaces become visible again

#### Scenario: Explicit compact toggles update the restored state
- **WHEN** the window is in compact mode and the user explicitly closes document properties before widening the window again
- **THEN** the widened layout keeps document properties closed
- **AND** only surfaces the user last explicitly left open are restored

### Requirement: This delta does not introduce document type or language controls
The system SHALL NOT add a `Document Type` row, language picker, or other document-language control to the document properties surface as part of this change.

#### Scenario: Document properties stays focused on the agreed scope
- **WHEN** the user opens document properties for a file-backed document after this change
- **THEN** the surface does not show a `Document Type` row or language picker
- **AND** the surface remains focused on the slower document-inspection details defined by this delta
