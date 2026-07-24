# Design: Close Consistency And Decomposition Gaps

## Context

A four-lens post-programme review (abstractions/type design, error handling,
pattern consistency, landed-as-intended audit) verified all nine archived
quality changes landed with structural guarantees and found no correctness or
data-safety defects. The residue is consistency debt in seven clusters:

1. `ui/markdown_preview/mod.rs` — 4,281 production lines, one ~2,100-line
   `impl` block mixing image decode, table building, code theming, footnote
   and link handling, and render orchestration. The only genuinely
   mixed-responsibility file the adapter-decomposition change missed.
2. `ui/editor_page/buffer_replacement.rs:68` — the last production
   `unreachable!` policing a type-representable illegal pairing
   (`CancelledBodyCallback::Guarded` × `ReplacementBody::Plain`).
3. `services/palette/index.rs` — `FileIndexBuildLedger` releases scratch and
   installed charges through 8+ hand-placed calls on scattered exit paths,
   the exact anti-pattern `with_construction_charge` was built to end;
   `notes.rs` retains two manual paired releases (canonical-folder,
   live-identity bytes).
4. Duplicated single-flight coordination — `model/search_flight.rs`
   reimplements `services/palette/runtime.rs`'s coordinator semantics;
   `local_history_service.rs:104` copies `PaletteSearchCancellation` where
   `bookmark_excerpt.rs:174` proved an alias suffices; three ~35-line
   `Guarded*Outcome` adapters hand-roll the same weight-then-own sequence.
5. Widget-test `present_window` re-fragmented into three divergent copies
   (`tests/widget/sidebar.rs:404` lacks the allocation wait that
   `command_palette.rs:77` and `window.rs:573` have); the widget-wiring rule
   claiming it lives in `common.rs` is stale.
6. Latent GTK-thread panic class — ~5 sites invoke registered callbacks
   under a held `RefCell::borrow()`.
7. Small hygiene stragglers — silent cancel-path cleanup, poisoned
   session-ordering lock panics at close time, five smoke drivers hand-copy
   the Gdk broken-pipe predicate, one process-global single-slot fault seam,
   10 dead pub fns, one boolean-flag one-shot timer, two byte-similar
   fire-and-forget spawn blocks, ~12 loose `*_generation` fields on
   `editor_page/imp.rs`, and no glossary for the programme's coordination
   vocabulary.

Constraints: no new crates or dependencies; gtk-lush crates stay leaf crates;
behavior and pixels must not change; all existing proof gates (`make check`,
widget lanes, visual-geometry when applicable, `check-policy`) stay green.

## Goals / Non-Goals

**Goals:**

- Retire the remaining vigilance-based contracts (panic-arm pairing, manual
  charge releases, per-exit release calls, single-slot seam) into
  ownership/type contracts, matching the standard the 07-23 closeouts set.
- One coordinator/cancellation primitive with a workflow-neutral home.
- `markdown_preview` decomposed by workflow, behavior- and pixel-neutral.
- One shared widget-test presentation helper; rules docs match reality.
- Close the latent callbacks-under-borrow panic class.
- Give the coordination vocabulary a documented glossary.

**Non-Goals:**

- No refactor of `DisposalOwned` internals (pragmatic, working core).
- No splitting of the other 24 over-budget-but-cohesive files.
- No shared `retained_byte_weight` trait (consistent naming suffices).
- No shrinking of the coordination vocabulary or type inventory itself.
- No new generic scheduler, retirement framework, or manager layers.
- No user-facing behavior, automation contract, or persisted-format change.

## Decisions

### D1: markdown_preview splits by workflow into sibling modules

Follow the proven `ui/window/` decomposition shape: extract sibling modules
under `ui/markdown_preview/` (indicatively `images.rs`, `tables.rs`,
`code_blocks.rs`, `links.rs` — final grouping decided against real cohesion
during extraction, not forced). The `LushtextMarkdownPreview` wrapper,
template, and `imp.rs` stay; extracted modules take narrow `&self`/owner
references or plain inputs. **Never split mid-impl**: move cohesive private
helper groups and their types, keep the trait impls and the public surface in
`mod.rs`. The documented idle+timeout code-block repair exception keeps its
exact mechanism (SourceId pair + cancellation + completion callbacks) — it
moves, it does not convert.

*Alternative considered*: leave it (biggest file, but tested and working).
Rejected: it is the single clearest violation of the project's own
decomposition rule, and every future markdown feature pays the navigation
tax; the window/editor_page precedent shows the split is safe and cheap
relative to its payoff.

*Proof*: existing markdown widget tests + visual lanes are the
behavior-neutral oracle. `make check-visual-proof-policy` decides whether the
diff counts as visual-sensitive; if it does, run `make visual-geometry-smoke`.

### D2: Buffer-replacement pairing becomes two request constructors over one
generic body-kind parameter

Make the illegal pairing unconstructible rather than policed. Preferred
shape: parameterize the request/session over the body kind (plain vs
guarded), where each body kind fixes its cancellation-callback type
(`Plain → plain callback`, `Guarded → guarded callback`). If generics ripple
too far into the session's five workflow call sites, fall back to two
concrete request types sharing a private core. `Default`/`mem::take`
placeholder semantics stay on the plain side, which is what the current
take-sites rely on.

*Alternative considered*: keep the enum pair and the panic arm, add a
constructor-only invariant comment. Rejected: this is the exact class the
typed-payload spec bans; constructor discipline is vigilance.

### D3: `with_construction_charge` generalizes; index.rs adopts it

Promote the scope-owned charge guard from a private `notes.rs` helper to a
shared palette-internal helper (module within `services/palette/`,
parameterized over the ledger's charge/release/consume operations), then:

- convert `FileIndexBuildLedger` scratch/installed handling in traversal to
  scope-owned guards (`ControlFlow`-style, matching the existing helper's
  API),
- convert the residual `notes.rs` manual paired releases
  (canonical-folder/live-identity bytes),
- keep genuinely direct settlements (e.g. `admit_parsed_sidecar`'s
  parse-reservation consume) as documented consume-path calls through the
  guard, not raw releases.

*Alternative considered*: unify the two ledgers themselves into one type.
Rejected: their truncation vocabularies and budgets are intentionally
separate policies; sharing the *release-ownership mechanism* is the actual
requirement.

### D4: Coordinator consolidation — move, alias, rename; no new semantics

- Move the generic coordinator + cancellation token out of
  `services/palette/runtime.rs` into a workflow-neutral home
  (`services/single_flight.rs` or similar; it is already GTK-free and
  generic). `PaletteSearchCoordinator`/`PaletteSearchCancellation` become
  aliases during migration or are renamed outright with call sites updated —
  prefer outright rename since all consumers are in-tree.
- `model/search_flight.rs`: reimplement `WorkspaceSearchFlight` as a thin
  wrapper over the shared coordinator that preserves its
  `Supersede { active_generation }` evidence surface, or migrate its
  consumers to the shared surface if the evidence maps 1:1. The deciding
  test is that existing search_flight unit tests pass unmodified against the
  wrapper.
- `local_history_service.rs`: replace the copied cancellation struct with an
  alias, following the `bookmark_excerpt.rs` precedent.
- The three `Guarded*Outcome` adapters: extract one small UI-side helper for
  the weight→shrink→own sequence only; each workflow keeps its own outcome
  enum and freshness checks explicit at the call site (the rules require
  explicit freshness — the helper must not hide it).

*Placement note*: the coordinator lives in `lushtext-core` services, not a
gtk-lush crate — it is LushText workflow policy, and gtk-lush graduation
gates are dormant per governance.

### D5: `present_window` gets one home in the widget-test common module

Consolidate to `crates/lushtext/tests/widget/common.rs` (LushText-side, not
the harness crate — presentation policy is app-test-specific and avoiding a
gtk-lush API addition keeps governance quiet; the spec deliberately allows
either home). The unified helper takes the strictest behavior of the three
copies: present + ≥5s allocation/realization wait + post-wait drain
(`flush_after_delay` where window.rs needed it stays a caller-side addition).
Sidebar's weaker copy is the one that gains behavior; its tests must be
rerun in isolation to prove no timing assumptions break. Update
`.agents/rules/widget-wiring.md` to name the real home.

### D6: Clone-then-call sweep

Mechanical: at each site (`load_save.rs:828`, `bookmarks.rs:326/426`,
`workspace_section/actions.rs:311-315`, `sidebar/mod.rs:295/311`), clone the
callback `Rc`s (or collect a snapshot Vec) inside a short borrow scope, drop
the borrow, then invoke. Follow `minimap.rs:459`'s existing shape. No
behavioral ordering change; add one regression test that re-enters
registration from a callback on the highest-traffic path (file-loaded).

### D7: Hygiene items take the smallest correct form

- `actions.rs:364`: add `tracing::warn!` with path + error on failed cleanup.
- `session_service.rs:87`: use the existing `lock_unpoisoned` recovery helper
  (state is a rebuildable `HashMap<PathBuf, u64>`); keep the `# Panics` doc
  removed/updated accordingly.
- Fault seam: convert `FAIL_REPLACE_BEFORE_RENAME_PATH` to the keyed
  `BTreeMap` + `#[must_use]` cleanup-ownership registry shape its sibling
  after-metadata hook already uses.
- Smoke classifiers: extend `scripts/accessibility_warning_allowlist.py` (or
  a sibling shared module) with the shared Gdk broken-pipe family and import
  it from the five drivers; lane-specific patterns stay put. Shell consumers
  (`run-visual-smoke.sh`, `compare-blueprint-visuals.sh`) call the module the
  same way `run-accessibility-smoke.sh` already does.
- Dead code: delete the 10 unreferenced pub fns; where a `*_for_test` hook's
  scenario still matters, resurrect the missing test instead (decide per
  hook by reading the git history of the test that referenced it).
- `workspaces.rs:179`: convert boolean-flag one-shot to `SupersedingTimer`
  (existing settle-timer-normalization spec already covers semantics).
- `actions.rs:325/362`: extract the shared fire-and-forget cleanup spawn into
  one local fn; keep the documented spawn_blocking_then exception rationale.
- Generation fields on `editor_page/imp.rs`: group related counters into
  small named structs per rust.md's own smell rule (e.g. load/path/editor
  lifecycle vs analysis/selection) — pure field moves, no semantic change.
- Glossary: add a short "Coordination vocabulary" section to
  `.agents/rules/rust.md` defining Admission, Budget, Coordinator, Ledger,
  Retirement, Continuation, and generation-counter conventions.

## Risks / Trade-offs

- [markdown_preview split touches a 4.3k-line hot widget] → behavior-neutral
  oracle is the existing test+visual suite; move code without editing logic
  in the same commit; run widget lanes and (if policy demands)
  visual-geometry smoke; forbid drive-by refactors during the move.
- [D2 generics ripple through five workflow call sites] → fallback decision
  pre-approved: two concrete request types over a private core; the spec
  requires unconstructibility, not a specific mechanism.
- [D4 could subtly change search supersession semantics] → keep
  `WorkspaceSearchFlight`'s tests running unmodified against the wrapper;
  the wrapper preserves its evidence enum verbatim.
- [Sidebar tests gain a realization wait they never had] → rerun sidebar
  widget tests in isolation and under load per flake discipline; a newly
  exposed timing assumption is a pre-existing blocker to fix, not to bypass.
- [Charge-guard generalization could double-release during migration] →
  migrate one ledger at a time; existing ledger accounting tests plus the
  spec's exactly-once scenarios are the oracle; keep peak/high-water
  evidence assertions.
- [Scope creep — "everything" invites drive-bys] → the Non-Goals list is
  binding; anything discovered beyond the enumerated items becomes a new
  proposal.

## Migration Plan

Implementation-order only (no deploy/rollback concerns — single-repo,
behavior-neutral):

1. Hygiene one-liners and dead-code removal first (D6, D7 small items) —
   they de-risk nothing else and shrink the review surface.
2. Type/ownership work (D2, D3) with per-ledger migration.
3. Coordinator consolidation (D4).
4. Test-infrastructure unification (D5) before the decomposition so the
   strongest harness watches the riskiest step.
5. markdown_preview decomposition (D1) last, as pure code movement.
6. Docs (glossary, widget-wiring rule, AGENTS/README module layout) in the
   same change set as the code they describe.

## Open Questions

- D2: generic body-kind parameter vs two concrete request types — decided by
  how far the generic ripples at implementation time (fallback
  pre-approved).
- D4: does `WorkspaceSearchFlight` survive as a thin wrapper or do its
  consumers migrate to the shared surface directly? Decided by whether its
  evidence enum maps 1:1 without test edits.
- D7 dead hooks: per-hook resurrect-the-test vs delete-the-hook, decided by
  reading the history of each orphaned `*_for_test` reference.
