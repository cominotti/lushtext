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

### Requirement: Visual smoke scenarios SHALL use automation state before capture
The visual smoke lane SHALL use the automation spine to verify intended app
state before screenshots whenever the state is exposed through actions or the
read-only automation snapshot. Screenshots MUST remain visual proof, not the
only proof that the app reached the intended workflow.

#### Scenario: Search/minimap capture verifies state first
- **WHEN** the visual smoke lane captures a search and minimap state
- **THEN** it first verifies the active document, search query, match count or
  bounded match summary, and minimap visibility through actions or automation
  state
- **AND** it then captures the screenshot and nonblank/window-bounds assertions

#### Scenario: Preview and properties captures verify state first
- **WHEN** the visual smoke lane captures Markdown preview or
  document-properties states
- **THEN** it first verifies preview mode, active document identity, and
  requested/rendered secondary-surface state through the automation snapshot or
  stateful actions
- **AND** compact and wide presentations remain distinguishable in artifacts

#### Scenario: State mismatch fails before accepting screenshot
- **WHEN** a scenario screenshot exists but the automation state does not match
  the requested scenario
- **THEN** the lane fails with the state mismatch, logs, and screenshot
  preserved
- **AND** it does not report the screenshot as proof of the intended state

### Requirement: Visual smoke covers both Markdown preview presentations
The visual smoke lane SHALL include real-session proof for both preview-only
Markdown rendering and the side-by-side Markdown preview surface whenever a
change modifies the preview shell or its geometry-sensitive template nodes. The
side-by-side proof MUST use documented preview target-state actions and bounded
automation state before accepting screenshots as evidence.

#### Scenario: Side-by-side preview smoke verifies state before capture
- **WHEN** a visual smoke scenario captures the side-by-side Markdown preview surface
- **THEN** it opens a Markdown fixture through the normal document path
- **AND** it requests side-by-side preview through the documented target-state action
- **AND** it verifies `surfaces.preview_pane_visible` and `surfaces.preview_mode` through automation before accepting the screenshot

#### Scenario: Preview-only and side-by-side captures remain distinct
- **WHEN** the visual smoke lane captures Markdown preview states for a preview-shell migration
- **THEN** one scenario proves preview-only mode with `surfaces.preview_mode=true`
- **AND** another scenario proves side-by-side preview with `surfaces.preview_pane_visible=true` and `surfaces.preview_mode=false`
- **AND** the artifacts distinguish compact and wide presentation when both are exercised

#### Scenario: Preview shell warnings fail visual smoke
- **WHEN** side-by-side preview or preview-only smoke captures finish
- **THEN** unexpected GTK, Libadwaita, GDK, renderer, and accessibility warnings emitted by the preview shell fail the lane
- **AND** the warning scan preserves logs alongside the screenshot and automation snapshot artifacts

### Requirement: Visual smoke SHALL include an automation-backed scenario matrix
The visual smoke lane SHALL support an automation-backed matrix that covers
representative user workflows and UI state extremes without relying on
coordinate input.

#### Scenario: Empty and no-context states are captured
- **WHEN** the matrix captures no-document, empty workspace, empty notes, empty
  bookmarks, empty search results, or no-required-context surfaces
- **THEN** the automation snapshot records the empty-state kind
- **AND** screenshots show readable empty states with reachable persistent
  commands and no fake rows

#### Scenario: Dense and awkward states are captured
- **WHEN** the matrix captures many tabs, long file names, dense workspace rows,
  many notes/bookmarks, or long search results
- **THEN** automation state records counts and selected identity
- **AND** screenshots show item-region-only scrolling, preserved
  headers/close/actions, and no unintended horizontal scrollbars or clipped
  primary controls

#### Scenario: Constrained geometry is captured
- **WHEN** the matrix captures narrow, compact, or short-window geometry
- **THEN** automation state records requested and rendered surfaces
- **AND** screenshots prove persistent chrome remains visible unless the tested
  mode intentionally hides it

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

### Requirement: Desktop visual smoke records recovery diagnostics when recovery state is exercised
The desktop visual smoke lane SHALL capture recovery-related runtime diagnostics when a visual or real-session run intentionally exercises corrupted, repaired, restored, or partially unavailable recovery state.

#### Scenario: Recovery warning is captured visually
- **WHEN** a visual smoke run launches with a fixture that produces a grouped startup recovery warning
- **THEN** the screenshot artifact includes the warning or status surface in its intended layout
- **AND** the assertion log records the underlying recovery diagnostic summary

#### Scenario: Quarantine summary is preserved as an artifact
- **WHEN** recovery metadata is quarantined or repaired during a visual smoke run
- **THEN** the smoke artifacts include a bounded quarantine or repair summary
- **AND** the artifact does not include unbounded user document contents

#### Scenario: Unexpected recovery warnings fail normal visual smoke
- **WHEN** a visual smoke state not intended to exercise recovery emits recovery diagnostics
- **THEN** the lane fails or marks the diagnostics as unexpected
- **AND** logs and screenshots are preserved for review

### Requirement: Recovery-focused visual captures stay stable and inspectable
Recovery-focused visual smoke captures SHALL use stable readiness checks before screenshot capture and SHALL preserve enough environment context to distinguish UI regressions from host capture limitations.

#### Scenario: Recovery capture waits for visible state
- **WHEN** a recovery-focused screenshot is requested
- **THEN** the smoke driver waits for the expected recovery warning, restored document content, or diagnostic state before capture
- **AND** it fails clearly if the expected state never appears

#### Scenario: Recovery capture is nonblank and bounded
- **WHEN** the recovery-focused screenshot is captured
- **THEN** it satisfies the same nonblank, monitor-bounded, and chrome-visibility invariants as ordinary visual smoke captures

#### Scenario: Capture tooling gaps skip clearly
- **WHEN** the host lacks screenshot, compositor, D-Bus, or accessibility tooling required for the recovery capture
- **THEN** the visual smoke lane reports a clear skip reason
- **AND** the unsupported recovery visual coverage is not counted as verified

### Requirement: Visual smoke automation docs SHALL stay synchronized
The project SHALL document each visual smoke scenario, its fixture data, actions,
state predicates, screenshots, artifacts, and host requirements. Scenario
definitions and documentation MUST change together.

#### Scenario: Scenario documentation explains proof chain
- **WHEN** maintainers read visual smoke or automation documentation
- **THEN** each scenario explains which action, D-Bus, AT-SPI, screenshot,
  warning-scan, and artifact assertions prove the workflow

#### Scenario: Scenario drift fails validation
- **WHEN** a scenario name, helper flag, fixture contract, state predicate, or
  expected artifact changes
- **THEN** the scenario documentation or generated reference check fails until
  it is updated

### Requirement: Scheduled End-User Smoke Includes Automation Lane
The scheduled and manually dispatched end-user smoke workflow SHALL run the
D-Bus automation smoke lane alongside the existing host-sensitive visual,
crash-recovery, portal/sandbox, accessibility, and performance smoke lanes.

#### Scenario: Automation lane is present in scheduled smoke matrix
- **WHEN** maintainers inspect the end-user smoke workflow
- **THEN** the smoke-lanes matrix includes an `automation` lane
- **AND** that lane runs `make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke`
- **AND** it uploads `build/smoke/automation` as the lane artifact path

#### Scenario: Automation lane preserves artifacts on failure
- **WHEN** the scheduled automation smoke lane fails or skips
- **THEN** the workflow still attempts to upload the automation artifact
  directory
- **AND** the uploaded artifacts include the scenario manifest, summary, warning
  scan, D-Bus/action/catalog/readiness artifacts, logs, and failure or skip
  reason when those files were produced

#### Scenario: Automation lane remains host-sensitive rather than PR-required
- **WHEN** pull-request CI runs the default required checks
- **THEN** the scheduled automation smoke lane is not required as a blocking PR
  check
- **AND** maintainers can still run it through the scheduled/manual end-user
  smoke workflow

#### Scenario: Documentation names automation scheduled coverage
- **WHEN** maintainers read end-user coverage or automation documentation
- **THEN** it identifies `automation-smoke` as a scheduled/manual real-process
  D-Bus lane
- **AND** it explains that unsupported compositor, D-Bus, or host-tooling
  environments must report clear skip or failure artifacts instead of false
  passes

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

### Requirement: Scheduled visual smoke identifies the Rust proof engine
The scheduled and manually dispatched end-user visual-geometry smoke lane SHALL
identify the proof engine, tool version, schema version, scenario source, and
parity status for each run. After parity is recorded, scheduled smoke MUST
default to the Rust live runner while preserving artifact upload roots and
unsupported-host semantics.

#### Scenario: Scheduled smoke reports Rust engine metadata
- **WHEN** the scheduled or manually dispatched visual-geometry lane runs after
  parity is recorded
- **THEN** the uploaded summary identifies `cargo-gtk-proof` or equivalent Rust
  engine metadata, schema versions, and scenario source
- **AND** maintainers can distinguish Rust proof from Python oracle or
  diagnostic compatibility artifacts

#### Scenario: Scheduled smoke keeps artifact upload compatibility
- **WHEN** the visual-geometry lane passes, fails, or skips
- **THEN** the workflow uploads the documented visual-geometry artifact root
- **AND** the root contains bounded summaries, warning scans, screenshots or
  frames when produced, and skip or failure reasons

### Requirement: End-user visual geometry smoke uses the extracted toolchain
The end-user visual-geometry smoke lane SHALL continue to run the same
LushText visual invariants while the extracted Rust proof toolchain becomes
the default live execution path. After parity is recorded, `cargo gtk-proof
run` SHALL own schema validation, corpus evidence, live same-session capture,
pixel-anchor comparison, animation-frame proof, warning scans, policy-ready
summaries, and bounded artifact generation. The scheduled/manual smoke
workflow, Makefile target, artifact directory, pass/fail/skip semantics, and
reviewable evidence layout MUST remain stable or provide documented
compatibility aliases.

#### Scenario: Scheduled visual-geometry lane still uploads evidence
- **WHEN** the scheduled or manually dispatched end-user smoke workflow runs
  the visual-geometry matrix lane after parity is recorded
- **THEN** it executes the stable visual-geometry Makefile or script wrapper
  backed by `cargo gtk-proof run`
- **AND** documentation identifies which evidence is produced by the Rust live
  runner and which optional diagnostics are produced by the Python oracle path
- **AND** it uploads the same bounded artifact root expected by existing
  maintainers and agents

#### Scenario: Unsupported host does not count as coverage
- **WHEN** the smoke host lacks required compositor, screenshot, D-Bus,
  PipeWire, GStreamer, image tooling, or LushText binary support
- **THEN** the lane reports a distinct skipped or unsupported status with
  bounded diagnostics
- **AND** proof policy and smoke summaries do not count the skipped invariant
  as verified

#### Scenario: Local command shape remains stable
- **WHEN** a developer runs `make visual-geometry-smoke` with existing artifact
  directory flags
- **THEN** the command remains accepted
- **AND** the default implementation invokes the Rust runner after parity
- **AND** Python diagnostics require an explicit documented oracle or
  compatibility flag

### Requirement: Smoke artifacts remain bounded and schema-valid
Visual smoke artifacts produced through the extracted toolchain SHALL remain
bounded, privacy-preserving, and schema-valid where Rust descriptors cover the
artifact type. They MUST include enough evidence for review: root summary,
per-case summaries, scenario manifests, screenshots or generated fixtures,
crop reports, animation reports when required, warning scans, environment
details, skip/failure reasons, engine metadata, and parity metadata when the
run compares Rust against the Python oracle.

#### Scenario: Passing smoke has reviewable proof
- **WHEN** the visual-geometry smoke lane passes
- **THEN** the artifact root contains schema-valid summaries that identify
  verified invariant IDs, capture modes, rendered-anchor evidence,
  animation-frame evidence when required, warning-scan status, engine metadata,
  and schema versions
- **AND** the terminal output points to artifact paths rather than embedding
  large screenshots or logs

#### Scenario: Failing smoke points to diagnosis
- **WHEN** a visual-geometry smoke case fails
- **THEN** the artifact root preserves the relevant scenario manifest,
  screenshots or frames, crops, comparison reports, geometry samples, warning
  scan, and failure reason
- **AND** none of those summaries expose user document text, note bodies,
  draft bodies, local-history contents, or private persistence identifiers

#### Scenario: Parity smoke records oracle comparison
- **WHEN** the visual-geometry smoke lane runs in parity mode before wrapper
  migration
- **THEN** artifacts record the Rust status, Python-oracle status, compared
  fields, mismatch count, and bounded paths for both engines
- **AND** a mismatch prevents default smoke from moving to Rust

### Requirement: Smoke workflow checks cover the new proof tool
Workflow and policy checks SHALL be updated so the end-user smoke workflow,
Makefile, and proof-policy gates cannot drift away from the extracted
toolchain. The same guard that checks the smoke matrix MUST recognize the
documented visual-geometry command, artifact path, Rust proof-tool boundary,
and Python oracle or diagnostic compatibility path.

#### Scenario: Smoke workflow matrix remains documented
- **WHEN** `make check-end-user-smoke-workflow` runs
- **THEN** it accepts the visual-geometry lane command only if it matches the
  documented wrapper or cargo-command boundary for the current phase
- **AND** the lane still names the expected artifact path
- **AND** it rejects undocumented workflow commands that bypass the Rust runner
  after parity is recorded

#### Scenario: Local policy target matches smoke artifact shape
- **WHEN** `make check-visual-proof-policy` validates local visual-sensitive
  changes
- **THEN** it reads the same visual-geometry summary shape produced by
  `make visual-geometry-smoke`
- **AND** it rejects stale, missing, skipped, unsupported, incomplete, or
  Python-only smoke evidence after the Rust migration point

#### Scenario: Workflow check preserves Python oracle discoverability
- **WHEN** the workflow or Makefile keeps a Python oracle or diagnostic alias
- **THEN** the check allows it only when the alias is documented as non-default
- **AND** the default visual-geometry lane remains Rust-backed after parity
