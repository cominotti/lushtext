# desktop-visual-smoke-coverage Specification

## Purpose
Define LushText's desktop visual smoke coverage so real-session rendering,
geometry-sensitive surfaces, and diagnostics remain reviewable.

## Requirements
### Requirement: Representative desktop states are captured in a real session
The project SHALL provide a repeatable visual smoke lane that launches LushText
inside an isolated real desktop session and captures representative end-user UI
states that widget allocation assertions cannot fully prove.

#### Scenario: Main editor shell is captured
- **WHEN** the visual smoke lane launches LushText with a normal text document
- **THEN** it captures the main window with the editor, tab strip, header bar,
  status bar, workspace control, and document surface visible
- **AND** it records the runtime, renderer, scale factor, theme, and window size
  used for the capture

#### Scenario: Geometry-sensitive surfaces are captured
- **WHEN** the visual smoke lane exercises geometry-sensitive UI states
- **THEN** it captures at least one narrow or compact layout, one short-window
  layout, one search/minimap state, one Markdown preview state, and one
  document-properties or dialog state

#### Scenario: Intended app state is verified before capture
- **WHEN** a screenshot is captured
- **THEN** the smoke lane first verifies that the intended document and UI state
  are active through stable actions, accessible names, or a narrow read-only
  inspection surface
- **AND** it does not rely only on fixed sleeps or coordinate guesses

### Requirement: Visual smoke asserts stable rendering invariants
The visual smoke lane SHALL validate coarse rendering and geometry invariants
instead of depending on large pixel-perfect golden image sets.

#### Scenario: Capture is nonblank and window-bounded
- **WHEN** the smoke lane captures a screenshot
- **THEN** the captured image is nonblank
- **AND** the app content is inside the expected virtual monitor bounds
- **AND** the capture is stored as an artifact for review

#### Scenario: Persistent chrome remains visible
- **WHEN** the smoke lane captures a short or compact layout
- **THEN** persistent chrome such as the tab strip, header controls, and normal
  mode status bar remains visible unless the tested mode intentionally hides it

#### Scenario: Runtime warnings fail the smoke lane
- **WHEN** the smoke lane finishes
- **THEN** GTK, Libadwaita, GDK, renderer, and accessibility warnings emitted by
  the run are scanned
- **AND** unexpected warnings or criticals fail the lane with the captured logs
  preserved

### Requirement: Theme, scale, and renderer coverage is explicit
The visual smoke lane SHALL document which desktop variations are covered and
which variations are intentionally scheduled, manual, or unsupported.

#### Scenario: Default visual smoke documents its environment
- **WHEN** the default visual smoke command runs
- **THEN** it prints the compositor, toolkit versions, renderer, scale factor,
  theme preference, and font configuration used for the run

#### Scenario: Alternate rendering coverage is available
- **WHEN** maintainers run the extended visual smoke lane
- **THEN** it covers at least one alternate environment dimension such as dark
  style preference, high scale factor, or non-Cairo renderer when supported by
  the host

#### Scenario: Unsupported environment skips are explicit
- **WHEN** a required desktop dependency is unavailable
- **THEN** the smoke lane skips with a clear reason instead of reporting a false
  pass
