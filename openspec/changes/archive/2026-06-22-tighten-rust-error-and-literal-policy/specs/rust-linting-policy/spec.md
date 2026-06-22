## ADDED Requirements

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
