## Context

LushText already has a Meson-driven Flatpak build in this repository:

- `build-aux/dev.cominotti.lushtext.Flatpak.json` builds the current checkout with `type: "dir"`.
- `build-aux/cargo-sources.json` vendors Cargo dependencies for offline Flatpak builds.
- `data/dev.cominotti.lushtext.metainfo.xml.in` contains AppStream metadata and release history.
- `.github/workflows/flatpak.yml` validates the local Flatpak build on pushes and pull requests.

That is enough for local validation, but not enough for release operations. A releasable Flathub update needs coordinated version surfaces, a public immutable source reference, fresh vendored Cargo sources, AppStream release metadata, a signed tag, and a reviewable update in the Flathub manifest repository. The app ID `dev.cominotti.lushtext` is also a custom-domain ID: Flathub verification is tied to `cominotti.dev`, not to the linked GitHub account.

## Goals / Non-Goals

**Goals:**

- Provide Invowk-style `make release` and `make release-bump` commands with semver bump intelligence, dry-run support, confirmation controls, prerelease streams, and promotion safeguards.
- Keep all user-visible and packaging-visible LushText versions synchronized before a release tag is created.
- Require release notes for AppStream release metadata so Flathub users see a meaningful changelog.
- Validate local Flatpak/AppStream/Desktop metadata and vendored Cargo sources before a real tag is pushed.
- Generate or update a Flathub-ready manifest that references the public GitHub release source instead of the local checkout.
- Open a Flathub manifest pull request after a release without defaulting to untested automerge.
- Provide deterministic guidance and checks for the `cominotti.dev` well-known verification token.

**Non-Goals:**

- Do not change LushText's current Flatpak permission posture in this change.
- Do not implement the future portal-first sandbox migration.
- Do not merge Flathub bot PRs automatically by default.
- Do not publish or manage the `cominotti.dev` website from this repository.
- Do not fold the active Snap packaging work into this change.

## Decisions

### Use a local release helper as the source of truth

The Makefile should expose the human-facing commands, but delegate release logic to a script under `scripts/`, following Invowk's pattern. The helper owns semver parsing, tag discovery, prerelease numbering, promotion checks, dirty-worktree checks, version-surface updates, validation, commit creation, signed tag creation, and push behavior.

Alternatives considered:

- **GitHub Actions workflow_dispatch creates the release tag directly**: convenient, but it makes local version edits, AppStream release notes, and signed local confirmation harder to inspect before release.
- **Manual version edits plus a tag-only helper**: close to Invowk mechanically, but too easy for LushText because it has multiple package and store metadata surfaces that must move together.

### Make the release helper commit only generated release files

The helper should require a clean `main` before it starts. After it updates release metadata, it should stage only the intended release files and verify the staged set exactly. This protects the known mixed-worktree pattern in this repository and avoids accidentally publishing scratch artifacts.

The intended release file set is expected to include:

- `meson.build`
- `crates/lushtext/Cargo.toml`
- `crates/lushtext-core/Cargo.toml`
- `Cargo.lock`
- `data/dev.cominotti.lushtext.metainfo.xml.in`
- `build-aux/cargo-sources.json` when regeneration changes it
- any generated Flathub manifest/update artifact if stored in this repository

### Require release notes for AppStream metadata

AppStream releases are part of the store-facing contract, not a cosmetic detail. A real release should fail before commit/tag creation unless release notes are supplied through a deterministic input such as `RELEASE_NOTES_FILE=...` or equivalent. Dry runs may show the missing-notes failure without modifying files.

Alternatives considered:

- **Generate generic "Maintenance release" notes**: cheap, but it degrades the Flathub listing and hides useful release context.
- **Only maintain GitHub Releases**: insufficient because AppStream release metadata is consumed by Flathub/GNOME Software.

### Keep local and Flathub manifests distinct but synchronized

The existing local manifest should stay optimized for validating the checkout with `type: "dir"`. Flathub publication should use a manifest/update artifact that references an immutable GitHub release tag/archive and commit. A helper can generate the Flathub-facing manifest from shared values so permissions, runtime, SDK extension, Meson options, and cargo sources do not drift.

Alternatives considered:

- **Use the local manifest directly in Flathub**: rejected because `type: "dir"` is a local checkout source and not a public release source.
- **Maintain two unrelated JSON files by hand**: rejected because runtime and permission drift would be easy.

### Open a Flathub PR instead of default automerge

The release workflow should create a Flathub manifest update branch and pull request when credentials are configured. The PR body should include the release tag, source commit, validation summary, and local test commands. Automatic Flathub bot merge should remain opt-in and undocumented as the default because a successful Flathub build does not prove the app launches or handles workspace behavior correctly.

### Treat domain verification as an external prerequisite with local checks

The app ID `dev.cominotti.lushtext` maps to the domain `cominotti.dev`. Verification requires a Flathub-generated token at:

```text
https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt
```

The repository can provide docs and a `make verify-flathub-domain` style check that verifies HTTPS, hostname correctness, URL reachability, and optionally token presence. It cannot generate or host the token because the token comes from Flathub after the app exists there.

## Risks / Trade-offs

- [Risk] The `cominotti.dev` HTTPS certificate or hosting is misconfigured when verification is attempted -> Mitigation: provide a local verification command that fails clearly on TLS hostname errors before asking Flathub to verify.
- [Risk] Release automation accidentally stages unrelated files -> Mitigation: require a clean tree before release mutation and check the exact staged file allowlist before committing.
- [Risk] Cargo vendored sources drift from `Cargo.lock` -> Mitigation: regenerate `build-aux/cargo-sources.json`, compare it with the committed file, and run a Flatpak build/lint path before a real tag.
- [Risk] Flathub manifest repository access is unavailable on release day -> Mitigation: tag creation and GitHub release validation remain useful; the workflow should fail or skip the Flathub PR step with a clear credential/repository message rather than pretending publication happened.
- [Risk] A release note input is skipped for speed -> Mitigation: fail closed before any commit/tag operation and document a short release-notes file workflow.
- [Risk] Two manifests drift over time -> Mitigation: introduce a shared generation/verification path and CI checks that compare local and Flathub-facing manifest invariants.

## Migration Plan

1. Add the release helper, Makefile targets, and tests in dry-run mode first.
2. Add version-surface synchronization and AppStream release-note insertion.
3. Add Flatpak/AppStream/Desktop/cargo-source validation gates.
4. Add the Flathub-facing manifest generation/update path.
5. Add GitHub Actions release validation and Flathub PR creation behind explicit secrets.
6. Document the domain-verification workflow and verify `cominotti.dev` HTTPS before attempting Flathub verification.

Rollback is simple before the first real release: remove the new helper/workflow files and continue using the existing local Flatpak build. After a release tag is pushed, rollback follows normal Git release practice: delete an erroneous unpublished tag or cut a corrective patch release if the tag has already been consumed.
