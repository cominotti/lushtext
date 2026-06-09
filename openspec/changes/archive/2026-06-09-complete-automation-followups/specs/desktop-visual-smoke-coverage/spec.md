## ADDED Requirements

### Requirement: Scheduled End-User Smoke Includes Automation Lane
The scheduled and manually dispatched end-user smoke workflow SHALL run the D-Bus automation smoke lane alongside the existing host-sensitive visual, crash-recovery, portal/sandbox, accessibility, and performance smoke lanes.

#### Scenario: Automation lane is present in scheduled smoke matrix
- **WHEN** maintainers inspect the end-user smoke workflow
- **THEN** the smoke-lanes matrix includes an `automation` lane
- **AND** that lane runs `make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke`
- **AND** it uploads `build/smoke/automation` as the lane artifact path

#### Scenario: Automation lane preserves artifacts on failure
- **WHEN** the scheduled automation smoke lane fails or skips
- **THEN** the workflow still attempts to upload the automation artifact directory
- **AND** the uploaded artifacts include the scenario manifest, summary, warning scan, D-Bus/action/catalog/readiness artifacts, logs, and failure or skip reason when those files were produced

#### Scenario: Automation lane remains host-sensitive rather than PR-required
- **WHEN** pull-request CI runs the default required checks
- **THEN** the scheduled automation smoke lane is not required as a blocking PR check
- **AND** maintainers can still run it through the scheduled/manual end-user smoke workflow

#### Scenario: Documentation names automation scheduled coverage
- **WHEN** maintainers read end-user coverage or automation documentation
- **THEN** it identifies `automation-smoke` as a scheduled/manual real-process D-Bus lane
- **AND** it explains that unsupported compositor, D-Bus, or host-tooling environments must report clear skip or failure artifacts instead of false passes
