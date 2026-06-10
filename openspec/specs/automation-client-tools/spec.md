# automation-client-tools Specification

## Purpose
Define LushText's supported automation client so same-user developers and
agents can inspect the Automation1 surface, wait for readiness, activate
cataloged actions, and summarize smoke artifacts without hand-writing common
raw D-Bus calls.
## Requirements
### Requirement: Supported Automation Client Exists
The project SHALL provide a supported command-line automation client for
same-user developers and agents to consume the documented Automation1 interface
and cataloged GTK/GIO action surface without hand-writing raw `gdbus` calls for
common workflows.

#### Scenario: Client exposes documented subcommands
- **WHEN** a developer runs the automation client help command
- **THEN** it lists supported subcommands for introspection, action catalog
  reads, snapshot reads, readiness predicate reads, workflow-event reads,
  readiness waits, cataloged action activation, artifact summaries, and
  self-test
- **AND** the help text names the default bus name, object path, interface, and
  action path assumptions or the flags that override them

#### Scenario: Client uses existing Automation1 defaults
- **WHEN** the client runs without explicit D-Bus destination flags
- **THEN** it targets the session bus name `dev.cominotti.lushtext`, object path
  `/dev/cominotti/lushtext/Automation`, and interface
  `dev.cominotti.lushtext.Automation1`
- **AND** it does not require a system bus, elevated privileges, or a
  portals-only environment

#### Scenario: Missing host tooling reports a stable error
- **WHEN** the client cannot find a required host tool such as `gdbus`
- **THEN** it exits nonzero with a stable `unsupported-host-tooling` status
- **AND** it does not report the command as an application failure

### Requirement: Client Provides Bounded Read-Only Inspection
The automation client SHALL expose bounded read-only commands for Automation1
introspection, action catalog reads, snapshots, readiness predicates, and
workflow events.

#### Scenario: Snapshot read returns machine-readable state
- **WHEN** LushText is running on the caller's session bus
- **AND** the developer runs the client snapshot command with JSON output
- **THEN** the command returns valid JSON containing a stable top-level result
  envelope
- **AND** the embedded snapshot is the bounded Automation1 snapshot without
  document text, note bodies, draft bodies, local-history contents, complete
  search result text, or private persistence identifiers

#### Scenario: Field extraction stays bounded
- **WHEN** the developer requests a specific snapshot field such as the active
  tab title, modified flag, or search match count
- **THEN** the client extracts that field without printing unrelated snapshot
  payloads
- **AND** missing fields report a stable error instead of printing misleading
  empty success output

#### Scenario: Workflow events are readable
- **WHEN** workflow events have been recorded by Automation1
- **THEN** the client events command returns the bounded workflow event snapshot
- **AND** it preserves event sequence, workflow id, phase, status, summary, and
  blocker fields without adding a new event source

### Requirement: Client Waits On Readiness Predicates
The automation client SHALL provide a readiness wait command that wraps
Automation1 `WaitForReady` and reports stable statuses and exit codes for
agents.

#### Scenario: Ready predicate succeeds
- **WHEN** the requested readiness predicate settles before the timeout
- **THEN** the client exits successfully with status `ready`
- **AND** JSON output records the predicate, timeout, ok flag, status, detail,
  and elapsed time or equivalent timing detail

#### Scenario: Predicate timeout is distinguishable
- **WHEN** the requested readiness predicate remains blocked until timeout
- **THEN** the client exits nonzero with status `predicate-timeout`
- **AND** the detail identifies the blocker returned by Automation1 when
  available

#### Scenario: Unknown predicate is distinguishable
- **WHEN** the developer requests a predicate not supported by the current
  Automation1 version
- **THEN** the client exits nonzero with status `unknown-predicate`
- **AND** it does not silently fall back to broad idle waits

### Requirement: Client Activates Only Cataloged Supported Actions
The automation client SHALL activate state-changing behavior only through
documented GTK/GIO actions that are represented in the action catalog as
supported exported actions.

#### Scenario: Supported action activation succeeds
- **WHEN** LushText is running and the developer activates a supported exported
  action such as `win.set-search-query` with a valid string parameter
- **THEN** the client validates the requested action against `GetActionCatalog`
- **AND** it calls `org.gtk.Actions.Activate` with the correct object path,
  action name, and GVariant parameter
- **AND** it reports success without directly mutating private widgets or
  Automation1 state

#### Scenario: Unsupported gap is rejected
- **WHEN** the developer requests an action cataloged as `unsupported-gap` or
  `visible-unregistered-gap`
- **THEN** the client refuses activation with status `unsupported-action`
- **AND** it prints the catalog row's docs anchor or label so the caller can
  find the documented blocker

#### Scenario: Parameter mismatch is rejected
- **WHEN** the developer supplies a parameter kind that does not match the
  cataloged action parameter type
- **THEN** the client refuses activation before calling D-Bus
- **AND** it exits with status `parameter-mismatch`

#### Scenario: Contextual disabled action remains app-owned
- **WHEN** a cataloged action requires UI context that is not currently present
- **THEN** the client reports the D-Bus or action-group failure without
  inventing private widget context
- **AND** the app's normal safety and enablement rules remain authoritative

### Requirement: Client Summarizes Smoke Artifacts
The automation client SHALL provide an artifact summary command that reads known
smoke artifact directories and emits bounded review summaries for humans and
agents.

#### Scenario: Automation smoke artifacts are summarized
- **WHEN** the developer runs artifact summary on `build/smoke/automation`
- **THEN** the client reports the scenario status, manifest path, summary path,
  warning-scan status, D-Bus assertion artifacts, action/catalog artifacts,
  readiness artifacts, workflow-event artifact, snapshot artifacts, and skip or
  failure reason when present
- **AND** it does not embed unbounded logs or full snapshot payloads in the
  summary

#### Scenario: Failed or skipped lane points to evidence
- **WHEN** a smoke artifact directory records failure or skip state
- **THEN** the client exits nonzero for failed artifacts and successfully or
  distinctly for skipped artifacts according to the documented exit-code
  contract
- **AND** it prints the relative paths to the most useful evidence artifacts

#### Scenario: Unknown artifact directory is handled clearly
- **WHEN** the developer points artifact summary at a directory without a
  recognized manifest, summary, or warning-scan shape
- **THEN** the client exits nonzero with status `artifact-error`
- **AND** it does not claim coverage passed

### Requirement: Client Output And Errors Are Stable
The automation client SHALL document and preserve a stable result envelope,
status vocabulary, and exit-code contract for automation consumers.

#### Scenario: Successful JSON output has a stable envelope
- **WHEN** any client command succeeds with JSON output enabled
- **THEN** the output contains `ok`, `status`, `command`, `detail`, and `data`
  fields
- **AND** additional fields are additive and documented before release

#### Scenario: Error JSON output has a stable envelope
- **WHEN** any client command fails with JSON output enabled
- **THEN** the output contains `ok=false`, a stable `status`, the `command`, a
  bounded `detail`, and any safe diagnostic `data`
- **AND** it does not include document contents or private persistence
  identifiers

#### Scenario: Exit codes are documented
- **WHEN** maintainers read the automation reference
- **THEN** it documents exit codes for success, app or predicate failure, usage
  or parameter mismatch, automation unavailable, and unsupported host tooling
- **AND** client tests prove representative commands return the documented
  classes

### Requirement: Client Documentation Stays Current
The automation client SHALL be documented as part of the public automation
contract and guarded by the automation documentation drift check.

#### Scenario: Client commands are documented
- **WHEN** users or maintainers read the automation guide and developer
  reference
- **THEN** they can find the supported client commands, flags, examples, status
  names, output envelope, artifact-summary behavior, safety boundaries, and
  troubleshooting guidance

#### Scenario: Drift check catches missing client docs
- **WHEN** a client command, flag, output field, status name, or exit-code class
  is added, removed, or renamed
- **THEN** `make check-automation-docs` fails until the automation documentation
  is updated
- **AND** the drift check self-test proves at least one representative missing
  client documentation case is caught

### Requirement: Automation client summarizes visual geometry artifacts
The automation client SHALL summarize visual geometry smoke artifacts through the stable result envelope. Summaries MUST identify scenario status, capture steps, geometry snapshots, crop comparison reports, warning scans, screenshots, masks, skip reasons, and failure evidence without embedding unbounded logs or image data.

#### Scenario: Passing visual comparison is summarized
- **WHEN** a developer runs the automation client artifact-summary command on a visual geometry smoke artifact directory
- **THEN** the client reports the scenario id, pass status, compared capture steps, protected regions with zero differences, allowed-changing regions, warning-scan result, and manifest path
- **AND** it exits successfully through the documented result envelope

#### Scenario: Failing visual comparison points to evidence
- **WHEN** a visual geometry smoke artifact directory records a failed crop comparison, readiness timeout, state mismatch, or warning scan
- **THEN** the client exits nonzero with a stable status such as `visual-comparison-failed`, `predicate-timeout`, `state-mismatch`, or `warning-scan-failed`
- **AND** it prints relative paths to the most useful bounded evidence artifacts

#### Scenario: Skipped visual geometry lane is distinct
- **WHEN** a visual geometry artifact directory records unsupported host tooling or compositor capture limitations
- **THEN** the client reports a skip status distinct from pass and fail
- **AND** it does not count the skipped invariant as verified

### Requirement: Automation client can wait for visual geometry readiness
The automation client SHALL support waiting on the documented visual geometry readiness predicate through its existing readiness wait command and stable status vocabulary.

#### Scenario: Visual readiness wait succeeds
- **WHEN** LushText is running and the client waits for visual geometry readiness
- **THEN** the client calls the documented Automation1 readiness predicate
- **AND** successful JSON output records the predicate, timeout, ready status, and bounded detail

#### Scenario: Visual readiness timeout is distinguishable
- **WHEN** visual geometry readiness times out
- **THEN** the client reports `predicate-timeout`
- **AND** the output includes the bounded Automation1 blocker detail without falling back to broad idle waits

### Requirement: Automation client preserves visual artifact privacy
The automation client SHALL keep visual artifact summaries bounded and privacy-preserving.

#### Scenario: Summary omits image payloads and content text
- **WHEN** the client summarizes screenshots, crop diffs, geometry state, or automation snapshots
- **THEN** it reports relative artifact paths and bounded counters or statuses
- **AND** it does not print image payloads, full logs, document text, note bodies, draft bodies, local-history contents, or complete search result text

### Requirement: Automation client summarizes animation-frame visual proof
The automation client SHALL summarize animation-frame visual geometry artifacts
through its stable result envelope. Summaries MUST distinguish stream
animation-frame evidence from final-settle evidence and MUST report whether the
required animation invariant, mapped intermediate frames, per-frame anchors,
timing/skew metadata, warning scans, and bounded failure artifacts are present.

#### Scenario: Passing animation proof is summarized
- **WHEN** a developer runs artifact-summary on a passing animation-frame visual geometry artifact directory
- **THEN** the client reports the scenario id, invariant id, capture mode, frame count, geometry sample count, intermediate sample count, mapped intermediate frame count, maximum row drift, maximum sample skew, final-settle status, and representative artifact paths
- **AND** it exits successfully through the documented result envelope

#### Scenario: Native minimap animation proof is summarized
- **WHEN** the client summarizes a visual geometry artifact with native minimap animation coverage
- **THEN** it reports the scenario id, status, animation invariant ids, sampled frame count, maximum row drift, failing frame details when present, and representative frame/crop artifact paths
- **AND** it preserves final-settle pixel evidence as a separate field when present

#### Scenario: Failing animation proof points to frame evidence
- **WHEN** an animation-frame visual geometry artifact records native minimap anchor drift, missing anchors, stale frame/sample pairing, missing intermediate frames, readiness timeout, or warning-scan failure
- **THEN** artifact-summary exits nonzero with a stable status
- **AND** it reports the most useful frame report, crop, screenshot, geometry sample, warning log, and manifest paths without embedding image data or unbounded logs

#### Scenario: Final-settle proof is labeled separately
- **WHEN** an artifact directory contains both animation-frame evidence and final-settle evidence
- **THEN** the client reports both lanes separately
- **AND** a passing final-settle lane does not mask a failing or missing animation-frame lane

#### Scenario: Missing animation proof reports missing coverage
- **WHEN** a visual-sensitive minimap change requires animation-frame coverage
- **AND** the artifact summary only contains final-settle minimap evidence
- **THEN** the client reports that animation coverage is missing
- **AND** it does not mark the animation invariant as verified

### Requirement: Automation client generates replayable animation scenarios from live captures
The automation client SHALL support generating or preserving replayable visual
geometry scenarios from a live window state that reproduces animation-sensitive
rendered-effect bugs. Generated scenarios MUST include explicit window size,
theme/wrap/minimap/sidebar state, action direction, stream capture settings,
required anchors, tolerances, and final-settle follow-up requirements.

#### Scenario: Live capture preserves animation settings
- **WHEN** a developer captures a live minimap/sidebar animation repro
- **THEN** the generated scenario records the current window size, visible surface state, minimap state, top-of-document or scroll anchor state, theme, wrap mode, action direction, stream frame count or duration, sample cadence, required anchors, and row tolerances
- **AND** the generated scenario can be replayed in an isolated smoke session without depending on private user document contents

#### Scenario: Missing live prerequisites fail explicitly
- **WHEN** live capture cannot determine a required field such as window size, minimap visibility, sidebar state, action direction, or screenshot-stream capability
- **THEN** the client reports a stable incomplete-capture status
- **AND** it does not generate a scenario that could be mistaken for verified animation proof

#### Scenario: Replay command is recorded
- **WHEN** the client writes a generated animation replay scenario
- **THEN** it records the exact visual geometry smoke command needed to run that scenario under headless capture
- **AND** the generated artifact summary explains that live screenshots are context while replayed screenshot-derived anchors are proof

### Requirement: Automation client enforces animation proof policy for sensitive changes
The automation client and proof-policy checks SHALL reject sensitive visual
changes unless the artifact set contains valid animation-frame evidence for the
declared invariant.

#### Scenario: Sensitive visual change without stream evidence fails policy
- **WHEN** proof policy evaluates a minimap/source-map/editor-width/sidebar-animation sensitive diff
- **AND** the provided artifacts lack stream-mode animation proof for the native minimap invariant
- **THEN** the policy fails with a stable missing-animation-proof status
- **AND** it names the required scenario or invariant id

#### Scenario: Stale or incomplete animation evidence fails policy
- **WHEN** proof policy evaluates an animation artifact with no mapped intermediate PNG, stale frame/sample pairings, missing required anchors, or missing per-frame pass/fail rows
- **THEN** the policy fails even if final-settle evidence passes
- **AND** the failure summary points to the incomplete evidence fields

### Requirement: Client captures and summarizes native minimap rendered-proof scenarios
The automation client SHALL help agents capture, replay, summarize, and policy-check native minimap rendered-proof scenarios. The client MUST surface whether the required screenshot-derived native minimap invariant was verified, skipped, or failed, and it MUST NOT count geometry-only evidence as a pass.

#### Scenario: Live capture emits native minimap rendered-proof fields
- **WHEN** the client generates a live visual-geometry scenario from a visible minimap/sidebar state
- **THEN** the generated manifest includes native minimap pixel anchors, final sidebar geometry requirements, source window size, sidebar direction, theme or requested color scheme, word-wrap state, fixture kind, and native minimap invariant id
- **AND** unknown fields are reported as missing-field or require explicit caller overrides rather than being guessed silently

#### Scenario: Artifact summary exposes native minimap pixel status
- **WHEN** the client summarizes a visual geometry artifact containing native minimap highlight coverage
- **THEN** the summary reports scenario id, status, pixel-verified invariant ids, native minimap anchor rows, row deltas, relationship deltas, crop paths, app-vs-rendered diagnostics, and final geometry
- **AND** a rendered-anchor failure exits with a stable nonzero status

#### Scenario: Proof policy rejects geometry-only native minimap evidence
- **WHEN** files that can affect native minimap rendering, source-map geometry, sidebar/editor allocation, or visual proof tooling change
- **THEN** proof-policy checks require a passing native minimap rendered-proof artifact for the reproduced intermediate-size invariant
- **AND** artifacts without screenshot-derived pixel anchors or without the required invariant id do not satisfy the policy

#### Scenario: Filtered runs do not overclaim coverage
- **WHEN** a developer runs a filtered visual geometry case that excludes the reproduced native minimap threshold scenario
- **THEN** the client summary reports the filtered result accurately
- **AND** it does not mark the full native minimap rendered invariant as verified unless the required scenario actually ran and passed

### Requirement: Automation client captures live visual geometry repros
The automation client SHALL provide a supported live visual-geometry capture command or helper for same-user developers and agents. The command MUST collect bounded live Automation1 state, write reviewable artifacts, and generate a visual-geometry scenario that can be replayed by the headless visual-geometry smoke runner.

#### Scenario: Live capture writes bounded artifacts
- **WHEN** LushText is running and a developer invokes the live visual-geometry capture command
- **THEN** the command writes a bounded live snapshot, capture manifest, generated scenario file, command metadata, and skip or failure reason when applicable
- **AND** it does not embed document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers

#### Scenario: Generated scenario is runnable
- **WHEN** the live capture command succeeds
- **THEN** it prints or records the exact `scripts/visual-geometry-smoke.py --scenario-dir ...` command needed to replay the generated case
- **AND** the generated scenario validates against the visual-geometry scenario loader before success is reported

#### Scenario: Overrides handle unknown live fields
- **WHEN** live state does not expose a required scenario value such as fixture kind, color scheme, word-wrap mode, or intended direction
- **THEN** the command accepts explicit override flags or records an actionable missing-field error
- **AND** it exits with a stable status instead of silently guessing

#### Scenario: Live capture distinguishes screenshot context from proof
- **WHEN** the command optionally captures a desktop screenshot for context
- **THEN** the result marks it as contextual evidence only
- **AND** the success status depends on Automation1 state and generated scenario validity, not on the screenshot containing the focused LushText window

### Requirement: Automation client summarizes visual geometry pixel evidence
The automation client SHALL summarize visual geometry artifacts with enough per-case pixel evidence to make rendered-effect regressions obvious to agents.

#### Scenario: Pixel anchor failure summary is actionable
- **WHEN** artifact summary reads a failed visual-geometry case with pixel-anchor failures
- **THEN** it reports the scenario id, invariant id, failure status, before and after detected row positions, screen Y delta, relevant final geometry rows, and crop artifact paths
- **AND** it exits nonzero through the documented result envelope

#### Scenario: App-vs-rendered disagreement is reported
- **WHEN** a visual-geometry comparison records different outcomes for Automation1 geometry anchors and screenshot-derived pixel anchors
- **THEN** artifact summary reports the disagreement as a diagnostic detail
- **AND** it does not collapse the result into a generic visual-comparison failure without the row evidence

#### Scenario: Passing summary proves pixel verification
- **WHEN** artifact summary reads a passing visual-geometry run
- **THEN** it lists the pixel-verified invariant ids from the root summary and per-case summaries
- **AND** missing pixel verification is distinct from a passing rectangle-only invariant
