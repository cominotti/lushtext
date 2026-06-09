## ADDED Requirements

### Requirement: Visual smoke SHALL support same-session paired captures
The visual smoke lane SHALL support scenarios that capture two or more states from the same isolated LushText process and compositor session so protected-region pixel invariants can be compared without cross-launch noise.

#### Scenario: Sidebar minimap pair is captured in one session
- **WHEN** visual smoke runs the minimap/sidebar invariant scenario
- **THEN** it opens the same fixture once, enables the minimap, captures a workspace-sidebar-visible state, toggles the workspace sidebar through the documented action path, waits for visual geometry readiness, and captures the workspace-sidebar-hidden state
- **AND** both screenshots share the same process, renderer, theme, scale factor, font configuration, window size, and fixture state

#### Scenario: State is asserted before every capture
- **WHEN** a paired visual smoke step is about to capture a screenshot
- **THEN** it verifies the active document, requested and rendered surfaces, minimap request state, visual geometry readiness, and any scenario-specific counts through Automation1 or supported actions
- **AND** it fails before accepting the screenshot if the state does not match the scenario

### Requirement: Visual smoke SHALL compare protected crops and masks
The visual smoke lane SHALL compare declared protected regions across paired captures. Regions marked unaffected MUST have exact zero pixel differences after masks and coordinate transforms are applied. Regions marked allowed-changing MUST be checked against their declared geometry relationship instead of ignored.

#### Scenario: Protected chrome comparison fails on nonzero difference
- **WHEN** a paired visual smoke scenario marks header controls or status controls as unaffected
- **THEN** the before and after protected crops match exactly
- **AND** any nonzero difference fails the scenario with crop artifacts and a pixel-difference summary

#### Scenario: Allowed editor movement is checked by anchors
- **WHEN** a sidebar toggle changes the editor allocation
- **THEN** visual smoke treats the editor and minimap body as allowed-changing regions
- **AND** it still asserts that the editor top visible line, minimap top content anchor, status bar, and header controls satisfy their declared geometry invariants

#### Scenario: Unmasked dynamic content is rejected
- **WHEN** a protected region contains dynamic content that changes between paired captures
- **THEN** the scenario either masks the dynamic subregion or fails as an invalid invariant definition
- **AND** the lane does not relax exact comparison for the entire protected region

### Requirement: Visual smoke SHALL preserve comparison artifacts
The visual smoke lane SHALL write reviewable artifacts for every paired visual invariant scenario, including step screenshots, automation snapshots, visual geometry state, masks or crop coordinates, comparison summaries, warning scans, runtime logs, environment reports, and scenario manifests.

#### Scenario: Passing paired scenario records proof chain
- **WHEN** a paired visual smoke scenario passes
- **THEN** its manifest lists each action, readiness wait, screenshot, geometry state file, comparison report, and warning-scan result
- **AND** the summary identifies which protected regions had exact zero pixel differences

#### Scenario: Failing paired scenario preserves diagnostics
- **WHEN** a paired visual smoke scenario fails due to geometry, pixels, readiness, state mismatch, or runtime warnings
- **THEN** the lane preserves all screenshots and logs produced before failure
- **AND** the failure message points to the most useful bounded artifacts

### Requirement: Visual smoke SHALL cover visual-invariant environment axes explicitly
The visual smoke lane SHALL name the environment axes covered for visual invariants and SHALL support at least light/dark style preference, default and constrained window sizes, sidebar on/off states, minimap on/off states where relevant, and word-wrap on/off controls for editor/minimap scenarios.

#### Scenario: Minimap top-edge matrix covers theme and wrapping
- **WHEN** the minimap/sidebar visual invariant scenario is run in its extended form
- **THEN** it covers light and dark style preferences
- **AND** it covers word-wrap enabled and disabled documents at top-of-file

#### Scenario: Unsupported extended axes skip clearly
- **WHEN** the host cannot provide an alternate renderer, scale factor, or compositor feature requested by the extended matrix
- **THEN** the affected axis reports a clear skip reason
- **AND** other supported axes continue to run
