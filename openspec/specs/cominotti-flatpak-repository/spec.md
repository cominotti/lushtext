# cominotti-flatpak-repository Specification

## Purpose
Define the official Cominotti Flatpak remote, static hosting layout, signing expectations, installer metadata, release automation, and verification gates for first-party LushText Flatpak publication.

## Requirements

### Requirement: Official Cominotti Flatpak Remote
The project SHALL define an official publisher-level Flatpak remote for Cominotti applications, with LushText published as the first application in that remote.

#### Scenario: Remote metadata identifies the publisher
- **WHEN** the Cominotti Flatpak repository metadata is generated
- **THEN** the suggested remote name is `cominotti`
- **AND** the collection ID is `dev.cominotti.Apps`
- **AND** the repository title and description identify the remote as the official Cominotti application repository

#### Scenario: LushText is the first app ref
- **WHEN** the Cominotti Flatpak repository is generated for the first supported release
- **THEN** it contains an installable application ref for `dev.cominotti.lushtext`
- **AND** the repository structure does not require a separate remote per application

### Requirement: First-Party Static Hosting Layout
The project SHALL publish the Cominotti Flatpak repository through stable HTTPS URLs under `flatpak.cominotti.dev` without requiring a specific static-hosting provider.

#### Scenario: Public repository URLs are stable
- **WHEN** publication artifacts are staged for deployment
- **THEN** the repository content is staged for `https://flatpak.cominotti.dev/repo/`
- **AND** the repository descriptor is staged as `https://flatpak.cominotti.dev/cominotti.flatpakrepo`
- **AND** the LushText installer reference is staged as `https://flatpak.cominotti.dev/lushtext.flatpakref`

#### Scenario: Hosting backend is provider-neutral
- **WHEN** maintainers deploy the staged Flatpak repository artifacts
- **THEN** the deployment contract requires only HTTPS static-file hosting for the published URL layout
- **AND** the implementation does not require GitHub Pages, GitLab Pages, or another specific hosting product

### Requirement: Signed Repository Publication
The project SHALL publish the Cominotti Flatpak repository with GPG verification enabled for public users.

#### Scenario: Repository summary and commits are signed
- **WHEN** a release is exported to the Cominotti Flatpak repository
- **THEN** the exported application commit is signed with the configured Cominotti Flatpak signing key
- **AND** the repository summary is signed with the configured Cominotti Flatpak signing key
- **AND** the public GPG key is available to clients through generated install metadata

#### Scenario: Public install instructions keep verification enabled
- **WHEN** public installation documentation is generated or updated
- **THEN** it does not instruct users to install the Cominotti remote with `--no-gpg-verify`
- **AND** it explains that unsigned or no-verification remotes are only acceptable for local testing

### Requirement: Update-Friendly Repository Metadata
The project SHALL generate repository metadata suitable for normal Flatpak installs and updates.

#### Scenario: Static deltas are generated
- **WHEN** the Cominotti Flatpak repository summary is updated for a release
- **THEN** static deltas are generated for published application refs
- **AND** old unreferenced objects may be pruned according to the documented retention policy

#### Scenario: Flatpak update can discover new releases
- **WHEN** a user has installed LushText from the `cominotti` remote
- **AND** a newer LushText release is published to the same remote and branch
- **THEN** `flatpak update` can discover and install the newer release without adding a new remote

### Requirement: LushText Flatpakref Installer
The project SHALL provide a LushText-specific `.flatpakref` that installs from the shared Cominotti remote.

#### Scenario: Flatpakref installs LushText from Cominotti
- **WHEN** a user installs `https://flatpak.cominotti.dev/lushtext.flatpakref`
- **THEN** Flatpak is given the app ID `dev.cominotti.lushtext`
- **AND** Flatpak is given the suggested remote name `cominotti`
- **AND** Flatpak is given the Cominotti repository URL and public GPG key

#### Scenario: Flatpakref declares the runtime source
- **WHEN** the LushText `.flatpakref` is generated
- **THEN** it declares `RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo`
- **AND** it does not imply that Cominotti publishes GNOME runtimes or SDKs

### Requirement: Release Automation Publishes Cominotti Artifacts
The release workflow SHALL prepare Cominotti Flatpak repository artifacts from tagged LushText releases.

#### Scenario: Release publication uses immutable source
- **WHEN** a release tag workflow prepares Cominotti Flatpak publication artifacts
- **THEN** the Flatpak build uses the tagged LushText source and matching commit
- **AND** the generated artifacts preserve the reviewed LushText Flatpak packaging contract

#### Scenario: Dry run reports without publishing
- **WHEN** a maintainer runs the release workflow in dry-run mode
- **THEN** it reports the intended Cominotti repository output, signing requirements, and deploy target
- **AND** it does not publish, deploy, create tags, or mutate the public repository

#### Scenario: Missing Cominotti deploy configuration is honest
- **WHEN** release validation succeeds but Cominotti deploy credentials or deploy targets are not configured
- **THEN** the workflow reports that Cominotti Flatpak deployment was skipped or failed due to missing configuration
- **AND** it does not claim that Cominotti Flatpak publication is complete

### Requirement: Cominotti Repository Verification
The project SHALL provide repeatable checks for generated Cominotti repository artifacts before maintainers publish them.

#### Scenario: Metadata verifier checks install descriptors
- **WHEN** maintainers verify generated Cominotti Flatpak artifacts
- **THEN** the verifier checks the `.flatpakrepo` URL, title, collection ID, and GPG key
- **AND** it checks the LushText `.flatpakref` app ID, suggested remote name, repository URL, runtime repo, and GPG key

#### Scenario: Repository verifier checks app availability
- **WHEN** maintainers verify a generated or staged Cominotti Flatpak repository
- **THEN** the verifier confirms that `dev.cominotti.lushtext` is available from the repository
- **AND** it confirms the app's effective Flatpak permissions and desktop identity remain consistent with the existing LushText Flatpak contract
