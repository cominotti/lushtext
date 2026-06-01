## Why

Flathub's current generative-AI submission policy makes it a poor primary distribution target for LushText. LushText already has a working Flatpak build, so the next practical step is to publish it through an official Cominotti-owned Flatpak remote where users trust the Cominotti publisher once and receive normal Flatpak updates.

## What Changes

- Add a publisher-level Flatpak repository for Cominotti apps, with LushText as the first application ref.
- Host the remote under `flatpak.cominotti.dev` rather than requiring the existing `cominotti.dev` website path.
- Add signed repository metadata, `.flatpakrepo`, and LushText-specific `.flatpakref` generation for user installs.
- Extend release automation so tagged LushText releases can build, sign, update, and stage the Cominotti Flatpak repository.
- Keep Flathub-specific handoff optional/secondary; Cominotti repository publication becomes the primary Flatpak distribution route.

## Capabilities

### New Capabilities
- `cominotti-flatpak-repository`: Defines the official publisher-owned Flatpak remote, app refs, install metadata, signing, hosting, and release publication behavior.

### Modified Capabilities
- `flathub-publication`: Flathub PR generation and domain verification stop being the primary Flatpak publication contract and must not block Cominotti repository publication.

## Impact

- Affected release and packaging scripts under `scripts/`, `Makefile`, and `.github/workflows/`.
- New generated or template artifacts for `.flatpakrepo`, `.flatpakref`, repository export, repository signing, and deploy staging.
- Documentation updates in `docs/next/flatpak-packaging.md` describing the Cominotti remote, install commands, GPG trust, and maintenance flow.
- Existing Flatpak manifest, AppStream metadata, desktop identity, and Cargo vendoring remain the source packaging contract for LushText.
