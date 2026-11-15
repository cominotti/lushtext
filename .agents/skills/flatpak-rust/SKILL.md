---
name: flatpak-rust
description: "LushText packaging: the Flatpak manifest, Meson install graph, desktop entry, AppStream metainfo, icons, cargo-sources.json vendoring and packaging CI. Use for changes under build-aux/ or data/, meson files, Flatpak permissions, or a Cargo dependency change that needs make cargo-sources."
---

# Flatpak and Meson packaging

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
- [Dormant Flathub tooling](references/flathub-review.md) only when the retained Flathub generators or their tests change.

## Preserve LushText's filesystem contract

LushText intentionally uses full host filesystem access because workspaces, file monitoring,
sidecars, search, rename/delete, and durable writes operate on arbitrary local paths. Preserve
the manifest's `--filesystem=host` permission and run `make check-flatpak-permissions` whenever
packaging or sandbox policy changes.

Do not describe this permission as universally recommended. It is a deliberate product
tradeoff with a large sandbox surface. A portals-only or narrower-permission migration is a
separate product/architecture change requiring explicit authorization, end-to-end behavior
coverage, and updates to the repository's permission policy.

## Dependency changes

Run `make cargo-sources` after any dependency, feature, source, or `Cargo.lock` change that
affects the build, and include the regenerated `build-aux/cargo-sources.json` in the same change
(`.agents/rules/build.md`, "Adding Dependencies" and "Flatpak"). Do not install a generator ad
hoc or invoke a copied command when the repository target is available.

## Proportional validation

Run the narrow checks first, then the build proof appropriate to the change:

```bash
make check-flatpak-permissions
make test-flatpak-install-origin
make check-agent-docs
make meson-build
make meson-test
make flatpak
make flatpak-install
make verify-flatpak-identity
git diff --check
```

- Run `make cargo-sources` before the build when dependency inputs changed.
- `make flatpak` is build-only; install immediately before `make verify-flatpak-identity` when claiming installed state (`build.md`). If installing into the user's Flatpak state is outside the task's authority, omit both installed-state commands and report build-only evidence.
- Run `make test-flatpak-install-origin` whenever `flatpak-install`, `scripts/flatpak-install-origin.sh`, or the flatpak/flatpak-builder version changes. It proves the install-origin guard (`flatpak-install-origin.sh check`, the first section of `make verify-flatpak-identity`) against real `flatpak` in a private `FLATPAK_USER_DIR`; a failure there means flatpak changed how it names, reuses or prunes origin remotes.
- Run `make meson-test` for the Meson-registered AppStream and desktop-file checks. The target requires both validator programs, reconfigures Meson, asserts both expected test names exist, and fails instead of treating an absent validator or test as proof. Use the release helper's direct validators as the additional release-readiness surface.
- Run release helper self-tests, and `scripts/test-flathub-manifest.sh` for the dormant Flathub generators, when those generators or release flows change.
- Do not claim launch, identity, permission, or store readiness without the corresponding current-tree evidence.

## Release safety

Use the repository release helpers and the `publish-release` skill for an actual release. Start
with a dry run and an explicit notes file. Never invent a version, date, tag, commit, screenshot,
remote, or release body. Use immutable tag-plus-commit sources for published manifests and verify
the exact generated artifact. The Cominotti Flatpak repository is the only publication channel;
Flathub publication is not planned (`build.md`, "Flatpak Release Automation").

## Review checklist

- App ID, command, runtime, SDK extension, Meson profile, installed filenames, and exported identity agree.
- The desktop entry, metainfo, icons, GSettings schema, and resources are installed by the current Meson graph.
- Manifest sources are offline-complete and generated sources match `Cargo.lock`.
- Full host access remains explicit, tested, and accurately described as a deliberate sandbox tradeoff.
- AppStream release metadata and screenshots are current files/URLs, not placeholders.
- CI Flatpak builds go through `.github/actions/flatpak-build` after checkout, never `flatpak-builder@` directly, so the image's system installation is relocated off overlayfs before a dependency update (`make check-flatpak-ci-builds`; `build.md`, `flatpak.yml`).
- Local-source manifests are not confused with immutable publication manifests.
- The install origin stays on the single shared path (`flatpak-install-origin.sh path`): `flatpak-install` exports with `--repo` to it and never with `--install` or a per-checkout, cache or state-dir repository, which leaves `lushtext-origin` dangling once that checkout or cache is deleted (`build.md`, "Flatpak").
- Tests report exact commands and failures; unsupported host capabilities are not presented as passes.
