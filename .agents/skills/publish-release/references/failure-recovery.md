# Failure Recovery

When something fails, first identify which public surfaces already changed: local files, local commit, local tag, pushed `main`, pushed tag, GitHub Release, Cominotti Flatpak repository artifact/deploy, Flathub PR, or merged Flathub update.

## Before Public Push

Dirty worktree, wrong branch, invalid semver, existing tag, missing tools, empty notes, stale `cargo-sources.json`, AppStream validation failure, desktop validation failure, or Flatpak build failure usually happens before public publication.

Recovery:

- Fix the underlying blocker.
- Rerun the dry run.
- If release-generated files were changed and the release is abandoned, revert only the known release files:
  - `meson.build`
  - `crates/lushtext/Cargo.toml`
  - `crates/lushtext-core/Cargo.toml`
  - `Cargo.lock`
  - `data/dev.cominotti.lushtext.metainfo.xml.in`
  - `build-aux/cargo-sources.json`
- Do not use broad resets or broad staging without explicit user approval.

## After Local Commit Or Tag

If the release commit exists locally but nothing was pushed:

- Prefer creating a corrective signed commit if the user still wants the release.
- Delete or recreate a local-only tag only after confirming it was not pushed.
- Do not rewrite history unless the user explicitly asks for it.

If `main` pushed but the tag did not:

- Retry the tag push after fixing the transport/signing issue.
- If abandoning the release, create a signed revert commit on `main`; do not force-push `main`.

If the tag pushed but the release workflow failed:

- Rerun the workflow if the tagged source is correct and the failure was transient or infrastructure-related.
- If the fix is in release tooling or workflow scripts after the public tag, keep the tag immutable and rerun from the workflow recovery ref while still resolving the released source from the public tag and commit.
- If source code or release metadata is wrong, publish a new patch release rather than moving the public tag.
- Only delete or replace a pushed tag with explicit maintainer approval and a clear downstream-impact note.

## GitHub Actions Problems

Any GitHub Actions run for the release commit, release tag, or recovery commit with conclusion other than `success` is a release blocker. This includes `failure`, `cancelled`, `timed_out`, `action_required`, `stale`, and required workflow runs that are skipped or expected but missing.

Recovery:

- Inspect the failed or cancelled run's jobs, steps, and logs before summarizing the release state.
- Fix the underlying cause, not only the symptom.
- Do not raise any job timeout above 30 minutes. If a job times out, reduce scope, fix the benchmark or workflow harness, split the work into bounded jobs, or dispatch a bounded replacement workflow.
- If the public tag is already pushed and the fix belongs to release tooling or workflow configuration, keep the tag immutable, commit the workflow/tooling fix on `main`, and dispatch the repaired workflow with the public tag as input when the workflow supports it.
- If the tagged source or release metadata itself is wrong, publish a new patch release rather than moving the public tag.
- Repeat the exact-SHA and tag-branch workflow sweep until every current workflow responsibility completes with conclusion `success` or has a successful replacement run for the same responsibility. If recovery succeeds through a replacement run, report the failed or cancelled run ID together with the replacement run ID; if recovery is blocked, report that the release is not fully green and name the external blocker.

## GitHub Release Problems

If the GitHub Release already exists:

```bash
gh release view "$VERSION"
gh release edit "$VERSION" --title "LushText $VERSION" --notes-file "$NOTES"
```

If generated notes replaced authored notes, edit the release body with the authored notes.

If a prerelease was created as a stable release, mark it:

```bash
gh release edit "$VERSION" --prerelease --latest=false
```

If a bad release is already public, prefer a new fixed release and a clear warning in the old release notes. Do not silently delete public context.

## Cominotti Flatpak Problems

If the workflow skipped Cominotti repository generation, check whether `COMINOTTI_FLATPAK_PRIVATE_KEY_B64`, `COMINOTTI_FLATPAK_PUBLIC_KEY_B64`, and `COMINOTTI_FLATPAK_GPG_KEY` are configured. Missing signing material is a skipped publication step, not a completed Flatpak publish.

If Cominotti repository generation succeeds but deploy is skipped, check whether Cloudflare Pages settings are configured: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, and `COMINOTTI_FLATPAK_CLOUDFLARE_PAGES_PROJECT` if using a non-default project name. If `COMINOTTI_FLATPAK_DEPLOY_COMMAND` is configured, check that override instead. Use the uploaded `cominotti-flatpak-repository` artifact as the manual deployment handoff.

If generated repository verification fails:

- regenerate with the intended tag and commit;
- verify `cominotti.flatpakrepo`, `lushtext.flatpakref`, and the release manifest all use the signed Cominotti remote metadata;
- if `flatpak-builder` cannot install runtime dependencies because the dependency remote is missing, make the generator configure the user-level runtime remote before rerunning CI;
- verify public install instructions do not use `--no-gpg-verify`;
- rerun `make verify-cominotti-flatpak-repo`.
- rerun `make verify-cominotti-pages-limits` before deploying to Cloudflare Pages.

If Cloudflare Pages limit verification fails:

- inspect the largest-file and file-count report;
- prefer Cloudflare R2 behind `flatpak.cominotti.dev` before GitHub Pages or Netlify;
- keep the `cominotti` remote name and repository signing key unchanged when moving the backend.

If a deployed Cominotti repository is broken, prefer publishing a new fixed release and repository summary rather than rewriting the public release tag. If the bad repository must be withdrawn, publish an explicit user-facing warning in the GitHub Release notes and preserve enough artifact context to recover the previous state.

## Optional Flathub Problems

If the workflow skipped the optional Flathub PR, check whether `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured. Then either rerun the workflow or create/update the PR manually from the generated artifact.

If generated manifest verification fails:

- regenerate with the intended tag and commit;
- verify the manifest has a public Git source, no `type: "dir"` source, `cargo-sources.json`, and `CARGO_NET_OFFLINE=true`;
- rerun `make verify-flathub-manifest`.

If the Flathub PR branch is wrong:

- update only the Flathub PR branch;
- use `--force-with-lease` only for that PR branch, never for `main`;
- leave a clear PR comment explaining the correction.

If the Flathub PR was merged and the release is broken:

- submit a new fixed release PR when a code fix exists;
- submit a revert PR to the previous known-good manifest only if the current release should be withdrawn from Flathub;
- update the GitHub Release notes with a warning and the replacement path.

If domain verification fails for `dev.cominotti.lushtext`:

- publish the exact Flathub token at `https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt`;
- verify TLS and caching;
- rerun `make verify-flathub-domain FLATHUB_VERIFICATION_TOKEN=<token>`.

## Transport And Signing

If Git SSH falls into askpass locally, retry with the known `ssh.github.com:443` signing-key fallback instead of looping on the broken default path.

If commit or tag signing fails, stop. Fix signing locally before retrying; do not use unsigned release commits or tags.
