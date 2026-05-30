## ADDED Requirements

### Requirement: Flathub Handoff Is Secondary To Cominotti Publication
The release workflow SHALL treat the Cominotti Flatpak repository as the primary Flatpak publication path, while keeping Flathub manifest generation or pull-request creation optional when explicitly configured.

#### Scenario: Cominotti publication does not require Flathub credentials
- **WHEN** a release completes Cominotti Flatpak repository publication
- **AND** `FLATHUB_TOKEN` or `FLATHUB_REPOSITORY` is not configured
- **THEN** the workflow reports the Flathub handoff as skipped
- **AND** it still treats the Cominotti Flatpak publication result as the release's primary Flatpak publication status

#### Scenario: Flathub handoff remains optional when configured
- **WHEN** a release completes Cominotti Flatpak repository publication
- **AND** Flathub credentials are configured
- **THEN** the workflow may generate or update the Flathub manifest pull request
- **AND** a Flathub handoff failure is reported separately from the Cominotti repository publication result

#### Scenario: Documentation names the primary Flatpak channel
- **WHEN** maintainers read the Flatpak packaging or release documentation
- **THEN** the Cominotti remote is documented as the primary Flatpak publication channel
- **AND** Flathub verification and pull-request steps are documented only as optional or secondary paths
