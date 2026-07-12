# Flathub review and handoff

Use this reference for a new Flathub submission, generated handoff manifest, or permission change.

## Evidence sequence

1. Read current Flathub submission requirements and `docs/next/flatpak-packaging.md`.
2. Inspect the repository's Flathub generator/tests and release workflow; do not create a second manual manifest path.
3. Generate from an immutable release tag and full commit, then run the repository's Flathub-manifest and release-helper tests.
4. Run the current Flathub manifest linter when available and preserve its exact output.
5. Review every permission, source, patch, build option, AppStream field, and exported filename before opening or updating a PR.

## Broad filesystem access

The local and generated LushText manifests intentionally retain `--filesystem=host`. This is
necessary for the current arbitrary-workspace contract but materially weakens sandbox isolation.
Flathub emphasizes minimal permissions and may request justification or architectural changes;
permission changes can also trigger moderation. State the tradeoff plainly. Do not promise
acceptance, silently narrow the permission, or substitute `home` and claim equivalent behavior.

## Handoff rules

- Keep app ID/domain ownership evidence, desktop/metainfo/icon identity, runtime, and SDK aligned.
- Use offline-complete Cargo sources matching the release `Cargo.lock`.
- Use release builds and immutable upstream sources with tag and full commit verification.
- Keep screenshots, releases, content rating, developer metadata, and URLs current and reachable.
- Distinguish a successful local build from Flathub policy acceptance and human review.
- Let the release workflow open/update the optional Flathub PR when configured; never invent credentials or external state.
