# GTK Lush Posture Audit

Date: 2026-06-12

Scope: GTK Lush roadmap, family README, governance, adoption evidence,
agent-facing rules, policy scripts, Makefile targets, and OpenSpec specs.

## Classification

### Current Internal-Platform Contract

- `crates/gtk-lush/README.md` describes the current functional `0.0.0`
  family crates and local verification lane.
- `AGENTS.md`, `.agents/rules/build.md`, and
  `.agents/rules/widget-wiring.md` keep GTK Lush path dependencies and local
  gates visible to future agents.
- `Makefile`, `scripts/check-gtk-lush-policy.py`, and
  `scripts/check-gtk-lush-adoption.py` keep the policy, doctest, example,
  adoption, MSRV, public-API advisory, and proof-adjacent checks discoverable
  without requiring publication.

### Dormant Publication Gates

- `crates/gtk-lush/GOVERNANCE.md` and the canonical OpenSpec governance spec
  contain useful publication, semver, docs.rs, maintainer-approval, and
  repository-graduation gates. These gates should remain, but they now apply
  only after a dedicated publication or graduation proposal is approved.
- Public-API and semver checks remain advisory local gates while crates stay
  unpublished `0.0.0` workspace APIs.

### Historical Roadmap Context

- `docs/next/gtk-lush.md` remains useful as the umbrella narrative and phase
  history, including why adoption validation happened before any publication
  track. The stale part was the first visible posture and later phase wording
  reading like scheduled work instead of superseded context.
- Phase names in archived specs and review logs are kept when they identify
  completed work or historical gate ordering.

### Stale Next-Step Wording

- Roadmap status, Phase 5b, Phase 6, repository split, and success-metric
  language needed to state that internal-platform stewardship is the current
  end state, while publication/graduation/upstreaming are dormant future
  tracks.
- Policy-script error strings and README-required phrases that named "Phase
  5b publication-ready" needed to become posture-neutral internal-platform
  wording.
- Adoption handoff language needed to say the evidence is maintained baseline
  evidence first, not a mandate to publish.

### Out Of Scope

- LushText Flatpak, Flathub, Snap, and app release publication guidance is not
  GTK Lush crate publication guidance and is left unchanged.
- Historical archive entries and spec scenarios that describe past phase
  boundaries are updated only when their wording would mislead future work.
- Runtime code, GTK Lush APIs, LushText UI, persistence, and Automation1
  behavior are intentionally unchanged by this stabilization.
