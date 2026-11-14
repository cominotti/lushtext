# Dormant Flathub tooling

Flathub publication is not planned (`.agents/rules/build.md`, "Flatpak Release Automation", and
`openspec/specs/flathub-publication/spec.md`). The generators stay tested so
they do not rot: `make flathub-manifest`, `make verify-flathub-manifest` and
`make verify-flathub-domain`, covered by `scripts/test-flathub-manifest.sh`.

When you change those scripts, keep them generating from an immutable release
tag and full commit with offline-complete Cargo sources, run
`scripts/test-flathub-manifest.sh`, and keep the manifest's `--filesystem=host`
permission unchanged. Do not open a Flathub PR, configure Flathub credentials,
or report a skipped Flathub step as outstanding. The Flathub *remote* stays the
source of the GNOME runtime and SDK.
