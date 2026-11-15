## MODIFIED Requirements

### Requirement: Blocking Clippy gate covers every stable workspace feature
The project SHALL run the standard blocking Clippy gate across the whole workspace, all targets, and all enabled workspace features. The local Makefile target, repository pre-commit hook, GitHub Actions lint job, and contributor guidance MUST name the same Clippy command unless a narrower command is explicitly documented as a non-blocking smoke shortcut. The pre-commit hook MAY skip the Clippy step, and only that step, for a commit that is not a merge and whose staged paths, both sides of renames and deletions included, are all on a fixed allowlist of documentation locations that no Rust source or build script reads. A policy audit SHALL fail when a compile-time input of a Rust source resolves to an allowlisted path: an `include_str!`, `include_bytes!` or `include!` argument or a `#[path]` attribute, resolved relative to its source file, or a path a build script names. An include argument the audit cannot resolve statically SHALL fail when it names an allowlisted location. Runtime reads of documentation SHALL stay allowed. `make pre-commit` and `make check` SHALL always include the Clippy step.

#### Scenario: Local Clippy gate covers all features
- **WHEN** a developer runs the documented blocking Clippy target
- **THEN** the command runs `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- **AND** feature-gated Rust code such as property-test, fuzz-replay, and test-utils support is included in the lint gate

#### Scenario: CI Clippy gate matches local policy
- **WHEN** GitHub Actions runs the lint job for a pull request or push
- **THEN** the Clippy step uses the same workspace/all-targets/all-features command as the local blocking Clippy target
- **AND** CI does not pass a narrower feature set without a documented reason and a separate all-feature gate

#### Scenario: Pre-commit gate stays aligned
- **WHEN** the repo-managed pre-commit hook runs
- **THEN** it invokes the same blocking format and Clippy policy as `make pre-commit`
- **AND** `make pre-commit` does not silently omit all-feature Clippy coverage
- **AND** the only exception is the Clippy step of a docs-only commit, which the hook skips while still running rustfmt and `make check-policy`

#### Scenario: Docs-only commit skips only Clippy
- **WHEN** every staged path of a non-merge commit is on the documentation allowlist
- **THEN** the hook runs rustfmt and `make check-policy` and does not run Clippy

#### Scenario: Any other commit runs the full gate
- **WHEN** a staged path is outside the allowlist, a rename crosses out of it, the commit is a merge, nothing is staged, or the scope cannot be determined
- **THEN** the hook runs `make pre-commit`, including the identical Clippy command

#### Scenario: A documentation path becomes a build input
- **WHEN** a Rust source includes a path on the allowlist at compile time, or a build script names one
- **THEN** `make check-policy` fails and names the source and the path

#### Scenario: A runtime read of documentation stays allowed
- **WHEN** a Rust test or tool reads a documentation file at run time, or contains a `docs/` string literal that is not an include argument
- **THEN** the audit passes for that source
