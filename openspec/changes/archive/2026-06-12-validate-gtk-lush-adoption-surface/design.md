## Context

GTK Lush has finished its first functional in-tree extraction arc. The family
now contains independently adoptable `0.0.0` crates for signal and binding
ownership, settle/timer helpers, background tasks, viewport observation,
geometry widgets, the widget proof harness, and the proof spine. The separate
`cargo-gtk-proof` workspace tool now owns the Rust live visual proof path, with
Python available only as an explicit oracle/diagnostic route.

The next program risk is no longer extraction parity. It is product fit. The
umbrella vision says every GTK Lush API is judged by whether a stock gtk-rs
application can adopt exactly one piece in an afternoon without restructuring
itself. LushText alone cannot prove that. This phase therefore creates a
second-consumer adoption lab, a stock starter adoption exercise, and an
unrelated-existing-project spike before publication or repository graduation.

The stakeholders are LushText maintainers, future GTK Lush maintainers, agents
implementing GTK changes, and hypothetical third-party gtk-rs consumers. The
main constraint is that adoption validation must not become a framework, a
publishing phase, or a shadow application platform. The phase may make breaking
pre-publication API changes because all functional family crates remain
`0.0.0`.

## Goals / Non-Goals

**Goals:**

- Prove the family through a maintained second consumer that is not LushText
  app code and not a GTK Lush family crate.
- Exercise every functional GTK Lush crate in realistic workflows, with a
  crate-by-crate adoption matrix that names the workflow, docs, examples,
  tests, proof evidence, and friction outcome.
- Run a timed stock gtk-rs starter adoption for at least one crate and preserve
  the journal as reviewable evidence.
- Run an unrelated-existing-project adoption spike for at least one crate and
  preserve candidate, patch, friction, and decision notes without vendoring the
  outside project.
- Let adoption evidence drive API review and breaking `0.0.0` improvements
  before any `0.1.0` stability promise.
- Clean stale proof-tool wording that still implies Rust live proof is staged
  or Python-authoritative.
- Update roadmap, governance, docs, examples, tests, and policy gates so the
  adoption phase is measurable and repeatable.
- Keep LushText's full phase gate green, including widget and visual proof
  where UI or visual-sensitive behavior changes.

**Non-Goals:**

- No functional crates.io publication, no `0.1.0`, and no release automation
  for external package publication.
- No split to a dedicated `gtk-lush` repository.
- No LushText migration from workspace path dependencies to published
  versions.
- No upstreaming round to GTK, gtk-rs, GtkSourceView, or docs projects.
- No new view DSL, component model, state/message loop, custom runtime, or
  cross-crate runtime dependency inside the GTK Lush family.
- No committed external project checkout, private user content, unbounded
  journals, or proof artifacts that cannot be reviewed.

## Decisions

1. Add a workspace adoption-lab crate outside `crates/gtk-lush/`.

   The adoption lab should live in the repository as a normal workspace member,
   for example `crates/gtk-lush-adoption-lab`, with a package name that makes
   its role obvious. It is a consumer, not a family crate: it may depend on
   multiple GTK Lush crates, but policy tooling must not apply leaf-crate
   package-name or inter-family dependency rules to it. Keeping it in the
   workspace gives CI, cargo-hakari, formatting, docs, and Makefile targets one
   stable place to exercise cross-crate adoption. The alternative was only
   per-crate examples, but those already exist and do not prove the pieces can
   be composed in a real app.

2. Make the adoption lab a usable GTK app/gallery, not a landing page.

   The first screen should be the working gallery itself: a quiet Libadwaita
   tool with tabs or sections for each adopted pattern. It should expose real
   controls and state transitions: reconnecting rows, rebinding objects,
   debounce and superseding timers, background task freshness, viewport rest
   observation, constrained geometry with `ClipBin`, render-hold behavior,
   proof-harness waits, and proof-spine readiness/snapshot values. UI
   acceptance must cover no-demo/no-required-context states, representative
   populated state, many or awkward rows, and constrained geometry without
   unintended root scrollbars or hidden controls.

3. Keep the stock starter fixture outside the workspace member graph.

   A stock gtk-rs starter-style fixture should use `cargo check --manifest-path`
   against path dependencies to exactly one GTK Lush crate. That keeps the
   exercise close to a third-party consumer and catches accidental reliance on
   workspace-only dependencies, LushText setup, generated resources, or hidden
   policy scripts. The fixture may live under a bounded directory such as
   `fixtures/gtk-lush-adoption/stock-<crate>/`. The alternative was a normal
   workspace crate, which would hide too much behind LushText's workspace
   configuration.

4. Treat the unrelated-existing-project spike as evidence, not vendored code.

   The implementation should choose a small, license-compatible, public gtk-rs
   or Libadwaita project and attempt to adopt one GTK Lush crate in a temporary
   local fork or external worktree. The repo should preserve a bounded journal:
   project identity, version or commit, selected crate, elapsed time, commands,
   patch summary or link, friction, and the decision to keep, redesign, or
   defer. It should not commit the outside source tree. This gives us the
   "unrelated existing project" signal without turning LushText into the owner
   of someone else's code.

5. Use an adoption matrix as the phase's central truth table.

   The matrix should live in docs or adoption-lab documentation and name each
   crate: `gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`,
   `gtk-lush-viewport`, `gtk-lush-widgets`, `gtk-lush-proof-harness`, and
   `gtk-lush-proof-spine`. For each crate it records the lab workflow,
   single-crate example, stock/adoption fixture status when applicable,
   proof/test lane, friction status, and API decision. This prevents "uses
   every crate in anger" from becoming subjective.

6. Let friction drive breaking pre-publication API changes.

   Every friction point should be classified as documentation, example,
   naming, type-shape, feature flag, missing helper, overreach, or not-fixable
   in this phase. API changes are allowed when they reduce consumer ceremony,
   remove LushText-shaped assumptions, clarify ownership, or keep the crate
   leaf-like. The implementation must update docs, CHANGELOGs, tests, examples,
   public API snapshots, and any LushText call sites affected by those changes.
   The alternative is to preserve awkward `0.0.0` APIs for continuity, but this
   is exactly the last cheap window for breaking corrections.

7. Add adoption policy checks without making all adoption work blocking forever.

   The phase should add targeted Makefile/script checks for the maintained
   adoption lab, the stock fixture, journals, and matrix completeness. Those
   checks should be deterministic and bounded. The unrelated-project spike is
   evidence for this phase, not a permanent CI dependency on an external repo.
   Future CI may verify the in-tree lab and stock fixtures; it should not need
   network access to re-clone the unrelated project.

8. Keep `cargo-gtk-proof` cleanup narrow and factual.

   The proof-tool cleanup should update source docs, README text, canonical
   OpenSpec wording during sync/archive, and any comments that still describe
   Rust live proof as staged or Python as the current execution oracle. It
   should not remove intentionally historical compatibility fixtures or rename
   serialized metadata fields unless the adoption/API review proves they are
   misleading to consumers and all compatibility tests are updated.

9. Require specialist review because this is a cross-cutting phase.

   The implementation should use focused review lanes for GTK testing,
   live/headless GTK behavior, GTK/Libadwaita contracts, performance, data
   safety/privacy of artifacts and journals, Rust architecture, and comment
   quality. Actionable review findings should be fixed before archive or
   recorded as accepted non-blockers with rationale.

## Risks / Trade-offs

- Adoption lab becomes a framework-shaped showcase -> Mitigate with ordinary
  gtk-rs and Libadwaita widgets, no custom view DSL, no app runtime, and a
  governance checklist that treats the lab as a consumer.
- Cross-crate lab hides single-crate adoption pain -> Mitigate with per-crate
  examples and at least one stock starter fixture that imports exactly one
  GTK Lush crate.
- External project spike is too costly or network-sensitive -> Mitigate by
  committing only bounded notes and patch summaries, while keeping permanent CI
  limited to in-tree fixtures.
- Breaking API churn destabilizes LushText -> Mitigate with path-dependency
  compilation, targeted migrations, public API snapshots, CHANGELOG entries,
  and the full LushText gate before archive.
- Adoption UI tests become flaky -> Mitigate by keeping widget assertions
  state-based where possible, using `gtk-lush-proof-harness` waits, and using
  visual proof only for rendered geometry effects that require pixels.
- Journals leak private or unbounded data -> Mitigate with a fixed template,
  no user document content, no external repository checkout committed, and
  bounded command excerpts.
- Proof-tool stale wording cleanup breaks intentional compatibility names ->
  Mitigate by separating user-facing/current-contract wording from historical
  fixture identifiers and documenting any retained `rust-staged` metadata.
- Phase creeps into publication -> Mitigate with explicit non-goals, roadmap
  split, governance deltas, and no release automation or crates.io tasks.

## Migration Plan

1. Update the roadmap and governance language so this change is recorded as
   the adoption-validation half of Phase 5, before publication/graduation.
2. Add the adoption-lab workspace consumer and wire it into the root workspace,
   cargo-hakari, dependency policy, and a dedicated check target.
3. Build lab workflows for all GTK Lush crates, starting with compileable
   controls and then adding state/geometry/proof assertions.
4. Add the adoption matrix and keep it synchronized with lab workflows,
   examples, tests, and friction decisions.
5. Add a stock gtk-rs starter fixture for at least one crate and a timed
   journal produced by adopting that crate from a fresh-session mindset.
6. Run the unrelated-existing-project spike and preserve the bounded notes or
   patch summary.
7. Classify friction and apply API/docs/example/test changes across the family
   and LushText call sites.
8. Clean stale proof-tool wording and document any intentionally retained
   historical compatibility identifiers.
9. Run the GTK Lush family gates, adoption checks, full LushText gates, visual
   proof if visual-sensitive files changed, and specialist reviews.

Rollback is straightforward before archive: remove the adoption-lab workspace
member, fixtures, and policy targets, then revert any API migrations that did
not pass adoption review. After archive, the adoption lab becomes permanent
pre-publication evidence and should be kept green until the later publishing
phase either graduates or redesigns the family.

## Open Questions

- Which unrelated gtk-rs project should be used for the spike? The proposal
  does not need to pick it; implementation should choose based on small scope,
  public availability, license compatibility, and whether one GTK Lush crate
  can plausibly fit without restructuring the project.
- Which crate should be used for the timed stock starter adoption first? A
  low-friction crate such as `gtk-lush-signals`, `gtk-lush-settle`, or
  `gtk-lush-tasks` is likely best, but implementation may choose the crate
  whose API most needs adoption pressure.
