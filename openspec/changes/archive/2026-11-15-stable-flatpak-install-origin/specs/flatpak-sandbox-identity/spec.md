## ADDED Requirements

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
