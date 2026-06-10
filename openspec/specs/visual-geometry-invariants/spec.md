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

### Requirement: Animation-frame rendered-effect invariants use timestamp-correlated stream proof
Visual geometry animation-frame rendered-effect scenarios SHALL support timestamp-correlated stream proof. Stream capture MUST record screenshot frames, action trigger
timing, Automation1 geometry sample timing, phase labels, and detector results.
For native or toolkit-owned rendered effects, screenshot-derived pixel anchors
MUST be the pass/fail oracle. App-owned geometry MAY bound crops and explain
failures, but it MUST NOT satisfy the invariant when rendered pixels drift.

#### Scenario: Stream capture proves intermediate animation frames
- **WHEN** a visual geometry scenario declares an animation-frame rendered-effect invariant
- **THEN** the runner captures a bounded stream of PNG frames during the action
- **AND** it records timestamped Automation1 geometry samples for the same time window
- **AND** at least one evaluated PNG frame maps to an intermediate transition phase within the declared skew bound
- **AND** the summary reports first frame time, last frame time, sample count, intermediate sample count, mapped intermediate frame count, and maximum sample skew

#### Scenario: Per-frame pixel anchors gate the invariant
- **WHEN** an animation-frame scenario declares required pixel anchors such as the native minimap viewport top edge
- **THEN** every evaluated frame that maps to a protected phase runs those detectors
- **AND** any required anchor missing or drifting outside the declared tolerance fails the scenario
- **AND** the failure records the frame index, timestamp, mapped sample timestamp, anchor rows, row deltas, crop paths, and failure reason

#### Scenario: Stale frame-to-geometry pairing fails
- **WHEN** a captured frame cannot be matched to a geometry sample inside the declared skew bound
- **THEN** that frame cannot be used as passing animation proof
- **AND** the scenario fails if the remaining evaluated frames do not prove the required intermediate phase

#### Scenario: Final-settle-only proof is insufficient for animation invariants
- **WHEN** a scenario protects a rendered effect during animation
- **AND** the artifacts include only before and after captures after final geometry settles
- **THEN** the visual geometry invariant is incomplete
- **AND** proof policy does not count the scenario as verified

### Requirement: Visual proof policy rejects incomplete animation evidence
The visual proof policy SHALL require animation-frame evidence for changes that
touch native minimap rendering, source-map geometry, editor width reflow,
workspace-sidebar consuming animations, animation capture tooling, or proof
policy itself. The policy MUST reject evidence that lacks stream mode,
intermediate mapped frames, required anchors, per-frame pass/fail results,
timing/skew metadata, or bounded failure artifacts.

#### Scenario: Sensitive diff requires animation proof
- **WHEN** a change modifies minimap rendering, source-map geometry, editor-page width reflow, workspace-sidebar animation coordination, animation capture tooling, or visual proof policy
- **THEN** proof policy requires a passing native-minimap animation-frame artifact for the relevant scenario
- **AND** a final-settle artifact alone does not satisfy the requirement

#### Scenario: Negative self-tests cover escaped failure classes
- **WHEN** visual proof policy self-tests run
- **THEN** they include negative cases for final-settle-only evidence, screenshot sampling without stream mode, no mapped intermediate PNG, stale frame/sample pairing, missing required anchors, and rendered pixel drift hidden by acceptable app geometry
- **AND** each negative case fails with a stable status and bounded diagnostic detail

#### Scenario: Unsupported host does not count as verified
- **WHEN** the host cannot provide compositor, screenshot, stream capture, image decoding, or Automation1 timing support required for animation proof
- **THEN** the scenario reports a distinct unsupported status with the missing capability
- **AND** skipped animation coverage is not counted as verified for sensitive visual changes
