## ADDED Requirements

### Requirement: Production builds write draft bodies only through the registration token
In any build without `cfg(test)` or the `test-utils` feature, the system SHALL
offer no way to write a draft body except `write_draft`, which takes a
`RegisteredDraft` token. The unchecked draft-body fixture writer, and the
generic filesystem fixture helpers that could write a draft body directly,
SHALL be compiled only under `cfg(test)` or `test-utils`, together with the
private filesystem backend operations that only those helpers use. `test-utils`
MUST NOT be a default feature, directly or through a feature that implies it.
No shipping build (release `[dependencies]` of the shipped binary, Meson,
Flatpak, or Snap) SHALL enable `test-utils` or any feature that implies it,
directly or transitively; the implying features SHALL be derived from the
workspace manifests rather than kept in a hand-written list. Test targets that
need the fixtures SHALL get them through a feature that implies `test-utils`
(for example `property-tests`) or an explicit `--features test-utils` on their
documented command, so a missing feature fails at compile time rather than
skipping the target. The project SHALL verify all of this with a policy check
that has a self-test. It SHALL also run `cargo check -p lushtext --bins
--locked` with default features in CI, because an all-features gate enables
`test-utils` and cannot see a production caller.

#### Scenario: Production code cannot call the fixture writer
- **WHEN** production code in `lushtext-core` or `lushtext` names `draft_service::fixture::write_body` or `filesystem::fixture`
- **THEN** the default-feature build of the shipped binary fails to compile

#### Scenario: Tests and benchmarks keep their seeding
- **WHEN** unit tests, `lushtext-core` integration and property tests, the `lushtext` integration and widget tests, or the benchmarks are built through their documented commands
- **THEN** they compile with the fixture helpers available and seed unregistered bodies exactly as before
- **AND** the property suite gets them because `property-tests` implies `test-utils`, and every documented benchmark command passes `--features test-utils`

#### Scenario: An ungated fixture module is caught
- **WHEN** either fixture module declaration loses its `cfg(any(test, feature = "test-utils"))` gate, or `test-utils` or a feature that implies it becomes a default feature, a shipped-binary dependency feature, or a feature enabled by a shipping build file (`build-aux/cargo.sh`, the Flatpak manifest, `snap/snapcraft.yaml`, or the Meson build and options files)
- **THEN** the policy check fails
