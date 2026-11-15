## Context

CI builds the Flatpak in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, with the container `--privileged`, through `flatpak/flatpak-github-actions/flatpak-builder@v6`. The action always runs
`xvfb-run flatpak-builder --repo=repo --disable-rofiles-fuse --install-deps-from=flathub … --ccache --state-dir .flatpak-builder flatpak_app <manifest>`.
It runs as root without `--user`, so it installs into the **system** installation `/var/lib/flatpak`. The image pre-populated that installation with `flatpak install -y flathub org.gnome.Platform//51 org.gnome.Sdk//51`. That is the image's last layer, 1,253 MiB compressed and 3.8 GiB on disk. The image was created 2026-09-22T13:43Z.

### Measured baseline (CI)

Run `43642169956` (PR #98, `flatpak.yml`). Every attempt used the same commit and the same cache key:

| Attempt | Job | Runner image (ubuntu-26.04) | `Updating org.gnome.Sdk/x86_64/51` | Result |
|---|---|---|---|---|
| 1 | 132591925079 | `20261111.82.6` | every ref: `Invalid cross-device link`; `Error: Failed to update org.gnome.Sdk` | `flatpak-builder` segfault, `xvfb-run` exit 139 |
| 2 | 132597336544 | `20261111.82.6` | same | same |
| 3 | 132600497134 | `20261111.196.1` | SDK, Locale, GL.default, codecs-extra, then Platform, updated | success, job 6 min 24 s |

- **Failing refs, in order:** `org.freedesktop.Platform.GL.default` (26.08 and 26.08-extra), `org.freedesktop.Platform.codecs-extra`, `org.gnome.Sdk.Locale`, `org.gnome.Sdk`. Each failed within 0.3–26 s of starting, after its download.
- **What differs:** the runner image version is the only difference visible in the logs.
- **Why it was hidden:** earlier green runs (`b3f7a273`, `b129f378`) logged no update, because the image and Flathub agreed at the time.

`release.yml` v0.8.3 (2026-09-22, runner ubuntu-24.04):

- **`Release Flatpak Build` (job `106956706909`):** `Updating org.gnome.Sdk/x86_64/50` → `Nothing to update.` It had the same exposure, but nothing triggered it.
- **`Cominotti Flatpak Repository` (job `106956706850`):** `Installing org.gnome.Sdk/x86_64/50 from flathub` and the Platform, a fresh **user** install, took 38 s (22:04:52.8 → 22:05:31.1). `generate-cominotti-flatpak-repo.sh` passes `--user` (lines 158–164), and `HOME=/github/home` is a host bind mount.

**`flatpak-builder@v6` inputs:** read from its `action.yml` at `v6`, rechecked in review. It is a `node24` action. Its inputs are `manifest-path`, `stop-at-module`, `bundle`, `build-bundle`, `build-debug-bundle`, `repository-url`, `repository-name`, `run-tests`, `cache`, `restore-cache`, `save-cache`, `cache-key`, `keep-build-dirs`, `branch`, `arch`, `mirror-screenshots-url`, `gpg-sign`, `build-dir`, `repo-dir`, `state-dir`, `verbose`, `upload-artifact` and `artifact-name`. None of them selects `--user` or passes extra arguments.

**Manifest source:** `build-aux/dev.cominotti.lushtext.Flatpak.json` uses `{"type": "dir", "path": ".."}`, which is the whole checkout. Anything copied into the checkout becomes build input.

### Mechanism, reproduced locally

`evidence/local-reproduction.md` reproduces the CI failure on the developer host with the same image (`sha256:78d969b1…`). On the host, overlay `redirect_dir` is `N`, and Docker 29 uses the containerd overlayfs snapshotter. The image's system installation lags Flathub on all five refs.

- **The probe:** `rename(2)` of the deployed `codecs-extra` commit directory returns `EXDEV`.
- **The update:** `flatpak update --system org.freedesktop.Platform.codecs-extra//26.08-extra` fails with `Error: Failed to update org.freedesktop.Platform.codecs-extra: Invalid cross-device link`, the CI text.
- **After relocation:** the installation was copied with `cp -a` (3.8 GiB, hardlinks kept) to a non-overlay volume and bind-mounted over `/var/lib/flatpak` by `mount(2)`. A later, separate `docker exec` still sees the mount, `rename-probe: ok`, and the same update succeeds (`1ac8ac60` → `2b9f95bc`).

The cause is established: overlayfs refuses to rename a directory that lives in a lower layer unless `redirect_dir` is on, and flatpak's update renames deployment directories. Still open: why runner image `20261111.196.1` updated successfully. Its overlay may have `redirect_dir` on, or it may use another snapshotter. The fix does not depend on the answer. The diagnostics (D3) record it on every run.

The image ships **no `mount` binary** (also no `umount` and no `busybox`). It does ship `findmnt`, GNU `cp` 9.7, `python3` with `ctypes`, and `perl`. The bind mount is therefore made with `mount(2)` through `python3` (D1).

### A pre-existing blocker found while designing the proof

As written on `main` today, `release.yml`'s manual dispatch (`dry_run` defaults to `true`) cannot pass `Release Metadata` with any `version` input. Each step below was checked locally in review:

- For a released version, `make release VERSION=v0.8.3 DRY_RUN=1` dies with `ERROR: tag 'v0.8.3' already exists` (`run_release`, `scripts/release.sh`).
- For an unreleased version, `make release VERSION=v0.8.4-dryrun.1 DRY_RUN=1` passes and mutates nothing. `Generate Flathub update artifact` then dies on `git rev-parse "$VERSION^{commit}"`.
- `generate-flathub-manifest.sh v0.8.4-dryrun.1 "$(git rev-parse HEAD)"` and `verify-flathub-manifest.sh` both pass. `release-dry-run.yml` already calls the generator with `v0.0.1 "$GITHUB_SHA"`. Using the dispatched commit is therefore enough (D7).

Because `Release Flatpak Build` needs `metadata`, a dry run cannot reach the Flatpak build. A dispatch also checks out `main` whatever ref it was dispatched from.

## Goals / Non-Goals

**Goals:**

- Every Flatpak build in CI can update dependencies newer than the image's on any runner, without retries.
- One mechanism for every job that builds against the image's system installation, enforced statically.
- Evidence in the job log on every run, so a future regression can be attributed from the log alone.
- A release dry-run dispatch that runs the release Flatpak build of the dispatched commit and publishes nothing.

**Non-Goals:**

- Changing the image, its tag, the GNOME runtime, the manifest, the `flatpak-builder@v6` version, or the Cominotti job's `--user` install.
- Making the Cominotti job, which publishes, runnable in a dry run. It stays gated on `dry-run != 'true'`.
- Fixing `flatpak-builder`'s segfault after a failed dependency update. That is upstream, and the deferral is recorded in D10.
- Rust, Cargo.lock, `cargo-sources.json`, Kani, GTK axioms and smoke lanes.

## Decisions

### D1. Relocate the system installation off overlayfs (copy + bind mount)

When `findmnt -no FSTYPE -T /var/lib/flatpak` reports `overlay`, `scripts/ci-flatpak-installation.py relocate` does the following:

1. Runs `cp -a /var/lib/flatpak "$RUNNER_TEMP/flatpak-system"`. `-a` keeps the hardlinks between the ostree repository and the deployments within the copy, so the copy costs what `du` reports, not double. Locally: 3.8 GiB in, 3.8 GiB out.
2. Calls `mount(2)` with `MS_BIND` from `"$RUNNER_TEMP/flatpak-system"` onto `/var/lib/flatpak`, through `ctypes` (`libc.so.6`, `use_errno=True`). The image has no `mount` binary. The job's container is `--privileged`, and the mount persists across the job's later steps, each of which is a separate `docker exec` (verified locally).
3. Re-runs the rename probe (D3) and fails the step when it does not return `ok`.

When the installation is not on overlayfs, for example after a future runner or image fix, the step skips relocation and logs why.

The destination is read from the **environment variable** `$RUNNER_TEMP`, never from the `${{ runner.temp }}` expression context. In container jobs the expression context resolves to the host path (`/home/runner/work/_temp`), while the variable holds the container path (`/__w/_temp`). The same applies to `$GITHUB_WORKSPACE`. `$RUNNER_TEMP` is a host bind mount, so it is not overlay, and it lies outside the checkout (`/__w/lushtext/lushtext`), so the manifest's `dir` source does not pull the copy into the build. The script refuses a destination inside `$GITHUB_WORKSPACE`.

Alternatives considered:

- **`--user` installation:** `flatpak-builder@v6` has no input for it (see the input list above). Wrapping `flatpak-builder` ourselves would mean re-implementing the action's cache, bundle and artifact handling.
- **The `mount` binary (`mount --bind`):** the image does not ship one (`evidence/local-reproduction.md`). Installing util-linux into the Freedesktop SDK image is not possible without a package manager.
- **`FLATPAK_SYSTEM_DIR` exported through `$GITHUB_ENV`:** libflatpak honours it, but only processes that inherit the variable see the relocated installation. A bind mount is transparent to everything that names `/var/lib/flatpak`. This is kept as the fallback if `mount(2)` is refused on a runner (it was not, locally).
- **A Docker named volume over `/var/lib/flatpak` in `container.options`/`volumes`:** it has to be declared in every job's YAML, so it cannot be the one shared mechanism. Docker's copy-up of image content into a new volume under the containerd image store is also unverified on these runners.
- **An empty installation on a non-overlay path plus `flatpak remote-add flathub`:** every run would download the full SDK and Platform (38 s in v0.8.3's user install). That drops the image's preinstalled content and adds a full Flathub download to every run. It stays the fallback if the copy exceeds its budget (D2).
- **`flatpak mask` / pinning the image's SDK:** this hides the newer SDK and builds release artifacts against a stale one.
- **Our own image, or waiting for upstream to rebuild:** an image we own means maintenance. Waiting only defers the next lag.

### D2. Relocation budget

Relocation (copy + mount + probe) has a budget of **90 s per job**.

- **Where it is judged:** the budget is an acceptance figure for this change, judged from the wall time logged on CI. It is deliberately **not** a runtime gate, because failing a job when one runner copies slowly would trade one flake for another.
- **Disk:** relocation must leave enough free disk for the build. The step prints `df -h` before and after, and the build must still complete.
- **Timeout:** the current job takes 6 min 24 s against its 30-minute ceiling, and `scripts/check-workflow-timeouts.py` keeps `timeout-minutes: 30`.
- **Local figure:** 6.8 s for 3.8 GiB (non-verdict; NVMe, page cache not controlled). That makes 90 s a margin of more than ten times over a local copy, for slower runner disks.
- **CI figure:** CI measures the budget for the first time at the programme's final CI sweep (D8). If CI measures more than 90 s there, or runs short of disk, a follow-up switches to D1's empty-installation fallback and records both figures here.
- **Source of the figure:** the fallback's measured 38 s plus a margin for copy variance. A larger cost would make every PR's Flatpak check visibly slower for no benefit over the fallback.

Timing: none (no timed path touched). The relocation's wall time is a CI cost figure, not a responsiveness measurement, so `scripts/measure.sh` and the `B_INTERACT`/`B_SETTLE` budgets do not apply.

### D3. Diagnostics and the rename probe in every Flatpak build log

`scripts/ci-flatpak-installation.py diagnose` prints the following under one `::group::`:

- `uname -r` and `/sys/module/overlay/parameters/redirect_dir`;
- `grep overlay /proc/mounts`;
- `findmnt -T` for `/var/lib/flatpak`, `$RUNNER_TEMP` and `/tmp`;
- `df -h` for the same paths;
- `ImageVersion` from the runner, if it is visible inside the container (otherwise the log's own "Runner Image" header, which the evidence table copies);
- the probe result.

**Probe target.** The probe renames the deployed commit directory of the installation's first runtime (`/var/lib/flatpak/runtime/<ref>/<arch>/<branch>/<commit>`, the kind of directory flatpak renames on update) to the sibling `<commit>.rename-probe` and back, with `os.rename` (`rename(2)`). It prints the target path and `rename-probe: ok` or `rename-probe: <errno name>` (for example `EXDEV`). If the rename succeeds and the rename back fails, the step fails immediately and names both paths. GNU `mv` cannot serve as the probe, because it falls back to copy-and-delete on EXDEV.

The probe runs before and after relocation. Before relocation it is diagnostic only. After relocation, any result other than `ok` fails the step, with a message that names the installation's filesystem. This turns the old failure mode, a segfault after a dependency update, into a direct, named error before `flatpak-builder` starts.

### D4. One composite action and one helper script

`.github/actions/flatpak-build/action.yml` (composite) takes `manifest-path`, `bundle` and `cache-key`. It runs `diagnose`, then `relocate`, then `flatpak/flatpak-github-actions/flatpak-builder@v6` with those inputs. It is the only file that names `flatpak-builder@`.

`flatpak.yml` and `release.yml`'s `flatpak` job call `uses: ./.github/actions/flatpak-build` after `actions/checkout`, which a local action needs. The action invokes the helper as `python3 "$GITHUB_WORKSPACE/scripts/ci-flatpak-installation.py" …`, through the environment variable (D1).

The helper is **Python, not shell**. Both the probe and the mount have to call into `rename(2)` and `mount(2)` directly, and `python3` is the one interpreter the image ships that can do both. One language keeps the helper testable: `--self-test` runs in `make check-flatpak-ci-builds` on a temporary directory, which is non-overlay, so the probe returns `ok`. The self-test also covers:

- planning skips relocation off overlay;
- a destination inside the workspace is refused;
- a probe target is chosen;
- the errno name is formatted.

The self-test's temporary directories are removed on exit (`tempfile.TemporaryDirectory`), as `.agents/rules/build.md` "Temporary Files" requires.

### D5. Policy check against divergence (the failing-first test)

`scripts/check-flatpak-ci-builds.py` follows `check-workflow-timeouts.py`: a simple line-oriented parser, `--self-test` over synthetic workflows, and a Makefile target `check-flatpak-ci-builds` in `check-policy`. It fails when:

1. a file under `.github/workflows/` names `flatpak/flatpak-github-actions/flatpak-builder@`;
2. a job whose `container` image contains `flatpak-github-actions` has no `uses: ./.github/actions/flatpak-build` step, unless it appears in the check's exemption table;
3. that step comes before the job's `actions/checkout` step;
4. an exempt job's stated reason no longer holds. The single exemption, `release.yml` `cominotti-flatpak`, holds only while `scripts/generate-cominotti-flatpak-repo.sh`'s `flatpak-builder` arguments include `--user`.

The check reports one finding per job, listing every rule the job breaks. On `main` today it reports two findings: `flatpak.yml` `flatpak` and `release.yml` `flatpak`, each breaking rules 1 and 2. That failing run is recorded before the workflows change.

### D6. Release dispatch builds the dispatched commit

In `release.yml`'s `Determine version`, a `workflow_dispatch` sets `checkout_ref` to `$GITHUB_SHA`, the dispatched commit, instead of `main`. A dispatch with `dry_run` other than `true` fails unless `GITHUB_REF` is `refs/heads/main`, and the error names the dispatched ref.

- **Real dispatches:** they still build `main`. They now build one fixed commit across every job, instead of each job resolving `main` again.
- **Dry runs:** a dry run from a branch is how a release-workflow change is proven before merge.
- **Tag pushes:** unchanged. They check out the tag.

### D7. Dry-run source commit

In `Generate Flathub update artifact`, the step gains `DRY_RUN` in its `env`. When `DRY_RUN` is `true` and `refs/tags/$VERSION` does not exist, `source_commit` is `HEAD`, the dispatched commit. Otherwise it stays the tag's commit.

`make release DRY_RUN=1` already refuses an existing tag, so every dry run that reaches this step takes the `HEAD` branch. A real release still requires the tag, as before. The `cominotti-flatpak` and `publish-context` jobs, which also resolve the tag, stay skipped in a dry run.

### D8. Evidence protocol: local proof gates the merge, CI confirms at the programme's final sweep

The maintainer's relayed instruction for this programme (2026-11-14) is to merge on local results and check CI once, at the end of the whole programme. This change follows it like every other change.

The coordinator's `PARALLEL-LEDGER.md` notes a "may dispatch/read CI" exception for this change. That note is a coordinator decision, not the maintainer's, and it does not override the relayed instruction. No stage of this change dispatches, waits for, polls or reruns GitHub CI.

**Before merge (local, failing-first):** the CI failure reproduces locally (`evidence/local-reproduction.md`), so the merge gate is a local run of the real helper in the real image:

- **L1, as shipped:** in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` (`--privileged`, its own Docker volume standing in for `$RUNNER_TEMP`), `diagnose` must report `overlay` and `rename-probe: EXDEV`. `flatpak-builder --install-deps-only --install-deps-from=flathub` on the manifest must fail with `Invalid cross-device link` while the image lags Flathub. This is the failing-first run. If the image no longer lags, record it, and L1 rests on the probe and on `codecs-extra` as in the reproduction.
- **L2, relocated:** in a fresh container from the same image, `diagnose` then `relocate` run as two separate `docker exec` calls, as GitHub runs steps. `relocate` must log its wall time, `df -h` before and after, a non-overlay filesystem under `/var/lib/flatpak` and `rename-probe: ok`. In a third exec, the same `flatpak-builder --install-deps-only` must succeed and update the SDK.
- **L3, refusal:** a run of `relocate` against a destination inside the checkout must be refused. A forced probe failure, for example a target on a read-only path, must fail the step and name the probe, the result and the filesystem.
- **Policy:** `make check-flatpak-ci-builds` fails on the unchanged workflows (two findings) and passes after the switch. `make check-workflow-timeouts` and `make check-policy` pass.

**After merge (the programme's final CI sweep, owned by the coordinator, not by this change's stages):**

- **C1:** `flatpak.yml`'s push run on `main` logs the diagnostics group (runner image, kernel, `redirect_dir`, pre-relocation probe), relocation within D2's 90 s, `rename-probe: ok`, and a green build. The SDK update has to run against the relocated installation, which is possible only while the image still lags Flathub. If it no longer lags, record that.
- **C2:** one `release.yml` dispatch from `main` with `dry_run=true` and an untagged prerelease version (for example `v0.8.4-dryrun.1`, after confirming the tag is absent locally and on `origin`). `Release Metadata` passes, `Release Flatpak Build` logs the same diagnostics and relocation and succeeds, and the publish jobs are skipped. Afterwards `git ls-remote --tags origin` and `gh release list` show no new tag or release.
- **C3:** if C1 or C2 fails, or relocation exceeds 90 s, the sweep fixes it under the pre-existing-blockers rule (D2's fallback for cost). The outcome goes in the programme record entry this change adds.

`flatpak.yml` gains `workflow_dispatch`, so the sweep and later checks can run that one workflow on demand.

### D9. No toolkit, proof or lane obligations

This change touches only CI wiring and repository scripts.

- **Axioms:** no GTK, GLib or Libadwaita behaviour is relied on, so it relies on no ledger axiom and needs no new axiom or sample.
- **Kani:** no pure policy or state machine changes. Kani: none (no proven module touched).
- **Timing:** no runtime path changes. Timing: none (no timed path touched), so the change adds no responsiveness-lane case, and `B_INTERACT`/`B_SETTLE` do not apply.
- **Tests:** the only tests are the two scripts' self-tests and the local container runs of D8.

### D10. Upstream

Two upstream defects are recorded in the programme record as report candidates:

- `flatpak-builder` segfaulting after `Error: Failed to update org.gnome.Sdk`, in `flatpak/flatpak-builder`;
- the lagging image, which belongs to `flathub-infra/flatpak-github-actions`.

Filing either is an outward action that needs the maintainer, and the `upstream-gtk-reports` skill does not cover these projects.

## Risks / Trade-offs

- **Disk:** the copy doubles the installation's on-disk footprint (3.8 GiB) for the duration of the job. Mitigation: `df -h` before and after, and D2's fallback.
- **A runner refuses `mount(2)`:** this did not happen locally under `--privileged`. If it happens on CI, the step fails with the errno before `flatpak-builder` runs, and the fallback is `FLATPAK_SYSTEM_DIR` (D1).
- **The image changes its tools** (drops `python3` or `findmnt`): the step fails at `diagnose` with a named missing tool, never later inside `flatpak-builder`.
- **Runner variance stays unexplained:** why `20261111.196.1` succeeded is not known. The fix holds either way, because relocation removes the overlay from the path. The diagnostics record the answer at the sweep.
- **Dispatch checkout change:** a non-dry dispatch from a branch was previously possible and built `main`. Now it is refused. That is a tightening with no known user, and the release skill documents dispatches from `main`.
- **Caching:** the flatpak-builder cache (`.flatpak-builder` in the checkout) is unaffected. The relocated installation is not cached, by design.

## Migration Plan

Merge on the local evidence of D8. The maintainer has authorised merging with `--admin` while required checks are pending. No rollback state exists: reverting the composite action's `relocate` step restores today's behaviour. CI confirmation (D8 C1–C3) is owed by the programme's final CI sweep. The programme record entry states that.

## Open Questions

None. Why the one runner image succeeded is recorded as unknown, and it does not affect the decision.
