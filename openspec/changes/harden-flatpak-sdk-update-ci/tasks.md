> **CI exception (this change only, maintainer request relayed 2026-11-14):** this change's evidence is CI logs. Its stages may push the branch, dispatch and rerun `flatpak.yml`, and dispatch `release.yml` with `dry_run=true` from the branch. They never push a tag, never dispatch with `dry_run=false`, and never publish anything. The generic "never wait for, poll or rerun CI" rule does not apply to the runs listed in design D8. It still applies to every other workflow. No Rust code, Cargo.lock, `cargo-sources.json`, Kani, GTK axiom or lane work belongs in this change (design D9).

## 1. Failing-first policy check (no workflow change yet)

- [ ] 1.1 Write `scripts/check-flatpak-ci-builds.py` per design D5. Model it on `scripts/check-workflow-timeouts.py`: line-oriented job and step parsing, an exemption table with the verified reason, and `--self-test` over synthetic workflows. The synthetic workflows must cover a direct `flatpak-builder@` use, a container job without the action, the action placed before checkout, an exempt job whose script loses `--user`, and a clean pass.
- [ ] 1.2 Add the `check-flatpak-ci-builds` Makefile target (self-test, then real check), add it to `check-policy` and `.PHONY`, and add its help line.
- [ ] 1.3 Run it on the unchanged workflows and record the expected failure, two findings (`flatpak.yml` `flatpak`, `release.yml` `flatpak`), in `openspec/changes/harden-flatpak-sdk-update-ci/evidence/policy-baseline.md`.

## 2. Shared mechanism, diagnostics only (Phase A)

- [ ] 2.1 Write `scripts/ci-flatpak-installation.sh` with subcommands `diagnose`, `probe` and `relocate` (design D1 and D3). `set -euo pipefail`, one `::group::`, and the probe through `rename(2)` (`python3`, else `perl`; never `mv`). `relocate` copies with `cp -a` into `$RUNNER_TEMP/flatpak-system`, bind-mounts the copy, re-probes, times itself, and prints `df -h` before and after. It is a no-op with a log line when the installation is not on overlayfs. Make it pass `shellcheck` if the repository's checks run it; otherwise run `bash -n`.
- [ ] 2.2 Write `.github/actions/flatpak-build/action.yml` (composite). Inputs: `manifest-path`, `bundle`, `cache-key`, and `relocate` (default `true`). Steps: `diagnose`, `relocate` when enabled, and `flatpak/flatpak-github-actions/flatpak-builder@v6` with the three build inputs.
- [ ] 2.3 Switch `flatpak.yml` and `release.yml`'s `flatpak` job to `uses: ./.github/actions/flatpak-build` after checkout. Keep each job's existing bundle name and cache key byte for byte. Add `workflow_dispatch:` to `flatpak.yml`. For Phase A only, pass `relocate: false` from `flatpak.yml`.
- [ ] 2.4 Fix the release dispatch per design D6 and D7: dispatch `checkout_ref="$GITHUB_SHA"`; refuse `dry_run != true` unless `GITHUB_REF == refs/heads/main`; in a dry run with no tag, the Flathub-artifact step uses `HEAD`.
- [ ] 2.5 Run `make check-flatpak-ci-builds` (it must pass now) and `make check-workflow-timeouts`. Commit, then push the branch.
- [ ] 2.6 Phase A evidence (design D8): dispatch `flatpak.yml` on the branch, up to 8 runs. If GitHub refuses the dispatch, open a draft PR and use its `Flatpak Build` reruns. Record every run in `evidence/runs.md` using the D8 columns. Record the verdict "confirmed", "refuted" or "not confirmed", with the deciding run ids. Also record which tools the image lacked, if any.
- [ ] 2.7 If Phase A refutes the hypothesis, revise design.md (cause, D1, risks) before continuing, and record the revision in `evidence/runs.md`.

## 3. Relocation on (Phase B)

- [ ] 3.1 Remove `relocate: false` from `flatpak.yml`. Push.
- [ ] 3.2 Phase B evidence (design D8): at least 4 dispatches of `flatpak.yml`, and up to 8 if none lands on a probe-EXDEV runner. Every run must be green and must log a non-overlay filesystem under `/var/lib/flatpak` after relocation, `rename-probe: ok`, and relocation time within 90 s. At least one run must show the SDK update running against the relocated installation, or the log must record that the image no longer lags. Record each run in `evidence/runs.md`.
- [ ] 3.3 If relocation exceeds 90 s or runs short of disk, switch to design D1's empty-installation fallback, record both figures in design.md D2, and repeat 3.2.
- [ ] 3.4 Release proof: dispatch `release.yml` from the branch with `dry_run=true` and an untagged prerelease version such as `v0.8.4-dryrun.1`. First confirm the version has no tag locally or on `origin`. `Release Metadata` must pass, and `Release Flatpak Build` must log the diagnostics and relocation and succeed. The Cominotti, deploy and publish jobs must be skipped. Record the run in `evidence/runs.md`. Confirm afterwards that no tag or release was created (`git ls-remote --tags origin`, `gh release list`).

## 4. Docs and records

- [ ] 4.1 `.agents/rules/build.md` CI section: update the `flatpak.yml` and `release.yml` bullets to cover the shared composite action, relocation and its reason, the diagnostics group, `workflow_dispatch`, the dispatch checkout and the dry-run rules. Add `check-flatpak-ci-builds` wherever `check-policy`'s members are listed. Update the `## CI` text that says how to bump the GNOME tag, if the composite action changes that procedure.
- [ ] 4.2 `AGENTS.md`: update the **CI** bullet in Meson / Flatpak Build (`flatpak.yml` / `release.yml`) and the Build Commands list, if it names policy checks. `.claude/CLAUDE.md` is a symlink to it.
- [ ] 4.3 `.agents/skills/publish-release/` and `.agents/skills/flatpak-rust/`: update any statement about dispatching `release.yml` (dispatched commit, dry run from a branch, non-dry only from `main`) or about the Flatpak CI job.
- [ ] 4.4 `README.md`: update it only if it documents the release dispatch or the policy targets. Check, and record "no change" otherwise.
- [ ] 4.5 Programme record `docs/next/long-line-documents.md`: add this change's queue entry with its outcome, the Phase A verdict, the relocation cost, and the two upstream report candidates (design D10).
- [ ] 4.6 Update `.claude/handoffs/flatpak-ci-exdev/notes.md` with the verdict and the change name, and put this change's handoff in `.claude/handoffs/harden-flatpak-sdk-update-ci/`.

## 5. Verification

- [ ] 5.1 `make check-policy` (through `scripts/heavy.sh`). It must pass, including the new target.
- [ ] 5.2 `openspec validate harden-flatpak-sdk-update-ci --strict`.
- [ ] 5.3 Re-read every changed workflow for unchanged cache keys, bundle names, `timeout-minutes` and permissions, and confirm no workflow outside `.github/actions/flatpak-build/` names `flatpak-builder@`.
