## 1. Posture And Scope Audit

- [x] 1.1 Inventory GTK Lush docs, governance, README, adoption handoff, local rules, skills, scripts, Makefile help text, and OpenSpec specs for wording that treats publication, repository graduation, or upstreaming as the automatic next step.
- [x] 1.2 Classify each finding as current internal-platform contract, dormant publication gate, historical roadmap context, stale next-step wording, or out of scope.
- [x] 1.3 Record any non-obvious decisions in the least noisy implementation note or design update before editing broad documentation.

## 2. Canonical Spec And Roadmap Updates

- [x] 2.1 Update canonical OpenSpec specs from this change's delta specs so the internal-platform posture is preserved after archive.
- [x] 2.2 Update `docs/next/gtk-lush.md` so the first visible posture is stable in-tree LushText infrastructure, with publication/graduation/upstreaming described only as explicitly reopened future tracks.
- [x] 2.3 Preserve useful publication gates in governance wording, but label them as dormant gates that apply only when a future publication or graduation proposal is approved.
- [x] 2.4 Keep the larger phase-level planning preference visible so future GTK Lush work does not fragment into many small per-crate specs by default.

## 3. Family Documentation And Evidence Updates

- [x] 3.1 Update `crates/gtk-lush/README.md` to describe the family as functional in-tree `0.0.0` infrastructure for LushText and future evidence refresh, not as publication-bound work.
- [x] 3.2 Update `crates/gtk-lush/GOVERNANCE.md` with the internal-platform review posture, dormant publication gates, and conditions for reopening publication or graduation.
- [x] 3.3 Update `docs/gtk-lush-adoption/archive-handoff.md` and related adoption docs so archived evidence is a maintained baseline, not a mandate to publish.
- [x] 3.4 Ensure accepted limitations in `docs/gtk-lush-adoption/matrix.toml` or `api-review.md` remain explicit and retain their trigger for reconsideration.

## 4. Local Guidance And Check Surface

- [x] 4.1 Update affected `.agents/rules/`, `.agents/skills/`, root `AGENTS.md`, or nested guidance only where stale GTK Lush publication or next-phase language would mislead future agents.
- [x] 4.2 Audit Makefile help text, policy scripts, and check output for stale language that claims crates are publication-ready or that publication is the next required phase.
- [x] 4.3 Keep GTK Lush policy, doctest, example, adoption, MSRV, public-API advisory, and proof checks discoverable as local internal-platform gates.
- [x] 4.4 Confirm generated adoption/proof artifacts and external checkout paths remain ignored or documented, and no private or unbounded evidence is committed.

## 5. Verification

- [x] 5.1 Run `openspec validate stabilize-gtk-lush-internal-platform --strict`.
- [x] 5.2 Run `openspec validate --changes --strict`.
- [x] 5.3 Run `openspec validate --specs --strict`.
- [x] 5.4 Run `openspec validate --all --strict`.
- [x] 5.5 Run `make check-gtk-lush-policy`.
- [x] 5.6 Run `make check-gtk-lush-adoption`.
- [x] 5.7 Run `make check-agent-docs` if agent guidance changed.
- [x] 5.8 Run `make check`.
- [x] 5.9 Run `git diff --check`.
