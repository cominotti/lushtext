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
- **AND** native minimap/sidebar animation frames record detected native viewport top-edge and first-content-row pixel anchors when visible
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

### Requirement: Rendered effects use screenshot-derived anchors as the oracle
The visual invariant system SHALL use screenshot-derived pixel anchors as the pass/fail oracle for native toolkit-rendered, CSS-rendered, or compositor-rendered visual effects. App-owned geometry MAY define safe bounded crops, readiness metadata, and diagnostics, but it MUST NOT by itself satisfy an invariant for a visible rendered effect.

#### Scenario: Geometry-stable rendered drift fails
- **WHEN** app-owned geometry reports a stable anchor before and after a visual change
- **AND** screenshot-derived pixels for the protected rendered effect move outside the manifest tolerance
- **THEN** the visual comparison fails
- **AND** the report identifies the app-vs-rendered disagreement with both app-owned anchor rows and screenshot-derived rows

#### Scenario: Pixel and app geometry agreement is visible
- **WHEN** app-owned geometry anchors and screenshot-derived pixel anchors both remain within the manifest threshold
- **THEN** the comparison report records the matching row positions and marks the rendered invariant as verified
- **AND** the root summary includes the invariant id in `pixel_verified_invariant_ids`

#### Scenario: Missing pixel anchor fails rendered-effect coverage
- **WHEN** a manifest declares coverage for a rendered visual effect such as the native minimap viewport highlight
- **AND** the runner cannot detect the required screenshot-derived anchor in before or after captures
- **THEN** the scenario fails or skips with an explicit unsupported reason
- **AND** skipped coverage is not counted as verified

#### Scenario: Crop geometry cannot replace pixel proof
- **WHEN** Automation1 exposes a bounded crop for a native rendered effect
- **THEN** the runner uses that crop only to limit screenshot inspection
- **AND** it still evaluates the declared pixel detector and row relationship before marking the invariant passed

#### Scenario: Native minimap detector rejects the reported bad state
- **WHEN** the visual PNG detector is run against the reported good minimap screenshot and the reported bad minimap screenshot
- **THEN** the good screenshot passes the native viewport highlight/content-row anchor relationship check
- **AND** the bad screenshot fails because the native viewport top edge is missing or shifted relative to the first minimap content row

#### Scenario: Live visual scenario compares screenshot-derived anchors
- **WHEN** a same-session visual scenario captures the minimap with the workspace sidebar shown and hidden
- **THEN** the scenario detects the first minimap content row from screenshot pixels
- **AND** it detects the native viewport highlight top edge from screenshot pixels
- **AND** it compares the vertical delta between those anchors across captures instead of accepting app-computed viewport geometry as proof

#### Scenario: Geometry-only evidence cannot satisfy rendered-effect coverage
- **WHEN** a visual-sensitive change touches minimap viewport rendering, minimap CSS, visual-geometry detector logic, or native-highlight scenario manifests
- **THEN** proof policy requires a root visual-geometry summary that lists the native minimap highlight invariant as verified by pixel-anchor assertions
- **AND** a run that reports only bounded rectangles, allocation relationships, or nonblank crops fails the proof-policy check for that invariant

### Requirement: Native minimap threshold coverage is mandatory for visual-sensitive minimap work
The visual invariant system SHALL include targeted native-minimap rendered-highlight scenarios for the reproduced intermediate-size threshold and SHALL require those scenarios for visual-sensitive changes that can affect minimap rendering, source-map geometry, sidebar allocation, editor width reflow, or visual proof tooling.

#### Scenario: Reproduced intermediate case is verified
- **WHEN** the visual geometry smoke lane runs native minimap highlight coverage
- **THEN** it includes the reproduced intermediate-size case around `1822x1272`
- **AND** it verifies sidebar hide and sidebar show directions with screenshot-derived native-highlight anchors

#### Scenario: Passing other sizes does not verify the threshold
- **WHEN** conventional size cases pass
- **AND** the reproduced intermediate-size case is missing, skipped, or filtered out
- **THEN** the native minimap highlight invariant is not counted as verified
- **AND** proof-policy checks fail for changes that require that invariant

#### Scenario: Final rendered frames are stable before comparison
- **WHEN** a sidebar/minimap visual scenario captures before or after screenshots
- **THEN** workflow readiness, final allocation geometry, and native rendered-effect anchor rows have remained stable across the required final samples
- **AND** a mid-animation or stale-frame capture fails with preserved geometry samples and crop artifacts

### Requirement: Visual reports expose rendered-anchor evidence
Visual geometry artifact summaries SHALL expose bounded rendered-anchor evidence for native rendered effects. Reports MUST include scenario id, final geometry, detected anchor rows, row deltas, relationship deltas, app-vs-rendered diagnostics, crop paths, verified invariant ids, and skip or failure reasons.

#### Scenario: Agent can see why native minimap failed
- **WHEN** a native minimap highlight comparison fails
- **THEN** the summary includes before and after screenshot row detections for the native viewport top edge and first minimap content row
- **AND** it includes final sidebar/editor/minimap geometry and app-vs-rendered disagreement details when available

#### Scenario: Passing report proves pixel verification
- **WHEN** a native minimap highlight comparison passes
- **THEN** the summary lists the native minimap invariant id as pixel-verified
- **AND** it records the crop artifacts and detected row relationship used for the pass

#### Scenario: Anchor failures preserve bounded evidence
- **WHEN** a rendered-effect pixel anchor is missing, shifted beyond tolerance, or confused with fill/background pixels
- **THEN** the visual-geometry runner writes bounded before/after crops, detector reports, anchor coordinates, relative-delta results, and failure reasons
- **AND** the artifacts do not expose document text, note bodies, draft bodies, local-history contents, or private persistence identifiers

### Requirement: Animation-frame reports expose bounded evidence
Animation-frame visual reports SHALL expose bounded evidence that explains
rendered minimap movement without embedding unbounded screenshots or document
content. Reports MUST include scenario id, sampled frame count, elapsed frame
times, sidebar/editor/minimap geometry, native minimap diagnostics, detected row
positions, row deltas, crop paths or frame paths, status, and skip/failure
reason when applicable.

#### Scenario: Passing animation report shows stable rows
- **WHEN** a native minimap animation scenario passes
- **THEN** its summary includes the sampled frame count and maximum rendered row drift for each declared anchor
- **AND** it lists the native minimap animation invariant as pixel-verified for animation coverage

#### Scenario: Failing animation report points to the drifting frame
- **WHEN** a sampled animation frame shows native minimap anchor drift outside tolerance
- **THEN** the report identifies the failing frame index, elapsed time, detected row positions, app geometry, and crop or frame artifact
- **AND** it distinguishes app-vs-rendered disagreement from app geometry that moved with the rendered pixels

#### Scenario: Unsupported animation capture skips explicitly
- **WHEN** the host cannot capture animation frames with the required compositor, screenshot, D-Bus, PipeWire, or image tooling
- **THEN** the animation scenario reports a stable unsupported-host reason
- **AND** skipped animation coverage is not counted as verified

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

### Requirement: Rust proof tool preserves invariant migration gates
The visual geometry invariant system SHALL accept `cargo-gtk-proof` as the
authoritative live runner only after a later parity phase proves the existing
invariant semantics: same-session capture, protected-region comparison, masks,
allowed-changing regions, screenshot-derived rendered anchors,
app-vs-rendered diagnostics, final geometry stability waits, warning scans,
skip reasons, and bounded artifacts. During this phase, the Rust tool provides
schema validation, pure PNG/corpus checks, and proof-policy parity while the
Python runner remains authoritative for live visual proof.

#### Scenario: Rust and Python retirement is corpus-gated
- **WHEN** the frozen compatibility corpus is replayed
- **THEN** wrappers remain on the Python live path unless Rust and Python agree
  on every required pass/fail/skip status, summary field, and bounded artifact
  path for the migration corpus
- **AND** the current Rust corpus records compared and failed counts for the
  implemented pure PNG/status fixtures

#### Scenario: Runner migration does not weaken pixel oracle
- **WHEN** a rendered-effect invariant is evaluated by the Rust runner
- **THEN** screenshot-derived pixel anchors remain the pass/fail oracle
- **AND** app-owned geometry is used only to bound crops and explain failures

### Requirement: Visual scenario schema descriptors are versioned and published
Visual geometry scenario manifests and generated artifacts SHALL have
versioned schema descriptors published with the Rust proof tool and linked
from the visual proof documentation. The descriptors MUST cover scenario
manifests, expanded cases, summary files, comparison reports, animation
reports, proof-policy metadata, and result envelopes. Later live-runner work
MUST deepen those descriptors as capture steps, masks, relative anchors, and
animation configuration move fully into Rust.

#### Scenario: Scenario schema validates current manifests
- **WHEN** schema validation runs over `scripts/visual-geometry-scenarios`
- **THEN** every current manifest validates against the supported schema
- **AND** validation failure names the manifest path, schema version, missing
  or malformed field, and stable status

#### Scenario: Artifact schema validates generated summaries
- **WHEN** a visual-geometry run completes
- **THEN** root summaries, case summaries, comparison reports, animation
  reports, and skip summaries validate against their supported schemas
- **AND** unsupported schema versions do not count as verified proof

### Requirement: Animation-frame evidence policy survives migration staging
Animation-frame rendered-effect invariants SHALL retain the current
timestamp-correlated stream proof requirements while Rust migration is staged.
The Rust proof-policy implementation MUST reject summaries missing stream
mode, mapped intermediate PNG frames, per-frame anchor evaluation, skew
metadata, final-settle status, or bounded failure artifacts for
animation-sensitive coverage.

#### Scenario: Final-settle-only Rust summary is rejected
- **WHEN** a Rust-generated summary for an animation-sensitive change contains
  only before/after final-settle captures
- **THEN** proof policy rejects the summary for missing animation-frame
  evidence
- **AND** the rejection status matches the documented proof-policy vocabulary

#### Scenario: Stale frame mapping remains a failure
- **WHEN** an animation frame cannot be matched to a geometry sample inside the
  declared skew bound
- **THEN** the Rust runner cannot use that frame as passing proof
- **AND** the report identifies stale pairing with frame/sample timing detail

### Requirement: Visual summaries keep engine and compatibility metadata
Visual geometry summaries SHALL identify the engine that produced them, the
tool version, schema versions, scenario source, artifact root, proof-policy
fingerprint when available, and compatibility-corpus status when a future run
is used to migrate from Python to Rust.

#### Scenario: Agent can tell which engine produced proof
- **WHEN** a developer or agent reads a visual root summary
- **THEN** it can identify whether the proof came from the Python runner, the
  Rust runner, or a parity run comparing both
- **AND** it can see the schema and tool versions used for policy validation

#### Scenario: Policy rejects stale engine metadata
- **WHEN** a summary lacks required engine or schema metadata after the Rust
  migration point
- **THEN** proof policy rejects it as incomplete
- **AND** the failure points to the summary path and missing metadata class
