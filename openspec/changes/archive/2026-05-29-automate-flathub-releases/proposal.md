## Why

LushText has a working local Flatpak build, but publishing to Flathub still depends on manual version edits, manual tag discipline, manual manifest updates, and out-of-band domain verification. We need a repeatable release path so a maintainer can cut a version from `main`, validate the Flatpak surface, and hand Flathub a reviewable update without rediscovering the process each time.

## What Changes

- Add an Invowk-style release command surface for LushText, including explicit `make release VERSION=vX.Y.Z` and intelligent `make release-bump TYPE=major|minor|patch` flows with `DRY_RUN`, `YES`, prerelease, and promotion safeguards.
- Make the release flow update LushText's version surfaces consistently before tagging, including Cargo package versions, Meson project version, Cargo lock metadata, and AppStream release metadata.
- Add release validation gates that check a clean `main`, tag uniqueness, AppStream metadata, desktop metadata, Flatpak vendored Cargo sources, and Flatpak build readiness before a real release tag can be pushed.
- Add a Flathub-facing publication path that prepares or updates the Flathub manifest from a public GitHub release tag/archive instead of the local checkout-only `type: dir` manifest.
- Add automation to open a reviewable Flathub manifest update pull request after a successful release, while keeping human testing/merge as the default for Flathub publication.
- Document and verify the `dev.cominotti.lushtext` Flathub identity flow, including the `cominotti.dev` HTTPS well-known token requirement and the fact that GitHub account linking does not verify custom-domain app IDs.
- Preserve the existing Snap packaging work as a separate change; this change affects the Flatpak/Flathub release lane only.

## Capabilities

### New Capabilities

- `flathub-publication`: Flathub publisher verification, version/tag release automation, Flathub-ready manifest updates, and end-to-end release readiness checks for LushText's Flatpak distribution.

### Modified Capabilities

<!-- No existing capability requirements change. The local Flatpak sandbox identity contract remains intact; Flathub publication adds a store/release contract on top of it. -->

## Impact

- Release commands and scripts: `Makefile`, a new or adapted `scripts/release.sh`, and release-script tests.
- Versioned metadata: `meson.build`, `crates/lushtext/Cargo.toml`, `crates/lushtext-core/Cargo.toml`, `Cargo.lock`, and `data/dev.cominotti.lushtext.metainfo.xml.in`.
- Flatpak/Flathub packaging: `build-aux/dev.cominotti.lushtext.Flatpak.json`, `build-aux/cargo-sources.json`, a Flathub-ready manifest/update artifact, and any helper scripts needed to generate or verify them.
- CI/CD: GitHub Actions release workflow, Flatpak release dry-run/build validation, and optional Flathub PR creation using repository secrets or GitHub tokens.
- Documentation: `README.md`, `docs/next/flatpak-packaging.md`, `.agents/rules/build.md`, and root `AGENTS.md` release guidance.
- External prerequisites: a valid HTTPS deployment for `cominotti.dev`, the Flathub-generated verification token at `/.well-known/org.flathub.VerifiedApps.txt`, and Flathub repository/collaborator access for `dev.cominotti.lushtext`.
