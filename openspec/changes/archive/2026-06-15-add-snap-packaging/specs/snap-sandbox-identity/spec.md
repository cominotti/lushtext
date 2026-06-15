## ADDED Requirements

### Requirement: Snap Desktop Identity Preserves The AppStream ID
The Snap SHALL keep the application's AppStream/desktop identity as
`dev.cominotti.lushtext` via the app `common-id`, while using a registered Snap
Store name in Snap's flat namespace, so the snapped desktop entry links to the
existing metainfo.

#### Scenario: App common-id matches the AppStream component
- **WHEN** `snapcraft.yaml` is inspected
- **THEN** the LushText app entry sets `common-id: dev.cominotti.lushtext` and
  references the project desktop file

#### Scenario: Registered snap name is distinct from the AppStream ID
- **WHEN** the Snap is registered in the store
- **THEN** the registered snap name (e.g. `lushtext`) is recorded, and the
  AppStream ID `dev.cominotti.lushtext` remains the desktop/metainfo identity

### Requirement: Strict Confinement With Portal-First Permissions
The Snap SHALL use strict confinement and SHALL NOT use classic confinement.
Host file access SHALL be provided through xdg desktop portals plus the `home`
and `removable-media` interfaces rather than broad unconfined filesystem access.

#### Scenario: Confinement is strict
- **WHEN** the built Snap's metadata is inspected
- **THEN** its confinement is `strict`

#### Scenario: Every declared plug has a rationale
- **WHEN** the Snap's declared plugs are reviewed
- **THEN** each plug beyond those auto-provided by the `gnome` extension (such as
  `home` and `removable-media`) is either required for current LushText behavior
  or removed, with the rationale documented

#### Scenario: No broad unconfined filesystem access by default
- **WHEN** the Snap permission posture is compared with the Flatpak's
  `--filesystem=host`
- **THEN** the Snap does not request classic confinement or an equivalent broad
  host-filesystem escape hatch as its default posture

### Requirement: Confinement Boundary For Workspace Roots Is Defined
The Snap SHALL define and document how workspace roots and files outside the
confined-accessible locations behave, because strict confinement is narrower than
the Flatpak's host access.

#### Scenario: Workspace root inside HOME works
- **WHEN** a user adds a workspace root under their home directory in the confined
  Snap
- **THEN** the file tree and editing operate normally through the `home` interface

#### Scenario: Out-of-scope path is handled gracefully
- **WHEN** the confined Snap is asked to open a path outside the
  confined-accessible locations (for example via a CLI argument or a restored
  workspace root under `/opt`)
- **THEN** the app surfaces an access error or routes through a portal rather than
  crashing or silently losing data

### Requirement: Unlisted Edge-Only Release Contract
The Snap SHALL be released with Unlisted store visibility and published only to
the `edge` channel, so it is omitted from store search and the default
`snap install lushtext` does not succeed without an explicit channel.

#### Scenario: Snap is omitted from search
- **WHEN** the released Snap's store visibility is checked
- **THEN** it is Unlisted (absent from `snap find` and store browse results)

#### Scenario: Installation requires the explicit edge channel
- **WHEN** a user runs `snap install lushtext` with no channel and revisions
  exist only on `edge`
- **THEN** the default install does not succeed, and installation requires
  `snap install lushtext --edge`

### Requirement: Snap Identity And Permissions Verification Is Deterministic
The implementation SHALL provide repeatable verification of the Snap's
confinement, declared plugs, and identity linkage, analogous to the Flatpak
identity verification.

#### Scenario: Verification reports confinement and plugs
- **WHEN** verification runs against the built or installed Snap
- **THEN** it reports the confinement type and the effective list of plugs and
  their connection state

#### Scenario: Verification asserts the common-id linkage
- **WHEN** verification inspects the Snap's app metadata
- **THEN** it confirms `common-id` is `dev.cominotti.lushtext` and the desktop
  entry is present
