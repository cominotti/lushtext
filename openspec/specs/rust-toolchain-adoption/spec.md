# rust-toolchain-adoption Specification

## Purpose
Define LushText's Rust 1.96.0 adoption contract so toolchain declarations,
validation gates, contributor guidance, accepted idioms, and Cargo registry
security posture stay aligned across development, CI, and packaging.

## Requirements
### Requirement: Rust floor is aligned across all stable toolchain surfaces
The implementation SHALL pin LushText's stable Rust floor to Rust 1.96.0 everywhere the repository declares, installs, bootstraps, or documents the stable toolchain. The implementation MUST keep nightly-only tooling, such as coverage-guided fuzz smoke, explicitly separate from the stable MSRV.

#### Scenario: Stable version surfaces agree
- **WHEN** the repository is searched for active Rust MSRV, rustup, CI stable-toolchain, Snap rustup bootstrap, README, AGENTS, rule, and skill references
- **THEN** active references name Rust 1.96.0 instead of Rust 1.95.0
- **AND** stale Rust 1.95.0 references remain only in archive/history context or are removed

#### Scenario: Nightly lanes remain explicit exceptions
- **WHEN** fuzz smoke or other nightly-only tooling is inspected
- **THEN** those lanes remain pinned to or invoked through nightly as required
- **AND** they are not presented as the stable Rust floor for normal development, CI, or packaging

#### Scenario: Packaging toolchains satisfy the floor
- **WHEN** Flatpak and Snap packaging surfaces are inspected
- **THEN** Snap's rustup bootstrap installs Rust 1.96.0
- **AND** the Flatpak manifest either continues to use a GNOME SDK Rust stable extension that satisfies Rust 1.96.0 or documents a concrete blocker before implementation can be considered complete

### Requirement: Rust 1.96 compatibility is proven before completion
The implementation SHALL prove the upgraded toolchain with deterministic commands that exercise formatting, Clippy, rustdoc, and representative tests. The implementation MUST record or report any validation that cannot be run and why.

#### Scenario: Core stable validation passes
- **WHEN** the change implementation is ready for review
- **THEN** `cargo +1.96.0 fmt --all -- --check` passes
- **AND** `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings` passes
- **AND** the rustdoc lint gate passes under Rust 1.96.0
- **AND** representative non-GTK tests pass under Rust 1.96.0

#### Scenario: New Clippy lint probe passes
- **WHEN** the workspace lint policy is updated for Rust 1.96 Clippy
- **THEN** an explicit Clippy run with `manual_option_zip`, `manual_pop_if`, and `manual_noop_waker` enabled passes
- **AND** any lint added to `[workspace.lints.clippy]` is supported by Rust 1.96.0

#### Scenario: Documentation and agent guidance validation passes
- **WHEN** README, AGENTS, `.agents/rules`, or `.agents/skills` are changed
- **THEN** `make check-agent-docs` passes
- **AND** the repository guidance does not contain active stale MSRV instructions

### Requirement: Rust 1.96 idioms are adopted selectively
The implementation SHALL adopt Rust 1.96 language and library features only where they improve diagnostics, correctness, or readability in LushText's existing patterns. The implementation MUST NOT perform broad mechanical rewrites solely because a feature is new.

#### Scenario: Pattern assertions produce better failures
- **WHEN** a test currently uses `assert!(matches!(value, pattern))` or `debug_assert!(matches!(value, pattern))`
- **THEN** the implementation evaluates whether `std::assert_matches` or `std::debug_assert_matches` gives better failure output
- **AND** converted modules explicitly import the macro because it is not in the prelude

#### Scenario: Copyable ranges are used only for clear span-value wins
- **WHEN** code stores a byte span as a named value object or struct field and later needs to reuse the range after reading its bounds
- **THEN** the implementation MAY use `core::range::Range` if its `Copy` behavior removes real clone or move friction
- **AND** any required conversion from range syntax is localized and clearer than the code it replaces

#### Scenario: Legacy ranges remain where they are clearer
- **WHEN** a site is mainly range syntax, property-test strategy generation, third-party API interop, or an API accepting caller-provided ranges
- **THEN** the implementation keeps `std::ops::Range` or accepts `impl RangeBounds<usize>` as appropriate
- **AND** it does not introduce `Range::from(...)` conversions that make the code harder to read

#### Scenario: New Clippy lints are curated
- **WHEN** Rust 1.96 Clippy introduces new lints relevant to LushText
- **THEN** only lints that pass cleanly and match the repo's correctness/readability policy are added to the workspace lint table
- **AND** broad lint groups are not enabled as a substitute for explicit lint choices

### Requirement: Agent and contributor guidance stays current
The implementation SHALL update human-facing and agent-facing guidance in the same change as the toolchain adoption. Guidance MUST state the new Rust floor and the accepted Rust 1.96 patterns so future agents and contributors review against the same rules as CI.

#### Scenario: Build guidance names the new floor
- **WHEN** `.agents/rules/build.md`, README build sections, and AGENTS tech-stack sections are inspected
- **THEN** they describe Rust 1.96.0 as the active MSRV or pinned stable toolchain
- **AND** they keep existing Edition 2024 and GNOME 50 guidance intact

#### Scenario: Rust coding guidance includes 1.96 decisions
- **WHEN** `.agents/rules/rust.md` is inspected
- **THEN** it documents explicit `assert_matches` imports, selective `core::range` adoption, and any new Clippy 1.96 lint expectations
- **AND** it preserves existing Rust 1.95 idioms that remain valid under 1.96

#### Scenario: Rust-review skills use the new assumptions
- **WHEN** `.agents/skills/gtk-perf-rust-optimize/SKILL.md` and any delegating Rust-quality skill prompts are inspected
- **THEN** they name Rust 1.96.0 as the project MSRV
- **AND** their modern-Rust audit criteria include the accepted 1.96 idioms without encouraging noisy micro-optimizations

### Requirement: Cargo registry-security posture is checked
The implementation SHALL account for the Cargo security fixes included in Rust 1.96.0. The implementation MUST confirm whether the repository uses third-party registries or sparse-registry credentials before relying on older Cargo behavior.

#### Scenario: No alternate registry configuration is found
- **WHEN** Cargo manifests, `.cargo` configuration, CI workflows, and packaging scripts are inspected for alternate registry configuration
- **THEN** the implementation records that LushText currently uses crates.io-compatible sources only
- **AND** no extra mitigation beyond adopting Cargo 1.96.0 is required

#### Scenario: Alternate registry configuration would block completion
- **WHEN** any third-party registry or registry credential configuration is discovered during implementation
- **THEN** the implementation does not treat the toolchain bump as complete until the registry flow is validated under Cargo 1.96.0
- **AND** any required mitigation is captured in the tasks or design before code changes proceed
