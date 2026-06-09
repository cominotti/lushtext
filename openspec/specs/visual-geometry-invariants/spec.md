# visual-geometry-invariants Specification

## Purpose
Define LushText's same-session visual geometry invariant system so agents and maintainers can prove protected UI regions remain stable while allowed regions change intentionally.

## Requirements
### Requirement: Visual invariant manifests define protected and allowed regions
The project SHALL define visual geometry invariants through reviewable manifests that name the scenario, fixture state, capture steps, readiness gates, protected regions, allowed-changing regions, and evidence artifacts. Protected regions MUST identify whether exact pixel equality, bounded movement, positive allocation, clipping absence, or coarse nonblank detail is the required proof.

#### Scenario: Manifest declares protected chrome
- **WHEN** a visual invariant scenario compares a sidebar-on capture with a sidebar-off capture
- **THEN** the scenario manifest identifies persistent chrome regions such as header controls and status bar controls that are not expected to repaint or move
- **AND** those protected regions require exact zero pixel differences after coordinate normalization

#### Scenario: Manifest declares allowed movement
- **WHEN** a visual invariant scenario changes workspace sidebar visibility
- **THEN** the scenario manifest identifies editor and minimap regions whose position or width may intentionally change
- **AND** the manifest records the expected direction, anchor, or geometry relationship instead of treating all image differences as failures

#### Scenario: Missing manifest blocks comparison
- **WHEN** a visual scenario captures multiple screenshots but no invariant manifest defines protected and allowed regions
- **THEN** the comparison tooling fails that scenario as incomplete
- **AND** it preserves the screenshots and state artifacts for manual diagnosis without claiming invariant coverage passed

### Requirement: Unaffected regions remain pixel-identical within one visual session
The visual invariant system SHALL compare protected regions from captures produced in the same process, compositor session, theme, scale factor, renderer, fixture, and font environment. For regions marked as unaffected, the system MUST require exact pixel equality after applying declared masks and coordinate transforms.

#### Scenario: Protected header has no variance
- **WHEN** a sidebar visibility scenario toggles the workspace sidebar while the header bar is not expected to change
- **THEN** the protected header crop from the before and after captures matches exactly
- **AND** any nonzero pixel difference fails the scenario with a crop report and diff artifact

#### Scenario: Dynamic regions are masked before comparison
- **WHEN** a protected crop includes caret blink, cursor hover, transient notification pulse, scrollbar animation, or timestamp-like dynamic content
- **THEN** the invariant manifest excludes that subregion from exact comparison
- **AND** the unmasked portion still requires exact pixel equality

#### Scenario: Cross-session captures do not claim exact equality
- **WHEN** captures come from different application launches or different compositor sessions
- **THEN** the tooling does not use those captures for exact protected-region equality
- **AND** it may only apply coarse nonblank, bounds, allocation, and warning checks unless the manifest explicitly declares cross-session tolerance rules

### Requirement: Visual invariant scenarios cover state extremes
Visual invariant coverage SHALL include state extremes for each surface it claims to protect: no required context, representative populated data, many or awkward data, and constrained geometry. Scenarios MUST prove commands remain reachable, empty states remain readable, dense item regions scroll internally, persistent chrome remains visible, and unintended scrollbars or fake rows do not appear.

#### Scenario: No-context surface remains readable
- **WHEN** a visual invariant scenario covers a no-document, no-workspace, no-notes, empty-search, or missing-context state
- **THEN** the captured surface has readable empty-state text and reachable persistent commands
- **AND** no unrelated fake row or unrelated context is introduced only to satisfy the screenshot

#### Scenario: Dense surface scrolls in the item region
- **WHEN** a visual invariant scenario covers many workspace rows, many notes/bookmarks, many command results, or long search results
- **THEN** the item region owns scrolling
- **AND** headers, close buttons, search controls, primary actions, and persistent chrome remain visible

#### Scenario: Constrained geometry preserves promised chrome
- **WHEN** a visual invariant scenario captures a narrow, compact, short, or maximized-like edge geometry
- **THEN** the scenario asserts which chrome is promised visible in that mode
- **AND** the capture fails if that chrome is clipped, overlapped, or pushed outside the window allocation

### Requirement: Visual evidence artifacts are complete and bounded
Every visual invariant run SHALL preserve enough bounded artifacts to diagnose failures without exposing user document contents. Artifacts MUST include screenshots, geometry state, masks or crop definitions, comparison reports, runtime warning scans, environment details, and skip or failure reasons.

#### Scenario: Failed comparison points to evidence
- **WHEN** a protected crop comparison fails
- **THEN** the scenario writes the before crop, after crop, mask or crop coordinates, pixel-difference summary, and scenario manifest
- **AND** the failure output points to those artifacts without embedding unbounded image data in terminal output

#### Scenario: Geometry state stays private
- **WHEN** a visual invariant artifact includes automation geometry state
- **THEN** it records bounded surface names, rectangles, allocation sizes, scroll positions, visibility, and scale factor
- **AND** it does not include document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers

#### Scenario: Unsupported host skips explicitly
- **WHEN** the host lacks required compositor, screenshot, D-Bus, PipeWire, AT-SPI, or image-decoding support
- **THEN** the visual invariant lane reports a clear skip reason
- **AND** skipped invariant coverage is not counted as verified
