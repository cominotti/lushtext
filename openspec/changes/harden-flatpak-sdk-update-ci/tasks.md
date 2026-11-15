> **Evidence scope (design D8):** the merge gate is local. These tasks never dispatch, wait for, poll or rerun GitHub CI. They follow the maintainer's relayed instruction for the programme (2026-11-14): merge on local results, check CI once at the end. The coordinator ledger's "may dispatch/read CI" note for this change is a coordinator decision, and it does not override that instruction. CI confirmation (D8 C1–C3) is owed by the programme's final CI sweep.
>
> Tests: the two helper scripts' `--self-test`s and the local container runs L1–L3. Kani: none (no proven module touched). Timing: none (no timed path touched). The relocation's wall time is a logged CI cost figure, not a responsiveness measurement. No Rust code, Cargo.lock, `cargo-sources.json`, GTK axiom or lane work belongs in this change (design D9).
>
> Run every container command through `scripts/heavy.sh -- docker run --privileged --cpuset-cpus=8-23 --memory=16g --memory-swap=16g …`, one command per call. `taskset` and `heavy.sh` constrain only the docker client: the container's processes belong to the daemon, so the limits go on `docker run`. Give each container its own named Docker volume as the `$RUNNER_TEMP` stand-in, and remove the container and the volume afterwards. The checkout reaches the container only as a **read-only** bind (`-v "$PWD":/__w/lushtext/lushtext:ro`, with `GITHUB_WORKSPACE=/__w/lushtext/lushtext` and `RUNNER_TEMP=/__w/_temp` set). Never add a writable host bind: the copy leaves root-owned files behind. Never write into the checkout.

## 1. Failing-first policy check (no workflow change yet)

- [ ] 1.1 Write `scripts/check-flatpak-ci-builds.py` as design D5 specifies. Model it on `scripts/check-workflow-timeouts.py`: line-oriented job and step parsing, an exemption table with the verified reason, and one finding per job listing every broken rule. Give it a `--self-test` over synthetic workflows covering these cases:
  - a direct `flatpak-builder@` use;
  - a container job without the action;
  - the action placed before checkout;
  - an exempt job whose script loses `--user`;
  - a clean pass.
- [ ] 1.2 Add the `check-flatpak-ci-builds` Makefile target (self-test, then the real check). Add it to `check-policy` and `.PHONY`, and give it a help line.
- [ ] 1.3 Run the target on the unchanged workflows. It must fail with two findings: `flatpak.yml` `flatpak` and `release.yml` `flatpak`, each breaking rules 1 and 2. Record the output in `openspec/changes/harden-flatpak-sdk-update-ci/evidence/policy-baseline.md`.
- [ ] 1.4 Commit (signed, through `scripts/heavy.sh`, `-F` message file) and push.

## 2. Helper script, failing-first in the real image (L1)

- [ ] 2.1 Write `scripts/ci-flatpak-installation.py` with the subcommands `diagnose`, `probe`, `relocate` and `--self-test` (design D1, D3 and D4).
  - Read paths only from the environment variables `RUNNER_TEMP` and `GITHUB_WORKSPACE`.
  - Print one `::group::`/`::endgroup::` per subcommand.
  - The probe renames the deployed commit directory of the installation's first runtime to `<commit>.rename-probe` and back with `os.rename`, and prints the target and `rename-probe: ok` or `rename-probe: <errno name>`. If the rename back fails, fail immediately.
  - `relocate` skips with a log line off overlay. On overlay it:
    - refuses a destination inside `GITHUB_WORKSPACE`;
    - runs `cp -a` into `$RUNNER_TEMP/flatpak-system`;
    - calls `mount(2)` `MS_BIND` through `ctypes`;
    - re-probes and fails unless the result is `ok`, naming the probe, the result and the filesystem;
    - prints its wall time and `df -h` before and after.
  - A missing tool (`findmnt`, `cp`) fails `diagnose` with its name.
  - `--self-test` covers the cases D4 lists, using `tempfile.TemporaryDirectory`.
- [ ] 2.2 Make `make check-flatpak-ci-builds` also run `scripts/ci-flatpak-installation.py --self-test`.
- [ ] 2.3 **L1 (must fail, design D8).** Pull `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` and record its image id. In a container, run `diagnose` from a read-only bind of the checkout (`-v "$PWD":/__w/lushtext/lushtext:ro`, `GITHUB_WORKSPACE` and `RUNNER_TEMP` set to the container paths). It must report `overlay` and `rename-probe: EXDEV`.
  - Then run `flatpak-builder --install-deps-only --install-deps-from=flathub --disable-rofiles-fuse --state-dir "$RUNNER_TEMP/state" "$RUNNER_TEMP/build" <manifest>`. It must fail with `Invalid cross-device link`.
  - If the image no longer lags Flathub, record that, and fall back to the probe plus the `codecs-extra` update of `evidence/local-reproduction.md`.
  - Record the output in `evidence/local-runs.md`.
- [ ] 2.4 Commit and push.

## 3. Shared mechanism and the fix (L2, L3)

- [ ] 3.1 Write `.github/actions/flatpak-build/action.yml` (composite). Its inputs are `manifest-path`, `bundle` and `cache-key`. Its steps are:
  1. `python3 "$GITHUB_WORKSPACE/scripts/ci-flatpak-installation.py" diagnose`;
  2. the same script's `relocate`;
  3. `flatpak/flatpak-github-actions/flatpak-builder@v6` with the three inputs.

  The shell steps use environment variables, never the `${{ runner.temp }}` or `${{ github.workspace }}` contexts (design D1).
- [ ] 3.2 Switch `flatpak.yml` and `release.yml`'s `flatpak` job to `uses: ./.github/actions/flatpak-build` after checkout. Keep each job's existing bundle name and cache key byte for byte. Add `workflow_dispatch:` to `flatpak.yml`.
- [ ] 3.3 Fix the release dispatch as design D6 and D7 specify:
  - A dispatch sets `checkout_ref="$GITHUB_SHA"`.
  - A dispatch with `dry_run != true` is refused unless `GITHUB_REF == refs/heads/main`, with an `::error::` naming the ref.
  - The Flathub-artifact step gains `DRY_RUN` in its `env`, and in a dry run with no tag it uses `HEAD`.

  Check the dry-run path locally: `make release VERSION=v0.8.4-dryrun.1 DRY_RUN=1` passes, and the step's new shell body, run in a scratch clone of the branch with `VERSION=v0.8.4-dryrun.1 DRY_RUN=true`, generates and verifies the Flathub manifest from `HEAD`. Confirm that `git status` is clean afterwards.
- [ ] 3.4 **L2 (must pass).** Start a fresh container from the same image, with `docker run -d … sleep`, the read-only checkout bind and its own volume, as in the header. Then run, as separate `docker exec` calls:
  1. `diagnose`;
  2. `relocate`, which must log its wall time, `df -h` before and after, a non-overlay filesystem and `rename-probe: ok`;
  3. `findmnt -T /var/lib/flatpak`, which must show the copy;
  4. the same `flatpak-builder --install-deps-only` as 2.3, which must succeed and log the `org.gnome.Sdk` update. If the image no longer lags, fall back to `codecs-extra`;
  5. a full build with CI's arguments (design D8 L2): export the committed tree into `$RUNNER_TEMP/src` (`git -c safe.directory='*' -C /__w/lushtext/lushtext archive HEAD | tar -x -C "$RUNNER_TEMP/src"`, so no `target/` or untracked file enters, like CI's clean checkout) and run `flatpak-builder --disable-rofiles-fuse --install-deps-from=flathub --force-clean --ccache --state-dir "$RUNNER_TEMP/state" "$RUNNER_TEMP/app" "$RUNNER_TEMP/src/build-aux/dev.cominotti.lushtext.Flatpak.json"`. It must succeed.

  Record the run in `evidence/local-runs.md`, with the relocation time labelled non-verdict.
- [ ] 3.5 **L3 (must refuse).** In a fresh container set up as in 3.4, show two refusals:
  - `relocate` with `RUNNER_TEMP` inside `GITHUB_WORKSPACE` is refused before any copy.
  - A forced probe failure fails with the probe, the result and the filesystem named. For example, point the post-relocation probe at a read-only bind, or pass a self-test hook.

  Record both in `evidence/local-runs.md`.
- [ ] 3.6 Run `make check-flatpak-ci-builds` (it must pass now) and `make check-workflow-timeouts`. Then run task 5.3's re-read. Commit and push.

## 4. Docs and records

- [ ] 4.1 `.agents/rules/build.md` CI section:
  - Update the `flatpak.yml` and `release.yml` bullets to cover the shared composite action, the relocation and its reason, the diagnostics group, `workflow_dispatch`, the dispatch checkout and the dry-run rules.
  - Add `check-flatpak-ci-builds` wherever `check-policy`'s members are listed.
  - Make the "When bumping gtk-rs" sentence name every `gnome-51` image use (`flatpak.yml` and both container jobs of `release.yml`).
- [ ] 4.2 `AGENTS.md`: update the **CI** bullet in Meson / Flatpak Build (`flatpak.yml` / `release.yml`). `.claude/CLAUDE.md` resolves to it.
- [ ] 4.3 `.agents/skills/publish-release/` and `.agents/skills/flatpak-rust/`: update any statement about dispatching `release.yml` (it builds the dispatched commit, a dry run works from a branch, a non-dry dispatch only from `main`) or about the Flatpak CI job.
- [ ] 4.4 `docs/next/flatpak-packaging.md`:
  - Correct the stale "Runtime/SDK" section (it says 50; the manifest says 51).
  - Add a short CI note under "Release Automation": the relocation, its cause and the dry-run rules.
- [ ] 4.5 `README.md`: update it only if it documents the release dispatch or the policy targets. Check, and record "no change" otherwise.
- [ ] 4.6 Programme record `docs/next/long-line-documents.md`: add this change's queue entry, with:
  - the outcome and the local evidence (L1–L3);
  - the relocation's local cost;
  - the CI confirmation owed by the final sweep (design D8 C1–C3), as an open item;
  - the two upstream report candidates (design D10).
- [ ] 4.7 Update `.claude/handoffs/flatpak-ci-exdev/notes.md` with the verdict and the change name, and keep this change's handoff in `.claude/handoffs/harden-flatpak-sdk-update-ci/`.
- [ ] 4.8 Commit and push.

## 5. Verification

- [ ] 5.1 `make check-policy` (through `scripts/heavy.sh`). It must pass, including the new target.
- [ ] 5.2 `openspec validate harden-flatpak-sdk-update-ci --strict`.
- [ ] 5.3 Re-read every changed workflow. Confirm that the cache keys, bundle names, `timeout-minutes` and permissions are unchanged, and that no file outside `.github/actions/flatpak-build/` names `flatpak-builder@`.
- [ ] 5.4 Confirm that no container or volume from L1–L3 is left (`docker ps -a`, `docker volume ls`) and that the checkout is clean apart from this change's files.
