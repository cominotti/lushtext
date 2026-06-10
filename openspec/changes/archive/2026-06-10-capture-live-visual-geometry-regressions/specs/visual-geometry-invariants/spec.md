## ADDED Requirements

### Requirement: Live visual geometry state can be captured as a reproducible scenario
The visual invariant system SHALL provide an agent-facing workflow that records the current live LushText visual-geometry state and emits a runnable visual-geometry scenario plus bounded evidence artifacts. The captured scenario MUST include enough state to reproduce the visible geometry class in headless Mutter without relying on portal screenshots as the proof source.

#### Scenario: Live minimap state emits a runnable scenario
- **WHEN** LushText is running with the minimap visible and the workspace sidebar in a live user-reproduced state
- **THEN** the capture workflow writes a scenario manifest that records the live window size, scale factor, sidebar requested and visible state, minimap requested state, color-scheme mode when known, word-wrap mode when known, active fixture identity when safely available, and intended sidebar action direction
- **AND** the generated scenario can be passed to the visual-geometry smoke runner with `--scenario-dir`

#### Scenario: Ambiguous live state is explicit
- **WHEN** the capture workflow cannot infer a required scenario field such as theme, fixture kind, word-wrap mode, or intended action direction
- **THEN** it records that field as unknown or requires an explicit caller override
- **AND** it does not claim a faithful reproduction scenario was generated from guessed state

#### Scenario: Portal screenshot is not required for proof
- **WHEN** a live capture is performed on a user's desktop session
- **THEN** the required proof artifacts come from Automation1 snapshots and the generated headless visual-geometry scenario
- **AND** any portal screenshot captured for context is marked optional and is not counted as invariant proof

### Requirement: Sidebar visual captures wait for final animated allocations
The visual invariant system SHALL wait for final sidebar and editor allocations before capturing before/after screenshots for workspace-sidebar visibility scenarios. The runner MUST require final geometry to remain stable across multiple samples before comparing rendered pixels.

#### Scenario: Hide waits for fully hidden sidebar
- **WHEN** a minimap/sidebar scenario hides the workspace sidebar
- **THEN** the after capture is not taken until the workspace sidebar allocation has `x == -width`, the editor viewport has `x == 0`, and the relevant editor and minimap rectangles remain stable across multiple visual-geometry snapshots
- **AND** timing out before that state fails the case with bounded sampled geometry evidence

#### Scenario: Show waits for fully visible sidebar
- **WHEN** a minimap/sidebar scenario shows the workspace sidebar
- **THEN** the after capture is not taken until the workspace sidebar allocation has `x == 0`, the editor viewport starts at the sidebar width, and the relevant editor and minimap rectangles remain stable across multiple visual-geometry snapshots
- **AND** timing out before that state fails the case with bounded sampled geometry evidence

#### Scenario: Mid-animation readiness is rejected
- **WHEN** `visual-geometry-settled` reports ready while the workspace sidebar is still between its final hidden and final visible allocations
- **THEN** the visual-geometry runner continues waiting for the final allocation predicate or fails with a state mismatch
- **AND** it does not capture a passing comparison from the intermediate geometry

### Requirement: Minimap sidebar invariants cover intermediate wrapped long-line geometry
The visual invariant system SHALL include minimap/sidebar scenarios that exercise intermediate desktop-sized windows where wrapped long-line minimap projection can cross rendering thresholds. Coverage MUST include the reproduced light-theme, word-wrap-enabled, long plain-line, top-of-file case around `1822x1272`.

#### Scenario: Live-size wrapped long-line regression is covered
- **WHEN** the visual-geometry smoke lane runs the minimap/sidebar top-of-file scenario matrix
- **THEN** it includes a light-theme, word-wrap-enabled, long plain-line fixture at or around `1822x1272`
- **AND** the scenario verifies the native minimap viewport top edge and first content row through screenshot-derived pixel anchors

#### Scenario: Threshold class does not pass by named resolution coverage alone
- **WHEN** 720p, 1080p, 1440p, or `1600x1000` cases pass
- **THEN** the visual-geometry lane still runs the intermediate wrapped long-line case before claiming the minimap/sidebar top invariant is covered
- **AND** skipped or filtered runs report that this threshold class was not verified

### Requirement: Rendered pixels are cross-checked against app-owned geometry anchors
The visual invariant system SHALL distinguish app-owned geometry anchor stability from rendered-pixel stability. For rendered effects such as the native minimap viewport highlight, screenshot-derived pixel anchors MUST be the pass/fail authority, and disagreement with app-owned anchors MUST be reported as a diagnostic failure detail.

#### Scenario: App geometry stable but pixels move
- **WHEN** Automation1 geometry anchors report the same screen Y before and after a sidebar action
- **AND** screenshot-derived minimap top-edge or first-content rows move beyond the manifest threshold
- **THEN** the invariant fails with a pixel-anchor failure
- **AND** the comparison report records both the app-owned anchor rows and the screenshot-derived rows

#### Scenario: Pixel and app geometry agreement is visible
- **WHEN** app-owned geometry anchors and screenshot-derived pixel anchors both remain within the manifest threshold
- **THEN** the comparison report records the matching row positions and marks the rendered invariant as verified
- **AND** the root summary includes the invariant id in `pixel_verified_invariant_ids`

### Requirement: Visual geometry summaries expose actionable per-case evidence
The visual invariant system SHALL make per-case evidence visible in summaries so agents can diagnose failures without manually reconstructing artifact structure. Summaries MUST include invariant ids, pixel verification status, final geometry rows, row deltas, and paths to bounded crop artifacts when available.

#### Scenario: Per-case summary names pixel invariants
- **WHEN** a visual-geometry case verifies screenshot-derived pixel anchors
- **THEN** the case summary records the relevant invariant id in a pixel-verification field
- **AND** the root summary aggregates only invariants that were actually pixel-verified in passing cases

#### Scenario: Failed minimap case points to crops
- **WHEN** a minimap pixel-anchor comparison fails
- **THEN** the failure summary includes before and after row positions, screen Y delta, final sidebar/editor geometry, and relative paths to the minimap top-edge and first-content-row crop artifacts
- **AND** the summary avoids embedding unbounded image or document data
