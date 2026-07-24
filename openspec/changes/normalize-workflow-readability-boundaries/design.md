## Context

LushText's layering is intact. `crates/lushtext-core/src/model/` (11,817 lines,
29 files) and `services/` (47,273 lines, 61 files) contain zero `gtk4`, `glib`,
`gio`, `libadwaita`, or `sourceview5` imports. `ui/` (67,200 lines, 104 files)
holds the adapters. The GTK Lush family already extracted the generic toolkit
machinery into named, independently adoptable crates.

What the last fourteen changes added was a coordination tier — admission,
budgets, ledgers, single-flight coordinators, retirement, continuations,
generation counters — documented as a vocabulary in `.agents/rules/rust.md`. That
tier is real, cohesive, and mostly well written. It is also **homeless**: half of
it sits in `model/` (pure policy) and half in `ui/` (`*_runtime.rs`,
`plain_disposal.rs`, `buffer_snapshot.rs`), with `services/single_flight.rs`,
`services/sync.rs`, and `services/palette/charge_scope.rs` in between. Nothing
names it, so nothing bounds it, so it leaks into whatever file needs it.

The measured symptoms, and the constraint that produced them:

| Symptom | Measurement |
|---|---|
| Mechanism-named modules in the pure domain layer | 8 of 29 `model/` files; 6 have exactly one consumer |
| Unnamed field bundles at seams | 90 production functions with ≥6 parameters |
| Same value renamed while crossing a seam | `load_save.rs:1387` — `cancel_pending_load` → `explicit_destination` |
| Shadow introspection API | 300 `pub fn *_for_test`, versus 18 typed `Automation*Snapshot` |
| Test scaffolding interleaved with domain logic | 639 `#[cfg(feature = "test-utils")]` sites; 23 files with ≥10 |
| Hand-maintained mutation exclusions | ~40 `exclude_re` entries naming GTK adapter methods |

The constraint: `.cargo/mutants.toml` `examine_globs` covers `model/**` and
`services/**`, plus exactly two hand-listed UI files. Pure policy that needs
mutation coverage has therefore been hoisted into `model/` regardless of whether
it is domain. The architecture is being shaped by a tooling glob.

## Goals / Non-Goals

**Goals:**

- Make each workflow readable in one place by one person, with the machinery
  reachable but not in the reading path.
- Make the correctness invariants the robustness programme established
  *verifiable by reading* — especially seam freshness, which is currently
  verifiable only by cross-referencing four signatures.
- Give the coordination tier a name, a home, and a boundary, the way the GTK Lush
  crates gave the toolkit tier one.
- Size and govern the full-codebase migration before starting it, so the shape
  cannot silently fork.
- Preserve every behavioral guarantee, every test, and the public D-Bus contract.

**Non-Goals:**

- Migrating any workflow other than the search-panel exemplar.
- Any user-visible behavior change, including timing, notification, and
  persistence behavior.
- Changing the externally visible D-Bus automation contract.
- Reifying any workflow's inverted drain control flow as an explicit state
  machine. That is the highest-risk item in the exploration and is deliberately
  left for a later, separately justified change.
- Extracting anything into a GTK Lush crate. The coordination tier being named
  here is LushText-specific, not generic toolkit machinery.
- Introducing traits, generic repositories, controllers, or manager objects to
  move code. `gtk-adapter-module-boundaries` already forbids this and the
  prohibition stands.

## Decisions

### D1: Co-locate pure policy with its consumer instead of creating a new named layer

The exploration surfaced two ways to stop `model/` from mixing domain and
mechanism: move mechanism *up* into a new named policy layer, or move it *down*
beside its consumer.

Chosen: **down**, as `ui/<workflow>/policy.rs`, for the six single-consumer
modules. `editor_memory` (five consumers spanning editor and window) and
`migration_ledger` (three consumers including `services/`) are genuinely
cross-cutting and stay where they are.

Rationale: a new horizontal layer would be a fourth place to look, and the
dependency data says these are not shared policy — they are the pure half of one
workflow. Moving them down collapses two problems into one move: `model/` becomes
21 files of actual domain, and the workflow's story becomes co-located, which is
the prerequisite for a narrative facade.

Alternative considered — a `policy/` sibling to `model/` and `services/`: rejected
because it preserves the split-brain reading experience (the workflow's logic is
still in another directory) while adding a layer whose only justification is
mutation tooling.

Alternative considered — leave everything in `model/` and fix only naming:
rejected because it keeps the parking-lot incentive alive. The next robustness
change would add `model/notes_admission.rs` for the same reason.

Purity is preserved by convention and enforced mechanically: `policy.rs` files
must contain no GTK imports, which is a grep-checkable property and is added to
the policy script.

### D2: Mutation scope becomes a naming convention, and this requires amending standing guidance

`.cargo/mutants.toml` gains `crates/lushtext-core/src/ui/**/policy.rs` to
`examine_globs`. The two hand-listed UI files and the ~40 `exclude_re` entries
enumerating `LushtextEditorPage::minimap_*` method names are retired as their
workflows migrate — the minimap entries specifically become unnecessary once pure
projection math is in `policy.rs` and the adapter is not in scope at all.

This directly contradicts `.agents/rules/build.md:378-381`, which says to keep UI
modules out of the cargo-mutants scope. That rule was written when the only way
to include UI code was to include whole adapter files, which is what it was
protecting against. It must be amended to distinguish *pure policy modules by
convention* (in scope) from *GTK adapters* (out of scope), rather than
distinguishing by directory.

Every policy relocation must ship mutation-coverage parity evidence: the same
mutants are generated and killed after the move as before. `make mutants-diff` is
the mechanism.

### D3: Reify seam identity as value objects, because the compiler is the only reviewer that scales

The `cancel_pending_load` → `explicit_destination` defect is the archetype: it is
invisible to review, invisible to tests (both names denote the same value today),
and it sits in stale-save rejection. The only durable fix is a type.

Each workflow gains an intent/identity value object constructed once at the
workflow entry point and validated as a unit. For the exemplar this is the search
flight identity; the same shape later applies to save intent, load intent, and
draft intent.

Rule: a value object is required when a field bundle crosses **two or more**
function boundaries or is reconstructed at two or more call sites. A single
five-parameter local helper does not need one. This bounds the work — it targets
the seams, not every long signature.

The existing `#[expect(clippy::too_many_arguments)]` at `load_save.rs:1373` is
treated as a marker of an unreified seam, not as an accepted exception. The
sweep change asserts zero such expectations remain in workflow code.

### D4: Evidence surfaces are internal types; automation snapshots project from them

The exploration left open whether test evidence should converge with the
`Automation*Snapshot` types or merely sit beside them. Converging fully would tie
the test surface to a public D-Bus contract, which risks either freezing tests or
inflating the external contract.

Chosen middle path: each workflow owns an internal typed evidence surface
(`evidence.rs`) that is the single source of workflow state for observers. Tests
read it directly. `Automation*Snapshot` types **project from** it rather than
gathering widget state independently. The public D-Bus schema does not change.

This kills the duplication (the automation spine and the test API stop
independently reimplementing "is the queue drained"), gives tests typed
observation instead of 300 scalar getters, and extends
`make check-automation-docs` drift coverage to the evidence surface — which is
where the auditability payoff comes from.

Alternative considered — make evidence public and drive automation from it
directly: rejected as an unnecessary widening of the external contract for an
internal readability goal.

### D5: Split `_for_test` by kind, and defer actuation

The 639 sites are not one problem. Classification and disposition:

| Kind | Count | Disposition in this programme |
|---|---|---|
| Inspection (`*_for_test` getters) | 351 (300 `pub`) | Collapse into `evidence.rs` per workflow |
| Configuration (delay/limit `static Atomic`) | 45 | Collapse into one per-workflow test policy value |
| Actuation (`autosave_tick_for_test`, `cancel_open_file_for_test`) | ~150 | **Deferred to a later change** |
| Probes / resets | 16 | Keep; they are legitimate lifecycle hooks |

Actuation seams exist because the real path runs through a `GtkFileChooser` or
`AdwAlertDialog` that headless tests cannot drive. They are evidence of a missing
workflow/dialog-presentation boundary. Fixing that is a design change with real
behavioral risk, so it gets its own change with its own justification rather than
being smuggled into a readability sweep.

### D6: Census before migration, and the matrix is the governing artifact

`docs/workflow-readability-matrix.md` enumerates every workflow with a stable row
id before the exemplar is migrated. It follows the established idiom of
`docs/accessibility-matrix.md`, the GTK Lush adoption matrix, and
`docs/end-user-coverage.md`: stable ids, explicit status, and a `check-policy`
gate that fails when a migrated workflow lacks a row or a row's claimed evidence
is missing.

Enumerating first is the mechanism that prevents the failure mode of
"pilot, then refine": the outliers are classified before the shape is normative.
Three are already known and must be resolved during the census, not discovered
later:

- `minimap.rs` (3,779 lines, pixel-verified geometry, 40 lines of mutation
  exclusions) — the largest potential win and the hardest fit.
- `editor_memory` (five consumers) — genuinely cross-cutting; the census must say
  so explicitly so a later change does not force it into one workflow.
- `markdown_preview` — decomposed days ago into 2,634 lines plus four modules; the
  census must decide whether that decomposition already satisfies the convention
  or needs a facade and evidence surface.

### D7: Vertical migration slices, ordered by risk, with retroactive amendments

Migration proceeds one workflow at a time (all layers for that workflow), not one
layer at a time across all workflows. The deliverable is a workflow that reads as
a story, which only materializes when facade, intent, policy, and evidence all
land together. Horizontal slicing would touch every file four times and never
produce a readable workflow until the final tier.

Order: search/replace and palette → save and load → draft/recovery and session →
workspace tree and notes → minimap → residual sweep. Risk increases monotonically;
user-data workflows come after the pattern has two proofs.

Governance rule: if a later workflow proves the convention wrong, the change that
amends the convention **must re-migrate every already-migrated workflow in the
same change**. Without this the programme produces coexisting convention
generations, which is precisely the disease being treated.

### D8: Standing guidance is revised in this change, not after it

Rules and skills are how this repo transmits convention across sessions. Leaving
them stale would make the convention decay immediately, and one rule already
contradicts it. Guidance revision is therefore in scope here, with a requirement
that no standing instruction contradict the convention.

Ownership map for the revision:

| Artifact | What changes |
|---|---|
| `.agents/rules/build.md` | Mutation scope by convention (D2); matrix and policy-check targets |
| `.agents/rules/rust.md` | Reframe "Coordination Vocabulary" as an implementation tier beneath domain vocabulary; add the seam value-object rule (D3); add `policy.rs` purity |
| `.agents/rules/widget-wiring.md` | Evidence surfaces replace ad-hoc `_for_test` inspection in widget tests |
| `.agents/rules/documentation.md` | Add the matrix to mandatory-update triggers |
| `rust-hex-arch` skill | Owns the workflow module shape and policy co-location |
| `gtk-testing` skill | Owns evidence-surface usage and the `_for_test` taxonomy |
| `rust-comments` skill | Owns the narrative-facade documentation expectation |
| `gtk-perf-review`, `data-safety` skills | Updated references to relocated policy modules |
| `AGENTS.md`, `README.md` | Module layout and architecture overview |

### D9: The programme record is a durable, discoverable artifact, not session context

This change is Phase 0 of roughly seven. Its reasoning, its measured baseline, and
the scope of the changes that follow currently exist only in the conversation that
produced it. Once this change is archived, a future session would find an empty
`openspec list` and no statement that six migrations and one sweep were expected.

The repository already has the mechanism: `docs/next/` holds 31 planned-work
documents, and `docs/next/gtk-lush.md` plus `crates/gtk-lush/GOVERNANCE.md` are the
precedent for a multi-phase programme with an explicit current posture and dormant
gates. The gap is discovery: `.agents/rules/documentation.md` does not list
`docs/next/` as a mandatory-update trigger, and planned-work documents are found
today only when `AGENTS.md` happens to mention them inline.

Therefore this change writes `docs/next/workflow-readability.md` as the programme
record and makes it reachable from the surfaces a future session loads
automatically. Three carriers with different jobs:

| Carrier | Job | Discovery |
|---|---|---|
| `openspec/specs/workflow-readability-boundaries/spec.md` | The normative contract | Permanent; reached from rules |
| `docs/workflow-readability-matrix.md` | Per-workflow status and evidence | Gated by `make check-policy` |
| `docs/next/workflow-readability.md` | Why, baseline, sequencing, remaining scope | Pointer from `AGENTS.md` and the matrix |

The archived change (`proposal.md` + this document) remains the primary source for
rationale, so the programme record links to it by change name rather than restating
it. The programme record's own job is to answer, in one read: what problem this
solves, how much is done, what is next, what is deferred and why, and what would
justify taking the deferred work on.

### D10: Baseline quantification and the unblock point are recorded, not inferred

Two facts a future session cannot reconstruct from the tree, and which therefore
must be written down:

**How much this change actually migrates.** The exemplar is 4,762 of the 79,017
lines in `ui/` + `model/`, or roughly six percent. It moves 2 of 8 policy modules,
covers 48 of 639 test seams, and reifies 2 of 90 long signatures. Everything else
that this programme promises is in the migration changes. Without this recorded, a
future session could read the completed capability spec and reasonably conclude the
work is done.

**Where the migration changes become writable.** They are blocked on the census
(section 1) and the four settled open questions (section 2), not on the exemplar.
After section 2, all five migration changes can be authored with real value-object
names, real per-workflow seam counts, and real risk tiers. The exemplar only refines
the task template. Recording this prevents a future session from either authoring
migrations on guesses or waiting for the whole change unnecessarily.

**Expected remaining scope**, to be kept current in the programme record:

| Change | Scope | Artifacts expected |
|---|---|---|
| 2 | search/replace + command palette | proposal + tasks |
| 3 | save + load | proposal + tasks |
| 4 | draft/recovery + session | proposal + tasks |
| 5 | workspace tree/watch + notes | proposal + tasks |
| 6 | minimap | proposal + design + tasks |
| 7 | residual sweep: retire remaining inspection seams, argument-count suppressions, `exclude_re` entries, matrix completion | proposal + tasks |
| deferred | actuation seams (missing workflow/dialog-presentation boundary) | not scoped |
| deferred | state-machine reification of inverted drains | may never be justified |

Migration changes are expected to be thin because this change holds the contract:
they consume `workflow-readability-boundaries` and check off matrix rows rather than
adding requirements. A migration change that needs a new capability or a spec delta
is a signal that this change's contract was incomplete, and the retroactive
amendment rule (D7) applies.

## Risks / Trade-offs

**[Refactoring immediately after a robustness programme is the highest-risk
refactor category — the invariants are subtle and tests are the only thing holding
them]** → Sequence strictly by degree of mechanical verification: renames and
value objects are compiler-verified; policy relocation is verified by mutation
parity; evidence consolidation is verified by test-suite equivalence plus the
automation docs gate. Nothing in this change alters control flow. The exemplar is
the workflow that touches no user data.

**[Moving pure policy out of `model/` silently drops mutation coverage]** →
`make mutants-diff` parity evidence is a required task, not an optional check, for
every relocation. The policy script additionally fails if a `policy.rs` exists
outside `examine_globs` reach.

**[Evidence consolidation could weaken test coverage by replacing 300 specific
assertions with a coarser surface]** → The evidence surface must expose every
field the retired getters exposed; the sweep change asserts the retired functions
have no remaining callers rather than deleting assertions. Test count must not
drop.

**[A 14-workflow programme could stall halfway, leaving two conventions in the
tree]** → Vertical slicing makes a partial programme coherent: migrated workflows
are readable, unmigrated ones are unchanged, and the matrix says which is which.
The retroactive-amendment rule (D7) prevents the worse failure of forked
conventions.

**[The census could be treated as paperwork and filled in shallowly]** → Row
completion requires naming the current file set, the target file set, the
consumer count, and the risk tier for each workflow. A row that cannot name its
seam value object is not complete.

**[`policy.rs` as a convention name could be applied to impure code over time]** →
Grep-checked in the policy script: no GTK/GLib/GIO/Adwaita/SourceView imports in
any `policy.rs`.

## Migration Plan

1. **Census.** Enumerate workflows into the matrix. Resolve the three known
   outliers explicitly. No code changes.
2. **Convention.** Write the two new capability specs. Amend the three modified
   capabilities.
3. **Enablers.** `.cargo/mutants.toml` convention; policy check script wired into
   `make check-policy`; guidance revision per D8.
4. **Exemplar.** Migrate `ui/search_panel/` to the target shape, including the
   relocation of `search_flight` and `search_retirement`, one intent value object,
   and one evidence surface replacing the panel's `_for_test` inspection getters.
5. **Verification.** Full gate run plus mutation parity plus behavior equivalence
   for search and replace.

Rollback: every step is independently revertable. The exemplar is one directory
plus two file moves; reverting it does not affect the census, the specs, or the
guidance revision, which retain standalone value as documentation of the target
state.

## Resolved Questions

All four questions this change opened were settled in section 2 against census
evidence, and each is now recorded in a capability spec. They are closed: a later
change that wants to revisit one must amend the spec and honor the retroactive
amendment rule (D7).

### `plain_disposal` placement (task 1.7) — cross-cutting, stays

Resolved during the census. `model/plain_disposal.rs` has exactly one consuming
file, but that file is `ui/plain_disposal.rs`, its **own coordination adapter**,
which is consumed by 21 files across 10 workflows. `DisposalOwned<T>` appears in the
signatures of search/replace, drafts, load, local history, buffer replacement,
markdown preview, command palette, notes, session persistence, and buffer snapshot.

It also stays out of GTK Lush scope: it encodes LushText payload admission and
retirement policy, not generic toolkit machinery. No rename is needed —
`plain_disposal` already names the domain concept rather than a mechanism.

The census exposed a defect in the relocation rule this question was testing: a
naive single-consuming-file test would have relocated it wrongly. The rule is now
stated in terms of **owning workflows**, not consuming files, in
`workflow-readability-boundaries`.

### Coordination role file naming (task 2.2) — bounded set of role names

The census found `runtime.rs` already present in four places naming three different
jobs: streaming execution (`search_panel/runtime.rs`, 978 lines), compact request
types (`command_palette/runtime.rs`, 59 lines), and byte admission
(`save_runtime.rs`, `load_runtime.rs`). Meanwhile
`ui/sidebar/workspace_section/` already uses role names for its three coordination
files (`tree_loading.rs`, `watch.rs`, `refresh.rs`), none called runtime.

A single fixed name is also structurally impossible without restructuring:
`ui/editor_page/` hosts 8 workflows and `ui/window/` hosts 12. Fixing one name would
force a subdirectory-per-workflow move across roughly 20 workflows, which is out of
scope and would itself require re-migration under D7 if decided later.

Resolved: a **bounded set of role names** stating the coordination job (admission,
execution, retirement, watch), extended only by amending
`gtk-adapter-module-boundaries`. `policy.rs` and `evidence.rs` keep fixed names.

### Facade size ceiling (task 2.3) — requirement now, number from the exemplar

The design's suggested ~300 lines is not supported by measurement. The smallest
existing thin public surface is `ui/window/mod.rs` at 247 lines, and the exemplar's
own `ui/search_panel/mod.rs` is already 578 lines for a workflow with 6 control-flow
inversions. Facades must narrate 3 to 7 inversions depending on the workflow, so a
number chosen in advance risks forcing the narration itself to be split, which
defeats the facade.

There is also a sequencing tension inside this change: task 5.10 measures the
exemplar facade as the input to this question, but this question sits in section 2,
ahead of section 5.

Resolved: the spec records the **requirement** that a budget exists and that the
first migration change sets the number from its measured facade. Under D7 the
cheapest moment to correct a wrong number is when exactly one workflow is migrated,
which is precisely that moment.

### Hard zero for argument-count suppressions (task 2.4) — zero in workflow code

The census makes this nearly forced. Clippy's `too_many_arguments` threshold is 7,
so it warns at 8 or more parameters. Exactly two functions in the crate have 8 or
more, and both are already suppressed:

- `ActionCatalogEntry::new` (`model/action_catalog.rs:181`) — 12 parameters, a
  `const fn` building static catalog rows, whose reason cites the automation
  contract.
- `begin_admitted_save` (`ui/editor_page/load_save.rs:1376`) — 9 parameters, the
  unreified save seam that `QueuedSaveTicket` removes.

The premise behind the allowlist option is false: `encoding.rs::append_choice_row`,
cited as the builder-style exception needing one, has 7 parameters and therefore
never triggers the lint.

Resolved: the residual sweep asserts **zero in workflow adapter and coordination
code**, with no allowlist. Domain catalog construction in `model/` is outside the
workflow-seam rule and keeps its reasoned suppression.
