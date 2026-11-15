## ADDED Requirements

### Requirement: Dependency refreshes observe a publish-age cooldown and the MSRV
A whole-graph dependency refresh SHALL run through one repository command. That command SHALL resolve only releases at least seven days old, and only releases whose `rust-version` the MSRV satisfies where one exists. It SHALL set both policies through environment variables, never `.cargo/config.toml`. Afterwards it SHALL regenerate `workspace-hack` and `cargo-sources.json`, check the workspace on the pinned toolchain, and run the cargo-deny bans check.

#### Scenario: A cooldown refresh skips too-new releases
- **WHEN** the refresh command runs and a dependency has a release younger than seven days
- **THEN** the refreshed `Cargo.lock` keeps the newest older release

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
