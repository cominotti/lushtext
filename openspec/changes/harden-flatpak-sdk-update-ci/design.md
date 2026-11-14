## Context

CI builds the Flatpak in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, with the container `--privileged`, through `flatpak/flatpak-github-actions/flatpak-builder@v6`. The action always runs
`xvfb-run flatpak-builder --repo=repo --disable-rofiles-fuse --install-deps-from=flathub … --ccache --state-dir .flatpak-builder flatpak_app <manifest>`.
It runs as root without `--user`, so it installs into the **system** installation `/var/lib/flatpak`, which the image pre-populated with `flatpak install -y flathub org.gnome.Platform//51 org.gnome.Sdk//51` (the image's last layer, 1,253 MiB compressed; image created 2026-09-22T13:43Z).

### Measured baseline

Run `43642169956` (PR #98, `flatpak.yml`, same commit and cache key on every attempt):

| Attempt | Job | Runner image (ubuntu-26.04) | `Updating org.gnome.Sdk/x86_64/51` | Result |
|---|---|---|---|---|
| 1 | 132591925079 | `20261111.82.6` | every ref: `Invalid cross-device link`; `Error: Failed to update org.gnome.Sdk` | `flatpak-builder` segfault, `xvfb-run` exit 139 |
| 2 | 132597336544 | `20261111.82.6` | same | same |
| 3 | 132600497134 | `20261111.196.1` | SDK, Locale, GL.default, codecs-extra, then Platform, updated | success, job 6 min 24 s |

Failing refs, in order: `org.freedesktop.Platform.GL.default` (26.08 and 26.08-extra), `org.freedesktop.Platform.codecs-extra`, `org.gnome.Sdk.Locale`, `org.gnome.Sdk`. Each failed within 0.3–26 s of starting, after its download. The runner image version is the only difference visible in the logs. With n = 3 it is a correlation, not a cause. Earlier green runs (`b3f7a273`, `b129f378`) logged no update, because the image and Flathub agreed at the time.

`release.yml` v0.8.3 (2026-09-22, runner ubuntu-24.04):

- `Release Flatpak Build` (job `106956706909`): `Updating org.gnome.Sdk/x86_64/50` → `Nothing to update.`, so the same exposure existed and was not triggered.
- `Cominotti Flatpak Repository` (job `106956706850`): `Installing org.gnome.Sdk/x86_64/50 from flathub` and the Platform, a fresh **user** install (`generate-cominotti-flatpak-repo.sh` passes `--user`, `HOME=/github/home` is a host bind mount) in 38 s (22:04:52.8 → 22:05:31.1).

`flatpak-builder@v6` inputs (read from its `action.yml` at `v6`): `manifest-path`, `stop-at-module`, `bundle`, `build-bundle`, `build-debug-bundle`, `repository-url`, `repository-name`, `run-tests`, `cache`, `restore-cache`, `save-cache`, `cache-key`, `keep-build-dirs`, `branch`, `arch`, `mirror-screenshots-url`, `gpg-sign`, `build-dir`, `repo-dir`, `state-dir`, `verbose`, `upload-artifact`, `artifact-name`. There is no `--user` input and no extra-arguments input.

`build-aux/dev.cominotti.lushtext.Flatpak.json` uses `{"type": "dir", "path": ".."}`, which is the whole checkout. Anything copied into the checkout becomes build input.

### Hypothesis (to confirm in the first evidence runs)

Overlayfs returns EXDEV from `rename(2)` of a directory that exists in a lower layer unless `redirect_dir` is on. Flatpak's update deploys the new commit and moves the old deployment directory aside, and that is a rename of a lower-layer directory. On runners whose Docker overlay mounts have `redirect_dir` off, that returns EXDEV. On runners where it is on, the update works. This explains why a lagging image fails on some runners and not others.

### A pre-existing blocker found while designing the proof

As written on `main` today, `release.yml`'s manual dispatch (`dry_run` defaults to `true`) cannot pass `Release Metadata` with any `version` input. `make release VERSION=… DRY_RUN=1` dies with `tag '…' already exists` for a released version (`run_release`, `scripts/release.sh`). For an unreleased version, `Generate Flathub update artifact` dies on `git rev-parse "$VERSION^{commit}"`. Because `Release Flatpak Build` needs `metadata`, a dry run cannot reach the Flatpak build. The only recorded dispatches (2026-06-01, an earlier revision of the workflow) ran the publish jobs, so neither was a dry run. `generate-flathub-manifest.sh` and `verify-flathub-manifest.sh` accept any version and any 7–40 hex commit, and `release-dry-run.yml` already calls the generator with `v0.0.1 "$GITHUB_SHA"`, so using the dispatched commit is enough to pass the step (D7). A dispatch also checks out `main` whatever ref it was dispatched from. That means a branch dispatch would run the branch's workflow against `main`'s local composite action, which does not exist before merge.

## Goals / Non-Goals

**Goals:**

- Every Flatpak build in CI can update dependencies newer than the image's on any runner, without retries.
- One mechanism for every job that builds against the image's system installation, enforced statically.
- Evidence in the job log on every run, so a future regression is attributable from the log alone.
- A release dry-run dispatch that runs the release Flatpak build of the dispatched commit and publishes nothing.

**Non-Goals:**

- Changing the image, its tag, the GNOME runtime, the manifest, `flatpak-builder@v6`'s version, or the Cominotti job's `--user` install.
- Making the Cominotti job, which publishes, runnable in a dry run (it stays gated on `dry-run != 'true'`).
- Fixing `flatpak-builder`'s segfault after a failed dependency update. That is upstream, and the deferral is recorded in D10.
- Rust, Cargo.lock, `cargo-sources.json`, Kani, GTK axioms and smoke lanes.

## Decisions

### D1. Relocate the system installation off overlayfs (copy + bind mount)

When `findmnt -no FSTYPE -T /var/lib/flatpak` reports `overlay`, `scripts/ci-flatpak-installation.sh relocate` does the following:

1. `cp -a /var/lib/flatpak "$RUNNER_TEMP/flatpak-system"`. `-a` keeps the hardlinks between the ostree repository and the deployments within the single copy, so the copy costs what `du` reports, not double.
2. `mount --bind "$RUNNER_TEMP/flatpak-system" /var/lib/flatpak`.
3. Re-runs the rename probe (D3) and fails the step when it does not return `ok`.

When the installation is not on overlayfs, for example after a future runner or image fix, it skips relocation and logs why.

`$RUNNER_TEMP` (`/__w/_temp`) is a host bind mount from `/home/runner/work/_temp`, so it is not overlay. It also lies outside the checkout (`/__w/lushtext/lushtext`), so the manifest's `dir` source does not pull the copy into the build.

Alternatives considered:

- **`--user` installation:** `flatpak-builder@v6` has no input for it (see the input list above). Wrapping `flatpak-builder` ourselves would mean re-implementing the action's cache, bundle and artifact handling.
- **`FLATPAK_SYSTEM_DIR` exported through `$GITHUB_ENV`:** libflatpak honours it, but only processes that inherit the variable see the relocated installation. A bind mount is transparent to everything that names `/var/lib/flatpak`. This is kept as the fallback if `mount --bind` turns out to be unavailable in the image (D5).
- **A Docker named volume over `/var/lib/flatpak` in `container.options`/`volumes`:** it has to be declared in every job's YAML, so it cannot be the one shared mechanism. Docker's copy-up of image content into a new volume under the containerd image store is also unverified on these runners.
- **An empty installation on a non-overlay path plus `flatpak remote-add flathub`:** every run would download the full SDK and Platform (38 s in v0.8.3's user install). It drops the image's preinstalled content and adds a Flathub download dependency on every run. It stays the fallback if the copy exceeds its budget (D2).
- **`flatpak mask` / pinning the image's SDK:** this hides the newer SDK and builds release artifacts against a stale one.
- **Our own image, or waiting for upstream to rebuild:** an image we own means maintenance. Waiting only defers the next lag.

### D2. Relocation budget

Relocation (copy + mount + probe) must cost at most **90 s per job**. The figure is logged on every run and judged in the evidence runs (D8). It is deliberately **not** a runtime gate: failing a job because one runner copies slowly would trade one flake for another. It must leave enough free disk for the build: the step prints `df -h` before and after, and the build still completes. The current job takes 6 min 24 s against its 30-minute ceiling, and `scripts/check-workflow-timeouts.py` keeps `timeout-minutes: 30`. If the first evidence runs measure more than 90 s, implementation switches to D1's empty-installation fallback and records both figures here. Budget source: the fallback's measured 38 s plus a margin for copy variance. A larger cost would make every PR's Flatpak check visibly slower for no benefit over the fallback.

### D3. Diagnostics and the rename probe in every Flatpak build log

`scripts/ci-flatpak-installation.sh diagnose` prints, under one `::group::`:

- `uname -r` and `/sys/module/overlay/parameters/redirect_dir`;
- `grep overlay /proc/mounts`;
- `findmnt -T` for `/var/lib/flatpak`, `$RUNNER_TEMP` and `/tmp`;
- `df -h` for the same paths;
- `ImageVersion` from the runner, if it is visible inside the container (otherwise the log's own "Runner Image" header, which the evidence table copies);
- the probe result.

The probe calls `rename(2)` through `python3` (or `perl` if `python3` is absent; implementation verifies which interpreter exists). It renames an existing lower-layer directory of the installation to a sibling name and back, and prints `rename-probe: ok` or `rename-probe: EXDEV`. GNU `mv` cannot serve as the probe, because it falls back to copy-and-delete on EXDEV. The probe runs before and after relocation. Before relocation it is diagnostic only. After relocation a non-`ok` result fails the step with a message that names the installation's filesystem. This turns the old failure mode (a segfault after a dependency update) into a direct, named error before `flatpak-builder` starts.

### D4. One composite action

`.github/actions/flatpak-build/action.yml` (composite) takes `manifest-path`, `bundle` and `cache-key`. It runs `diagnose`, then `relocate`, then `flatpak/flatpak-github-actions/flatpak-builder@v6` with those inputs. It is the only file that names `flatpak-builder@`. `flatpak.yml` and `release.yml`'s `flatpak` job call `uses: ./.github/actions/flatpak-build` after `actions/checkout`, which a local action needs. The shell logic lives in `scripts/ci-flatpak-installation.sh`, so `shellcheck` and the policy check can read it, and the action stays a thin wiring file.

### D5. Policy check against divergence (the failing-first test)

`scripts/check-flatpak-ci-builds.py` follows `check-workflow-timeouts.py`: a simple line-oriented parser, `--self-test` over synthetic workflows, and a Makefile target `check-flatpak-ci-builds` in `check-policy`. It fails when:

1. a file under `.github/workflows/` names `flatpak/flatpak-github-actions/flatpak-builder@`;
2. a job whose `container` image contains `flatpak-github-actions` has no `uses: ./.github/actions/flatpak-build` step, unless it appears in the check's exemption table;
3. that step comes before the job's `actions/checkout` step;
4. an exempt job's stated reason no longer holds. The single exemption, `release.yml` `cominotti-flatpak`, holds only while `scripts/generate-cominotti-flatpak-repo.sh`'s `flatpak-builder` arguments include `--user`.

On `main` today it reports two findings, `flatpak.yml` `flatpak` and `release.yml` `flatpak`. That failing run is recorded before the workflows change.

### D6. Release dispatch builds the dispatched commit

In `release.yml`'s `Determine version`, a `workflow_dispatch` sets `checkout_ref` to `$GITHUB_SHA`, the dispatched commit, instead of `main`. A dispatch with `dry_run` other than `true` fails unless `GITHUB_REF` is `refs/heads/main`. That leaves real (non-dry) dispatches exactly as before: they build `main`. It also means a dry run from a branch is the way to prove a release-workflow change before merge. Tag pushes are unchanged and check out the tag.

### D7. Dry-run source commit

In `Generate Flathub update artifact`, when `DRY_RUN` is `true` and `refs/tags/$VERSION` does not exist, `source_commit` is `HEAD`, the dispatched commit. Otherwise it stays the tag's commit. `make release DRY_RUN=1` already refuses an existing tag, so every dry run takes the `HEAD` branch. A real release still requires the tag, as before. The `cominotti-flatpak` and `publish-context` jobs, which also resolve the tag, stay skipped in a dry run.

### D8. Evidence protocol (the CI exception for this change)

This change's evidence is CI logs, so the maintainer's no-CI rule is explicitly lifted for it alone. It may push its branch, dispatch and rerun `flatpak.yml`, and dispatch `release.yml` with `dry_run=true`. It never pushes a tag and never runs `dry_run=false`. `flatpak.yml` gains `workflow_dispatch`. If GitHub refuses a branch dispatch because `main`'s copy lacks the trigger, a draft PR provides the runs instead.

- **Runs are sequential.** `flatpak.yml`'s concurrency group is keyed on `github.ref` with `cancel-in-progress: true`, so a second dispatch on the branch cancels the first. Each dispatch waits for the previous run to complete. Alternatively, rerun one completed run (`gh run rerun <id>`), which also draws a new runner.
- **Phase A (diagnostics only, relocation off by an action input):** up to 8 dispatches. The table in `evidence/runs.md` records, per run: run/job id, runner image version, kernel, `redirect_dir`, filesystem under `/var/lib/flatpak`, pre-relocation probe, whether `Updating … org.gnome.Sdk` updated or found nothing, and the result. The cause is **confirmed** when a run on which the probe returned EXDEV fails the update with EXDEV, and a run on which the probe returned `ok` updates. It is **refuted** if a probe-EXDEV run updates, or a probe-`ok` run fails with EXDEV. If no run lands on a probe-EXDEV runner, it is recorded as "not confirmed".
- **Phase B (relocation on):** at least 4 dispatches, plus more up to 8 if none lands on a probe-EXDEV runner. Acceptance requires all of the following. Every run is green. Every run logs a non-overlay filesystem under `/var/lib/flatpak` after relocation and `rename-probe: ok`. At least one run's log shows the SDK update running against the relocated installation, which is valid only while the image still lags Flathub (if the image catches up, record it, and the proof rests on the probe). Relocation time is within D2. Preferably at least one run is on a runner whose pre-relocation probe returned EXDEV. If none is, record that, and the fix stands on the post-relocation probe.
- **Release:** one `release.yml` dispatch from the branch with `dry_run=true` and an untagged prerelease version (for example `v0.8.4-dryrun.1`). Acceptance: `Release Metadata` passes, and `Release Flatpak Build` logs the same diagnostics and relocation and succeeds. The publish jobs are skipped.

### D9. No toolkit, proof or lane obligations

This change touches only CI wiring and two repository scripts. No GTK, GLib or Libadwaita behaviour is relied on, so it needs no axiom or sample. No pure policy or state machine changes, so it needs no Kani shard. No runtime path changes, so it adds no responsiveness-lane case and no budget measurement, and `B_INTERACT`/`B_SETTLE` do not apply.

### D10. Upstream

`flatpak-builder` segfaulting after `Error: Failed to update org.gnome.Sdk` is a separate upstream defect (`flatpak/flatpak-builder`). The lagging image belongs to `flathub-infra/flatpak-github-actions`. Both are recorded in the programme record as report candidates. Filing either is an outward action that needs the maintainer, and the `upstream-gtk-reports` skill does not cover them.

## Risks / Trade-offs

- **Disk:** the copy doubles the installation's on-disk footprint (about 3–4 GB) for the duration of the job. Mitigation: `df -h` before and after, and D2's fallback.
- **The image lacks a tool (`findmnt`, `mount`, `python3`, GNU `cp`):** the diagnostics phase finds out before relocation is enabled. The fallbacks are `stat -f -c %T` for the filesystem, `FLATPAK_SYSTEM_DIR` for the mount, and `perl` for the probe.
- **The hypothesis is wrong**, for example if the EXDEV comes from a rename between two different mounts (`/tmp` → `/var/lib/flatpak`) rather than from overlay `redirect_dir`. Relocation onto a separate mount could then fail the same way. Phase A's per-runner probe and the `findmnt` output separate the two cases. The post-relocation probe and Phase B's green runs on probe-EXDEV runners are what prove the fix. If Phase A refutes the hypothesis, design.md, the overlay-specific requirements of the `flatpak-ci-build` delta spec, and the fix are revised before Phase B. A `flatpak update -vv --ostree-verbose` of one small ref (for example `org.freedesktop.Platform.codecs-extra`) on a failing runner then names the exact path whose rename fails.
- **Dispatch checkout change:** a non-dry dispatch from a branch was previously possible and built `main`. Now it is refused. That is a tightening with no known user, and the release skill documents dispatches from `main`.
- **Caching:** the flatpak-builder cache (`.flatpak-builder` in the checkout) is unaffected. The relocated installation is not cached, by design.

## Migration Plan

Merge on local evidence plus this change's own CI evidence (D8). The maintainer has authorised bypassing rulesets if required checks are pending. No rollback state exists. Reverting the composite action's `relocate` step restores today's behaviour.

## Open Questions

None. Phase A decides between "confirmed", "refuted" and "not confirmed", and each outcome has a recorded path.
