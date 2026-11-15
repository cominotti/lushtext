## ADDED Requirements

### Requirement: Flatpak CI builds update dependencies on a non-overlay installation
Before `flatpak-builder` runs against the system Flatpak installation of a `flatpak-github-actions` container, every CI job SHALL make sure `/var/lib/flatpak` is not on overlayfs. When it is, the job SHALL copy the installation, keeping hardlinks, to a host-backed directory outside the checkout, and bind-mount that copy over `/var/lib/flatpak`. The image's preinstalled runtimes SHALL be kept, so a dependency update downloads only what changed.

#### Scenario: A newer SDK on Flathub updates on any runner
- **WHEN** Flathub publishes a newer `org.gnome.Sdk` than the container image carries, and the Flatpak build job runs on a runner whose overlay mounts cannot rename lower-layer directories
- **THEN** the job updates the SDK and its related refs without `Invalid cross-device link` and completes the build on its first attempt

#### Scenario: The relocation survives into the build step
- **WHEN** the preparation step has bind-mounted the copy over `/var/lib/flatpak`, and a later step of the same job runs `flatpak-builder`
- **THEN** that step sees the copy, not the overlay, under `/var/lib/flatpak`

#### Scenario: An installation already off overlayfs is left in place
- **WHEN** `/var/lib/flatpak` is not on overlayfs
- **THEN** the job skips relocation and logs why

#### Scenario: The copy is not build input
- **WHEN** the installation is relocated
- **THEN** the copy lies outside the checkout that the manifest's `dir` source reads, so no file of it enters the build

### Requirement: A rename probe gates the Flatpak build
The installation preparation SHALL verify, with a directory `rename(2)` probe inside `/var/lib/flatpak`, that the installation can rename its directories. When the probe fails after preparation, the job SHALL fail before `flatpak-builder` starts.

#### Scenario: A failed relocation stops the job with a named cause
- **WHEN** the rename probe fails after relocation
- **THEN** the job fails before `flatpak-builder` starts, and its log names the probe, the result and the filesystem under `/var/lib/flatpak`

### Requirement: Relocation reports its cost without gating on it
Relocating the installation SHALL log its wall time and the free disk space before and after, and SHALL NOT fail the job because the copy was slow. Every Flatpak job SHALL keep its 30-minute ceiling. The acceptance budget for the copy is recorded in the change's design, not enforced at runtime.

#### Scenario: Relocation time is logged against the budget
- **WHEN** the installation is relocated
- **THEN** the log reports the relocation's wall time and free disk space before and after
- **AND** a slow copy does not by itself fail the job

### Requirement: Every Flatpak build log records the installation's storage facts
Before `flatpak-builder` runs, every Flatpak build job in a `flatpak-github-actions` container SHALL print one log group. It SHALL contain the kernel release, the overlay `redirect_dir` parameter, the overlay lines of `/proc/mounts`, the filesystem and free space of the installation and temporary directories, and the rename probe's results.

#### Scenario: A regression is attributable from the log alone
- **WHEN** a Flatpak build fails while updating a dependency
- **THEN** its log shows whether the installation was on overlayfs, whether it was relocated, and the rename probe results, without rerunning the job

### Requirement: One shared mechanism builds the Flatpak in CI
CI SHALL build the Flatpak through one repository composite action that runs the installation preparation and then `flatpak/flatpak-github-actions/flatpak-builder`. No workflow SHALL reference `flatpak/flatpak-github-actions/flatpak-builder` directly. A job in a `flatpak-github-actions` container SHALL use that action after checking out the repository.

#### Scenario: A direct use of flatpak-builder fails policy
- **WHEN** a workflow step uses `flatpak/flatpak-github-actions/flatpak-builder@…` outside the composite action
- **THEN** `make check-policy` fails and names the workflow and job

#### Scenario: A new Flatpak container job cannot skip the preparation
- **WHEN** a job is added that runs in a `flatpak-github-actions` container, does not use the composite action, and is not exempt
- **THEN** `make check-policy` fails and names the job

### Requirement: Policy exemptions hold only while their reason holds
The policy check SHALL exempt a job from the shared mechanism only by name and only for a reason the check verifies. Its one exemption SHALL be the Cominotti repository job, while that job's `flatpak-builder` call installs dependencies with `--user`. The check SHALL have a self-test and run in `make check-policy`.

#### Scenario: The Cominotti exemption follows its script
- **WHEN** `scripts/generate-cominotti-flatpak-repo.sh` stops passing `--user` to `flatpak-builder`
- **THEN** `make check-policy` fails and names the Cominotti repository job as no longer exempt

### Requirement: A release dry run builds the dispatched commit's Flatpak
A manual dispatch of the release workflow SHALL check out the commit it was dispatched from. With `dry_run` true and an untagged version, release metadata validation SHALL pass, using the dispatched commit wherever a tag commit would be used. The release Flatpak build SHALL then run through the shared mechanism.

#### Scenario: A branch dry run proves a release-workflow change
- **WHEN** a maintainer dispatches the release workflow from a branch with `dry_run` true and an untagged version
- **THEN** release metadata validation passes, and the release Flatpak build checks out that branch's commit and runs the shared mechanism

### Requirement: A release dry run publishes nothing
A dry-run dispatch SHALL NOT create a tag, publish a release, deploy a repository or open a Flathub pull request. A dispatch that is not a dry run SHALL be refused unless it was dispatched from `main`. A tag push SHALL keep building the tagged commit.

#### Scenario: The publish jobs stay skipped
- **WHEN** the release workflow runs with `dry_run` true
- **THEN** the Cominotti repository, deploy and publish jobs are skipped, and no tag or release exists afterwards that did not exist before

#### Scenario: A real release cannot be dispatched off main
- **WHEN** the release workflow is dispatched from a branch other than `main` with `dry_run` false
- **THEN** the metadata job fails before any build or publication, naming the dispatched ref

#### Scenario: A tag release builds the tag
- **WHEN** a `v*` tag is pushed
- **THEN** every release job checks out the tag, and the Flatpak build uses the shared mechanism
