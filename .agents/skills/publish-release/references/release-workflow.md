# Release Workflow

Use this as the release runbook. Prefer dry runs and read-only inspection until the version, notes, and release scope are clear.

## Preflight

1. Confirm branch, cleanliness, remotes, and tags:

```bash
git fetch --tags --prune origin
git status --short --branch
git tag --list 'v*' --sort=-v:refname | head -10
```

For a strict read-only planning pass, use `git ls-remote --tags origin 'v*'` instead of mutating local refs with `git fetch`. Before a real release, fetch tags locally.

2. If the default Git SSH path falls into askpass trouble, use the known fallback for fetch or push:

```bash
GIT_SSH_COMMAND='ssh -F /dev/null -i ~/.ssh/id_cominotti_code_signing -o IdentitiesOnly=yes -o HostName=ssh.github.com -o Port=443 -o BatchMode=yes' git fetch --tags --prune origin
```

3. Confirm tool availability before a real release:

```bash
command -v gh jq appstreamcli desktop-file-validate flatpak-builder flatpak-cargo-generator
```

The helper also needs Cargo/Rust, Git signing, and reachable `origin`.

## Choose Version

- Use explicit versions when the user gives one: `VERSION=vX.Y.Z`.
- Use `make release-bump TYPE=patch DRY_RUN=1` to preview the next stable patch.
- If no stable tags exist, prefer an explicit `VERSION=` over `release-bump`; the bump helper starts from `v0.0.0` and may not match the package version already in the source tree.
- Use `PRERELEASE=alpha|beta|rc` for prereleases.
- Use `PROMOTE=1` when stable release promotion follows existing prerelease tags for that target.
- Never guess through an invalid semver or existing tag; stop and report the blocker.

## Build Release Context

Gather the raw context first:

```bash
.agents/skills/publish-release/scripts/collect-release-context.sh > /tmp/lushtext-release-context.md
```

If the base tag is unusual, pass it explicitly:

```bash
.agents/skills/publish-release/scripts/collect-release-context.sh v0.1.0 HEAD > /tmp/lushtext-release-context.md
```

If there is no previous `v*` tag, stop and clarify the baseline. Do not invent a last release from AppStream metadata or package versions. For the first public release, either confirm that no prior-release diff exists or ask the user for the commit/tag that represents the previous shipped code.

Then do semantic analysis:

- Inspect changed files and meaningful hunks, not just commit subjects.
- Classify user-visible features, behavior changes, fixes, packaging changes, data/schema changes, performance changes, accessibility changes, and risk.
- Check AppStream, Flatpak, desktop, GSettings, migrations, file I/O, draft/session persistence, and user data paths for manual actions or warnings.
- Use subagents when available:
  - Semantic diff pass: ask for user-visible changes, behavior changes, risks, manual actions, and bug fixes since the previous tag.
  - Release packaging pass: ask for LushText-specific release blockers, Cominotti Flatpak repository/GitHub failure modes, optional Flathub handoff risks, and rollback concerns.

## Draft Notes

Follow [release-notes.md](release-notes.md). Keep the notes file outside the repo:

```bash
NOTES=/tmp/lushtext-release-vX.Y.Z.md
```

Validate required sections and local stanza reuse:

```bash
.agents/skills/publish-release/scripts/validate-release-notes.py "$NOTES"
```

Double-check GitHub Release bodies when `gh` is available:

```bash
.agents/skills/publish-release/scripts/validate-release-notes.py "$NOTES" --gh-repo cominotti/lushtext
```

If this fails because GitHub is unreachable, do not treat stanza uniqueness as confirmed. Either restore access and rerun or clearly ask the user whether to proceed with only local evidence.

## Dry Run

Run a no-mutation release preview before any real release:

```bash
RELEASE_NOTES_FILE="$NOTES" make release VERSION=vX.Y.Z DRY_RUN=1
```

or:

```bash
RELEASE_NOTES_FILE="$NOTES" make release-bump TYPE=patch DRY_RUN=1
```

The dry run does not prove the worktree is clean, because the real release checks that later. Verify cleanliness separately before the real run.

## Real Release

Real releases must run from clean `main`. Use `YES=1` only after the user has clearly asked to proceed or confirmed the release.

```bash
RELEASE_NOTES_FILE="$NOTES" make release VERSION=vX.Y.Z YES=1
```

The helper should:

- update `meson.build`;
- update `crates/lushtext/Cargo.toml`;
- update `crates/lushtext-core/Cargo.toml`;
- refresh `Cargo.lock`;
- insert AppStream release notes in `data/dev.cominotti.lushtext.metainfo.xml.in`;
- regenerate `build-aux/cargo-sources.json`;
- validate version surfaces, vendored sources, AppStream, desktop metadata, and Flatpak build;
- commit `chore(release): vX.Y.Z`;
- create a signed `vX.Y.Z` tag;
- push `main` and the tag to `origin`.

## Monitor Publication

After the tag push, monitor the release workflow:

```bash
gh run list --workflow release.yml --limit 5
gh run watch <run-id> --exit-status
```

If the GitHub Release exists, replace generated notes with the authored notes:

```bash
gh release edit vX.Y.Z --title "LushText vX.Y.Z" --notes-file "$NOTES"
```

If it does not exist after the workflow is complete:

```bash
gh release create vX.Y.Z --verify-tag --title "LushText vX.Y.Z" --notes-file "$NOTES"
```

For prerelease tags, mark the GitHub Release as prerelease:

```bash
gh release edit vX.Y.Z-alpha.1 --prerelease --latest=false
```

Stable releases can let GitHub choose `latest` automatically unless the user asks otherwise.

## Cominotti Flatpak Publication

The release workflow treats the Cominotti Flatpak repository at `https://flatpak.cominotti.dev/` as the primary Flatpak channel. When `COMINOTTI_FLATPAK_PRIVATE_KEY_B64`, `COMINOTTI_FLATPAK_PUBLIC_KEY_B64`, and `COMINOTTI_FLATPAK_GPG_KEY` are configured, it generates and verifies the signed repository artifact for the release tag.

The default hosted backend is Cloudflare Pages direct upload. Configure `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, and optionally `COMINOTTI_FLATPAK_CLOUDFLARE_PAGES_PROJECT` (default `cominotti-sw-flatpak`) for automatic deployment. If `COMINOTTI_FLATPAK_DEPLOY_COMMAND` is configured, the workflow uses that command instead with `COMINOTTI_FLATPAK_STAGING_DIR` pointing at the generated staging directory. If deploy config is missing, report the uploaded `cominotti-flatpak-repository` artifact as the manual publication handoff.

Local generation uses the public Git tag and commit, produces signed remote descriptors, and should never ask users to install with `--no-gpg-verify`:

```bash
make cominotti-flatpak-repo VERSION=vX.Y.Z COMINOTTI_FLATPAK_PUBLIC_KEY=/path/to/public.asc COMINOTTI_FLATPAK_GPG_KEY=<fingerprint-or-keyid>
make verify-cominotti-flatpak-repo
make verify-cominotti-pages-limits
```

For CI-style metadata checks without building the full repository, use:

```bash
COMINOTTI_FLATPAK_SKIP_BUILD=1 make cominotti-flatpak-repo VERSION=vX.Y.Z COMINOTTI_FLATPAK_PUBLIC_KEY=/path/to/public.asc
make verify-cominotti-flatpak-repo
make verify-cominotti-pages-limits
```

If the Pages-limit check fails, use Cloudflare R2 behind `flatpak.cominotti.dev` before falling back to GitHub Pages or Netlify.

## Optional Flathub Handoff

The release workflow opens a Flathub PR only when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured. If it skips optional Flathub publication, use the uploaded `flathub-update` artifact or regenerate locally:

```bash
make flathub-manifest VERSION=vX.Y.Z
make verify-flathub-manifest
```

The Flathub manifest must use the public Git tag and commit, include `cargo-sources.json`, avoid local `type: "dir"` sources, and set `CARGO_NET_OFFLINE=true`.

Flathub publication is intentionally reviewable by default. Do not enable `flathub.json` automerge unless the user explicitly changes that policy.

## Final Verification

Report:

- release commit SHA and tag;
- GitHub Release URL or exact blocker;
- release workflow result;
- Cominotti Flatpak repository artifact/deploy result or exact skipped/manual action;
- optional Flathub PR URL or exact skipped/manual action;
- any AppStream, Flatpak, or packaging caveats;
- any user-facing manual actions, warnings, deprecations, or rollback notes.
