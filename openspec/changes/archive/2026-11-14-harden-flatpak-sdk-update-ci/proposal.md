## Why

The `Flatpak Build` job is flaky, and under `.agents/rules/preexisting-blockers.md` a flaky check is a blocker.

**What happened.** PR #98's run `43642169956` failed on attempts 1 and 2 and passed on attempt 3. All three attempts used the same commit and the same cache key (`flatpak-builder-gnome-51-5c2d9fc0…`). In each one, `flatpak-builder --install-deps-from=flathub` updated `org.gnome.Sdk//51`. Flathub has published a newer GNOME 51 SDK than the one installed in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, an image built on 2026-09-22 and not rebuilt since. On the two failing runners every ref in the update failed with `Invalid cross-device link` (EXDEV). The SDK update then failed, `flatpak-builder` segfaulted, and the action reported `xvfb-run … exit code 139`.

**Reproduced locally.** The failure reproduces locally with the same image (`evidence/local-reproduction.md`). The image's `/var/lib/flatpak` lives in an overlayfs lower layer. With overlay `redirect_dir` off, the default on the developer host, `rename(2)` of a deployed runtime directory returns EXDEV. `flatpak update --system org.freedesktop.Platform.codecs-extra` then fails with the exact CI message. After copying the installation (`cp -a`, 3.8 GiB, hardlinks kept) to a non-overlay path and bind-mounting it back, the probe returns `ok`, the same update succeeds, and the mount survives into a later `docker exec`.

**What is still unknown.** The failing attempts ran runner image `ubuntu-26.04` `20261111.82.6`; the passing one ran `20261111.196.1`. Why that one runner succeeded is not known: its overlay may have `redirect_dir` on, or it may use another snapshotter. The fix does not depend on the answer.

**Why it stayed hidden.** Earlier green runs logged no SDK update. The defect only shows once Flathub moves ahead of the image, and from then on it hits every run on an affected runner until the image is rebuilt.

`release.yml`'s `Release Flatpak Build` job uses the same image and the same `flatpak-builder@v6` call. A tag release can therefore fail halfway through, after its tag is already public. That is the user-facing risk this change removes.

## What Changes

- **The fix: relocate the system installation off overlayfs.** When `/var/lib/flatpak` is on overlayfs, a preparation step copies it with `cp -a` (which keeps ostree's hardlinks) to `$RUNNER_TEMP`. That is a host bind mount outside the checkout. The manifest's `dir` source is the repository root, so the copy has to stay out of it. The step then bind-mounts the copy over `/var/lib/flatpak` with `mount(2)`. The job's container is already `--privileged`, and the image ships no `mount` binary. A rename probe must pass after relocation, or the step fails and names the cause before `flatpak-builder` runs. The image's preinstalled runtimes are kept, so updates stay deltas.
- **Diagnostics in every Flatpak build log.** The same step prints one log group with:
  - `uname -r` and `/sys/module/overlay/parameters/redirect_dir`;
  - the overlay lines of `/proc/mounts`;
  - the filesystem and free space of `/var/lib/flatpak`, `$RUNNER_TEMP` and `/tmp`;
  - the runner image version;
  - the rename probe's results before and after relocation.

  That makes a future regression attributable from the log alone.
- **One shared mechanism.** A composite action, `.github/actions/flatpak-build`, runs `scripts/ci-flatpak-installation.py` (`diagnose`, `relocate`) and then `flatpak/flatpak-github-actions/flatpak-builder@v6`. `flatpak.yml` and `release.yml`'s `Release Flatpak Build` call it. After this change, no workflow references `flatpak-builder@v6` directly.
- **A policy check keeps the jobs from diverging.** `scripts/check-flatpak-ci-builds.py --self-test` runs in `make check-policy`. It fails when a workflow uses `flatpak-builder@` outside the composite action, or when a job in a `flatpak-github-actions` container builds without it. The only exemption is `release.yml`'s `Cominotti Flatpak Repository` job, and only while the check verifies that `generate-cominotti-flatpak-repo.sh` passes `--user` to `flatpak-builder`. That job installs its dependencies into `/github/home`, a host bind mount. v0.8.3's log shows a fresh user install, so the job never updates the image's system installation. The check fails on `main` today with two findings.
- **Release dry runs can prove a release-workflow change.** This fixes a pre-existing blocker found during design, and it was checked locally in review. Today no `dry_run=true` dispatch can pass `Release Metadata`. `make release DRY_RUN=1` refuses an existing tag. For an untagged version, the next step runs `git rev-parse "$VERSION^{commit}"`, which needs the tag.
  - In a dry run with no tag, that step now uses the dispatched commit.
  - A dispatch now checks out the commit it was dispatched from, instead of always `main`.
  - A non-dry-run dispatch is refused unless it was dispatched from `main`.
- **`flatpak.yml` gains `workflow_dispatch`,** so that one workflow can be re-checked on demand.
- **Evidence.** The merge rests on local evidence: the real helper, in the real image, run failing-first as shipped and then passing with relocation. This follows the maintainer's instruction for the programme: merge on local results, check CI once at the end. The CI confirmation (a green `flatpak.yml` run on `main` with the diagnostics, and one release dry-run dispatch) is owed by the programme's final CI sweep (design D8).
- **Docs.** The CI section of `.agents/rules/build.md`, the CI paragraph of `AGENTS.md`, the Makefile help, `docs/next/flatpak-packaging.md`, the programme record, the release and Flatpak skills, and the handoff notes.

No Rust code, Cargo.lock, `cargo-sources.json`, Flatpak manifest, permission, runtime or shipped behaviour changes. No GTK axiom, Kani shard or responsiveness-lane case applies (design D9).

## Non-goals

- Changing the CI image, its tag, the GNOME runtime, the manifest or the `flatpak-builder@v6` version.
- Changing the Cominotti repository job's `--user` install, or making that publishing job runnable in a dry run.
- Fixing `flatpak-builder`'s segfault after a failed dependency update. That is upstream (design D10).
- Explaining why runner image `20261111.196.1` did not hit the failure. The diagnostics record it, and the fix does not depend on it.

## User-visible impact

Users see no change. The app, the Flatpak bundle and the published repository are built from the same inputs as before.

For maintainers:

- Release tags can no longer fail halfway on this cause.
- PR Flatpak checks stop flaking on it.
- Each Flatpak job spends up to about 90 s, and 3.8 GiB of runner disk, on the copy.
- A non-dry release dispatch from a branch other than `main`, which used to build `main`, is now refused. Nothing is lost compared with `main`: that dispatch had no known use, and a dry run from a branch now works where it never could.

## Capabilities

### New Capabilities

- `flatpak-ci-build`. Purpose: CI's Flatpak container jobs build against a Flatpak installation that can update its dependencies on any runner, through one shared, policy-checked mechanism whose log records the installation's storage facts, and a release dry run builds the dispatched commit's Flatpak without publishing.

### Modified Capabilities

None. `cominotti-flatpak-repository`'s "Dry run reports without publishing" still holds unchanged. `flatpak-sandbox-identity`'s requirement that the CI jobs use the `gnome-51` image is unchanged.

## Impact

- **Workflows:** `.github/workflows/flatpak.yml` (composite action, `workflow_dispatch`). `.github/workflows/release.yml` (composite action in `flatpak`, dispatch checkout, dry-run source commit, refusal of non-dry dispatches off `main`).
- **New files:** `.github/actions/flatpak-build/action.yml`, `scripts/ci-flatpak-installation.py` (`diagnose`, `relocate`, `probe`, `--self-test`), `scripts/check-flatpak-ci-builds.py`, and the `check-flatpak-ci-builds` Makefile target, which joins `check-policy`.
- **CI cost:** relocation copies 3.8 GiB (6.8 s locally, non-verdict). The budget is 90 s or less per job, and it is confirmed at the programme's final CI sweep (design D2). The job's 30-minute ceiling is unchanged.
- **Release safety:** a real release (`v*` tag push) keeps checking out the tag. A dispatch checks out the dispatched commit, and a non-dry dispatch is accepted only from `main`.
