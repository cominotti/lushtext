# rust-toolchain-adoption Specification

## Purpose
Define LushText's Rust toolchain adoption contract (current floor: Rust 1.98.0, pinned toolchain Rust 1.98.1; the idiom and lint requirements below were adopted with Rust 1.96.0) so toolchain declarations,
validation gates, contributor guidance, accepted idioms, and Cargo registry
security posture stay aligned across development, CI, and packaging.

## Requirements

### Requirement: Rust floor is aligned across all stable toolchain surfaces
The implementation SHALL pin LushText's stable Rust floor (MSRV) to Rust 1.98.0. It SHALL pin the toolchain to the Rust version that `org.freedesktop.Sdk.Extension.rust-stable` ships for the Freedesktop SDK branch of the Flatpak manifest's GNOME runtime: `26.08` for GNOME 51, which shipped Rust 1.98.0 on 2026-11-09. The pin SHALL be identical everywhere the repository declares, installs, bootstraps, or documents the stable toolchain, so the binary that is tested and measured is built by the compiler that builds the shipped binary. A runtime change SHALL re-check that extension's version and move the pin in the same change. The implementation MUST keep nightly-only tooling, such as coverage-guided fuzz smoke, explicitly separate from the stable MSRV. The one stable-toolchain exception is the weekly advisory next-stable lane: it SHALL select the latest stable toolchain explicitly, and it MUST NOT run on pull requests or pushes, be a required check, or be presented as the pin.

#### Scenario: Stable version surfaces agree
- **WHEN** the repository is searched for active Rust MSRV, rustup, CI stable-toolchain, Snap rustup bootstrap, README, AGENTS, rule, and skill references
- **THEN** active references name Rust 1.98.0 as the floor and the shipped extension's Rust version as the pinned toolchain
- **AND** the only CI reference to another stable toolchain is the advisory next-stable lane, which selects `stable` explicitly and is documented as an exception
- **AND** a previous pin (such as Rust 1.98.1 from the GNOME 50 SDK's `25.08` extension) remains only in archive/history context or is removed

#### Scenario: Nightly lanes remain explicit exceptions
- **WHEN** fuzz smoke or other nightly-only tooling is inspected
- **THEN** those lanes remain pinned to or invoked through nightly as required
- **AND** they are not presented as the stable Rust floor for normal development, CI, or packaging

#### Scenario: Packaging toolchains satisfy the floor
- **WHEN** Flatpak and Snap packaging surfaces are inspected
- **THEN** Snap's rustup bootstrap installs the pinned toolchain
- **AND** the Flatpak manifest's GNOME SDK provides a Rust stable extension whose version equals the pinned toolchain and satisfies Rust 1.98.0, or a concrete blocker is documented before implementation can be considered complete

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
The implementation SHALL update human-facing and agent-facing guidance in the same change as the toolchain adoption. Guidance MUST state the current Rust floor, the pinned toolchain, and the accepted Rust idioms, so future agents and contributors review against the same rules as CI.

#### Scenario: Build guidance names the new floor
- **WHEN** `.agents/rules/build.md`, README build sections, and AGENTS tech-stack sections are inspected
- **THEN** they describe Rust 1.98.0 as the active MSRV and Rust 1.98.0, the release the shipped Flatpak SDK extension provides, as the pinned stable toolchain
- **AND** they keep existing Edition 2024 guidance and the current GNOME runtime and source-floor guidance intact

#### Scenario: Rust coding guidance includes 1.96 decisions
- **WHEN** `.agents/rules/rust.md` is inspected
- **THEN** it documents explicit `assert_matches` imports, selective `core::range` adoption, and any new Clippy 1.96 lint expectations
- **AND** it preserves existing Rust 1.95 idioms that remain valid under 1.96

#### Scenario: Rust-review skills use the new assumptions
- **WHEN** `.agents/skills/gtk-perf-rust-optimize/SKILL.md` and any delegating Rust-quality skill prompts are inspected
- **THEN** they derive the project MSRV from the current `Cargo.toml` instead of naming a stale version
- **AND** their modern-Rust audit criteria include the accepted idioms without encouraging noisy micro-optimizations

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

### Requirement: Toolchain versions with known miscompiles are never adopted
The build guidance SHALL name the Rust releases LushText must not pin because of known miscompiles, the floor of each affected series, and any fix the shipped compiler lacks. If the Flatpak SDK extension ships an excluded release, the pin SHALL stay where it is until the extension ships an acceptable one.

#### Scenario: The SDK extension ships an excluded release
- **WHEN** the `rust-stable` extension branch the Flatpak uses moves to Rust 1.99.0, which miscompiles FFI `bool` returns on x86_64
- **THEN** the build guidance tells the maintainer not to move the pin to it
- **AND** names 1.99.1 as the lowest acceptable 1.99 release

#### Scenario: The policy is discoverable where the pin rule lives
- **WHEN** `.agents/rules/build.md`'s `rust-version` item is read
- **THEN** it states the excluded releases and the series floors
- **AND** it records that the shipped Rust 1.98.0 lacks the 1.98.1 boxed-async vtable fix

### Requirement: A weekly advisory lane builds with the latest stable toolchain
The repository SHALL run an advisory CI workflow, weekly and on manual dispatch, that runs the blocking Clippy command with the latest stable Rust toolchain and prints the version it tested. It MUST NOT run on pull requests or pushes and MUST NOT be a required check. The build guidance SHALL record it as an explicit exception to pinning every CI toolchain step, beside the nightly lanes.

#### Scenario: New stable Clippy churn is reported the week it ships
- **WHEN** a new stable Rust release adds a lint that fires in the workspace
- **THEN** the next weekly run of the advisory workflow fails and names the toolchain version
- **AND** no pull request or `main` push is blocked by it

#### Scenario: The advisory lane cannot silently test the pinned toolchain
- **WHEN** the advisory workflow's Clippy step is inspected
- **THEN** it selects the stable toolchain explicitly (for example `cargo +stable`), so `rust-toolchain.toml` cannot pin it back to the shipped version

### Requirement: Build tooling tolerates Cargo's build-directory layout v2
Repository tooling that reads Cargo's build directory SHALL accept both the layout-v1 and layout-v2 build-script output paths. The pinned cargo-nextest SHALL be at least 0.9.143. The generated `workspace-hack` package SHALL keep an empty `[lints]` table outside its generated section.

#### Scenario: Generated sources are mapped under either layout
- **WHEN** the advisory lint lane sees a diagnostic under `build/<pkg>-<hash>/out` (layout v1) or `build/<pkg>/<hash>/out` (layout v2)
- **THEN** it classifies the path as generated code in both cases

#### Scenario: The nextest pin satisfies the layout-v2 floor
- **WHEN** the CI-installed cargo-nextest version is inspected
- **THEN** it is one exact version, 0.9.143 or later, the first release that resolves dylib and artifact paths under layout v2 and `build.build-dir`

#### Scenario: Regenerating workspace-hack keeps its lints table
- **WHEN** `cargo hakari generate` rewrites `workspace-hack/Cargo.toml`
- **THEN** the empty `[lints]` table outside the hakari section is still present
- **AND** newer Cargo releases do not warn that `workspace-hack` fails to inherit the workspace lints
