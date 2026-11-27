## MODIFIED Requirements

### Requirement: Production builds write draft bodies only through the registration token
Without `cfg(test)` or `test-utils`, draft bodies SHALL be written only via `write_draft(RegisteredDraft)`. Fixture writers MUST stay gated by `cfg(test)` or `test-utils`, which SHALL NOT be default or enabled by any shipping build; CI verifies this via policy self-tests and default-feature `cargo check -p lushtext --bins --locked`.

#### Scenario: Production code cannot call the fixture writer
- **WHEN** production code in `lushtext-model`, `lushtext-services`, `lushtext-ui`, or `lushtext` names `draft_service::fixture::write_body` or `filesystem::fixture`
- **THEN** the default-feature build of the shipped binary fails to compile

#### Scenario: Tests and benchmarks keep their seeding
- **WHEN** unit tests, `lushtext-model` / `lushtext-services` / `lushtext-ui` integration and property tests, the `lushtext` integration and widget tests, or the benchmarks are built through their documented commands
- **THEN** they compile with the fixture helpers available and seed unregistered bodies exactly as before
- **AND** the property suite gets them because `property-tests` implies `test-utils`, and every documented benchmark command passes `--features test-utils`

#### Scenario: An ungated fixture module is caught
- **WHEN** either fixture module declaration loses its `cfg(any(test, feature = "test-utils"))` gate, or `test-utils` or a feature that implies it becomes a default feature, a shipped-binary dependency feature, or a feature enabled by a shipping build file (`build-aux/cargo.sh`, the Flatpak manifest, `snap/snapcraft.yaml`, or the Meson build and options files)
- **THEN** the policy check fails
