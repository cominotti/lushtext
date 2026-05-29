## 1. Release Command Surface

- [x] 1.1 Add `scripts/release.sh` for LushText by adapting Invowk's release helper shape to this Rust/Flatpak repository.
- [x] 1.2 Implement semver validation for `vMAJOR.MINOR.PATCH` plus optional prerelease suffixes.
- [x] 1.3 Implement stable `major`, `minor`, and `patch` bump computation from existing stable Git tags.
- [x] 1.4 Implement prerelease stream computation for `alpha`, `beta`, and `rc` labels.
- [x] 1.5 Implement the prerelease-to-stable promotion guard requiring explicit `PROMOTE=1`.
- [x] 1.6 Implement `DRY_RUN=1` output that reports the computed version, intended file changes, validations, commit, tag, and push targets without mutating the repo.
- [x] 1.7 Add `make release VERSION=vX.Y.Z` and `make release-bump TYPE=major|minor|patch` targets plus help text and environment-variable documentation.
- [x] 1.8 Add shell tests for version validation, stable bumping, prerelease bumping, promotion guard behavior, and dry-run no-mutation behavior.

## 2. Version And AppStream Synchronization

- [x] 2.1 Update the release helper to prepare `meson.build` for the target release version.
- [x] 2.2 Update the release helper to prepare `crates/lushtext/Cargo.toml` and `crates/lushtext-core/Cargo.toml` for the target release version.
- [x] 2.3 Update `Cargo.lock` after package version changes and verify the lockfile records the target package versions.
- [x] 2.4 Add deterministic release-notes input handling, such as `RELEASE_NOTES_FILE`, and fail real releases when notes are missing.
- [x] 2.5 Insert a new AppStream `<release>` entry with version, current date, and release-note description in `data/dev.cominotti.lushtext.metainfo.xml.in`.
- [x] 2.6 Verify after preparation that all Meson, Cargo, Cargo.lock, and AppStream version surfaces match the requested release.
- [x] 2.7 Add tests for version-surface synchronization, missing release notes, and AppStream release insertion.

## 3. Release Safety And Validation Gates

- [x] 3.1 Make real release commands require branch `main`, a clean working tree before mutation, a reachable `origin`, and a non-existing target tag.
- [x] 3.2 Make the release helper stage only the intended generated release files and fail if unrelated files would be committed.
- [x] 3.3 Regenerate or verify `build-aux/cargo-sources.json` against `Cargo.lock` during release preparation.
- [x] 3.4 Add AppStream validation for release metadata using the strongest local or Flatpak-builder lint path available.
- [x] 3.5 Add generated desktop-entry validation so `data/dev.cominotti.lushtext.desktop.in` changes cannot break release packaging.
- [x] 3.6 Add a release Flatpak build validation path that builds from release inputs before the release is considered publishable.
- [x] 3.7 Create the release commit with a clear `chore(release): vX.Y.Z` message before creating the signed tag.
- [x] 3.8 Create and push the signed tag only after all release validation gates pass.

## 4. Flathub Manifest Update Path

- [x] 4.1 Add a Flathub-facing manifest template or generator that references a public GitHub release tag/archive and commit instead of the local `type: "dir"` checkout source.
- [x] 4.2 Preserve the local development manifest at `build-aux/dev.cominotti.lushtext.Flatpak.json` for checkout builds.
- [x] 4.3 Add checks that the Flathub-facing manifest preserves app ID, command, runtime, SDK, Rust SDK extension, Meson release profile, finish arguments, cleanup, and Cargo vendored sources from the reviewed Flatpak contract.
- [x] 4.4 Include current `build-aux/cargo-sources.json` content in the Flathub update artifact or Flathub PR branch.
- [x] 4.5 Add tests for Flathub manifest generation, including a guard that rejects `type: "dir"` in Flathub publication output.

## 5. Flathub Pull Request Automation

- [x] 5.1 Add a GitHub Actions release workflow that runs on `v*` tags and supports manual dry runs.
- [x] 5.2 In the release workflow, validate AppStream metadata, desktop metadata, vendored Cargo sources, and the Flatpak build for the tagged source.
- [x] 5.3 Create or update the GitHub Release context for the tag without breaking the existing release benchmark report upload workflow.
- [x] 5.4 Add a Flathub PR step that opens or updates a branch in the Flathub manifest repository when the required token and repository settings are configured.
- [x] 5.5 Include the release tag, source commit, validation summary, and manual test instructions in the Flathub PR body.
- [x] 5.6 Make missing Flathub token or repository configuration fail or skip with an explicit message that publication is not complete.
- [x] 5.7 Keep Flathub automerge disabled by default and document that enabling it requires a later explicit policy change.
- [x] 5.8 Add a CI release dry-run job for pull requests that touch release scripts, Flatpak manifests, AppStream metadata, desktop metadata, or cargo vendoring.

## 6. Domain Verification Guidance

- [x] 6.1 Add a `scripts/verify-flathub-domain.sh` helper that checks HTTPS validity for `cominotti.dev` and the well-known verification URL.
- [x] 6.2 Let the helper accept an expected Flathub token and verify that it appears as a non-comment line in `https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt`.
- [x] 6.3 Add `make verify-flathub-domain` with usage documentation for token and no-token checks.
- [x] 6.4 Document that linked GitHub accounts do not verify the custom-domain app ID `dev.cominotti.lushtext`.
- [x] 6.5 Document the exact Flathub Developer Portal sequence for obtaining and publishing the verification token.

## 7. Documentation And Agent Guidance

- [x] 7.1 Update `README.md` with the release command workflow, release-notes input, Flathub PR flow, and domain verification steps.
- [x] 7.2 Update `docs/next/flatpak-packaging.md` with the Flathub publication model, local-vs-Flathub manifest distinction, and verification prerequisites.
- [x] 7.3 Update `.agents/rules/build.md` with release automation, Flathub manifest generation, cargo-sources validation, and CI release dry-run rules.
- [x] 7.4 Update root `AGENTS.md` with release command and Flathub publication guidance, keeping the rules index synchronized if rule files change materially.
- [x] 7.5 Ensure docs continue to describe the active Snap packaging work as separate from the Flathub release lane.

## 8. Final Verification

- [x] 8.1 Run the release helper shell tests.
- [x] 8.2 Run `make release-bump TYPE=patch DRY_RUN=1` against representative tags and confirm it leaves the working tree unchanged.
- [x] 8.3 Run AppStream and desktop metadata validation through the same commands used by the release path.
- [x] 8.4 Run `make cargo-sources` or the release-path cargo-source check and verify no stale vendoring diff remains.
- [x] 8.5 Run the Flatpak build validation path.
- [x] 8.6 Run `make check`.
- [x] 8.7 Run `openspec validate automate-flathub-releases --strict`.
- [x] 8.8 Run `openspec validate --changes --strict` and `openspec validate --specs --strict`.
