## Context

The completed `add-cominotti-flatpak-repository` change currently stages public Flatpak artifacts under `https://cominotti.dev/flatpak/...`. The user already hosts `cominotti.dev` on Netlify while FastMail serves the domain's DNS records, and now wants the repository to live at `flatpak.cominotti.dev`.

Flatpak repository hosting is static-file hosting with HTTPS, but it is bandwidth-sensitive: installs and updates fetch repository metadata, app objects, and static deltas. GitHub Pages can host static artifacts from GitHub Actions, but its published-site and bandwidth limits make it a weaker default for update distribution. Cloudflare Pages is a better default because static asset requests are free and unlimited on Cloudflare Pages, while the practical limits are per-file size, file count, deploy count, and abuse-policy boundaries.

Current verified hosting facts:

- GitHub Pages custom subdomains use DNS `CNAME` records and can deploy static artifacts from GitHub Actions.
- GitHub Pages sites have a 1 GB published-site limit and a soft 100 GB/month bandwidth limit.
- Cloudflare Pages supports direct-upload deployments from GitHub Actions through Wrangler.
- Cloudflare Pages Free allows 500 builds/deploys per month, 20,000 files per site, 100 custom domains per project, and 25 MiB maximum file size per static asset.
- Cloudflare Pages static asset requests are free and unlimited, provided the requests do not invoke Pages Functions.
- Netlify remains viable for static hosting and atomic deploys, but it is already used for the website and is not the best first choice when bandwidth is the deciding factor.

## Goals / Non-Goals

**Goals:**
- Move the official Cominotti Flatpak repository URLs to `https://flatpak.cominotti.dev/`.
- Use Cloudflare Pages direct upload as the default deployment backend for the static Flatpak repository.
- Keep deployment driven from the existing GitHub release workflow.
- Add explicit preflight checks for Cloudflare Pages static hosting limits before deployment.
- Document Cloudflare R2 as the fallback when Pages static asset size or file count limits are exceeded.
- Provide a maintainer-facing step-by-step manual covering DNS, Cloudflare Pages, GitHub secrets, release validation, public install testing, and fallback decisions.
- Update the completed `add-cominotti-flatpak-repository` artifacts before archive so the final OpenSpec history has one coherent URL layout.

**Non-Goals:**
- Moving `cominotti.dev` itself away from Netlify.
- Moving FastMail nameservers away from FastMail.
- Publishing unsigned Flatpak remotes or public `--no-gpg-verify` instructions.
- Building a custom app store, dynamic download service, or Pages Function.
- Solving future multi-app repository size growth beyond documenting the R2 fallback.

## Decisions

### D1: Use `flatpak.cominotti.dev` as the public Flatpak host

The public layout becomes:

```text
https://flatpak.cominotti.dev/repo/
https://flatpak.cominotti.dev/cominotti.flatpakrepo
https://flatpak.cominotti.dev/lushtext.flatpakref
```

This removes the repo from the website path namespace and lets the Flatpak host move independently later. The `cominotti` remote name, `dev.cominotti.Apps` collection ID, `dev.cominotti.lushtext` app ID, and Flathub runtime repository stay unchanged.

Alternative: keep `https://cominotti.dev/flatpak/...`. That is simpler if the existing Netlify website deploy is the only host, but it couples Flatpak update artifacts to website routing, cache, and deploy decisions.

### D2: Prefer Cloudflare Pages over GitHub Pages and Netlify

Cloudflare Pages is the default backend because static asset requests are free and unlimited, which directly addresses the Flatpak bandwidth concern. The deployment should use Wrangler direct upload from GitHub Actions so the generated `flatpak/` staging directory can be published without storing repository objects in Git.

Alternative: GitHub Pages is simpler inside GitHub, but its site and bandwidth limits make it a poor default for install/update distribution. Netlify is viable and already familiar for `cominotti.dev`, but adding a second Netlify site does not improve the bandwidth story as clearly as Cloudflare Pages.

### D3: Add Pages-limit preflight checks

Before Cloudflare deployment, automation should fail or skip honestly if the generated Flatpak staging directory cannot fit Cloudflare Pages static asset limits:

- no individual static file over 25 MiB;
- no more than 20,000 files on the Free plan;
- no Pages Functions or rewrites required to serve repository objects.

The check should report the largest files and total file count so the maintainer knows whether the correct fallback is shrinking deltas, changing retention, using a paid Pages plan where useful, or moving repository objects to R2.

Alternative: deploy and let Cloudflare reject oversized assets. That gives worse release diagnostics and makes the failure look like an infrastructure mystery instead of a clear packaging-hosting decision.

### D4: Use Cloudflare R2 as the first fallback for oversized repository objects

If Cloudflare Pages' 25 MiB asset limit or file count limit is exceeded, the documented fallback should be Cloudflare R2 behind the same `flatpak.cominotti.dev` hostname. R2 keeps the bandwidth/cost story closer to Cloudflare's strengths and avoids GitHub Pages' bandwidth ceiling.

Alternative: fall back to Netlify or GitHub Pages. Those remain acceptable only for small/low-traffic artifacts or temporary staging, not as the preferred public update channel when bandwidth is crucial.

### D5: Manual setup guide is a required deliverable

The implementation must end with a step-by-step manual for the human-only setup pieces:

1. Create or confirm the Cloudflare account and Pages project.
2. Add `flatpak.cominotti.dev` as the Pages custom domain.
3. In FastMail DNS, replace any existing `flatpak` A/AAAA/CNAME records with the Cloudflare-provided CNAME target.
4. Create a Cloudflare API token with the minimum Pages deployment scope.
5. Add GitHub Actions secrets and variables for Cloudflare deployment and Flatpak signing.
6. Run metadata-only verification.
7. Run a real release or staged release deployment.
8. Verify HTTPS, `.flatpakrepo`, `.flatpakref`, `flatpak remote-ls`, and `flatpak install`.
9. Use the R2 fallback path if Pages limits are exceeded.

This manual belongs in project documentation, not only in OpenSpec, so it remains available during the real release.

## Risks / Trade-offs

- [Cloudflare Pages 25 MiB asset limit blocks a Flatpak object or delta] -> Add preflight checks and document R2 as the immediate fallback.
- [Cloudflare Pages file-count limit blocks a growing repository] -> Check file count, keep pruning policy visible, and move to R2 when the repository outgrows Pages.
- [DNS record conflict at FastMail] -> Manual must say to remove existing `flatpak` A/AAAA records before adding the Pages CNAME.
- [Pages deploy succeeds but Flatpak metadata is stale] -> Verify the public URLs and run `flatpak remote-ls` or an install smoke after publication.
- [Cloudflare API token is too broad] -> Manual should request only the minimum Pages edit/deploy permissions needed by Wrangler.
- [Cloudflare account policy or abuse protection objects to package distribution] -> Keep Netlify/R2/object-storage alternatives documented and preserve provider-neutral script environment overrides.

## Migration Plan

1. Update generator/verifier defaults from `https://cominotti.dev/flatpak` to `https://flatpak.cominotti.dev`.
2. Add Cloudflare Pages staging-limit checks to the verifier or a dedicated script.
3. Update release CI to deploy the generated `flatpak/` directory to Cloudflare Pages when Cloudflare credentials and project config are present.
4. Keep provider-neutral `COMINOTTI_FLATPAK_DEPLOY_COMMAND` support as an escape hatch.
5. Update docs, release dry-run output, and tests for the new public URLs.
6. Update the `add-cominotti-flatpak-repository` proposal/design/spec/tasks wording before archive so it no longer points at `cominotti.dev/flatpak/...`.
7. Publish the manual setup guide and verify every manual step is testable by a maintainer.

Rollback is DNS-first: keep generated artifacts provider-neutral, point `flatpak.cominotti.dev` to the previous working backend if Cloudflare Pages fails, and avoid changing the `cominotti` remote name or collection ID.

## Resolved Defaults

- Cloudflare Pages project name: `cominotti-flatpak`, overridable through `COMINOTTI_FLATPAK_CLOUDFLARE_PAGES_PROJECT`.
- First public deployment path: Cloudflare Pages only, with Cloudflare R2 documented as the first fallback when Pages limits are exceeded.
- Public host contents: Flatpak metadata and repository files only. A landing page can be added later, but it is not required for the repository to function.
