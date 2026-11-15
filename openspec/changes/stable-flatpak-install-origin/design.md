## Context

`make flatpak-install` (Makefile, `flatpak-install`) runs
`flatpak-builder --disable-rofiles-fuse --force-clean --user --assumeyes
--install build-flatpak <manifest>` with no `--state-dir` and no `--repo`.

What the tools do, read from source rather than assumed:

- **flatpak-builder 1.4.12** (`src/builder-main.c`, the installed version):
  - Without `--repo`, the export repository is the state dir's cache,
    `builder_context_get_cache_dir` (lines 1095–1098).
  - `do_install` (lines 349–385) adds **no** remote. It runs `flatpak install
    --user -y --noninteractive --reinstall <export repo path> <ref>`.
  - `build-export` runs without `--no-update-summary`, so the export
    repository gets a `summary`.
- **flatpak 1.18.4** (the installed version):
  - `app/flatpak-builtins-install.c:396-401` turns an absolute path argument
    into its `file://` URI (`g_file_get_uri`).
  - `common/flatpak-transaction.c:2701-2724` passes that URI to
    `flatpak_dir_create_origin_remote` (id, title "Local repo").
  - `create_origin_remote_config` (`common/flatpak-dir.c:16029-16070`) names
    the remote `<last id component>-origin`, then `<prefix>1-origin`, and so
    on. It **reuses** a candidate only when `origin_remote_matches` holds:
    the URL is byte-equal (`g_strcmp0`), the remote is `xa.noenumerate`,
    `gpg-verify` agrees, and `xa.main-ref` agrees. It never looks at
    `xa.disable`.
  - On a reinstall from a different origin, the old origin is removed
    (`flatpak-dir.c:10636-10643`, `flatpak_dir_prune_origin_remote`). It is
    removed only when it is a no-enumerate `-origin` remote with no remaining
    deploys.

The maintainer's machine now (2026-11-14):

- `lushtext-origin` is `disabled,no-enumerate,no-gpg-verify`, with URL
  `file:///var/home/danilo/Workspace/github/cominotti/lushtext/.claude/worktrees/adopt-gnome-51/.flatpak-builder/cache`.
  That path no longer exists.
- It is the origin of `dev.cominotti.lushtext/x86_64/master` (installed size
  10.8 MB).
- `dev.cominotti.lushtext//stable` from `cominotti` is installed beside it.

## Measured baseline

`evidence/origin-experiment.sh` (log
beside it) runs the exact `flatpak install --user -y --noninteractive
--reinstall <path> <ref>` that flatpak-builder runs. It uses a private
`FLATPAK_USER_DIR` and a dependency-free runtime ref `org.test.lushtext`
exported with `flatpak build-export --runtime`. The ref's last id component
is `lushtext`, so the remote names are the real ones.

| Case | State before the install | Result |
| --- | --- | --- |
| a | none | `lushtext-origin` → repo A, enabled; `remote-ls --updates` OK |
| b | origin → A; install from B | `lushtext1-origin` → B created, `lushtext-origin` pruned |
| c | origin's repo B deleted | `remote-ls --updates` exits 1: "Remote listing for lushtext1-origin not available; server has no summary file. Check the URL passed to remote-add was valid." (the reported error) |
| c2 | c, then `remote-modify --disable` | `remote-ls --updates` OK, error hidden (the machine now) |
| d | c2; install from A | new `lushtext-origin` → A, dangling one pruned |
| e | origin → A **disabled**; A has a new commit; install from A | exit **0**, "Warning: Treating remote fetch error as non-fatal since … is already installed", **old commit still deployed** |
| f | e, `remote-modify --enable` first | new commit deployed |
| g | origin repointed (`remote-modify --url=B`) first; install from B | `lushtext-origin` reused, stays the name |
| h | origin → B; install from B through a symlinked spelling of the path | `lushtext1-origin` created: URL comparison is textual |
| i | 4 commits in B; `flatpak build-update-repo --prune --prune-depth=0` | 1 commit left, 12 objects freed; install from B still OK |

Cases c, d and h mean that a path chosen per checkout always produces a
remote that breaks once that checkout or its cache is deleted. Case e means
that a fixed path is not enough by itself. Once the URL matches, a disabled
origin (the state this machine is in, and the coordinator's documented
workaround) makes every later install a silent no-op that exits 0. The
target must re-enable the remote before installing, and it must check
afterwards that the deployed commit is the one it built.

**When to re-enable (stage-2 review).** Two more experiments, also in a
private `FLATPAK_USER_DIR`, start from the maintainer's state: a disabled
`lushtext-origin` whose repository was deleted.

- `evidence/prepare-order-experiment.{sh,log}`, author's order. Repointing and
  enabling the remote before the build, at a shared repository the first
  build has not created yet, brings the exact reported error back at once:
  `remote-ls --updates` exits 1. It stays back if the build then fails, which
  is common in development. The first `make flatpak-install` after this
  change would have been that case.
- Same experiment, reordered. Export first, then repoint and enable, then run
  `flatpak install --reinstall <repo> <ref>`. The remote keeps its name, the
  deployed commit equals the head, `flatpak remote-info -c` serves that
  commit, and `remote-ls --updates` exits 0.
- `evidence/empty-repo-experiment.{sh,log}`, the rejected alternative. An
  empty archive repository with a summary keeps `remote-ls --updates` at 0.
  But `flatpak update --user` then warns "No such ref … in remote
  lushtext-origin" for as long as the build runs, and indefinitely after a
  failed build.

So the invariant is: **an enabled development origin never points at a
repository that lacks the installed ref**, during a build or after a failed
one.

## Goals / Non-Goals

**Goals:** after this change, these keep the development install's origin
valid:

- installing from any checkout or worktree;
- deleting a worktree, a checkout or a build cache;
- disabling the remote by hand;
- a build that fails, at any point, including the first one after this change.

The next install repairs any remote already in a bad state. Installed-state
verification names a broken origin and prints the fix.

**Non-goals:** see proposal.md. Briefly: `make flatpak` and the state-dir
location, the published remote, CI workflows, uninstalling anything.

## Decisions

### D1. Pin the install origin with `--repo`, not with a shared `--state-dir`

`flatpak-install` passes `--repo=<shared repo>` and leaves the state dir
alone. It does **not** pass `--install`: flatpak-builder only exports, and
the install runs afterwards from `scripts/flatpak-install-origin.sh install`
(D3), after the remote has been repaired.

- `--repo` decides the export repository, and so the URL flatpak puts in the
  origin remote (lines 1095–1098 and `do_install`).
- Dropping `--install` is what lets the repair run between the export and the
  install. `--install` would run `do_install` straight after the export, with
  no point in between. The script runs the same command `do_install` runs:
  `flatpak install --user -y --noninteractive --reinstall <repo> <ref>`, with
  the untyped ref `<id>/<arch>/<branch>`. The arch comes from
  `flatpak --default-arch`, and the branch is `master`, because the manifest
  sets none.
- The state dir stays per checkout, so `make flatpak` and `make
  flatpak-install` in one checkout keep sharing their builder cache (ccache,
  Cargo, module stages).
- The design does not depend on where `reduce-build-disk-writes` puts the
  state dir. Its design D7 requires the state dir and the app dir to share a
  mount, because flatpak-builder hardlinks between them. The export
  repository is an `archive-z2` repository that `build-export` writes
  compressed copies into, so it may sit on another mount. That is the
  measured layout today too: `~/.cache` and `~/Workspace` are separate btrfs
  subvolumes (inode 256 for each).

*Rejected: one shared `--state-dir` for installs* (the original task text).
It works for the origin URL, but:

- every checkout's install would rebuild from a cold state dir that `make
  flatpak` does not share;
- concurrent installs from two worktrees would share one builder state;
- it would contradict `reduce-build-disk-writes`, which keys the state dir per
  checkout so `build-cache-prune` can reclaim it. After that change lands, a
  per-checkout state dir as the origin is exactly what prune deletes.

### D2. The repository lives in `XDG_DATA_HOME`, outside every checkout and cache

The default path is
`${XDG_DATA_HOME:-$HOME/.local/share}/lushtext-flatpak-install/repo`.
`scripts/flatpak-install-origin.sh path` computes it, and that script is the
only place the location is computed. `LUSHTEXT_FLATPAK_INSTALL_REPO`
overrides it for tests. `path` canonicalises the result with `realpath -m`.
Case h shows that two spellings of one directory create two origins, and this
host reaches the home directory as both `/home/danilo` and `/var/home/danilo`
(Silverblue's `/home` symlink). The Makefile passes the canonical path to both
`install` and flatpak-builder's `--repo`, so every run, from any shell's
`$HOME` spelling, names the same URI.

- **Not `~/.cache/lushtext/`** (the task's suggestion). `make
  clear-lushtext-xdg` deletes `$XDG_CACHE_HOME/lushtext`
  (`scripts/clear-lushtext-xdg.sh:213`, `app_data_dir_name="lushtext"`).
  Clearing app data would recreate the dangling origin.
- **Not under the build-cache root** (`~/.cache/lushtext-build`, the
  advisor's alternative):
  - the root does not exist on `main` yet;
  - its `prune` and its documented use are for reclaimable build trees;
  - any `~/.cache` location is disposable by the XDG definition, so a user
    clearing caches (a routine cleanup) would bring back the exact error this
    change removes.
  - The repository is the origin of an installed application. It is data
    flatpak relies on, like `~/.local/share/flatpak`.
- **Not `$XDG_DATA_HOME/lushtext`.** That directory is the native dev build's
  app data, which `clear-lushtext-xdg` deletes and tests must never touch. A
  sibling name, `lushtext-flatpak-install`, is owned by neither.
- **Snapshots.** `~/.cache` is its own btrfs subvolume and `~/.local/share` is
  in the home subvolume. A home snapshot therefore retains at most one pruned
  copy of the app per install (D5). The installed size is 10.8 MB, and
  archive objects are compressed. That cost is accepted in exchange for
  durability.
- **The `reduce-build-disk-writes` audit** (`build-cache.sh audit`) rejects
  `XDG_CACHE_HOME`/`~/.cache` on a line with a build-path word. This path
  uses `XDG_DATA_HOME`, so the audit neither fires nor needs an exception.

### D3. Export, then repoint and re-enable `lushtext-origin`, then install (`install`)

`flatpak-install` hands the whole sequence to one subcommand. The export
command comes after `--`, so the fixture test (D7) runs the same code with
`flatpak build-export` in place of flatpak-builder:

```
scripts/flatpak-install-origin.sh install <repo> <ref> -- <export command…>
```

Before the lock, `install` validates the repository path. It must be
absolute and canonical, and contain only `[A-Za-z0-9._~/-]`, which
`g_filename_to_uri` never escapes. Otherwise `install` refuses before
anything is built, and asks for an `LUSHTEXT_FLATPAK_INSTALL_REPO` without
other characters. This keeps the URI, `file://` + the path, byte-equal to the
one flatpak computes (case h). The default path on this host,
`/var/home/danilo/.local/share/lushtext-flatpak-install/repo`, is within the
set. `install` then creates the repository's parent directory, which also
holds the lock file. `build-export` creates the repository but not its parent
(measured: `mkdirat: No such file or directory`).

Then, under the lock (D4), in this order:

1. Run the export command. If it fails, stop with its exit status **before
   touching any remote**. The origin stays exactly as it was. On the
   maintainer's machine it is disabled, so nothing new shows in GNOME
   Software. This is the invariant from the baseline section: an enabled
   origin never points at a repository that lacks the ref.
2. `prepare`. If a user remote `lushtext-origin` exists (with
   `--show-disabled`) and its URL differs from the URI, or it is disabled,
   run `flatpak remote-modify --user --enable --url=<uri> lushtext-origin` and
   print one line naming the old URL. The repository now serves the ref, so
   the enabled remote is valid from this moment on.
3. `flatpak install --user -y --noninteractive --reinstall <repo> <ref>`, the
   command `do_install` runs (D1).
4. Prune and check (D5).

The remote name comes from the app id's last component, as
`create_origin_remote_config` derives it. Numbered leftovers
(`lushtext1-origin`, …) are left alone. Once the install deploys from
`lushtext-origin`, flatpak prunes the remote the ref came from (case b). The
post-install check (D5) fails if the name or URL still differs.

*Rejected:*

- *Repair before the build* (this design's first version). The first install
  after this change would enable the maintainer's disabled origin at a shared
  repository that does not exist yet. That brings the reported error back for
  the whole build, and for good if the build fails
  (`prepare-order-experiment`).
- *Repair before the build, at an empty repository with a summary that
  `prepare` creates.* This keeps `flatpak-builder --install`.
  `remote-ls --updates` stays at 0, but `flatpak update` warns "No such ref"
  until a build succeeds (`empty-repo-experiment`). That is a weaker version
  of the same symptom.
- *Rely on flatpak's own fallback* (case d). It repairs a dangling remote with
  a different URL, but the name drifts (`lushtext1-origin`). It also cannot
  fix case e: a disabled remote with a matching URL is reused and the install
  is silently skipped.
- *Delete the remote and let the install recreate it.* Between the delete and
  a successful build the installed ref has no origin, and a failed build
  leaves it that way.
- *Disable the origin after every install, so a vanished repository is never
  queried.* Every later install would hit case e.
- *Use `gio info` to compute the URI.* It needs the path to exist and adds a
  dependency (in this toolbox `gio` comes from linuxbrew). The conservative
  character set is simpler, and D5 catches any mismatch.

### D4. Serialize installs with a lock beside the repository

`install` takes `flock` on `<repo>.lock` itself and holds it for the whole
sequence of D3: export, repair, install, prune and check. The Makefile does not take the lock as well: a second `flock` on the
same file from a child process would wait on its own parent.

- Two worktrees installing at once would otherwise race on the shared
  repository's summary, and the prune of one could remove objects the other's
  export is writing.
- The last install to finish would win nondeterministically.
- Serializing costs a wait only when two installs actually overlap. That is
  rare, and a wait is the right outcome then. The lock covers the build,
  because the build is the export, so a second install waits for the first
  install's whole build.

The export command must not inherit the lock's descriptor. A background
process a build leaves running would otherwise hold the lock after
`make flatpak-install` returns, and the next install would wait forever. So
`install` re-executes itself under `flock --close <repo>.lock` (an internal
`__locked` subcommand) instead of holding the lock on an inherited
`exec {fd}>` descriptor.

`flock` (util-linux, `/usr/bin/flock` in the toolbox) is already a dependency
of the heavy-work scripts.

### D5. Prune, then check the post-condition, after every install

Still under the lock:

- `flatpak build-update-repo --prune --prune-depth=0 <repo>` keeps only the
  heads (the app ref and the appstream branches), so the repository stays at
  one commit (case i).
- `scripts/flatpak-install-origin.sh check --installed <repo> <ref>` then
  fails the target unless all of the following hold:
  - the installed ref (`dev.cominotti.lushtext/<arch>/master`) has origin
    `lushtext-origin` (`flatpak info --user -o`);
  - that remote is enabled and its URL is the shared repository's URI;
  - the deployed commit (`flatpak info --user -c`) equals the commit the
    remote serves (`flatpak remote-info --user -c lushtext-origin <ref>`).
    That is the commit an update would fetch, so the check tests "serves the
    deployed commit" literally, and the install path needs no `ostree` CLI
    (measured in `prepare-order-experiment`).

  The failure names the expected and the actual origin, URL and commit.

This turns cases e and h from silent states into failures.

### D6. The guard runs in `make verify-flatpak-identity` and fails for disabled dangling origins too

`scripts/flatpak-install-origin.sh check` lists user remotes with
`--show-disabled --columns=name,url,options`. For every remote named
`^<prefix>[0-9]*-origin$` with a `file://` URL, it fails when the
percent-decoded path lacks `config` or `summary`. The message names:

- the remote, its URL, and whether it is enabled;
- the symptom: GNOME Software and `flatpak update` report "server has no
  summary file" on every refresh, or, when the remote is disabled, the
  development build can no longer be updated;
- the one-line fix: `make flatpak-install` from any LushText checkout, which
  repoints the remote (D3), or `flatpak uninstall --user
  dev.cominotti.lushtext//master` to drop the development build. flatpak
  then prunes the remote itself.

A disabled dangling origin fails as well. It leaves the installed build
stranded, and it is the half-repaired state the machine is in now. Installed
state is good only when no development origin dangles. A remote whose valid
repository is not the shared one (for example an old checkout that still
exists) only gets a warning. It works today but will break when that checkout
goes.

`verify-flatpak-identity.sh` calls the check in a new "Flatpak install
origin" section. The existing stub-`flatpak` test
(`scripts/test-flatpak-identity-verifier.sh`) gains a `remotes` answer, with
no remotes in the existing cases and a dangling one in a new failing case. The
real behaviour is proved by D7.

*Rejected: match by `xa.main-ref`.* The flatpak CLI does not print it, and
reading `$FLATPAK_USER_DIR/repo/config` directly depends on flatpak's private
layout. The name is derived from the id by the same rule flatpak uses, so a
name match is exact.

### D7. A failing-first fixture test with real flatpak

`scripts/test-flatpak-install-origin.sh`, run as `make
test-flatpak-install-origin` and wired into `make check-policy`:

- **Isolation.**
  - The target runs under `$(TEMP_SCOPE)` (`scripts/with-temp-scope.sh`), as
    `testing.md` requires of every test lane. It therefore also fails on a
    `TEMP-LEAK:`.
  - Inside that scope it creates private `HOME`, `FLATPAK_USER_DIR`,
    `XDG_DATA_HOME`, `XDG_CACHE_HOME`, `XDG_CONFIG_HOME` and
    `LUSHTEXT_FLATPAK_INSTALL_REPO`. Before any `flatpak` call it asserts that
    each of them lies under its own `TMPDIR`.
  - It removes that directory from an `EXIT`/`INT`/`TERM` trap.
- **Fixture.** It uses the `org.test.lushtext` runtime fixture, with
  `LUSHTEXT_FLATPAK_APP_ID=org.test.lushtext` and the ref
  (`runtime/org.test.lushtext/<arch>/master`) as a parameter of `install` and
  `check --installed`.
- **The production path.** Every install in the test goes through
  `flatpak-install-origin.sh install <repo> <ref> -- flatpak build-export
  --runtime <repo> <dir> master`. That is the code the Makefile runs, with only
  the export command swapped. The test starts from the maintainer's state: a
  disabled `lushtext-origin` whose repository was deleted.
- **Cases.** It asserts:
  1. a dangling enabled origin: `flatpak remote-ls --user --updates` fails
     with the reported message (the baseline), and `check` fails naming the
     remote and printing `make flatpak-install`;
  2. the same remote disabled: `remote-ls` passes and `check` still fails;
  3. **failed build**: `install … -- false` exits non-zero and leaves the
     remote's URL and disabled state unchanged; `remote-ls --updates` still
     passes;
  4. **repair**: `install` with a real export repoints and enables the remote.
     It keeps the name `lushtext-origin`, the deployed commit equals the one
     the remote serves, the repository holds one commit after the prune, and
     `remote-ls --updates` and `check` pass;
  5. **case e pinned**: with the remote disabled, a bare `flatpak install
     --reinstall` from a newer export (no `install`) leaves a stale commit,
     and `check --installed` fails it, naming both commits. A following
     `install` deploys the new commit;
  6. **lock**: while the test holds `flock` on `<repo>.lock`, `timeout 2
     install … -- touch <marker>` exits 124 and the marker does not exist;
  7. **character set**: `LUSHTEXT_FLATPAK_INSTALL_REPO` containing a space
     makes `install` refuse before the export runs.
- **Skips.** It prints `SKIP:` and exits 0 when `flatpak`, `ostree` (the
  fixture's `ostree init`/`log`) or `flock` is absent.

Where it runs:

- **Locally, through `check-policy`.** The pre-commit hook runs `make
  pre-commit`, which includes `check-policy`, so it runs before every local
  commit, where `make flatpak-install` is used. The wiring lands with the
  guard (tasks group 2), never before it, because a red `check-policy` would
  block the hook.
- **Not in CI.** CI never runs `check-policy`: the `Lint` job of
  `.github/workflows/ci.yml` calls individual policy targets, and its
  `fedora:45` container installs no `flatpak`. The test therefore cannot red
  `main` for parallel changes. Should CI ever run `check-policy`, the explicit
  `SKIP:` keeps it green.
- **Time limit.** Before the wiring, the implementation measures the test's
  wall time. Above 10 s, it stays out of `check-policy` and runs from
  `test-flatpak-identity-verifier` instead. The stage-1 experiment, which
  covers nine installs, ran in 0.7 s (stage-2 rerun). Case 6 adds its 2 s
  timeout.

### D8. Coordination with `reduce-build-disk-writes`

Both changes edit the `flatpak-install` recipe. That change adds
`./scripts/build-cache.sh checkout >/dev/null` and `--state-dir` through
`FLATPAK_BUILDER_FLAGS`.

Whichever lands second merges `origin/main` and keeps both:

- the state dir follows `FLATPAK_BUILDER_FLAGS`;
- `--repo` without `--install`, and the `install` subcommand that wraps the
  export, come from this change.

The install origin stays on the single shared path in every case. The rules
`build.md` gains (D9) say why: a per-checkout location is what
`build-cache-prune` reclaims.

### D9. Documentation

- `.agents/rules/build.md` "Flatpak" and the Dev Builds bullet about
  `flatpak-install`: the shared repository, why it is shared and outside
  caches, the repair and post-check, the guard, and the one-line fix.
- `README.md` "From This Checkout": the shared repository, and runtime 50 →
  51.
- The `flatpak-rust` skill: proportional validation and the review checklist
  gain the origin guard.
- `docs/next/flatpak-packaging.md`: the install section, and runtime/SDK
  50 → 51.

README and the packaging record are corrected to 51 because the manifest
says `"runtime-version": "51"`. That drift predates this change and is fixed
under the pre-existing-blockers rule.

## Toolkit behaviour relied on

No GTK, Libadwaita, GtkSourceView or Pango behaviour is involved, so no
ledger axiom applies. The flatpak and flatpak-builder behaviour this change
relies on is pinned by D7's real-`flatpak` test instead:

- origin naming;
- reuse by textual URL;
- disabled-origin reuse with a silent skip;
- pruning of the previous origin.

That test is the regression alarm when either tool updates.

## Timing

Timing: none (no timed path touched).

## Risks / Trade-offs

- **The shared repository is deleted by hand anyway.** The guard names the
  state and the fix, and the next install recreates the repository and
  repoints the remote.
- **A flatpak release changes origin naming or matching.** D7 fails, and D5
  fails the install target instead of leaving a silent state.
- **A user sets `LUSHTEXT_FLATPAK_INSTALL_REPO` per checkout.** That brings
  back the per-checkout origin. The variable is documented as a test hook.
- **Concurrent installs wait on the lock.** This is intended (D4).

## Migration

None. The first `make flatpak-install` after this change repoints the
existing `lushtext-origin`, including the disabled dangling one on the
maintainer's machine, and leaves `.flatpak-builder/` directories alone.

## Implementation notes (stage 3)

Decisions taken while implementing, each measured in the fixture test:

- **D5, the commit comparison.** `flatpak remote-info --user -c lushtext-origin
  <ref>` exits 1 on a disabled remote ("Can't fetch summary from disabled
  remote"), so in case 5 it cannot name the commit the update should have
  deployed. `check --installed` therefore compares the deployed commit with
  both the commit the remote serves and the head of the ref in the shared
  repository. It reads the head through the flatpak CLI (`flatpak repo
  --branches`, then the first `Commit:` of `flatpak repo --commits=<typed
  ref>`, which lists the head first; case 5 runs it on a two-commit
  repository), so the install path still needs no `ostree`. Each failing
  condition is reported, not only the first.
- **D6, placement.** The "Flatpak install origin" section is the verifier's
  first section, so a broken origin is reported even when a later section
  (metadata, MIME registration) fails.
- **D7, extra coverage.** Beyond cases 1–7 the test checks:
  - `path`: the default location, and that a symlinked `XDG_DATA_HOME`
    spelling yields the same canonical path;
  - 5b, the second half of the scenario "A silently skipped install fails the
    target": a bare install through a symlinked spelling of the repository
    deploys from `lushtext1-origin` (case h), `check --installed` fails naming
    the expected and the actual origin, and the next `install` moves the ref
    back to `lushtext-origin` and leaves no numbered origin;
  - 8: a valid origin that is not the shared repository makes `check` warn
    and pass.

  The test uses `ostree` as an independent oracle for repository heads and
  commit counts, which is why it skips without `ostree`. Its ref is the
  untyped `org.test.lushtext/<arch>/master`, the form production passes.
- **Lock message.** `install` prints one line when another install holds the
  lock, before it waits.
- **Temp scope.** `make test-flatpak-identity-verifier` now runs under
  `$(TEMP_SCOPE)` as well, as `testing.md` requires of every test lane.
