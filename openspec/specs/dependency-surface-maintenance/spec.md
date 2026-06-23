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
