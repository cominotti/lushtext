---
name: flatpak-rust
description: Guide and review LushText Flatpak, Flathub, Meson, desktop-entry, AppStream, icon, Cargo vendoring, packaging CI, and release-distribution work. Trigger for changes under build-aux/ or data/, root meson files, Cargo dependency changes that require cargo-sources.json, Flatpak permissions, app-store submission, release packaging, and any *.desktop*, *.metainfo*, *.Flatpak.json, or meson* file.
---

# Flatpak and Flathub packaging

Treat the checkout as the authority for LushText packaging. Do not paste generic manifests,
metainfo, Meson files, release versions, screenshots, runtime versions, or CI jobs into the
repository. They become stale quickly and can erase project-specific behavior.

## Start from current state

1. Read `SOUL.md`, root `AGENTS.md`, `.agents/rules/build.md`, and any instructions local to the changed path.
2. Inspect `Makefile`, `meson.build`, `meson_options.txt`, `build-aux/cargo.sh`, the active manifest under `build-aux/`, `data/meson.build`, and the current desktop/metainfo/icon files relevant to the task.
3. For releases or distribution changes, inspect `docs/next/flatpak-packaging.md`, the release workflows, and release helper scripts before proposing commands.
4. Fetch current upstream documentation for Flatpak, Flathub, AppStream, Meson, or a CLI before making an upstream-policy or syntax claim. Keep repository contracts distinct from upstream recommendations.
5. Modify the smallest current surface that owns the behavior. Never reconstruct a repository file from a reference example.

Read these references only when their topic applies:

- [Meson and Cargo integration](references/meson-cargo.md) for build integration and vendored Cargo sources.
- [AppStream and desktop metadata](references/appstream.md) for desktop metadata and AppStream checks.
- [Flathub review and handoff](references/flathub-review.md) for submission, broad-permission review, and immutable release sources.

## Preserve LushText's filesystem contract

LushText intentionally uses full host filesystem access because workspaces, file monitoring,
sidecars, search, rename/delete, and durable writes operate on arbitrary local paths. Preserve
the manifest's `--filesystem=host` permission and run `make check-flatpak-permissions` whenever
packaging or sandbox policy changes.

Do not describe this permission as universally recommended or guaranteed to pass Flathub review.
It is a deliberate product tradeoff with a large sandbox surface. Flathub expects minimal
permissions and permission changes receive review, so a Flathub submission must explain why the
editor's current semantics require host access. A portals-only or narrower-permission migration
is a separate product/architecture change requiring explicit authorization, end-to-end behavior
coverage, and updates to the repository's permission policy.

## Dependency changes

After any dependency, feature, source, or `Cargo.lock` change that affects the build:

```bash
make cargo-sources
git diff --check
```

Review the generated `build-aux/cargo-sources.json` and include it in the same change. Do not
install a generator ad hoc or invoke a copied command when the repository target is available.
The Flatpak build is offline; the generated sources and Cargo configuration must remain aligned
with the current manifest and wrapper.

## Proportional validation

Run the narrow checks first, then the build proof appropriate to the change:

```bash
make check-flatpak-permissions
make check-agent-docs
make meson-build
make meson-test
make flatpak
make flatpak-install
make verify-flatpak-identity
git diff --check
```

- Run `make cargo-sources` before the build when dependency inputs changed.
- `make flatpak` proves that the manifest builds; it does not replace or verify an already installed app. Run `make flatpak-install` immediately before `make verify-flatpak-identity` whenever claiming installed identity, permissions, MIME registration, or launch behavior. If installing into the user's Flatpak state is outside the task's authority, omit both installed-state commands and report build-only evidence.
- Run `make meson-test` for the Meson-registered AppStream and desktop-file checks. The target requires both validator programs, reconfigures Meson, asserts both expected test names exist, and fails instead of treating an absent validator or test as proof. Use the release helper's direct validators as the additional release-readiness surface.
- Run release helper self-tests and Flathub-manifest tests when their generators or release flows change.
- Do not claim launch, identity, permission, or store readiness without the corresponding current-tree evidence.

## Release safety

Use the repository release helpers and the `publish-release` skill for an actual release. Start
with a dry run and an explicit notes file. Never invent a version, date, tag, commit, screenshot,
remote, or release body. Use immutable tag-plus-commit sources for published manifests and verify
the exact generated artifact. Treat Cominotti repository publication and optional Flathub handoff
as distinct validation surfaces.

## Review checklist

- App ID, command, runtime, SDK extension, Meson profile, installed filenames, and exported identity agree.
- The desktop entry, metainfo, icons, GSettings schema, and resources are installed by the current Meson graph.
- Manifest sources are offline-complete and generated sources match `Cargo.lock`.
- Full host access remains explicit, tested, and accurately described as a Flathub review risk.
- AppStream release metadata and screenshots are current files/URLs, not placeholders.
- Local-source manifests are not confused with immutable publication manifests.
- Tests report exact commands and failures; unsupported host capabilities are not presented as passes.
