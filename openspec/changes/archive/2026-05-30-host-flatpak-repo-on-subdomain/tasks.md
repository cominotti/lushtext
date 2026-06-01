## 1. URL Contract And Existing Change Sync

- [x] 1.1 Update `add-cominotti-flatpak-repository` proposal, design, specs, tasks, docs references, and expected URL examples from `https://cominotti.dev/flatpak/...` to `https://flatpak.cominotti.dev/...`.
- [x] 1.2 Update Cominotti Flatpak generator defaults for `COMINOTTI_FLATPAK_BASE_URL`, `COMINOTTI_FLATPAK_REPO_URL`, descriptor URLs, homepage text, and dry-run reporting.
- [x] 1.3 Update Cominotti Flatpak verifier defaults and tests so `flatpak.cominotti.dev` is the expected public host.
- [x] 1.4 Keep environment overrides provider-neutral so maintainers can temporarily target Netlify, R2, a VPS, or another static host during recovery.

## 2. Cloudflare Pages Deployment

- [x] 2.1 Add release workflow support for Cloudflare Pages direct upload of the generated `flatpak/` staging directory.
- [x] 2.2 Use configurable GitHub Actions secrets or variables for `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, and the Cloudflare Pages project name.
- [x] 2.3 Ensure missing Cloudflare credentials or project settings are reported as an honest skipped deployment rather than a successful publication.
- [x] 2.4 Preserve the existing generic `COMINOTTI_FLATPAK_DEPLOY_COMMAND` path as an advanced override or emergency fallback.
- [x] 2.5 Ensure Cloudflare Pages deployment does not require Pages Functions, redirects, or rewrites to serve Flatpak repository files.

## 3. Pages Limit Preflight

- [x] 3.1 Add a preflight check for the generated `flatpak/` staging directory that counts files and reports the largest files.
- [x] 3.2 Fail or skip Cloudflare Pages deployment when any static asset exceeds the documented Pages per-file limit.
- [x] 3.3 Fail or skip Cloudflare Pages deployment when the generated file count exceeds the configured Pages plan limit.
- [x] 3.4 Add regression tests for passing Pages-limit checks, oversized-file failure, and excessive-file-count failure.
- [x] 3.5 Include fallback guidance in failure output that points maintainers to Cloudflare R2 before GitHub Pages or Netlify.

## 4. Documentation And Manual

- [x] 4.1 Update `README.md`, `docs/next/flatpak-packaging.md`, `AGENTS.md`, `.agents/rules/build.md`, and release skill references to name `flatpak.cominotti.dev` as the public Flatpak host.
- [x] 4.2 Add a maintainer manual with step-by-step setup for Cloudflare Pages, FastMail DNS, GitHub Actions secrets, Flatpak signing secrets, release deployment, and public install verification.
- [x] 4.3 The manual must explicitly tell maintainers to remove any existing `flatpak.cominotti.dev` A/AAAA records in FastMail before adding the Cloudflare-provided CNAME.
- [x] 4.4 The manual must include the Cloudflare Pages limits that matter to Flatpak hosting and the R2 fallback decision path.
- [x] 4.5 Update user-facing install commands to use `https://flatpak.cominotti.dev/lushtext.flatpakref` and `https://flatpak.cominotti.dev/cominotti.flatpakrepo` without `--no-gpg-verify`.

## 5. Validation

- [x] 5.1 Run Cominotti Flatpak repository generator and verifier tests in metadata-only mode.
- [x] 5.2 Run release dry-run validation and confirm it reports the Cloudflare/`flatpak.cominotti.dev` publication plan.
- [x] 5.3 Run workflow syntax validation for modified GitHub Actions files.
- [x] 5.4 Run `openspec validate host-flatpak-repo-on-subdomain --strict`.
- [x] 5.5 Run `openspec validate --all --strict` and confirm the follow-up change plus the previous Cominotti repository change remain coherent before archive.
