# snap-ci-and-testing Specification

## Purpose
Define LushText's Snap CI validation, gated build/publish workflow, and local
confined smoke coverage so Snap regressions are caught without making unavailable
platform dependencies fail routine pipelines.

## Requirements
### Requirement: CI Always Validates The Snap Definition
CI SHALL validate the Snap packaging definition on every relevant change,
independent of whether the GNOME 50 platform snap is available, so packaging
regressions are caught early.

#### Scenario: Snap definition is validated on pull requests
- **WHEN** a pull request changes `snap/snapcraft.yaml` or related packaging files
- **THEN** CI parses/validates the Snap definition and fails if it is malformed,
  without requiring a successful full Snap build

### Requirement: Snap Build And Publish Are Gated On Platform Availability
CI SHALL build and publish the Snap only when the required GNOME 50 platform snap
is available, and the absence of that platform SHALL NOT fail the pipeline.

#### Scenario: Build job is inactive while the platform is missing
- **WHEN** CI runs while the `core26` / GNOME 50 platform snap is unavailable
- **THEN** the Snap build/publish job is skipped or treated as non-failing, and
  the overall pipeline status is not red because of the missing platform

#### Scenario: Build and publish to edge when the platform is available
- **WHEN** CI runs with the GNOME 50 platform snap available and store credentials
  configured
- **THEN** it builds the strict-confined Snap and publishes the revision to the
  `edge` channel

#### Scenario: Publishing uses stored credentials, not interactive login
- **WHEN** the publish step runs in CI
- **THEN** it authenticates via the configured `SNAPCRAFT_STORE_CREDENTIALS`
  secret and never requires interactive login

### Requirement: Local Confined Smoke Test
The implementation SHALL provide a repeatable local smoke test that installs the
built Snap, launches it under confinement headlessly, and checks for runtime
denials, because native and Flatpak tests cannot detect confinement-only
failures.

#### Scenario: Confined snap launches and loads its resources
- **WHEN** the smoke test installs the built `.snap` and launches it headlessly
- **THEN** the app starts, loads its GResource and GSettings schema, and can open
  a file located in an accessible directory

#### Scenario: AppArmor denials fail the smoke test
- **WHEN** the confined app run produces AppArmor/seccomp denials (for example via
  `snappy-debug` or the system journal)
- **THEN** the smoke test reports those denials and fails rather than passing
  silently

### Requirement: Snap Tooling Is Exposed Through Make Targets
The repository SHALL expose Make targets to build and smoke-test the Snap
locally, consistent with the existing `flatpak` / `verify-flatpak-identity`
targets.

#### Scenario: Make targets exist for the Snap workflow
- **WHEN** the `Makefile` is inspected after this change
- **THEN** it provides targets to build the Snap and to run the local confined
  smoke test
