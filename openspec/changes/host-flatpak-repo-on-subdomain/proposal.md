## Why

The Cominotti Flatpak repository should have its own stable host name instead of living inside the existing `cominotti.dev` website path. This keeps release artifacts independent from the marketing site, while still preserving the first-party Cominotti trust signal.

## What Changes

- Change the public Flatpak repository layout from `https://cominotti.dev/flatpak/...` to `https://flatpak.cominotti.dev/...`.
- Prefer Cloudflare Pages as the first hosting backend because static asset requests are free and unlimited, which makes bandwidth a first-class fit for Flatpak install and update traffic.
- Add guardrails for Cloudflare Pages' static asset limits, especially per-file size and total file count, and document Cloudflare R2 as the fallback if Flatpak repository objects or deltas exceed Pages limits.
- Keep GitHub Pages and Netlify as secondary fallbacks for small, low-traffic repository hosting only; GitHub Pages is not preferred because its soft bandwidth limits make it a weaker fit for update distribution.
- Update release automation, verification defaults, documentation, and dry-run output to use the subdomain URLs.
- Add a maintainer-facing step-by-step manual for DNS, Cloudflare Pages configuration, release secrets, deployment verification, and fallback recovery.

## Capabilities

### New Capabilities
- `flatpak-repository-subdomain-hosting`: Covers the public `flatpak.cominotti.dev` URL layout, Cloudflare Pages deployment backend, DNS expectations, and required manual setup guide.

### Modified Capabilities
None. The implementation will update the still-active `add-cominotti-flatpak-repository` change artifacts before archive so the two changes describe one final public URL layout.

## Impact

- Affected scripts: `scripts/generate-cominotti-flatpak-repo.sh`, `scripts/verify-cominotti-flatpak-repo.sh`, `scripts/test-cominotti-flatpak-repo.sh`, `scripts/release.sh`, and release helper tests.
- Affected automation: `.github/workflows/release.yml`, `.github/workflows/release-dry-run.yml`, Cloudflare Pages direct-upload deployment, and any deploy secrets needed by the release job.
- Affected docs: `README.md`, `docs/next/flatpak-packaging.md`, `AGENTS.md`, `.agents/rules/build.md`, and release skill references.
- Affected external systems: FastMail DNS records for `flatpak.cominotti.dev`, Cloudflare Pages custom-domain settings, Cloudflare API credentials, GitHub Actions secrets for signing, and optional R2/Netlify/GitHub Pages fallback deployment.
