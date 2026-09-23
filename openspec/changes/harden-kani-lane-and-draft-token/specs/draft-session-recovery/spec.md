## ADDED Requirements

### Requirement: Production builds write draft bodies only through the registration token
In any build without `cfg(test)` or the `test-utils` feature, the system SHALL
offer no way to write a draft body except `write_draft`, which takes a
`RegisteredDraft` token. The unchecked draft-body fixture writer, and the
generic filesystem fixture helpers that could write a draft body directly,
SHALL be compiled only under `cfg(test)` or `test-utils`. `test-utils` MUST NOT
be a default feature. No shipping build (release, Meson, Flatpak, or Snap)
SHALL enable it. The project SHALL verify all of this with a policy check. It
SHALL also compile the shipped binary with its default features in CI, because
an all-features gate enables `test-utils` and cannot see a production caller.

#### Scenario: Production code cannot call the fixture writer
- **WHEN** production code in `lushtext-core` or `lushtext` names `draft_service::fixture::write_body` or `filesystem::fixture`
- **THEN** the default-feature build of the shipped binary fails to compile

#### Scenario: Tests and benchmarks keep their seeding
- **WHEN** unit tests, `lushtext-core` integration and property tests, the `lushtext` integration and widget tests, or the benchmarks are built through their documented commands
- **THEN** they compile with the fixture helpers available and seed unregistered bodies exactly as before

#### Scenario: An ungated fixture module is caught
- **WHEN** either fixture module declaration loses its `cfg(any(test, feature = "test-utils"))` gate, or a shipping manifest enables `test-utils`
- **THEN** the policy check fails
