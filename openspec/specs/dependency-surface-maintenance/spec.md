# dependency-surface-maintenance Specification

## Purpose
Define how LushText keeps Cargo, Flatpak vendoring, side lockfiles, product behavior, and validation in sync during dependency refreshes.

## Requirements

### Requirement: Compatible Dependency Refresh Boundary
LushText dependency maintenance SHALL refresh stable Cargo dependency locks within the existing manifest constraints unless the change explicitly proposes a manifest-range, major-line, or pre-release migration.

#### Scenario: Stable compatible workspace refresh
- **WHEN** a dependency-maintenance change refreshes the main workspace lockfile
- **THEN** the refresh MUST stay within the existing `Cargo.toml` dependency ranges unless the proposal lists a broader migration

#### Scenario: Non-compatible candidates are deferred
- **WHEN** a major-line, manifest-range, or pre-release candidate is discovered during a dependency review
- **THEN** the dependency-maintenance change MUST either defer it explicitly or introduce a separate requirement and validation plan for adopting it

### Requirement: Flatpak Vendoring Synchronization
LushText dependency maintenance SHALL keep Flatpak Cargo vendoring metadata synchronized with the main workspace lockfile whenever the main lockfile changes.

#### Scenario: Workspace lockfile changes
- **WHEN** `Cargo.lock` changes as part of dependency maintenance
- **THEN** `build-aux/cargo-sources.json` MUST be regenerated from the updated lockfile before the change is considered ready

#### Scenario: Vendoring validation
- **WHEN** the dependency refresh is validated
- **THEN** Flatpak vendoring or build validation MUST prove that `cargo-sources.json` matches the refreshed dependency graph

### Requirement: Side Lockfile Coverage
LushText dependency maintenance SHALL include repository side lockfiles that are part of supported validation or adoption workflows.

#### Scenario: Fuzz lockfile refresh
- **WHEN** compatible updates are available for the fuzz crate dependency graph
- **THEN** `fuzz/Cargo.lock` MUST be refreshed or the reason for deferring it MUST be recorded

#### Scenario: GTK Lush fixture lockfile refresh
- **WHEN** compatible updates are available for the GTK Lush stock fixture dependency graph
- **THEN** `fixtures/gtk-lush-adoption/stock-settle/Cargo.lock` MUST be refreshed or the reason for deferring it MUST be recorded

### Requirement: Product Behavior Preservation
LushText dependency maintenance SHALL preserve existing runtime behavior unless a proposal explicitly scopes a user-facing feature change.

#### Scenario: No bundled feature adoption
- **WHEN** dependency versions are refreshed
- **THEN** the change MUST NOT intentionally alter editor behavior, Markdown rendering semantics, workspace watching semantics, search behavior, persistence formats, automation APIs, Flatpak permissions, or UI contracts

#### Scenario: Feature candidate recorded
- **WHEN** a dependency review identifies a useful feature that would change runtime behavior
- **THEN** the dependency-maintenance change MUST record whether the feature is adopted, deferred, or split into a separate proposal

### Requirement: Dependency Refresh Validation
LushText dependency maintenance SHALL validate the refreshed dependency graph across build, policy, packaging, and affected side-workflow surfaces.

#### Scenario: Repository validation
- **WHEN** dependency maintenance is implemented
- **THEN** the repository validation MUST include the standard fast gate and any focused checks needed for changed dependency surfaces

#### Scenario: Side workflow validation
- **WHEN** side lockfiles or generated packaging metadata change
- **THEN** validation MUST include the corresponding fuzz, GTK Lush fixture, or Flatpak checks before the change is complete

### Requirement: Dependency refreshes observe a publish-age cooldown and the MSRV
A whole-graph dependency refresh SHALL run through one repository command. That command SHALL resolve only releases at least seven days old, and only releases whose `rust-version` the MSRV satisfies where one exists. It SHALL set both policies through environment variables, never `.cargo/config.toml`. Afterwards it SHALL regenerate `workspace-hack` and `cargo-sources.json`, check the workspace on the pinned toolchain, and run the whole cargo-deny policy.

#### Scenario: A cooldown refresh skips too-new releases
- **WHEN** the refresh command runs and a dependency has a release younger than seven days
- **THEN** the refreshed `Cargo.lock` keeps the newest older release

#### Scenario: The refresh command completes before any exception is applied
- **WHEN** a change introduces or first applies the refresh command
- **THEN** the command runs once to completion, through `cargo deny check advisories bans sources licenses`, before the change applies any recorded cooldown exception

#### Scenario: A refresh that brings a new licence or advisory stops
- **WHEN** the refreshed graph fails cargo-deny's licence, advisory or source check
- **THEN** the refresh command exits with a failure
- **AND** the change either adds a licence compatible with a GPL-3.0-or-later application to the allow list, with a reason naming the crate that brought it, or holds that crate with a recorded `--precise`, or upgrades past the advisory

#### Scenario: A refresh never selects a release above the MSRV
- **WHEN** the refresh command runs with a Cargo newer than the pinned toolchain
- **THEN** it requests the MSRV-aware resolver fallback
- **AND** the refreshed graph compiles with the pinned toolchain

### Requirement: Cooldown exceptions and duplicate versions are recorded with their reasons
A dependency taken inside the cooldown window SHALL be a recorded exception: the reason, the versions, the command used and the expiry date. A manifest floor raised for it SHALL be listed in the proposal. When a refresh fails cargo-deny's duplicate-version ban for an upstream-structural reason, the fix SHALL be a version-scoped `skip` with a reason and a removal condition, or a reasoned `--precise` hold. `multiple-versions` MUST NOT be weakened.

#### Scenario: sourceview5 0.11.3 is taken before its cooldown ends
- **WHEN** the workspace needs sourceview5 0.11.3 on its release day, because only 0.11.3 provides the safe `replace_all` count
- **THEN** the floor is raised to `0.11.3` and the exception is applied with `--precise` after the cooldown refresh, without the cooldown
- **AND** the build guidance records the reason and that the refresh command fails on this floor until 2026-11-21

#### Scenario: Upstream-structural duplicate majors are skipped with a removal condition
- **WHEN** a refresh brings in a second major of a crate only because upstream crates have not all moved (for example `syn` 2 and 3, or `system-deps` 7 and 9)
- **THEN** `deny.toml` skips the older version with a reason that names the lagging crates and the condition for removing the skip
- **AND** `cargo deny check bans` passes
