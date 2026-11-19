# Spec Delta

## MODIFIED Requirements

### Requirement: Filesystem and policy audits join normal lint validation
The project SHALL include fast policy audits that catch lint-adjacent architectural drift in the normal local and CI validation path. The filesystem-boundary audit MUST be available as a Makefile target and MUST run in CI or in a documented aggregate lint/policy target used before publication. Every prerequisite of the local policy target MUST run in the CI policy job, be recorded as covered by another named CI job, or be recorded as local-only with the reason it cannot run there.

#### Scenario: Filesystem boundary audit is callable
- **WHEN** a developer runs the documented filesystem-boundary audit target
- **THEN** `scripts/check-filesystem-boundary.sh` runs from the repository root
- **AND** the command fails on direct raw filesystem or private backend drift outside allowlisted modules

#### Scenario: CI runs fast policy audits
- **WHEN** the CI lint or policy job runs
- **THEN** it runs the filesystem-boundary audit or an aggregate target that includes it
- **AND** policy drift fails before code reaches release validation

#### Scenario: Normal gate documentation lists policy audits
- **WHEN** README, AGENTS, and `.agents/rules/build.md` describe validation commands
- **THEN** they name the policy audit target alongside rustfmt, Clippy, rustdoc, cargo-deny, and OpenSpec validation

#### Scenario: Every local policy audit is in CI or recorded
- **WHEN** the policy-coverage check compares `check-policy`'s prerequisites with its three lists
- **THEN** it fails on a prerequisite on none of the lists (CI aggregate, covered by another job, local-only) or on more than one, or on an entry that is not a prerequisite
- **AND** each local-only entry names its reason, and each covered entry names its job

#### Scenario: A push without the local hook still meets the static audits
- **WHEN** a commit that fails `make check-signal-connections` is pushed without running the pre-commit hook
- **THEN** the CI `Lint` job fails on that audit

## ADDED Requirements

### Requirement: CI format check covers every source the local gate formats
The CI format step SHALL check every Rust source the local format gate checks, including widget-test modules that `cargo fmt` cannot reach through its module walk. CI and the local gate MUST run the same format target.

#### Scenario: Unformatted widget test fails CI
- **WHEN** a file under `crates/lushtext/tests/widget/` is not rustfmt-clean
- **THEN** the CI `Lint` job fails at its format step
- **AND** `make check-fmt` fails locally on the same file

#### Scenario: Format reach stays non-empty
- **WHEN** the widget-test source glob matches no file
- **THEN** the format target fails instead of checking nothing

### Requirement: CI fails on workspace-hack drift
CI SHALL verify, whenever a workspace manifest, `Cargo.lock` or the hakari configuration changes, that `workspace-hack` matches what `cargo hakari generate` writes and that every non-excluded workspace crate depends on it. The verifier MUST be a pinned `cargo-hakari` version recorded with the other CI tool pins.

#### Scenario: Stale workspace-hack fails
- **WHEN** a pull request leaves `workspace-hack` different from what `cargo hakari generate` would write
- **THEN** the CI hakari check fails and names the drift

#### Scenario: New crate without the dependency fails
- **WHEN** a pull request adds a workspace crate that is neither excluded nor depends on `workspace-hack`
- **THEN** the CI hakari check fails and names the crate
