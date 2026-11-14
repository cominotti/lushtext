## Why

The `Flatpak Build` job is flaky, and under `.agents/rules/preexisting-blockers.md` a flaky check is a blocker. PR #98's run `43642169956` failed on attempts 1 and 2 and passed on attempt 3, with the same cache key (`flatpak-builder-gnome-51-5c2d9fc0…`) and the same commit. All three attempts ran the same thing: `flatpak-builder --install-deps-from=flathub` updated `org.gnome.Sdk//51`, because Flathub has published a newer GNOME 51 SDK than the one installed in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`. That image was built on 2026-09-22 and has not been rebuilt since. On the two failing runners every ref in the update failed with `Invalid cross-device link` (EXDEV). The SDK update then failed, `flatpak-builder` segfaulted, and the action reported `xvfb-run … exit code 139`.

Evidence points to the runner, not the build. Both failing attempts ran runner image `ubuntu-26.04` `20261111.82.6`. The passing attempt ran `20261111.196.1`. That is a correlation over three attempts, not yet a cause. The working hypothesis is that `/var/lib/flatpak` lives in the container image's overlayfs lower layer. Without overlayfs `redirect_dir`, renaming a lower-layer directory returns EXDEV, and on some runners the Docker overlay mount has `redirect_dir` off. Earlier green runs logged no SDK update at all, so the defect stays hidden until Flathub moves ahead of the image. After that it hits every run on an affected runner until the image is rebuilt.

`release.yml`'s `Release Flatpak Build` job uses the same image and the same `flatpak-builder@v6` call. A tag release can therefore fail halfway after its tag is already public. That is the user-facing risk this change removes.

## What Changes

- **Confirm the cause first.** One shared preparation step prints, in every Flatpak container job, what the hypothesis rests on: `uname -r`, `/sys/module/overlay/parameters/redirect_dir`, the overlay lines of `/proc/mounts`, the filesystem under `/var/lib/flatpak`, free disk space, and the runner image version. It also runs a direct **rename probe**: it renames a lower-layer directory of the installation to a sibling and back with `rename(2)`, and records `ok` or `EXDEV`. A diagnostics-only phase runs before the fix. The aim is to land on a failing runner and confirm, or refute, that the probe's EXDEV predicts the build failure. If no run lands on such a runner, the change records that the cause could not be confirmed.
- **The fix: relocate the system installation off overlayfs.** When `/var/lib/flatpak` is on overlayfs, the step copies it with `cp -a` (which keeps ostree's hardlinks) to `$RUNNER_TEMP`, a host bind mount outside the checkout. The manifest's `dir` source is the repository root, so the copy must stay outside the checkout. The step then bind-mounts the copy over `/var/lib/flatpak` (the job's container is already `--privileged`). The rename probe must pass after relocation, or the step fails and names the cause before `flatpak-builder` runs. The image's preinstalled runtimes are kept, so updates stay deltas.
- **One shared mechanism.** A composite action, `.github/actions/flatpak-build`, runs the preparation step and then `flatpak/flatpak-github-actions/flatpak-builder@v6`. `flatpak.yml` and `release.yml`'s `Release Flatpak Build` call it. After this change, no workflow references `flatpak-builder@v6` directly.
- **A policy check keeps the jobs from diverging.** `scripts/check-flatpak-ci-builds.py --self-test` runs in `make check-policy`. It fails when a workflow uses `flatpak-builder@` outside the composite action, or when a job in a `flatpak-github-actions` container builds without it. The only exemption is `release.yml`'s `Cominotti Flatpak Repository` job, and only because the check verifies that `generate-cominotti-flatpak-repo.sh` passes `--user` to `flatpak-builder`. That job installs its dependencies into `/github/home`, a host bind mount, and v0.8.3's log shows a fresh user install, so the image's system installation is never updated. The check fails on `main` today, with two findings.
- **Release dry runs can prove a release-workflow change.** This fixes a pre-existing blocker found during design. Today no `dry_run=true` dispatch can pass `Release Metadata`. `make release DRY_RUN=1` refuses an existing tag, and the next step then runs `git rev-parse "$VERSION^{commit}"`, which needs that tag. In a dry run with no tag, that step now uses the dispatched commit. A dispatch now checks out the commit it was dispatched from, instead of always `main`, so the workflow file and the local composite action come from the same commit. A non-dry-run dispatch is refused unless it was dispatched from `main`, which keeps today's behaviour for real releases.
- **`flatpak.yml` gains `workflow_dispatch`.** Evidence runs target this one workflow instead of a whole PR's CI, and the maintainer can re-check it later on demand.
- **Docs.** The CI section of `.agents/rules/build.md`, the CI paragraph of `AGENTS.md`, the Makefile help, the programme record, and the handoff notes.

No Rust code, Cargo.lock, `cargo-sources.json`, Flatpak manifest, permission, runtime or shipped behaviour changes. No GTK axiom, Kani shard or responsiveness-lane case applies (design D9).

## Capabilities

### New Capabilities

- `flatpak-ci-build`: how CI's Flatpak container jobs prepare the Flatpak installation they build against, how one shared mechanism and a policy check keep those jobs from diverging, what every Flatpak build log must record, and how a release dry run exercises the Flatpak build of the dispatched commit without publishing.

### Modified Capabilities

None. `cominotti-flatpak-repository`'s "Dry run reports without publishing" still holds unchanged. `flatpak-sandbox-identity`'s requirement that the CI jobs use the `gnome-51` image is unchanged.

## Impact

- **Workflows:** `.github/workflows/flatpak.yml` (composite action, `workflow_dispatch`) and `.github/workflows/release.yml` (composite action in `flatpak`, dispatch checkout, dry-run source commit, refusal of non-dry dispatches off `main`).
- **New files:** `.github/actions/flatpak-build/action.yml`, `scripts/ci-flatpak-installation.sh` (`diagnose`, `relocate`, `probe`), `scripts/check-flatpak-ci-builds.py`, and the `check-flatpak-ci-builds` Makefile target, which joins `check-policy`.
- **CI cost:** relocation copies about 3–4 GB uncompressed (the image's flatpak layer is 1,253 MiB compressed). The budget is ≤ 90 s per job, measured in the first evidence runs (design D2). The job's 30-minute ceiling is unchanged.
- **Release safety:** a real release (`v*` tag push) keeps checking out the tag. A dispatch checks out the dispatched commit, and a non-dry dispatch is accepted only from `main`.
