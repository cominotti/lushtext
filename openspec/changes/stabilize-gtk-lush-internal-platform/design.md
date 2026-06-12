## Context

GTK Lush started as a way to extract LushText's hardened GTK4/Libadwaita
patterns into reusable, independently adoptable crates. That work has already
produced functional in-tree `0.0.0` crates for signal/binding lifetimes,
settle timers, bounded background tasks, viewport observation, geometry
widgets, proof harnessing, and proof-spine value objects. LushText consumes
those crates through workspace path dependencies, and adoption validation has
recorded lab, fixture, timed-stock, external-spike, and API-review evidence.

The remaining question is not whether GTK Lush works for LushText. It does.
The question is whether the project should keep moving toward publication and
repository graduation without an external pull signal. This change makes the
answer explicit: for now, GTK Lush is a stable in-tree LushText platform.

## Goals / Non-Goals

**Goals:**

- Record an intentional steady-state posture for GTK Lush as in-tree
  infrastructure.
- Keep existing GTK Lush checks, adoption evidence, examples, doctests,
  policy checks, and proof gates alive enough that the platform does not rot.
- Make future GTK Lush work demand-driven: LushText pain, evidence drift,
  proof-tooling improvement, or real external adopter need.
- Rewrite roadmap/governance/readme/handoff language so agents do not treat
  publication, repository split, or upstreaming as automatic next work.
- Preserve larger phase-level planning for any future GTK Lush change.

**Non-Goals:**

- Do not publish functional GTK Lush crates.
- Do not prepare `0.1.0` releases or crates.io credentials.
- Do not split GTK Lush into a separate repository.
- Do not move LushText from workspace path dependencies to published crates.
- Do not change public GTK Lush APIs unless stale-publication wording reveals
  a small docs-only mismatch.
- Do not change LushText runtime behavior, UI, persistence formats, D-Bus
  automation contracts, or visual design.

## Decisions

### Treat Internal Platform As A Successful End State

GTK Lush is useful even if it never becomes a public crate family. The in-tree
crates already remove repeated LushText-specific GTK lifecycle and proof
patterns from application modules, and adoption validation found no blocking
API friction. The new steady state is:

```text
LushText app code
      |
      v
workspace path dependencies
      |
      v
GTK Lush in-tree crates and proof tool
      |
      v
policy, adoption, doctest, example, API advisory, and proof gates
```

Alternative considered: continue directly to publication and repository split.
That would create semver, docs.rs, release, support, and treadmill obligations
without a current external adopter asking for them.

### Keep Publication Possible But Not Scheduled

Publication remains a valid later track, but it must be reopened explicitly.
That reopened track should cite current adoption evidence, refresh any stale
evidence, record maintainer approval, and perform publication-specific work as
its own larger phase-level change.

Alternative considered: delete all publication language. That would throw away
useful guardrails. The better move is to keep the gates, but stop presenting
them as the natural next step.

### Maintain Evidence, Do Not Expand It By Default

The adoption lab, matrix, stock fixture, external-spike note, API review, and
specialist review notes become baseline evidence for internal stewardship.
They should be updated when GTK Lush APIs, examples, fixtures, or lab
workflows change. They should not force recurring external-project spikes or
timed adoption exercises unless the publication track reopens.

Alternative considered: rerun adoption validation periodically. That would add
process cost without new signal. Evidence should be refreshed when something
changes or when publication is reconsidered.

### Make Future GTK Lush Work Demand-Driven

Future GTK Lush changes should pass a simple filter before they are proposed:

```text
Is there real LushText pain?
Is a current GTK Lush check/evidence artifact drifting?
Would proof tooling materially improve UI confidence?
Is there a real external adopter asking for this?
```

If the answer is no, the work should stay out of scope.

Alternative considered: keep implementing the original phase plan. That plan
was useful to get here, but it should not outrank the current evidence that
the internal platform is already good enough.

### Keep Specs Phase-Sized

This stabilization is one coherent OpenSpec change. It should not split into
one change per crate or one change per document. Future GTK Lush work should
also start as one phase-sized proposal and split only when implementation
ownership or verification genuinely requires it.

Alternative considered: create small cleanup specs per stale document. That
would optimize for workflow mechanics instead of the user's current preference
and would make the strategic posture harder to review.

## Risks / Trade-offs

- [Risk] The family quietly drifts because publication pressure is gone. ->
  Mitigation: keep local policy, adoption, doctest, example, public-API
  advisory, and `make check` gates in the normal verification surface.
- [Risk] Future agents read old roadmap language and resume publication work.
  -> Mitigation: rewrite roadmap, governance, README, archive handoff, and
  local guidance to state that internal platform is the current end state.
- [Risk] Useful upstreaming is lost by deprioritizing publication. ->
  Mitigation: allow small upstream documentation/issues when they remove
  carried LushText maintenance cost, but do not require a broad upstreaming
  phase.
- [Risk] External adopters appear later and the project has stale evidence. ->
  Mitigation: require any reopened publication track to refresh evidence,
  semver/public-API snapshots, docs, changelogs, and maintainer approval.
- [Risk] "Internal platform" becomes a dumping ground for new abstractions. ->
  Mitigation: keep the anti-framework constitution and require future work to
  pass the demand-driven filter.

## Migration Plan

1. Update OpenSpec contracts for governance, workspace integration, adoption
   evidence, and the new internal-platform capability.
2. Rewrite GTK Lush roadmap and family docs to state the current posture:
   functional in-tree APIs, not publication-driven.
3. Update adoption handoff and local guidance so the archived Phase 5a
   evidence is baseline evidence rather than a mandate to publish.
4. Audit checks and scripts for stale assumptions that the next GTK Lush step
   is publication, repository split, or upstreaming.
5. Run OpenSpec validation, GTK Lush policy/adoption checks, agent-doc checks,
   and `make check`.

Rollback is documentation/spec rollback. This change is intended to make no
runtime behavior changes.

## Open Questions

- Should the old umbrella vision remain under `docs/next/gtk-lush.md` with a
  steady-state preface, or should it move to an archived planning note?
- Should a new lightweight stewardship note exist under `docs/gtk-lush-*`, or
  is the canonical OpenSpec capability plus README/GOVERNANCE wording enough?
- Should `make check` continue to run adoption matrix checks permanently, or
  should some adoption-only checks become explicit GTK Lush maintenance gates?
