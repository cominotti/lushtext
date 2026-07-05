# editor-glyph-rendering Specification

## Purpose

Define the main editor glyph rendering contract, including active-line ink visibility, scoped editor font metrics, live glyph smoke coverage, and fresh bundled resources during developer runs.

## Requirements

### Requirement: Active-line editor glyph ink remains visible while typing
The main editor SHALL render tall glyph top ink while the line is still active during normal typing. The fix MUST NOT depend on pressing Enter, changing focus, clicking outside the window, or waiting for an unrelated redraw to make the glyph appear complete.

#### Scenario: Bracket glyph remains visible before Enter
- **WHEN** the app opens a fresh blank document in the main editor
- **AND** the user types repeated `[` characters on the first line
- **THEN** the active line shows the upper horizontal bracket ink before Enter is pressed
- **AND** the rendered glyphs are not missing their top stroke

#### Scenario: Focus change is not required
- **WHEN** the user continues typing in the same editor window
- **THEN** the glyph top ink remains visible without focusing another window
- **AND** the previous line does not need a later redraw to become readable

### Requirement: Main editor font metrics are scoped and safe
The system SHALL apply the selected editor font through a main-editor-specific metric contract. Editor-only line-height guards MUST NOT leak into minimap, sidebar, or other auxiliary monospace surfaces.

#### Scenario: Selected monospace font still applies
- **WHEN** the user selects a custom editor font or zoom level
- **THEN** the main editor applies that effective font through its editor surface class
- **AND** the editor remains visually monospaced even though `GtkTextView:monospace` is disabled

#### Scenario: Editor line-height protects top ink
- **WHEN** the main editor renders Adwaita Mono at the default font size
- **THEN** the editor line box leaves enough room for upper bracket ink
- **AND** normal editor row spacing remains compact

#### Scenario: Auxiliary monospace surfaces keep their own metrics
- **WHEN** the minimap, file tree, labels, or other widgets use `.monospace`
- **THEN** they do not inherit the main editor's line-height guard
- **AND** their existing compact layout contracts remain unchanged

### Requirement: Live glyph smoke prevents regression
The end-user smoke suite SHALL include a live typing lane that reproduces the active-line bracket case with real key events and screenshot-derived pixel analysis.

#### Scenario: Smoke fails before Enter when top ink is missing
- **WHEN** the smoke opens an isolated fresh document and types repeated `[` key events
- **THEN** it captures the editor before pressing Enter
- **AND** it fails if the active-line crop lacks the expected upper horizontal ink

#### Scenario: Smoke records diagnostic artifacts
- **WHEN** the smoke runs
- **THEN** it writes the before-Enter screenshot, after-Enter screenshot, crops, threshold images, and a JSON summary
- **AND** unsupported hosts report missing tooling instead of silently passing

#### Scenario: CI runs the live glyph lane
- **WHEN** the end-user smoke workflow runs
- **THEN** the editor glyph live lane executes `make editor-glyph-live-smoke`
- **AND** the workflow uploads the lane artifacts

### Requirement: Resource edits are fresh under make run
The build SHALL rebuild compiled resources when the resource manifest or bundled GtkSourceView style resources change, and the bundled style-resource prefix SHALL match the path registered by the app.

#### Scenario: GResource path matches style manager registration
- **WHEN** the app prepends `resource:///dev/cominotti/lushtext/gtksourceview/styles`
- **THEN** the compiled GResource exposes bundled style files at that path

#### Scenario: Resource edits trigger Cargo rebuild
- **WHEN** the GResource XML or bundled GtkSourceView style files change
- **THEN** Cargo reruns the resource build script for `make run`
- **AND** the developer does not need a manual `cargo clean` to see those resource edits
