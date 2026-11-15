# Policy baseline: `make check-flatpak-ci-builds` on the unchanged workflows

Run 2026-11-14, stage 3, on branch `harden-flatpak-sdk-update-ci` at `3003b747`
plus the new `scripts/check-flatpak-ci-builds.py` and its Makefile target, with
`.github/workflows/` unchanged from `main`. This is the failing-first run of
design D5: two findings, one per job, each breaking rules 1 and 2.

```
$ make check-flatpak-ci-builds
Checking CI Flatpak builds use the shared composite action...
./scripts/check-flatpak-ci-builds.py --self-test
check-flatpak-ci-builds self-test passed
Flatpak CI build policy violations:
  - .github/workflows/flatpak.yml:13 job `flatpak` breaks rule 1: uses `flatpak/flatpak-github-actions/flatpak-builder@` directly (line 27); build through `uses: ./.github/actions/flatpak-build`; rule 2: runs in container image `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` without a `uses: ./.github/actions/flatpak-build` step
  - .github/workflows/release.yml:114 job `flatpak` breaks rule 1: uses `flatpak/flatpak-github-actions/flatpak-builder@` directly (line 128); build through `uses: ./.github/actions/flatpak-build`; rule 2: runs in container image `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` without a `uses: ./.github/actions/flatpak-build` step
make: *** [Makefile:751: check-flatpak-ci-builds] Error 1
exit=2
```

The self-test passed first (direct use, a direct use outside a Flatpak
container, a container job without the action, the action before checkout, an
exempt job whose script loses `--user`, a vanished exemption, an inline
`container:` image, and a clean pass), then the real check failed with
exactly the two expected findings.

**Sequencing deviation (task 1.2).** The target is added to `.PHONY` and
`help` in this group, but joins `check-policy` only in group 3, together with
the workflow switch. `.githooks/pre-commit` runs `make pre-commit`, which runs
`check-policy`, so a failing-first member of `check-policy` would block the
group 1 and group 2 commits, and `--no-verify` is forbidden. The CI `Lint` job
step is added in group 3 for the same reason.
