---
name: publish-release
description: Publish a LushText release to GitHub and the Cominotti Flatpak repository. Use when preparing, validating, tagging, pushing, monitoring, repairing, rolling back, or writing release notes for a new LushText version. Covers semantic diff analysis since the last release, warm user-facing release notes with required sections and non-repeated poem stanzas, GitHub Release publication, Cominotti Flatpak repository publication, optional Flathub PR handoff, AppStream metadata, release CI, all release-related GitHub Actions workflows, and recovery from release or packaging failures.
---

# Publish Release

Use this skill for the public LushText release lane. Treat every release as a stateful operation across Git history, GitHub Releases, AppStream metadata, GitHub Actions, the Cominotti Flatpak repository, and optional Flathub handoff.

## Load First

1. Read [references/release-workflow.md](references/release-workflow.md) before running release commands.
2. Read [references/release-notes.md](references/release-notes.md) before drafting notes or choosing a poem stanza.
3. Read [references/failure-recovery.md](references/failure-recovery.md) as soon as any command, CI job, GitHub Actions workflow, GitHub Release, Cominotti Flatpak publication, or optional Flathub PR step fails, is cancelled, times out, or is otherwise not conclusively successful.
4. Use [scripts/collect-release-context.sh](scripts/collect-release-context.sh) to gather raw diff context.
5. Use [scripts/validate-release-notes.py](scripts/validate-release-notes.py) before a real release.

## Non-Negotiables

- Start from the real checkout state. Fetch tags, inspect `git status`, and verify the branch before recommending or running a release.
- Do not rely only on conventional commits. Perform a semantic analysis of the diff between the to-be-released code and the previous release tag.
- Use subagents when available for non-trivial releases: one semantic diff pass and one release/packaging failure-mode pass. Keep their tasks read-only unless the user explicitly asks them to edit.
- Draft release notes in a warm, collaborative tone that explains user-visible changes plainly.
- Include exactly these release-note sections: `Poetic Opening`, `What's Changed`, `Manual Actions Needed`, `Warnings and Deprecations`, and `Bug Fixes`.
- The `Poetic Opening` stanza or verse must come from Rimbaud, Oscar Wilde, Baudelaire, Edgar Allan Poe, Shakespeare, or Florbela Espanca. Use a complete source-checked stanza or verse, never a fragment or a few opening lines. For non-English originals, include both the full original stanza or verse and a full English rendering. Never repeat a stanza across releases; double-check local history and GitHub Release bodies before using it.
- Keep the release notes file outside the repo, such as `/tmp/lushtext-release-vX.Y.Z.md`, unless the user intentionally provides a clean tracked file. A new untracked notes file inside the repo makes the real release helper fail its clean-tree gate.
- Stage only intended release files. Never use `git add .` or `git add -A` during release recovery.
- Do not rewrite public release tags, delete public releases, enable Flathub automerge, or force-push release branches unless the user explicitly approves that exact operation.
- Never report a release or its CI as green until every GitHub Actions workflow run started for the release commit, release tag, and any recovery commits or dispatches has completed with conclusion `success`. Any `failure`, `cancelled`, `timed_out`, `action_required`, `stale`, skipped required workflow, missing expected release workflow, or other non-success conclusion is a release blocker that must be investigated, corrected, rerun, and rechecked.

## Current Local Release Surface

- `make release VERSION=vX.Y.Z RELEASE_NOTES_FILE=/tmp/notes.md [YES=1] [DRY_RUN=1]`
- `make release-bump TYPE=major|minor|patch [PRERELEASE=alpha|beta|rc] [PROMOTE=1] [YES=1] [DRY_RUN=1]`
- `make cominotti-flatpak-repo VERSION=vX.Y.Z COMINOTTI_FLATPAK_PUBLIC_KEY=/path/to/public.asc COMINOTTI_FLATPAK_GPG_KEY=<fingerprint-or-keyid>`
- `make verify-cominotti-flatpak-repo`
- `scripts/release.sh` updates version surfaces, inserts AppStream release notes, validates metadata, builds Flatpak, creates the release commit, creates a signed tag, and pushes `main` plus the tag.
- `.github/workflows/release.yml` validates `v*` tag releases, builds a GNOME 50 Flatpak, creates or updates the GitHub Release, generates and verifies the signed Cominotti Flatpak repository when signing material is configured, verifies Cloudflare Pages static asset limits, deploys to Cloudflare Pages when Cloudflare credentials are configured, supports `COMINOTTI_FLATPAK_DEPLOY_COMMAND` as an override, and opens a Flathub PR only when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured.
- `.github/workflows/release-benchmark.yml` is release-critical: it generates and uploads the benchmark report for release tags. Treat a cancelled, timed-out, failed, or missing benchmark-report run as an incomplete release until repaired and rerun successfully.
- `scripts/generate-cominotti-flatpak-repo.sh`, `scripts/verify-cominotti-flatpak-repo.sh`, `scripts/verify-cominotti-pages-limits.sh`, and `scripts/test-cominotti-flatpak-repo.sh` generate, verify, Pages-preflight, and regression-test the Cominotti-hosted Flatpak repository descriptors and release manifest.
- `scripts/generate-flathub-manifest.sh` and `scripts/verify-flathub-manifest.sh` produce and verify the Flathub-facing manifest with a public Git tag/commit source and `cargo-sources.json`.

## Completion Standard

A release is complete only when:

1. The release notes passed local and GitHub-body uniqueness checks for the poem stanza.
2. The real release command succeeded, creating and pushing the signed release commit and tag.
3. Every GitHub Actions workflow run created for the release commit, release tag, and any recovery commit or recovery dispatch completed with conclusion `success`, including `release.yml`, `release-benchmark.yml`, and normal push CI such as CI, Flatpak, Snap, and Release Dry Run when they run.
4. Any workflow failure, cancellation, timeout, missing expected release workflow, or skipped required workflow was fixed and rerun to success; if external credentials or maintainer-only settings block repair, report the release as not fully green and name the exact blocker.
5. Any skipped publication step inside a successful workflow is explicitly reported with its reason and next manual action.
6. The GitHub Release body contains the authored release notes, not only generated notes.
7. The Cominotti Flatpak repository artifact or deploy result is reported, or the exact reason it was skipped is documented with the next manual action.
8. A Flathub PR exists when optional handoff is configured, or the exact reason it was skipped is documented.
9. Any rollback or follow-up action is concrete and preserves public history unless the user approved a rewrite.
