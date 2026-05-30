## ADDED Requirements

### Requirement: Dedicated Flatpak Repository Subdomain
The project SHALL publish the official Cominotti Flatpak repository through stable HTTPS URLs under `flatpak.cominotti.dev`.

#### Scenario: Public repository URLs use the subdomain
- **WHEN** Cominotti Flatpak publication artifacts are generated for deployment
- **THEN** the repository content is staged for `https://flatpak.cominotti.dev/repo/`
- **AND** the repository descriptor is staged as `https://flatpak.cominotti.dev/cominotti.flatpakrepo`
- **AND** the LushText installer reference is staged as `https://flatpak.cominotti.dev/lushtext.flatpakref`

#### Scenario: Apex website remains independent
- **WHEN** maintainers deploy or document the Flatpak repository
- **THEN** the deployment does not require changing the existing `cominotti.dev` website host
- **AND** the Flatpak host can move independently by changing DNS for `flatpak.cominotti.dev`

### Requirement: Bandwidth-Aware Static Hosting
The project SHALL prefer a hosting backend whose static asset delivery is suitable for Flatpak install and update bandwidth.

#### Scenario: Cloudflare Pages is the default backend
- **WHEN** release automation deploys Cominotti Flatpak artifacts with the default supported hosted backend
- **THEN** it uses Cloudflare Pages direct upload for the generated static `flatpak/` staging directory
- **AND** the deployment does not require Pages Functions or dynamic request handling

#### Scenario: GitHub Pages is not the preferred public backend
- **WHEN** maintainers read the Flatpak hosting documentation
- **THEN** GitHub Pages is documented only as a secondary option for small or temporary static hosting
- **AND** the documentation explains that bandwidth limits make GitHub Pages a weaker fit for Flatpak update distribution

### Requirement: Cloudflare Pages Limit Verification
The project SHALL verify that generated Flatpak repository artifacts fit the Cloudflare Pages static asset limits before deployment.

#### Scenario: Oversized assets block Pages deployment
- **WHEN** a generated Flatpak repository contains a file larger than the documented Cloudflare Pages static asset limit
- **THEN** the verification step fails or skips Cloudflare Pages deployment honestly
- **AND** the report identifies the oversized file and the fallback recommendation

#### Scenario: Too many repository files block Pages deployment
- **WHEN** a generated Flatpak repository contains more files than the documented Cloudflare Pages project limit for the selected plan
- **THEN** the verification step fails or skips Cloudflare Pages deployment honestly
- **AND** the report identifies the total file count and the fallback recommendation

#### Scenario: Successful Pages preflight
- **WHEN** the generated Flatpak repository satisfies the configured Pages file-size and file-count limits
- **THEN** release automation may deploy it to the Cloudflare Pages project for `flatpak.cominotti.dev`

### Requirement: Cloudflare-Native Fallback Path
The project SHALL document Cloudflare R2 as the first fallback when Cloudflare Pages cannot host the generated Flatpak repository safely.

#### Scenario: Pages limits are exceeded
- **WHEN** Pages-limit verification fails because of repository size, per-file size, or file count
- **THEN** the maintainer manual points to Cloudflare R2 behind `flatpak.cominotti.dev` as the preferred fallback
- **AND** Netlify and GitHub Pages are documented only as secondary alternatives when their bandwidth and size trade-offs are acceptable

### Requirement: Maintainer Setup Manual
The project SHALL provide a step-by-step manual for setting up and validating `flatpak.cominotti.dev`.

#### Scenario: Manual covers external setup
- **WHEN** maintainers follow the Flatpak repository hosting manual
- **THEN** it explains how to configure the Cloudflare Pages project and custom domain
- **AND** it explains how to update FastMail DNS records for `flatpak.cominotti.dev`
- **AND** it explains how to configure GitHub Actions secrets and variables for Cloudflare deployment and Flatpak signing

#### Scenario: Manual covers release verification
- **WHEN** maintainers finish a deployment
- **THEN** the manual tells them how to verify HTTPS access to the repository descriptor and installer reference
- **AND** it tells them how to verify Flatpak can list or install `dev.cominotti.lushtext` from the published `cominotti` remote
- **AND** it tells them what to do if Cloudflare Pages limits or DNS propagation block publication
