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

### Requirement: Every font-following surface takes the zoom through its class
Each widget that follows the editor font SHALL be registered when it is
created and SHALL then carry the class of the current zoom level, so that a
widget created after a zoom step (a new tab, a search-result row realized
later, a Markdown preview rendered later, the file peek) shows the current
size. The computed font family, size and editor line height of every
registered widget SHALL equal what the display-wide rules produce on `main`
for the same font, zoom and system-font state; with the system font at 100 %,
auxiliary `.monospace` widgets SHALL keep GTK's default font. The same SHALL
hold under the desktop text scale and the high-contrast style, and the Focus
Mode readable-column margins, which derive from the editor's Pango character
width, SHALL equal `main`'s at every zoom level. Application code SHALL NOT
add `.monospace` or enable `GtkTextView:monospace` (in Rust or in Blueprint)
except through the registering helper of `ui/font_zoom.rs`, or, for a
Blueprint widget, with a registration in the owning widget's construction that
the policy check maps to it. The workspace file tree uses the default font and does
not follow the editor font.

#### Scenario: A widget created after a zoom step takes the current size
- **WHEN** the user zooms to 130 % and then runs a workspace search
- **THEN** the result rows' line labels use the zoomed monospace size, the same
  as on `main` at 130 %

#### Scenario: The default state is unchanged
- **WHEN** the system font is used at 100 %
- **THEN** search results, the file peek, the minimap and Markdown code blocks
  use GTK's default monospace font, and the editor uses the system monospace
  family with its line-height guard

#### Scenario: Focus Mode margins follow the zoom as on main
- **WHEN** Focus Mode is active and the user zooms from 100 % to 130 %
- **THEN** the editor's readable-column margins equal those `main` computes at
  130 % for the same width and target columns

#### Scenario: No monospace widget is left unregistered at run time
- **WHEN** a window with a populated search panel, an open file peek, a
  Markdown preview with a code block and an image fallback, the notes browser,
  the local-history browser and the Line Inspector is walked
- **THEN** every widget carrying `.monospace` or `GtkTextView:monospace` is
  registered

#### Scenario: An unregistered monospace widget fails the policy check
- **WHEN** application code adds the `monospace` class to a widget without
  registering it
- **THEN** `make check-policy` fails and names the call site

