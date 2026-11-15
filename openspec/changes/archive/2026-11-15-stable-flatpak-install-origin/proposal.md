## Why

On 2026-11-14 every GNOME Software refresh on the maintainer's machine showed:

```
Remote listing for lushtext-origin not available; server has no summary file.
Check the URL passed to remote-add was valid.
```

It was not caused by GNOME 51. `make flatpak-install` runs `flatpak-builder
--install` with the builder's default state dir, `.flatpak-builder/` inside
whatever checkout or worktree it runs from. With `--install` and no `--repo`,
flatpak-builder exports into that state dir's `cache/` repository and runs
`flatpak install --user -y --noninteractive --reinstall <that path> <ref>`
(`builder-main.c`, `do_install`, flatpak-builder 1.4.12). flatpak turns the
path into a no-enumerate, no-gpg-verify user remote named `lushtext-origin`
(`create_origin_remote_config`, flatpak 1.18.4). On 2026-11-10 the install
ran from `.claude/worktrees/adopt-gnome-51`. That worktree was later deleted,
so the remote pointed at
`file://…/.claude/worktrees/adopt-gnome-51/.flatpak-builder/cache`, which no
longer exists. `flatpak remote-ls --user --updates`, which GNOME Software
runs, then failed on every refresh. The coordinator stopped the error by
disabling the remote (`flatpak remote-modify --user --disable
lushtext-origin`). The development build `dev.cominotti.lushtext//master`
stays installed with that disabled, dangling origin.

**Reproduced baseline** (private `FLATPAK_USER_DIR`, a dependency-free
runtime ref `org.test.lushtext` with the same `lushtext` prefix; script and
log in this change's `evidence/`, summarised in
design.md):

- Deleting the repository an origin points at makes `flatpak remote-ls --user
  --updates` exit 1 with the exact message above (case c).
- Installing from a second path creates `lushtext1-origin` and prunes the old
  remote. The remote's name and URL follow whichever checkout installed last
  (case b).
- **Silent stale install.** If the origin is disabled and its URL already
  matches, `flatpak install --reinstall` exits 0. It prints "Treating remote
  fetch error as non-fatal" and leaves the previous commit deployed (case e).
  Disabling the remote is the workaround now applied on the maintainer's
  machine. So once the install path is stable, the next `make
  flatpak-install` would report success and install nothing, unless the
  target re-enables the remote first (case f).

The parallel change `reduce-build-disk-writes` moves the builder state dir to
`~/.cache/lushtext-build/checkouts/<checkout>/flatpak-builder`, one per
checkout. Its `make build-cache-prune` deletes those directories once a
checkout is gone. Moving the state dir alone would therefore recreate this bug
on every worktree cleanup.

## What Changes

- `make flatpak-install` exports to **one shared install repository**, the
  same path from every checkout and worktree, through flatpak-builder's
  `--repo`. The repository lives outside every checkout and every build
  cache, at
  `${XDG_DATA_HOME:-$HOME/.local/share}/lushtext-flatpak-install/repo`. It
  can be overridden with `LUSHTEXT_FLATPAK_INSTALL_REPO`. The builder state
  dir stays wherever `make flatpak` keeps it: per checkout today, per checkout
  in the build cache after `reduce-build-disk-writes`.
- flatpak-builder only exports; it no longer runs `--install`. A new
  `scripts/flatpak-install-origin.sh install` wraps the export and then, in
  order:
  - repoints and re-enables an existing `lushtext-origin` to the shared
    repository, so a dangling, foreign or disabled origin is repaired by the
    next install. This includes the one on the maintainer's machine now.
  - runs the same `flatpak install --reinstall` that flatpak-builder ran.

  The repair runs only **after** a successful export. A failed build
  therefore leaves the origin as it was. It never enables the remote at a
  repository that does not serve the app yet, which would bring the reported
  error straight back (stage-2 review, `evidence/prepare-order-experiment`).
- Two runs of the target, from different worktrees, are serialized by a lock
  beside the repository.
- After installing, the target prunes the repository to the installed head.
  It then checks that the installed ref's origin is `lushtext-origin`, points
  at the shared repository, is enabled, and serves the deployed commit. Any
  other state fails the target.
- A new guard, `scripts/flatpak-install-origin.sh check`, runs inside `make
  verify-flatpak-identity`. It fails when a user `lushtext*-origin` remote
  (enabled or disabled) points at a path without an OSTree `config` and
  `summary`, and prints the one-line fix.
- A real-`flatpak` fixture test runs in a private `FLATPAK_USER_DIR`, wired as
  `make test-flatpak-install-origin` into `make check-policy`. It is
  failing-first against the baseline above. It runs under the temp-scope
  wrapper and drives the same `install` subcommand the Makefile runs.
- Docs: `.agents/rules/build.md` (Flatpak), `README.md`, the `flatpak-rust`
  skill, `docs/next/flatpak-packaging.md`. README and the packaging record
  still say GNOME 50; this change corrects them to 51 as a pre-existing doc
  drift.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `flatpak-sandbox-identity`: two ADDED requirements. The development
  install's origin stays valid regardless of which checkout installed it.
  Installed-state verification detects a broken install origin.

## Non-goals

- Changing `make flatpak` (build only), `flatpak-deps`, or where the builder
  state dir lives. The state dir belongs to `reduce-build-disk-writes`; this
  change works with either location.
- Changing the published Cominotti remote or `generate-cominotti-flatpak-repo.sh`,
  or touching CI workflows (`harden-flatpak-sdk-update-ci` owns
  `flatpak.yml`/`release.yml`).
- Removing the development `//master` install or the `//stable` install from
  `cominotti`.
- Making `make clear-lushtext-xdg` delete the install repository. The
  repository is the origin of an installed app, not app data. Deleting it is
  exactly how this bug arises.

## Impact

- User-visible: GNOME Software and `flatpak update` stop failing after a
  worktree, checkout or build-cache cleanup. The next `make flatpak-install`
  repairs a machine left in the broken or disabled state. A broken origin
  is named by `make verify-flatpak-identity`, which also prints the one-line
  fix.
- Loss compared with `main`: none. Two concurrent `make flatpak-install` runs
  now take turns instead of racing on the user installation.
- Disk: one pruned OSTree repository holding the installed commit. The
  installed `//master` measures 10.8 MB (`flatpak info --user`), and the
  archive repository is compressed.
- Files: `Makefile` (`flatpak-install`, new `test-flatpak-install-origin`,
  `check-policy`), new `scripts/flatpak-install-origin.sh` and
  `scripts/test-flatpak-install-origin.sh`, `scripts/verify-flatpak-identity.sh`,
  `scripts/test-flatpak-identity-verifier.sh`, docs above, the
  `flatpak-sandbox-identity` delta spec.
