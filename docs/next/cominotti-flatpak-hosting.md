# Cominotti Flatpak Hosting Manual

This manual sets up the official Cominotti Flatpak repository at
`https://flatpak.cominotti.dev/`. LushText is the first app in the shared
`cominotti` remote.

## Target Layout

The host must serve these static files over HTTPS:

```text
https://flatpak.cominotti.dev/repo/
https://flatpak.cominotti.dev/cominotti.flatpakrepo
https://flatpak.cominotti.dev/lushtext.flatpakref
```

The default backend is Cloudflare Pages because Pages static asset requests are
free and unlimited when they do not invoke Pages Functions. The repository must
remain plain static files; do not add Pages Functions, redirects, or rewrites to
serve Flatpak objects.

## Cloudflare Pages Setup

1. Sign in to Cloudflare and open **Workers & Pages**.
2. Create or confirm a Pages project named `cominotti-sw-flatpak`.
3. Use direct upload, not a Git-connected website build. Release CI uploads the
   generated `flatpak/` directory with Wrangler.
4. Add `flatpak.cominotti.dev` under the Pages project's **Custom domains**.
5. Copy the CNAME target Cloudflare gives for the custom domain. It is usually a
   `*.pages.dev` hostname for the Pages project.

## FastMail DNS Setup

FastMail is authoritative for `cominotti.dev`, so DNS changes happen there.

1. Open FastMail **Settings > Domains > cominotti.dev > Edit DNS records**.
2. Remove any existing `flatpak` A, AAAA, or CNAME records. A stale A/AAAA
   record will conflict with the Cloudflare Pages CNAME.
3. Add a CNAME record:
   - Name/Host: `flatpak`
   - Value/Target: the Cloudflare Pages CNAME target from the custom-domain
     setup, for example `cominotti-sw-flatpak.pages.dev`
4. Save the DNS record.
5. Wait for DNS propagation. Cloudflare may take additional time to provision
   HTTPS for the custom domain after the CNAME resolves.

## GitHub Actions Configuration

Add these repository secrets:

```text
COMINOTTI_FLATPAK_PRIVATE_KEY_B64
COMINOTTI_FLATPAK_PUBLIC_KEY_B64
COMINOTTI_FLATPAK_GPG_KEY
CLOUDFLARE_API_TOKEN
CLOUDFLARE_ACCOUNT_ID
```

`COMINOTTI_FLATPAK_PRIVATE_KEY_B64` is the base64-encoded private signing key.
`COMINOTTI_FLATPAK_PUBLIC_KEY_B64` is the base64-encoded public key. Keep the
private key outside the repository and rotate it if it is ever exposed.

Create the Cloudflare token as a custom API token with the minimum Pages
deployment scope available in the Cloudflare dashboard: Account-level
Cloudflare Pages edit/deploy access for the account that owns the
`cominotti-sw-flatpak` project.

Optional repository variables:

```text
COMINOTTI_FLATPAK_CLOUDFLARE_PAGES_PROJECT=cominotti-sw-flatpak
COMINOTTI_FLATPAK_DEPLOY_COMMAND
```

Leave `COMINOTTI_FLATPAK_DEPLOY_COMMAND` unset for the normal Cloudflare Pages
path. If it is set, release CI treats it as an emergency custom deployment
override and skips the default Cloudflare Pages deploy.

## Preflight Locally

Generate metadata-only artifacts first:

```bash
printf 'test public key\n' >/tmp/cominotti-flatpak-public.gpg
COMINOTTI_FLATPAK_SKIP_BUILD=1 \
COMINOTTI_FLATPAK_PUBLIC_KEY=/tmp/cominotti-flatpak-public.gpg \
  make cominotti-flatpak-repo VERSION=v0.0.1
make verify-cominotti-flatpak-repo
make verify-cominotti-pages-limits
```

For a real signed repository, use the real public key and signing key:

```bash
make cominotti-flatpak-repo VERSION=vX.Y.Z \
  COMINOTTI_FLATPAK_PUBLIC_KEY=/path/to/cominotti-flatpak-public.gpg \
  COMINOTTI_FLATPAK_GPG_KEY=<key-id>
make verify-cominotti-flatpak-repo
make verify-cominotti-pages-limits
```

Cloudflare Pages limits that matter here:

- 25 MiB maximum per static asset.
- 20,000 files per deployment on the Free plan when using Wrangler direct upload.
- 500 deploys per month on the Free plan.

`make verify-cominotti-pages-limits` prints the file count and largest assets.
If this check fails, do not deploy to Pages.

## Release Deployment

1. Confirm release notes and run a dry run:

   ```bash
   RELEASE_NOTES_FILE=/tmp/lushtext-release.md make release-bump TYPE=patch DRY_RUN=1
   ```

2. Run the real release from clean `main` only after the notes and version are
   final:

   ```bash
   RELEASE_NOTES_FILE=/tmp/lushtext-release.md make release VERSION=vX.Y.Z YES=1
   ```

3. Watch the release workflow. It should:
   - build the Flatpak;
   - generate and verify the Cominotti repository;
   - verify Cloudflare Pages limits;
   - upload the `cominotti-flatpak-repository` artifact;
   - deploy the artifact to Cloudflare Pages when Cloudflare credentials are set.

4. If the workflow says Cloudflare credentials or project settings are missing,
   add the missing secret or variable and rerun the failed job or workflow.

## Public Verification

After DNS and deployment finish:

```bash
curl -I https://flatpak.cominotti.dev/cominotti.flatpakrepo
curl -I https://flatpak.cominotti.dev/lushtext.flatpakref
curl -I https://flatpak.cominotti.dev/repo/summary
```

Then add the remote and verify Flatpak can see the app:

```bash
flatpak remote-add --user --if-not-exists --from cominotti https://flatpak.cominotti.dev/cominotti.flatpakrepo
flatpak remote-ls --user cominotti --app | grep -x dev.cominotti.lushtext
```

For an install smoke test:

```bash
flatpak install --user cominotti dev.cominotti.lushtext
flatpak run dev.cominotti.lushtext
```

Direct installer reference:

```bash
flatpak install --user https://flatpak.cominotti.dev/lushtext.flatpakref
```

Never publish public `--no-gpg-verify` instructions.

## Fallbacks

Use Cloudflare R2 behind `flatpak.cominotti.dev` if Pages limits block the
repository because an object, delta, or future multi-app repository becomes too
large. R2 is the preferred fallback because it keeps the host on Cloudflare and
fits bandwidth-heavy static object distribution better than GitHub Pages.

GitHub Pages and Netlify are secondary options only for small or temporary
hosting. GitHub Pages has soft bandwidth limits, and Netlify would couple this
repository more closely to a website-hosting workflow.

Rollback is DNS-first: point `flatpak.cominotti.dev` back to the last working
backend, keep the `cominotti` remote name unchanged, and publish a new fixed
release rather than rewriting a public release tag.
