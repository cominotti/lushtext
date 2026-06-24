## MODIFIED Requirements

### Requirement: Release CI Covers The Flathub Path
GitHub Actions SHALL exercise the release-specific Flatpak and Flathub update path before maintainers depend on it for a real release. Release-related workflows MUST remain within the repository's 30-minute job timeout ceiling, and release completion MUST be reported only from the exact current workflow surface and any successful replacement runs.

#### Scenario: Release dry run runs in CI
- **WHEN** a pull request changes release scripts, Flatpak manifests, AppStream metadata, desktop metadata, or cargo vendoring
- **THEN** CI runs a release dry-run or equivalent check that validates version computation and release packaging checks without publishing a tag
- **AND** the dry-run job timeout is 30 minutes or less

#### Scenario: Tag workflow validates release artifacts
- **WHEN** a `v*` tag is pushed
- **THEN** the release workflow checks out that tag
- **AND** it validates AppStream metadata, desktop metadata, Cargo vendored sources, and the Flatpak build for the tagged source
- **AND** every release workflow job stays within the 30-minute timeout ceiling

#### Scenario: Release workflow creates GitHub release context
- **WHEN** the tag workflow succeeds
- **THEN** it creates or updates the GitHub Release context needed by downstream release assets and Flathub update messaging
- **AND** it keeps benchmark-report upload behavior compatible with the release benchmark workflow's bounded report contract

#### Scenario: Release benchmark report is part of the release surface
- **WHEN** a release tag is pushed
- **THEN** the expected release benchmark report workflow is part of the release verification surface
- **AND** a cancelled, timed-out, failed, skipped-required, or missing benchmark report workflow prevents the release from being reported fully green

#### Scenario: Failed release workflow responsibility can be superseded
- **WHEN** a release-related workflow run fails, is cancelled, or times out after the public tag already exists
- **THEN** maintainers may repair tooling or workflow configuration on `main` and dispatch a replacement workflow that still resolves the immutable public tag as the release source
- **AND** release status reports identify both the failed run and the successful replacement run that satisfied the same workflow responsibility

#### Scenario: Release completion requires exact workflow evidence
- **WHEN** a release is reported as green
- **THEN** the report includes the release commit, release tag, checked workflow names, run IDs, and conclusions for the exact release tag or recovery dispatches
- **AND** no expected workflow responsibility remains in a non-success state without a successful replacement run
