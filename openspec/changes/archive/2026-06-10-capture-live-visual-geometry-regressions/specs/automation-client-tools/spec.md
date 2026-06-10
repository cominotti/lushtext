## ADDED Requirements

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
