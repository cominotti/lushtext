## ADDED Requirements

### Requirement: Rust live runner owns bounded process orchestration
`cargo-gtk-proof` SHALL implement the live visual runner for LushText
visual-geometry scenarios. The runner MUST isolate runtime state, probe host
dependencies, launch the app inside the required desktop session, drive only
documented Automation1 and action surfaces, preserve same-session capture
semantics, and write bounded artifacts before it reports any invariant as
verified.

#### Scenario: Live run uses isolated desktop state
- **WHEN** `cargo gtk-proof run` executes visual-geometry scenarios on a
  supported host
- **THEN** it creates isolated runtime, data, cache, config, D-Bus, and
  screenshot/session state for the run
- **AND** it launches the required compositor, capture helpers, and LushText
  binary without reusing stale user state
- **AND** cleanup failures are reported as diagnostics without deleting
  unrelated paths

#### Scenario: Unsupported host writes non-proof evidence
- **WHEN** the host lacks required compositor, D-Bus, PipeWire, GStreamer,
  screenshot, image-decoding, or LushText binary support
- **THEN** the run exits with a stable skipped or unsupported-host status
- **AND** the summary records the missing capability
- **AND** no visual invariant is counted as verified

#### Scenario: Same-session capture is preserved
- **WHEN** a scenario declares paired before/after captures
- **THEN** the runner captures all compared screenshots from the same app
  process, compositor session, theme, renderer, scale factor, font
  configuration, fixture, and window size
- **AND** cross-session captures are not used for exact protected-region proof

### Requirement: Rust live runner records parity before wrapper migration
`cargo-gtk-proof` SHALL include a parity mode that compares Rust live-runner
output with the Python oracle before local wrappers, scheduled smoke workflows,
or proof-policy defaults move to Rust. Parity MUST cover status, exit class,
required summary fields, invariant IDs, warning-scan result, and bounded
artifact path shape.

#### Scenario: Parity mismatch blocks default migration
- **WHEN** a Python-oracle parity replay produces a different status, exit
  class, invariant set, warning-scan decision, required field, or bounded
  artifact path than Rust
- **THEN** wrapper defaults remain on the Python path
- **AND** the parity report identifies the scenario, case, field, and artifact
  roots needed for diagnosis

#### Scenario: Parity success records migration evidence
- **WHEN** all required parity scenarios agree
- **THEN** the Rust summary records the parity corpus identity, Python-oracle
  status, Rust engine metadata, schema versions, compared case count, failed
  mismatch count, and migration timestamp or equivalent run identity
- **AND** documentation can cite that evidence before wrappers flip

### Requirement: Rust live runner protects artifact privacy and resource budgets
`cargo-gtk-proof` SHALL keep live-runner artifacts bounded and
privacy-preserving. It MUST cap scenario, JSON, log, PNG, frame, and summary
sizes; use safe artifact-root reset rules; report relative artifact paths in
terminal output; and avoid exposing user document contents or private
persistence identifiers.

#### Scenario: Terminal output stays bounded
- **WHEN** a run passes, fails, skips, or reports unsupported host tooling
- **THEN** stdout and stderr identify status, high-level detail, and relative
  evidence paths
- **AND** they do not embed raw screenshots, image bytes, full logs, document
  text, note bodies, draft bodies, local-history contents, complete search
  results, or private persistence identifiers

#### Scenario: Artifact root reset is guarded
- **WHEN** the runner prepares an output artifact directory
- **THEN** it refuses to recursively clear unsafe roots such as `/`, the home
  directory, the workspace root, or an empty path
- **AND** it only resets the intended bounded artifact root for the current run

## MODIFIED Requirements

### Requirement: Cargo GTK proof tool exposes stable staged subcommands
The workspace SHALL provide a Rust cargo subcommand named `cargo-gtk-proof`
whose binary can be invoked through cargo as `cargo gtk-proof`. The tool MUST
provide documented subcommands for validating schemas, summarizing artifacts,
replaying the compatibility corpus, enforcing proof policy, and executing live
visual scenarios. Once parity is recorded, `cargo gtk-proof run` SHALL be the
authoritative live visual runner for LushText visual-geometry proof rather
than a reserved non-coverage command.

#### Scenario: Help lists proof subcommands
- **WHEN** a developer runs `cargo gtk-proof --help`
- **THEN** the help output lists the run, schema, summarize, corpus, and policy
  subcommands or their documented equivalents
- **AND** it identifies default artifact paths, scenario paths, schema/tool
  metadata, live-runner host requirements, parity/oracle flags, and the current
  support boundary for diagnostic Python compatibility

#### Scenario: Live runner reports real coverage only after execution
- **WHEN** a developer runs `cargo gtk-proof run` on a supported host with a
  valid scenario set
- **THEN** the command executes the live visual scenarios through the Rust
  runner
- **AND** it reports verified invariant IDs only for cases whose screenshots,
  readiness waits, comparisons, animation evidence, and warning scans passed

#### Scenario: Unsupported live runner does not claim coverage
- **WHEN** a developer runs `cargo gtk-proof run` on an unsupported host or
  with missing host tooling
- **THEN** the command exits with a stable unsupported-host or skipped status
- **AND** it writes bounded diagnostic artifacts
- **AND** it does not count any visual invariant as verified

### Requirement: Tool preserves visual-geometry compatibility boundaries
`cargo-gtk-proof` SHALL implement the Rust compatibility surface required for
live visual proof: versioned schemas, bounded result envelopes, pure PNG
crop/diff/anchor primitives, frozen corpus replay, proof-policy evaluation,
same-session runner orchestration, Automation1 readiness/action integration,
warning scans, and animation-frame stream proof. The Python visual runner
SHALL remain available only as an explicit oracle or diagnostic compatibility
path after Rust corpus, live-runner, animation, and wrapper parity are
recorded.

#### Scenario: Pure PNG corpus is replayed
- **WHEN** the compatibility corpus contains exact comparison, masked
  comparison, pixel-anchor, minimap-detector, and drift regression cases
- **THEN** the Rust tool produces the expected pass/fail decision for those
  bounded fixtures
- **AND** the corpus command reports compared and failed counts in a stable
  result envelope

#### Scenario: Rendered pixel oracle is preserved in Rust
- **WHEN** the Rust live runner evaluates native toolkit-rendered,
  CSS-rendered, or compositor-rendered effects
- **THEN** screenshot-derived pixel anchors remain the pass/fail oracle for
  rendered effects
- **AND** app-owned geometry alone cannot satisfy the invariant
- **AND** the report identifies any app-vs-rendered disagreement with bounded
  crop and detector artifacts

#### Scenario: Python compatibility path is explicit
- **WHEN** a maintainer runs Python oracle or diagnostic compatibility mode
  after wrappers default to Rust
- **THEN** artifacts clearly identify the Python engine and oracle purpose
- **AND** the Python result is not mistaken for the default authoritative Rust
  proof unless parity mode is explicitly requested

### Requirement: Tool preserves animation proof policy semantics
`cargo-gtk-proof` SHALL preserve the policy semantics that make animation
proof meaningful while moving live capture to Rust. Rust live execution and
policy self-tests MUST reject final-settle-only evidence, missing stream mode,
missing mapped intermediate PNG evidence, stale frame/sample pairing, missing
anchors, and rendered pixel drift hidden by app geometry.

#### Scenario: Intermediate animation evidence remains required by policy
- **WHEN** an animation-frame invariant is declared in a proof summary
- **THEN** Rust policy rejects the summary unless at least one evaluated frame
  maps to an intermediate transition phase inside the declared skew bound
- **AND** final-settle-only evidence is not counted as animation proof

#### Scenario: Rust runner records timestamp-correlated stream proof
- **WHEN** a live visual scenario declares animation-frame proof
- **THEN** the Rust runner captures a bounded screenshot stream during the
  action
- **AND** it records timestamped Automation1 geometry samples for the same
  window
- **AND** it writes per-frame pass/fail rows, mapped sample timestamps,
  intermediate-frame counts, maximum skew, required anchor results, and bounded
  failure artifacts

#### Scenario: Animation negative cases are stable
- **WHEN** the Rust proof-policy self-tests run
- **THEN** stale frame/sample pairing, missing stream mode, missing mapped
  intermediate PNG, missing required anchors, rendered drift hidden by app
  geometry, and final-settle-only fixtures fail with stable statuses
- **AND** no raw image data is embedded in terminal output

### Requirement: Compatibility corpus gates Python retirement
The Rust tool SHALL include a bounded compatibility corpus that blocks wrapper
defaults from moving away from the Python runner or policy checker until
parity is proved. The corpus MUST cover checked-in status fixtures, pure PNG
comparison/detector fixtures, and representative live-result fixtures for
passing, failing, skipped, unsupported-host, missing-manifest, malformed
artifact, missing-anchor, rendered-anchor drift, warning-scan failure, stale
frame/sample pairing, final-settle-only, and successful animation cases before
wrappers flip.

#### Scenario: Corpus parity blocks wrapper migration
- **WHEN** any compatibility corpus case produces a different Rust pass/fail
  decision, required status, required summary field, required invariant ID,
  required exit class, or required bounded artifact path than the Python oracle
- **THEN** wrapper defaults remain on the Python path
- **AND** the Rust mismatch is reported with the corpus case identity and
  artifact path

#### Scenario: Corpus records replay metadata
- **WHEN** a corpus replay completes
- **THEN** the result envelope records the corpus root, Rust tool metadata,
  schema version, compared count, failed count, oracle engine when used, and
  check details
- **AND** this metadata is available to proof-policy checks and CI logs

#### Scenario: Negative corpus cases stay deterministic
- **WHEN** corpus replay evaluates malformed, warning-scan, rendered-drift,
  stale-frame, missing-anchor, and final-settle-only fixtures
- **THEN** each fixture produces the documented failure status
- **AND** the replay remains independent of live compositor availability

### Requirement: Proof policy is ported without flipping wrappers
`cargo-gtk-proof` SHALL implement visual proof-policy detection and negative
self-tests, and after parity is recorded it SHALL become the default local
proof-policy implementation. The Rust policy MUST preserve visual-sensitive
file detection, required invariant mapping, fingerprint matching,
artifact-summary validation, negative self-tests, status vocabulary, and
unsupported-host semantics. `scripts/check-visual-proof-policy.py` MAY remain
as a compatibility wrapper, but it MUST delegate to or agree with the Rust
policy before reporting success.

#### Scenario: Negative self-tests still fail
- **WHEN** the Rust proof-policy self-tests run
- **THEN** cases for final-settle-only evidence, missing stream mode, no mapped
  intermediate PNG, stale frame/sample pairing, missing required anchors,
  rendered pixel drift hidden by app geometry, stale fingerprints, missing
  summaries, and unsupported-host overclaiming all fail with stable statuses

#### Scenario: Sensitive change requires current proof
- **WHEN** a local diff touches visual-sensitive paths
- **THEN** the Rust policy requires a current passing visual-geometry summary
  whose fingerprint matches the diff and whose verified invariant IDs cover the
  required rendered and animation invariants
- **AND** skipped or unsupported-host summaries do not satisfy the policy

#### Scenario: Compatibility wrapper cannot mask Rust policy failure
- **WHEN** the compatibility script delegates to Rust policy and Rust rejects
  the provided evidence
- **THEN** the script exits nonzero with the documented policy status
- **AND** it preserves the Rust evidence path and bounded failure detail

### Requirement: Existing LushText visual commands remain stable
Existing LushText script and Makefile entry points SHALL remain functional
while the Rust proof toolchain becomes authoritative. Wrappers MUST preserve
documented flags, default artifact locations, result envelopes, and exit-code
classes. After parity is recorded, the default local and scheduled
visual-geometry paths SHALL call `cargo gtk-proof run`, while Python remains
available only through documented oracle or diagnostic compatibility aliases.

#### Scenario: Visual geometry smoke target uses Rust by default
- **WHEN** `make visual-geometry-smoke` runs after parity is recorded
- **THEN** it invokes the Rust live runner or a documented wrapper around it
- **AND** it writes the same bounded artifact directory shape expected by
  existing maintainers, agents, and workflow upload steps
- **AND** it clearly identifies the Rust engine in summary metadata

#### Scenario: Compatibility alias preserves Python diagnostics
- **WHEN** a maintainer intentionally runs the Python oracle or diagnostic path
- **THEN** the command remains discoverable through documented flags or
  compatibility aliases
- **AND** its artifacts identify the Python engine and do not replace required
  Rust proof for default policy unless parity mode is explicitly requested

#### Scenario: Proof policy target has Rust parity evidence
- **WHEN** `cargo gtk-proof policy --self-test` and the local policy wrapper
  run
- **THEN** they execute the Rust policy negative cases successfully
- **AND** the default wrapper path is Rust-backed once local policy parity is
  recorded
