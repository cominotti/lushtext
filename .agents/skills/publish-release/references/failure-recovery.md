# Failure Recovery

When something fails, first identify which public surfaces already changed: local files, local commit, local tag, pushed `main`, pushed tag, GitHub Release, Flathub PR, or merged Flathub update.

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
- If source code or release metadata is wrong, publish a new patch release rather than moving the public tag.
- Only delete or replace a pushed tag with explicit maintainer approval and a clear downstream-impact note.

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

## Flathub Problems

If the workflow skipped the Flathub PR, check whether `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured. Then either rerun the workflow or create/update the PR manually from the generated artifact.

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
