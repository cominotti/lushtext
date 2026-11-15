# flatpak-sandbox-identity Specification

## Purpose
Define LushText's Flatpak desktop identity, development desktop-entry coexistence, static permission posture, and Settings-visible sandbox metadata contract.

## Requirements

### Requirement: Flatpak-Backed Desktop Identity
The packaged LushText Flatpak SHALL export the production desktop entry as a Flatpak-backed application entry for `dev.cominotti.lushtext`.

#### Scenario: Exported desktop entry identifies the Flatpak app
- **WHEN** the installed Flatpak desktop export for `dev.cominotti.lushtext.desktop` is inspected
- **THEN** it contains `X-Flatpak=dev.cominotti.lushtext`

#### Scenario: Flatpak metadata is available for the production app id
- **WHEN** verification queries Flatpak metadata for `dev.cominotti.lushtext`
- **THEN** the app is installed as a Flatpak application with command `lushtext`

### Requirement: Development Desktop Entries Do Not Shadow Production Flatpak Identity
Local development workflows SHALL NOT leave a same-ID non-Flatpak desktop entry that causes GNOME or GLib to resolve `dev.cominotti.lushtext.desktop` as a host application when the Flatpak is installed.

#### Scenario: Normal development run cleans up production-id staging
- **WHEN** the standard development run workflow exits without an explicit keep-staged request
- **THEN** `~/.local/share/applications/dev.cominotti.lushtext.desktop` does not remain as a non-Flatpak desktop entry shadowing the Flatpak export

#### Scenario: Persistent development desktop entry avoids production shadowing
- **WHEN** a development workflow intentionally keeps a desktop entry staged after the process exits
- **THEN** the kept entry uses a non-production identity or another verified mechanism that does not override the Flatpak-backed `dev.cominotti.lushtext.desktop` app info

### Requirement: GNOME Settings Resolves LushText As A Sandboxed Package
GNOME/GLib application metadata for the production LushText desktop ID SHALL resolve to the Flatpak-backed application when the Flatpak is installed.

#### Scenario: Active app-info metadata is Flatpak-backed
- **WHEN** verification resolves application info for `dev.cominotti.lushtext.desktop`
- **THEN** the resolved desktop entry exposes the Flatpak identity marker for `dev.cominotti.lushtext`

#### Scenario: Settings sandbox warning is not caused by missing Flatpak identity
- **WHEN** GNOME Settings opens the app details for the installed LushText Flatpak
- **THEN** it has the Flatpak metadata needed to avoid treating LushText as an unsandboxed host application

### Requirement: Static Flatpak Permissions Are Intentional And Documented
LushText SHALL keep only the static Flatpak permissions required for the currently shipped workspace, editor, and desktop-integration behavior, and any broad filesystem permission SHALL have a documented rationale.

#### Scenario: Permission manifest is reviewed
- **WHEN** the Flatpak manifest is checked
- **THEN** every declared socket, device, bus, and filesystem permission is either required for current behavior or removed

#### Scenario: Broad filesystem access has a rationale
- **WHEN** the Flatpak manifest includes broad `home` or `host` filesystem access
- **THEN** the packaging documentation explains the current LushText behavior that requires it and identifies the portal-first follow-up needed to narrow it safely

#### Scenario: GNOME Text Editor comparison does not broaden permissions by default
- **WHEN** LushText permissions are compared with GNOME Text Editor's current Flatpak permissions
- **THEN** LushText does not add broader `host`, GVfs, or file-manager bus permissions unless implementation verification proves they are required for LushText behavior

### Requirement: Flatpak Identity Verification Is Deterministic
The implementation SHALL provide repeatable verification for Flatpak identity, desktop-entry precedence, and static permission reporting.

#### Scenario: Verification detects a shadowing development entry
- **WHEN** a non-Flatpak `~/.local/share/applications/dev.cominotti.lushtext.desktop` would outrank the Flatpak export
- **THEN** verification fails with the shadowing path and explains that it prevents Settings from resolving the Flatpak identity

#### Scenario: Verification reports effective permissions
- **WHEN** verification runs against the installed LushText Flatpak
- **THEN** it reports the effective Flatpak permissions from `flatpak info --show-permissions dev.cominotti.lushtext`

### Requirement: The Flatpak ships the toolkit the test lanes run
The Flatpak manifest SHALL build LushText on the GNOME runtime whose toolkit the local development environment and the CI test, measurement, and lint containers run: `org.gnome.Platform` and `org.gnome.Sdk` 51 (GTK 4.24, GtkSourceView 5.22, Libadwaita 1.10). Its SDK and Rust extension SHALL follow that runtime. CI's Flatpak build and release jobs SHALL use the matching `flatpak-github-actions` image (`gnome-51`). A runtime change SHALL rebuild the Flatpak and pass `make verify-flatpak-identity` before it is accepted. It SHALL NOT change the app id, command, finish arguments, or permissions.

#### Scenario: Manifest runtime and test toolkit agree
- **WHEN** `build-aux/dev.cominotti.lushtext.Flatpak.json`, the CI workflows, and the documented development toolbox are inspected
- **THEN** the manifest names `runtime-version` 51, the Flatpak jobs use `flatpak-github-actions:gnome-51`, and every Fedora test container carries GTK 4.24, except the `GTK Axioms (GNOME 50 floor)` job, which probes the GNOME 50 floor

#### Scenario: Identity survives the runtime move
- **WHEN** the GNOME 51 Flatpak is built and installed locally
- **THEN** `make verify-flatpak-identity` passes with the same app id, command, permissions, and MIME registration as on GNOME 50

### Requirement: The development install origin stays valid
`make flatpak-install` SHALL export to one shared install repository outside every checkout and cache, the same from every checkout. Only after a successful export SHALL it repoint and enable an existing `lushtext-origin` to that repository and install from it. After installing, it SHALL fail unless the installed ref's origin is that enabled remote and serves the deployed commit.

#### Scenario: Deleting the installing worktree leaves the origin valid
- **WHEN** `make flatpak-install` runs from a worktree that is then deleted, with its build state
- **THEN** `flatpak remotes --user -d` shows `lushtext-origin` at `file://` followed by the canonical (`realpath`) form of `${XDG_DATA_HOME:-$HOME/.local/share}/lushtext-flatpak-install/repo`, and `flatpak remote-ls --user --updates` exits 0

#### Scenario: A dangling or disabled origin is repaired by the next install
- **WHEN** `lushtext-origin` points at a removed path, or is disabled (the 2026-11-14 state), and `make flatpak-install` runs
- **THEN** the remote keeps the name `lushtext-origin`, is enabled, points at the shared repository, and the installed `//master` commit equals that repository's head

#### Scenario: A failed build leaves the origin as it was
- **WHEN** `lushtext-origin` is disabled and points at a removed path, and `make flatpak-install` fails while building or exporting
- **THEN** the target exits non-zero, the remote keeps its URL and stays disabled, and `flatpak remote-ls --user --updates` still exits 0

#### Scenario: A silently skipped install fails the target
- **WHEN** the install leaves an older commit deployed, as flatpak does with exit 0 for a disabled matching origin, or deploys from another remote such as `lushtext1-origin`
- **THEN** the target exits non-zero and names the expected and the actual origin or commit

#### Scenario: Concurrent installs take turns
- **WHEN** two checkouts run `make flatpak-install` at the same time
- **THEN** they serialize on the lock beside the shared repository, and the repository is pruned to the installed head after each

### Requirement: Verification detects a broken install origin
Installed-state verification SHALL fail when a user `lushtext*-origin` remote, enabled or disabled, has a `file://` URL whose path lacks an OSTree `config` or `summary`. The failure SHALL name the remote, its URL and its enabled state, and SHALL print the one-line fix.

#### Scenario: A dangling enabled origin fails verification
- **WHEN** `lushtext-origin` points at a deleted path and `flatpak remote-ls --user --updates` fails with "server has no summary file"
- **THEN** `make verify-flatpak-identity` fails, naming the remote and URL and printing `make flatpak-install` (or `flatpak uninstall --user dev.cominotti.lushtext//master`) as the fix

#### Scenario: A disabled dangling origin still fails verification
- **WHEN** that remote is disabled, so `remote-ls` passes
- **THEN** verification still fails and says the development build can no longer be updated

#### Scenario: The guard is proved against real flatpak in isolation
- **WHEN** `make test-flatpak-install-origin` runs
- **THEN** it reproduces the failure and the repair with real `flatpak` in a private `FLATPAK_USER_DIR` under its own `TMPDIR`, never the real user installation, and prints `SKIP:` when `flatpak` or `ostree` is absent
