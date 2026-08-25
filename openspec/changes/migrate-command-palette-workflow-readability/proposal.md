## Why

This is **slot 2a** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`. Slot 1
(`normalize-workflow-readability-boundaries`) settled the convention and proved
it on exactly one workflow, `WFR-SEARCH-REPLACE`'s search and preview half. Six
migration slots remain, and slot 2 is the one that cannot simply be executed: it
carries a **tier-3 half** (the Replace All durable write path and its undo
journal) while only **one** completed lower-risk migration precedes it, and the
convention requires two.

**This change resolves that ordering by splitting slot 2 into 2a and 2b, and it
is 2a: the tier-2 `WFR-COMMAND-PALETTE` migration.** It is the second proof the
convention needs. The tier-3 replace/undo half moves to
`complete-search-replace-workflow-readability` (slot 2b), which may not begin
until this change is archived and this row is green.

The split is deliberate, not convenience. Three reasons:

1. **The two-proof rule wants a *completed* migration, and completion is
   observable only at the change boundary.** Sequencing the palette first inside
   one change would make the gate a promise in a task list. As two changes it is
   mechanically enforced: 2b cannot pass `make check-workflow-boundaries` until
   the matrix marks `WFR-COMMAND-PALETTE` migrated and the slot ledger says slot
   2a is complete.
2. **A tier-3 durable rewrite plus undo journal deserves its own verification
   section.** 2b needs Replace All / undo behavior equivalence, `search_backup`
   failure-path proof, and a real-session run of a workflow that mutates the
   user's files. Bundling that with an 11,179-line palette migration produces one
   change whose review cannot be done honestly.
3. **The programme anticipated it.** The slot ledger grammar was extended in slot
   1 (task 7.9, finding N2) so a slot can keep its number and take a letter
   suffix, precisely for this remedy. Using it is following the plan, not
   deviating from it.

Beyond the ordering, the palette earns its own migration on the same measured
grounds the programme was created for:

- **`ui/command_palette/mod.rs` is not a facade.** 667 lines: the widget's public
  API interleaved with the entire incremental file-index-mutation workflow —
  bounded queue admission with capacity-growth byte math, a 75 ms debounce, a
  disposal-capacity retry wakeup, a `spawn_blocking_then` worker, generation
  arbitration, replay-on-loss, and retirement accounting. A reader answering
  "what happens when I press Ctrl+Shift+P" and a reader answering "what happens when a
  file is renamed while the palette is closed" are in the same file with no
  narration for either.
- **`ui/command_palette/runtime.rs` is exactly the name the convention
  rejects.** 59 lines holding a seam value object, a worker entry point, and a
  test-only delay static. `runtime` says only that the module is machinery.
- **Untyped inspection.** `index_update_queue_snapshot_for_test()` returns
  `(usize, u64, bool, usize, u64)` — queue length, bytes, a pending flag, and two
  policy caps — as a bare 5-tuple. Alongside it: 5 more inspection functions, 3
  process-global retirement counters, and 2 timing-override statics, with no
  single surface. Two of the counters (`observed_search_cancellations`,
  `last_cancelled_search_examined`) are ungated `Cell` fields compiled into
  production builds to serve tests.
- **Three coordinator snapshot types, none of them a workflow surface.**
  `FileIndexBuildCoordinatorSnapshot` has no reader outside
  `services/palette/tests.rs`; `PaletteSearchCoordinatorSnapshot` has three
  readers, all test-gated (`ui/command_palette/mod.rs:424`,
  `ui/window/notes/mod.rs:228`, and an alias in
  `services/bookmark_excerpt.rs:201`). Meanwhile `FileIndexBuildCoordinator` is a
  semantically equivalent hand-rolled duplicate of the shared
  `SingleFlightCoordinator` that `services/palette/runtime.rs` already knows how
  to alias — differing only in its snapshot (4 fields against the shared type's 6,
  which add high-water marks) and in two shared methods it does not expose
  (`clear_pending()`, `active_generation()`) plus the shared type's generic
  request parameter.
- **Duplicated freshness logic on the mutation seam.** The
  `file_index_generation == base_generation` arbitration and the
  `last_owned && previous.len() == MAX_INDEXED_FILES` retirement-cap predicate
  are each written out three times.
- **`window.command_palette` re-derives its state.** All eight D-Bus snapshot
  fields are gathered by calling seven separate widget accessors, which is the
  pattern `window.content_search` stopped doing in slot 1.

Finally, the **normative facade line budget is still unset**, and the
retroactive-amendment rule makes the cheapest moment to set it the moment when
exactly one workflow is migrated. That moment is now, and it expires when this
change lands its second facade.

## What Changes

- **Split slot 2 into 2a (this change, tier-2) and 2b
  (`complete-search-replace-workflow-readability`, tier-3).** Record the split in
  the programme record's remaining-scope table, its machine-readable slot ledger
  (`slot 2a` / `slot 2b` labels, never renumbering slots 3-7), and the matrix's
  migration-order table. Register both change names in the record's slot table so
  a cold session can find them.

- **Set the normative facade line budget at 370 physical lines.** Declared as the
  single machine-readable line the matrix's "Facade size budget" section
  documents, which activates the previously inert check in
  `scripts/check-workflow-boundaries.py`. 370 is derived from the exemplar's
  measured 350 with modest headroom; the matrix already records that a budget
  below roughly 370 would force that facade's narration to split, which defeats
  the facade. Under the retroactive-amendment rule this change verifies the
  already-migrated `WFR-SEARCH-REPLACE` facade against the number in the same
  change, and holds this change's own new facade to it.

- **Migrate `WFR-COMMAND-PALETTE` to the convention as one vertical slice:**
  - a **narrative facade** at `ui/command_palette/mod.rs` that narrates both of
    the row's stage orders (query search; file-index mutation) with every
    inversion and resumption point named, and owns no timers, ledgers,
    generations, or widget mutation;
  - **coordination modules** named from the bounded role set, replacing
    `runtime.rs`, which is retired;
  - a pure **`policy.rs`** holding the queue-admission byte and cap math, the
    escalate-to-rebuild decision, batch-kind selection, generation arbitration
    and replay-on-loss, the retirement-cap classification, and the
    header-skipping result-navigation predicates — all currently expressed inline
    in the adapter;
  - one typed **`evidence.rs`** replacing the six inspection functions and the
    untyped 5-tuple, folding in the existing coordinator snapshot types rather
    than leaving a second observation path;
  - one **`test_policy.rs`** collapsing the palette's timing/limit overrides, so
    no override storage compiles without the test feature and the two
    test-serving counters stop being unconditional production fields;
  - the **seam value object** for the file-index mutation seam, which is the
    palette's one genuinely unreified bundle. The query seam needs no new type:
    its coordinator already owns the generation and exposes `is_current`, which
    the convention accepts as the seam value object.

- **First `WFR-AUTOMATION-SPINE` projection beyond the search fields.**
  `window.command_palette` projects from the palette evidence surface, exactly as
  `window.content_search` does: every field except `command_palette.visible`,
  which stays window shell state. The `command-palette-search` and
  `command-palette-index` readiness blockers keep their documented semantics; the
  palette half of the `command-palette-index` composition projects from evidence
  while the note-source half is untouched.

- **Retire the palette's hand-rolled single-flight duplicate.** Express
  `FileIndexBuildCoordinator` through the shared `SingleFlightCoordinator` the way
  `services/palette/runtime.rs` already aliases `PaletteSearchCoordinator`, which
  is the convention's "reuse the established shape rather than parallel it" rule
  applied to the one instance fully owned by this workflow. The snapshot type
  gains the shared type's two high-water fields; its only readers are
  `services/palette/tests.rs` and the benchmarks.

- **Implement the evidence-to-snapshot drift gate that Phase 0 specified but
  never built.** `openspec/specs/workflow-evidence-surfaces/spec.md` requires
  `make check-automation-docs` to fail, naming both the evidence field and the
  snapshot field, when a projected evidence field changes. That check does not
  exist: `scripts/check-automation-docs.py` has no evidence-surface awareness, and
  `SearchPanelEvidence` is referenced nowhere outside prose. Slot 1 could leave it
  unnoticed with one projection; this change makes projections plural, so the gap
  becomes load-bearing and enters scope now per
  `.agents/rules/preexisting-blockers.md`. The check must cover the existing
  `SearchPanelEvidence` projection as well as the palette's new one.

- **Advance the matrix and the programme record in the same change:** the
  `WFR-COMMAND-PALETTE` row to `migrated` with a `Migrated Workflow Roles`
  subsection, its seam and evidence cells updated, `WFR-AUTOMATION-SPINE` kept
  `pending` and carried on 2b's outstanding ledger line while appearing as
  `(partial)` on slot 2a's complete line, and slot 1's five residue obligations
  re-stated as 2b's with the facade-budget item struck off.

**Explicit non-goals.** Nothing here touches the Replace All write path,
`services/search_backup.rs`, `activate_undo_replacements`,
`model/workspace_search.rs`, or `replace.rs`'s role name: all five are 2b's. This
change does **not** re-plan slot 1's capped-result delivery fix or the `WalkStop`
stop-semantics split. It does not retire `NoteSourceRefreshCoordinator`, the
palette's other hand-rolled single-flight duplicate. The reason is not that the
type is shared state — there are **two independent instances**, one on the window
imp for the palette (`ui/window/imp.rs:464`, read through
`ui/automation.rs:724`'s `has_work()`) and one for the Notes browser
(`ui/window/notes/mod.rs:198`, read through `NotesBrowserRuntimeSnapshot`). The
reason is stronger: deduping the *type* changes the notes-browser snapshot's
shape, which is `WFR-NOTES-BOOKMARKS` surface area and belongs to slot 5. It does not change user-visible
behavior, palette ranking or grouping results, the exported D-Bus contract, any
accessibility anchor, or any timing default. It does not reify any workflow as an
explicit state machine, and it does not retire actuation test seams, which remain
a programme-level deferral.

## Capabilities

### New Capabilities

None. Phase 0 established the contract this change consumes.

### Modified Capabilities

- `workflow-readability-boundaries`: the facade-budget requirement is written in
  the future tense — the normative maximum "SHALL be set by the first migration
  change that follows the exemplar", and the exemplar "records that measurement
  and leaves the number unset". **This change is that first migration**, so after
  it lands that text instructs a future migration to do something already done,
  and a slot-3 author could reasonably read it as licence to re-derive the
  number. The delta restates the requirement in the settled tense: the budget is
  declared, the declaration lives in the matrix, mechanical enforcement is
  active, and changing the number is a convention amendment governed by the
  retroactive-amendment rule. No scenario is weakened and no new obligation is
  added.

- `gtk-adapter-module-boundaries`: the bounded-role-name requirement scopes
  coordination names *per workflow* within a shared directory. It does not say
  what happens when **one** workflow owns several ordered stage orders in one
  directory and more than one of them needs a coordination module of the same
  shape. The palette is exactly that case — a query flight and an incremental
  file-index mutation, each with its own admission, worker, and completion
  arbitration — so this change hits the gap first and closes it: a coordination
  module MAY qualify a bounded role name with the stage order it serves, using the
  workflow's own domain vocabulary for the qualifier while the suffix stays a
  bounded role name, so the bounded set is not widened. The delta also records
  that the bounded set is **not** mechanically enforced —
  `scripts/check-workflow-boundaries.py` validates that declared role paths exist,
  not that a name is drawn from the set — so no slot assumes a gate will catch an
  off-set name.

Nothing else changes at requirement level. Every other element above is already
provided for: the role names themselves come from the bounded set
`gtk-adapter-module-boundaries` already lists; the evidence surface, its
visibility rule, the test-policy collapse, the automation projection, and the
evidence-to-snapshot drift gate are existing requirements of
`workflow-evidence-surfaces` and `dbus-automation-spine` — the drift gate is
*implemented* here, not newly required; and the `ui/**/policy.rs` mutation scope
plus relocation parity are existing requirements of `mutation-testing`.

**Three flags for the reviewer, per the record's instruction to raise spec-delta
needs loudly rather than absorb them quietly.**

1. **The `workflow-readability-boundaries` delta is spec hygiene, not a contract
   gap.** It exists because fulfilling a future-tense requirement necessarily
   makes that requirement's own wording stale. It is not evidence that Phase 0
   under-specified anything, and it adds no capability.
1a. **The `gtk-adapter-module-boundaries` delta *is* a real, small contract
   gap**, and it is the kind the convention sanctions: the bounded-role-name
   requirement already says a coordination job no listed name describes must
   amend that spec. The one-workflow-two-stage-orders collision is adjacent to
   that and is closed the same way. It adds no role name and no capability.
2. **The record's "proposal + tasks only" expectation is not achievable under
   strict validation, and that is a defect in the record, not in this change.**
   `openspec validate <change> --strict` fails with "Change must have at least
   one delta" for any change carrying no `specs/` delta (verified against this
   change before the delta was written: exit 1). So *every* migration slot needs
   at least one delta to validate, and slots 3 through 7 will hit the same wall.
   This change corrects the record's expectation rather than leaving six future
   slots to rediscover it.

If implementation finds that a palette coordination job fits none of `admission`,
`execution`, `retirement`, or `watch` even with the qualification rule above, that
is a further `gtk-adapter-module-boundaries` amendment adding the role name — the
one spec change the convention explicitly sanctions. Discovering the need for any
*other* new requirement or capability must be raised loudly and resolved by
amending Phase 0's specs, not by quietly adding a capability here.

## Impact

**Prerequisite.** None outstanding. Slot 1 is archived at
`openspec/changes/archive/2026-08-25-normalize-workflow-readability-boundaries/`,
its five deltas are merged into `openspec/specs/`, and its mutation evidence
files are at that archive path — which is where this change's tasks cite them.

**Code touched**

- `crates/lushtext-core/src/ui/command_palette/**` — `mod.rs` (667) becomes the
  facade; `runtime.rs` (59) is retired into role-named modules; `imp.rs` (761)
  loses its inline navigation, no-results, and value-text policy; new `policy.rs`,
  `evidence.rs`, `test_policy.rs`, and coordination modules.
- `crates/lushtext-core/src/services/palette/index.rs` — the
  `FileIndexBuildCoordinator` single-flight dedup and its snapshot type.
- `crates/lushtext-core/src/ui/automation.rs` — `command_palette_snapshot` and
  the two palette readiness blockers become evidence projections.
- `crates/lushtext/tests/widget/command_palette.rs` (2,499 lines) — tests read
  the evidence surface instead of the six retired inspection functions; the
  project test count must not decrease.
- `crates/lushtext/tests/widget/window.rs` — palette inspection call sites.
- `.cargo/mutants.toml` — the new `policy.rs` is in scope by the
  `ui/**/policy.rs` convention with no hand-listed path. **Expected `exclude_re`
  retirements: none.** The only palette-adjacent entries name
  `services/palette/index.rs`'s `truncate_to_index_limit` and the
  `commands.rs` property-test bridge, neither of which this change moves.
- `scripts/check-automation-docs.py` — the evidence-field drift check, with
  `--self-test` cases following that script's existing convention.
- `docs/workflow-readability-matrix.md`, `docs/next/workflow-readability.md`,
  `docs/automation.md`, `docs/automation-reference.md`, `AGENTS.md`, `README.md`,
  and any `.agents/rules/*.md` or skill reference naming a relocated path.

**Code enumerated but not touched**

`services/palette/notes.rs` (3,428), `commands.rs`, `fuzzy.rs`, and `grouped.rs`
stay as they are: they are already pure, GTK-free, and correctly placed. The
palette row's size is dominated by them, so the migrated diff is far smaller than
the row's 11,179-line footprint.

**Two risks stated up front**

- **The facade budget leaves 20 lines of headroom.** 370 against the exemplar's
  measured 350 is deliberately tight, because a loose budget enforces nothing. The
  consequence is that this change's own facade — narrating **two** stage orders —
  may not fit on the first attempt. The response is always to delegate more into
  the coordination modules, never to raise the number, because raising it is a
  convention amendment that would require re-migrating every migrated row. If the
  palette facade cannot fit 370 after an honest split, that is real evidence the
  number is wrong, and correcting it now is far cheaper than correcting it after
  slot 2b — which is the argument for setting it here rather than later.
- **Mutation "parity" for the palette is an asymmetry, not an equality.**
  `ui/command_palette/**` is not in the mutation scope today, so the pure logic
  about to move generates a baseline of **zero mutants by construction**. Moving
  it into `policy.rs` means *gaining* mutants that must all be killed, which is
  strictly stronger than the requirement's equal-counts phrasing; that phrasing
  governs relocations between two already-scoped locations. The evidence file must
  say this plainly rather than reporting "0 → 0 parity holds".

**Verification**

`make check`, `make check-policy` (including the newly active facade-budget check
and the record/matrix ledger agreement), `make test`,
`make test-widget-headless` with zero `FLAKY:` lines, `make mutants-diff` with
recorded parity evidence for every policy module this change creates or
relocates, `make check-automation-docs`, `make automation-client-self-test`,
`make check-agent-docs`, `make check-agent-skills`, the visual-geometry and
accessibility proof lanes that `ui/` changes make mandatory, and a live
`make run` exercising palette open, all four modes, dense results, no results,
and a file rename that drives an incremental index mutation, with a clean stderr.
Acceptance is behavior equivalence: identical palette results, ordering,
grouping, focus, and accessibility metadata, and an unchanged D-Bus contract.
