## Purpose

Define LushText's repository-wide Rust linting, advisory lint discovery,
dependency policy, validation-tool pinning, and synchronized documentation
contract.

## Requirements

### Requirement: Blocking Clippy gate covers every stable workspace feature
The project SHALL run the standard blocking Clippy gate across the whole workspace, all targets, and all enabled workspace features. The local Makefile target, repository pre-commit hook, GitHub Actions lint job, and contributor guidance MUST name the same Clippy command unless a narrower command is explicitly documented as a non-blocking smoke shortcut.

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

### Requirement: Curated high-signal Clippy lints are blocking only after cleanup
The project SHALL promote high-signal Clippy lints into `[workspace.lints.clippy]` only after the current workspace passes those lints under the standard blocking Clippy command. The implementation MUST clean or narrowly justify all current findings for `manual_midpoint`, `unchecked_time_subtraction`, `case_sensitive_file_extension_comparisons`, `significant_drop_tightening`, `needless_collect`, `redundant_clone`, `derive_partial_eq_without_eq`, and `wildcard_imports`.

#### Scenario: High-signal lint cleanup is complete
- **WHEN** the implementation is ready for review
- **THEN** the standard blocking Clippy command passes with every newly promoted high-signal lint set to `deny`
- **AND** no current finding for the promoted lint set remains hidden behind a broad lint suppression

#### Scenario: Exceptions are explicit and self-pruning
- **WHEN** a promoted lint is intentionally not applied at a specific site
- **THEN** that site uses `#[expect(..., reason = "...")]` with a concise project-specific reason
- **AND** the project does not use a broad `#[allow]` attribute to silence the promoted lint category

#### Scenario: Generated or framework-shaped code is reviewed before exception
- **WHEN** a promoted lint fires in generated, build-script, GTK subclass, signal-closure, benchmark, or test harness code
- **THEN** the implementation either changes the code safely or records a narrow `#[expect]` reason explaining the local invariant
- **AND** that exception does not weaken the lint for unrelated modules

### Requirement: Error type names identify cross-boundary workflows
Rust error types that are public, `pub(crate)`, re-exported, service-facing, UI-facing, or used across crate boundaries SHALL include enough workflow or domain context in their type name for callers to understand the failing operation without relying on the defining module path. Private helper errors MAY keep shorter names when all uses stay local and the surrounding function already supplies the workflow context.

#### Scenario: Cross-boundary errors carry domain context
- **WHEN** a typed error is returned from a service API, re-exported through a UI module, or matched outside its defining module
- **THEN** the error type name includes the owning workflow, domain, or artifact context
- **AND** the name is more specific than a bare `Error`, `LoadError`, `SaveError`, `ValidationError`, or mechanism-only name when those names would be ambiguous at the call site

#### Scenario: Local helper errors may stay narrow
- **WHEN** an error type is private to one small helper workflow and is not re-exported or matched outside the owning module
- **THEN** the implementation MAY keep a concise local name
- **AND** the surrounding function, module, or variant names make the failing operation clear

### Requirement: Numeric literals encode policy through named owners
Rust numeric literals that define user-visible behavior, persistence limits, file-size thresholds, retry budgets, debounce intervals, timeout windows, UI geometry, protocol constants, schema limits, or resource caps SHALL be expressed as named typed constants or small policy values near the module, service, model, or tool that owns the decision. Inline literals SHALL remain acceptable for obvious identities, indexes, counters, simple arithmetic identities, and test fixture data that does not mirror production policy.

#### Scenario: Behavioral numeric policy is named
- **WHEN** implementation adds or edits a timeout, debounce, cap, threshold, schema limit, file-size budget, UI geometry budget, or retry count
- **THEN** the value is represented by a name that describes the policy in domain or workflow terms
- **AND** the constant lives near the owner rather than in a generic cross-project constants module

#### Scenario: Harmless literals remain inline
- **WHEN** implementation uses literals such as `0`, `1`, indexes, empty counts, identity arithmetic, tuple coordinates in a local fixture, or expected values in a narrow test
- **THEN** the literal MAY stay inline
- **AND** the implementation does not introduce low-value constants such as `ZERO`, `ONE`, or names that only restate the number

#### Scenario: Shared policy moves only when ownership is shared
- **WHEN** two or more modules use the same numeric value
- **THEN** the implementation moves it to a shared constant only if the modules intentionally share one policy
- **AND** coincidentally equal values remain local to their separate workflow owners

### Requirement: Broad Clippy groups remain advisory discovery inputs
The project SHALL NOT enable `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, or `clippy::cargo` as blanket blocking groups. Broad groups MAY be used only through advisory discovery commands or through individually curated lints that have been proven useful for LushText.

#### Scenario: Restriction group is not blanket-enabled
- **WHEN** the workspace lint table, crate attributes, and CI Clippy commands are inspected
- **THEN** they do not set `clippy::restriction` to `warn`, `deny`, or `forbid` as a whole group
- **AND** any restriction lint in the blocking policy is named individually

#### Scenario: Pedantic and nursery findings are advisory unless curated
- **WHEN** advisory lint discovery runs with `clippy::pedantic` or `clippy::nursery`
- **THEN** findings from those groups are summarized for review
- **AND** they do not fail the blocking lint gate unless the specific lint has been added to the curated deny list

#### Scenario: Cargo lint group does not force nightly stable-CI behavior
- **WHEN** Cargo or Clippy cargo-related lint discovery is configured
- **THEN** stable CI does not require nightly-only Cargo linting features as a blocking gate
- **AND** any cargo-related lint that becomes blocking is supported by the pinned stable toolchain and documented in the lint policy

### Requirement: Numeric literal linting stays curated and low-noise
The project SHALL use Clippy numeric-literal lints as curated policy tools rather than as a blanket magic-number ban. Low-noise literal-format lints MAY be promoted to `[workspace.lints.clippy]` only after the workspace is clean under `cargo clippy --workspace --all-targets --all-features -- -D warnings`; noisy numeric lints MUST remain advisory unless a future cleanup proves they are suitable for the blocking gate.

#### Scenario: Low-noise literal lints are promoted only after cleanup
- **WHEN** a numeric-literal Clippy lint is added to the blocking workspace lint table
- **THEN** the standard all-targets/all-features Clippy gate passes without broad suppressions
- **AND** any local exception uses `#[expect(..., reason = "...")]` with a project-specific reason

#### Scenario: No blanket magic-number lint is simulated through noisy rules
- **WHEN** lint policy is reviewed after this change
- **THEN** `clippy::restriction`, `clippy::pedantic`, `clippy::nursery`, and `clippy::cargo` remain advisory discovery inputs rather than blanket blocking groups
- **AND** high-volume lints such as `clippy::default_numeric_fallback`, `clippy::float_arithmetic`, and `clippy::integer_division` are not promoted unless their current findings are cleaned or narrowly classified without broad suppressions

#### Scenario: Advisory policy records numeric lint decisions
- **WHEN** `make lint-advisory` or an equivalent numeric-lint discovery probe finds a new numeric lint category
- **THEN** `scripts/lint-advisory-policy.toml` classifies it as a blocking candidate, must-stay-zero advisory, accepted advisory, generated-code noise, or resolved policy exception
- **AND** the rationale explains whether the lint protects literal readability, numeric safety, or remains too noisy for GTK/test/proof-tool code

### Requirement: Advisory lint lane is repeatable and classified
The project SHALL provide a documented advisory lint lane that runs broad Clippy, selected Clippy design-smell, selected rustc, and selected dependency-policy probes in a repeatable way. The advisory lane MUST classify every known lint category as blocking candidate, must-stay-zero advisory, accepted advisory, generated-code noise, or dependency-policy follow-up resolved within this change.

#### Scenario: Advisory command produces structured output
- **WHEN** a developer runs the advisory lint target
- **THEN** the command emits a deterministic summary grouped by lint code, count, first file, first line, and first message
- **AND** the output is suitable for comparing future lint drift without manually scanning thousands of lines

#### Scenario: New advisory categories are not ignored
- **WHEN** the advisory lint target finds a lint category that is not classified by the checked-in policy
- **THEN** the advisory target fails or reports the category as unclassified
- **AND** implementation cannot be considered complete until the category is fixed, promoted, or classified with rationale

#### Scenario: No untriaged findings remain
- **WHEN** the implementation completes the lint-hardening work
- **THEN** every advisory finding discovered during this change is either fixed, promoted to the blocking policy, marked must-stay-zero, or recorded as accepted advisory with rationale
- **AND** no finding is deferred without classification

### Requirement: rustc lint candidates are cleaned or classified
The project SHALL evaluate stable rustc lint candidates that improve readability, API boundaries, or future compatibility. Current findings for `unused_qualifications`, `unreachable_pub`, and `unused_crate_dependencies` MUST be cleaned or classified before the change is complete, and future-compatibility and Edition 2024 rustc probes MUST remain clean under the pinned stable toolchain.

#### Scenario: Future compatibility probes are clean
- **WHEN** the rustc advisory probe runs under Rust 1.96.0
- **THEN** future-incompatible and Edition 2024 compatibility lint groups produce no unclassified warnings
- **AND** any relevant lint already covered by `[workspace.lints.rust]` remains enforced there

#### Scenario: Unnecessary qualifications are addressed
- **WHEN** the implementation examines `unused_qualifications` findings
- **THEN** unnecessary qualifications are removed where doing so improves readability
- **AND** any retained qualification is classified with a reason such as macro clarity, GTK trait disambiguation, or generated-code constraints

#### Scenario: Unreachable public items are addressed
- **WHEN** the implementation examines `unreachable_pub` findings
- **THEN** public visibility is narrowed where the item is not part of a reachable API surface
- **AND** any retained unreachable public visibility is classified with a reason such as macro expansion, GTK subclassing, integration-test access, or future public surface intentionally documented by the owning module

#### Scenario: Unused dependency lint respects cargo-hakari
- **WHEN** the implementation examines `unused_crate_dependencies` findings
- **THEN** real unused direct dependencies are removed from manifests
- **AND** generated cargo-hakari dependencies, `workspace-hack`, dev dependencies, benches, and build dependencies are handled through explicit policy rather than silent suppression

### Requirement: Project-specific API policy uses the right enforcement tool
The project SHALL use `clippy.toml` `disallowed-methods` or `disallowed-types` only for globally safe bans that apply across all paths where Clippy runs. Path-sensitive policies, including filesystem-boundary ownership, private backend access, approved engine adapters, and fixture exceptions, MUST remain enforced by path-aware audit tooling.

#### Scenario: Globally disallowed APIs are configured only when safe
- **WHEN** implementation adds or updates `clippy.toml`
- **THEN** each disallowed method or type includes a reason and replacement when one exists
- **AND** the ban does not require broad local suppressions in approved backend, build-support, fixture, generated, or test-harness code

#### Scenario: Filesystem boundary remains path-aware
- **WHEN** production or test code imports raw filesystem APIs, direct backend APIs, or approved filesystem engine APIs outside allowlisted modules
- **THEN** the filesystem-boundary audit reports the violation
- **AND** the project does not rely on Clippy alone for this path-sensitive policy

#### Scenario: clippy.toml absence is intentional if no global ban is safe
- **WHEN** implementation determines there is no globally safe disallowed-methods or disallowed-types policy to add
- **THEN** that decision is documented in the lint policy guidance or design notes
- **AND** no empty or misleading `clippy.toml` is created solely to appear comprehensive

### Requirement: Filesystem and policy audits join normal lint validation
The project SHALL include fast policy audits that catch lint-adjacent architectural drift in the normal local and CI validation path. The filesystem-boundary audit MUST be available as a Makefile target and MUST run in CI or in a documented aggregate lint/policy target used before publication.

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

### Requirement: cargo-deny enforces complete dependency policy
The project SHALL enforce cargo-deny advisories, bans, sources, and licenses as the normal dependency policy gate. The policy MUST use all workspace features and the supported target graph, and every license, duplicate-version, source, yanked, unmaintained, or unsound dependency exception MUST be explicit and justified.

#### Scenario: License policy passes
- **WHEN** the dependency policy gate runs
- **THEN** `cargo deny check advisories bans sources licenses` passes
- **AND** the license allow-list is compatible with distributing a GPL-3.0-or-later application through LushText's supported channels

#### Scenario: workspace-hack is licensed intentionally
- **WHEN** cargo-deny evaluates the generated `workspace-hack` package
- **THEN** it does not report the package as unlicensed
- **AND** the package's license metadata remains compatible with the rest of the workspace

#### Scenario: Duplicate crate versions are justified
- **WHEN** cargo-deny evaluates multiple crate versions
- **THEN** avoidable duplicate versions are removed or unified
- **AND** unavoidable foundational duplicates such as target-specific platform crates are represented by narrow `skip` or `skip-tree` entries with reasons

#### Scenario: Workspace dependency policy is intentional
- **WHEN** cargo-deny evaluates workspace dependency duplicates and unused workspace dependencies
- **THEN** the configured levels reflect an intentional cargo-hakari-aware policy
- **AND** unused non-hakari direct dependencies are not silently accepted

### Requirement: CI-installed Rust validation tools are pinned
The project SHALL pin Rust validation helpers installed by CI or scripted validation so lint and test outcomes do not drift because a tool's latest release changed. Version pins MUST be centralized or documented clearly enough that future updates are intentional.

#### Scenario: cargo-deny install is version-pinned
- **WHEN** the dependency policy job installs cargo-deny
- **THEN** the install command or action selects a specific cargo-deny version
- **AND** the selected version is documented in repository guidance or workflow configuration

#### Scenario: cargo-nextest install is version-pinned
- **WHEN** CI installs cargo-nextest for non-widget or property tests
- **THEN** the install path selects a specific cargo-nextest version instead of a moving latest download
- **AND** every workflow that installs cargo-nextest uses the same configured version unless a difference is documented

#### Scenario: other Rust validation helpers are pinned or excepted
- **WHEN** CI installs cargo-fuzz, cargo-mutants, actionlint, or another Rust validation helper
- **THEN** the tool is pinned to an exact version where practical
- **AND** any unpinned nightly or external-tool exception is documented with the reason it cannot be pinned equivalently

### Requirement: Lint policy documentation is authoritative and synchronized
The project SHALL document the Rust linting policy for contributors and agents in the same change that changes the gate. Documentation MUST explain the blocking gate, advisory lane, cargo-deny policy, tool pinning, exception style, and broad-group policy without contradicting CI.

#### Scenario: Root guidance matches CI
- **WHEN** AGENTS, README, and `.agents/rules/build.md` describe Rust validation
- **THEN** they name the all-targets/all-features Clippy gate
- **AND** they include the complete cargo-deny policy gate and fast policy audits

#### Scenario: Rust guidance describes curated lint promotion
- **WHEN** `.agents/rules/rust.md` is inspected
- **THEN** it explains that broad Clippy groups are advisory discovery inputs
- **AND** it names the newly promoted blocking lints and the expected `#[expect(..., reason = "...")]` exception style

#### Scenario: Rules index stays synchronized
- **WHEN** any `.agents/rules/*.md` file is added or materially changed for lint policy
- **THEN** the root AGENTS rules index is updated in the same change
- **AND** `make check-agent-docs` passes

### Requirement: Agent guidance repeats error and literal policy
Repository guidance SHALL teach future agents and contributors the same error-type naming and numeric-literal policy that the Rust linting spec requires. The implementation MUST update relevant `.agents/rules` and local skills in the same change when their existing Rust, architecture, comment, lint, or review guidance would otherwise omit or contradict the policy.

#### Scenario: Rust rule guidance is synchronized
- **WHEN** `.agents/rules/rust.md` is inspected after this change
- **THEN** it describes cross-boundary error-type naming expectations
- **AND** it describes which numeric literals should become named constants or policy values

#### Scenario: Relevant skills are synchronized
- **WHEN** a relevant local skill guides Rust architecture, comments, lint-sensitive review, performance review, data safety, or OpenSpec implementation
- **THEN** the skill either incorporates the error/numeric-literal policy or remains intentionally unchanged because its scope does not review Rust code or guidance
- **AND** the implementation notes any intentionally unchanged obvious candidate

#### Scenario: Agent documentation validation passes
- **WHEN** the implementation changes `.agents/rules`, root `AGENTS.md`, or local skills for this policy
- **THEN** `make check-agent-docs` passes
- **AND** the root AGENTS rules index is updated if a rule file is added or materially changed

### Requirement: Implementation completion is proven by full validation
The lint-hardening implementation SHALL NOT be considered complete until the repository passes all blocking lint, dependency, policy, documentation, workflow, and OpenSpec validation required by the changed surfaces.

#### Scenario: Rust and dependency gates pass
- **WHEN** the implementation is ready for review
- **THEN** `cargo fmt --all -- --check` passes
- **AND** `cargo clippy --workspace --all-targets --all-features -- -D warnings` passes
- **AND** the rustdoc lint gate passes
- **AND** `cargo deny check advisories bans sources licenses` passes

#### Scenario: Policy and documentation gates pass
- **WHEN** the implementation changes policy scripts or agent guidance
- **THEN** `scripts/check-filesystem-boundary.sh` passes
- **AND** `make check-agent-docs` passes
- **AND** the advisory lint target runs and leaves no unclassified findings

#### Scenario: Workflow and OpenSpec gates pass
- **WHEN** GitHub workflow files or OpenSpec artifacts are changed
- **THEN** `actionlint` passes for the changed workflows
- **AND** `openspec validate --all --strict` passes

#### Scenario: Touched behavior is tested
- **WHEN** lint cleanup changes Rust behavior rather than only formatting or visibility
- **THEN** relevant unit, integration, property, widget, or benchmark-compile validation runs for the touched modules
- **AND** any validation that cannot be run is reported with the concrete reason
