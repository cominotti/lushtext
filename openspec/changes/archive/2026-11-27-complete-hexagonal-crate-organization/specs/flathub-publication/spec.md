## MODIFIED Requirements

### Requirement: Release Metadata Stays Synchronized
The release workflow SHALL keep all LushText package and store metadata versions synchronized before creating a release tag.

#### Scenario: Version surfaces are updated together
- **WHEN** the release helper prepares version `v1.2.3`
- **THEN** `meson.build`, `crates/lushtext/Cargo.toml`, `crates/lushtext-ui/Cargo.toml`, `crates/lushtext-services/Cargo.toml`, `crates/lushtext-model/Cargo.toml`, `Cargo.lock`, and `data/dev.cominotti.lushtext.metainfo.xml.in` reflect release version `1.2.3`

#### Scenario: AppStream release notes are required
- **WHEN** the release helper prepares a real release
- **THEN** it refuses to create a release commit or tag unless release notes are provided through a deterministic release-notes input
- **AND** the AppStream release entry includes the release version, current release date, and release description

#### Scenario: Version mismatch blocks tagging
- **WHEN** any package, Meson, Cargo lock, or AppStream version surface does not match the requested release version after preparation
- **THEN** the release helper fails before creating a commit or tag
