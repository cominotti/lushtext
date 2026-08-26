# Workflow Readability — Programme Record

Status: **Phase 0 complete, slot 3 complete in both halves, four migration
changes outstanding (4 through 7).** The convention is normative, the census is complete, the
mechanical gate is wired into `make check-policy`, the normative facade line
budget is declared and enforced, and four workflows are migrated:
`WFR-SEARCH-REPLACE` (**both halves** — search and preview in slot 1, the
Replace All write path and its undo journal in slot 2b),
`WFR-COMMAND-PALETTE` (slot 2a), `WFR-DOCUMENT-SAVE` (slot 3a, the first
tier-3 workflow migrated on its own), and `WFR-DOCUMENT-LOAD` (slot 3b, the
second). **`ui/editor_page/load_save.rs` no longer exists**: slot 3a lifted the
save half out and slot 3b dissolved the rest, so the programme's third measured
symptom is now history rather than a live file. Everything else in `ui/` and
`model/` is unchanged and behaviorally untouched. Slot 4 is next and slots 4
through 7 are authorable now; two items are deliberately deferred and may never
be taken on.

This document answers, in one read: what problem the programme solves, how much
is done, what is next, what is deferred and why, and what would justify taking
the deferred work on. It is the narrative carrier; the normative contract is
spread across **five** capability specs, and a migration must read all of them
that touch its scope:

| Carrier | Job |
| --- | --- |
| `openspec/specs/workflow-readability-boundaries/spec.md` | the workflow module shape, the facade contract and its size-budget rule, seam value objects, intent-first naming, the census matrix, risk tiers, retroactive amendment |
| `openspec/specs/workflow-evidence-surfaces/spec.md` | evidence surfaces, their single visibility rule, the inspection/configuration/actuation/probe seam taxonomy, the evidence→automation projection relationship |
| `openspec/specs/gtk-adapter-module-boundaries/spec.md` | the decomposition contract and **the bounded set of coordination role names** (`admission`, `execution`, `retirement`, `watch`, `journal`); adding a role name amends *this* spec, not the two above |
| `openspec/specs/mutation-testing/spec.md` | the `ui/**/policy.rs` scope convention and the mutation-parity requirement for relocated policy |
| `openspec/specs/dbus-automation-spine/spec.md` | snapshots project from evidence while the exported D-Bus contract stays unchanged |
| `docs/workflow-readability-matrix.md` | per-workflow status, roles, seams, risk tier, slot; gated by `make check-workflow-boundaries` |
| this file | why, baseline, sequencing, remaining scope, deferrals |

Slot 2b's `replace.rs` role-name decision landed on the third row, and it needed
both mechanisms. The stage-order qualification rule slot 2a added covered the
preview half (`replace_execution.rs`, because `execution.rs` in that directory is
already the search stage order's execution module), but no listed role name
described the journal half's job, so 2b amended
`gtk-adapter-module-boundaries` to add **`journal`**: maintaining a durable,
generation-guarded record that a later stage of the same workflow reads back.
`retirement` was the closest existing name and means its opposite. That amendment
triggered the section 8 re-check of every migrated row; both passed as
confirmations, recorded in the matrix's
[Retroactive amendment](../workflow-readability-matrix.md#retroactive-amendment)
section.

Full rationale for every decision below lives in the OpenSpec change
**`normalize-workflow-readability-boundaries`** (`proposal.md` + `design.md`,
decisions D1–D10 and Resolved Questions). Look for it under
`openspec/changes/` or `openspec/changes/archive/*-normalize-workflow-readability-boundaries/`.
If this document and the capability specs ever disagree, the specs win and this
document must be corrected in the same change.

## 1. The problem, measured

Fourteen robustness changes landed between 2026-07-10 and 2026-07-24. They are
correct and worth keeping. They left a codebase readable only to someone already
holding the coordination machinery in their head. Four measured symptoms:

1. **The domain layer became a testability parking lot.** 8 of `model/`'s 29
   files were named after mechanism (`save_admission`, `search_flight`,
   `search_retirement`, `plain_disposal`, `migration_ledger`, `editor_memory`,
   `buffer_replacement`, `minimap_analysis`) rather than domain, and 6 of those 8
   had exactly one consumer, almost always in the adjacent `ui/` directory. They
   lived in `model/` because `.cargo/mutants.toml` examined only `model/**` and
   `services/**`: a tooling glob, not a design decision, was shaping the
   architecture.
2. **Field bundles crossed layer seams unnamed, and drifted while crossing.**
   the programme published this as "90 production functions take six or more
   parameters"; the census measured **88** under that same receiver-counted
   definition, and 43 under the strict non-receiver definition (matrix,
   Measurement Definitions). Use 88 receiver-counted or 43 strict, and always say
   which. At
   `ui/editor_page/load_save.rs` the same value was passed as
   `cancel_pending_load` and received as `explicit_destination`, inside
   stale-save rejection. A reader cannot verify that by reading it.
3. **No workflow had a narrator.** Answering "what happens on Ctrl+S" required
   13 hops across 6 files, with control inverted through an `idle_add_once`
   drain, and no document or module narrated it. `load_save.rs` was 1,795 lines
   holding two distinct workflows.
4. **Two parallel introspection APIs existed and did not know about each
   other.** 18 typed `Automation*Snapshot` types (read-only over D-Bus,
   documented, drift-gated) alongside 639 `#[cfg(feature = "test-utils")]` sites
   growing a shadow API of 300 externally reachable `*_for_test` functions over
   overlapping state, with no types, no documentation, and no drift gate.

None of this called for a new abstraction layer. The hexagonal boundary held
(`model/` and `services/` contain zero GTK imports) and the machinery modules
were individually well written. What was missing: **a workflow had no home where
its whole story fits** — pure policy, coordination, adapter, and evidence were
scattered across four directories and two vocabularies.

## 2. Baseline: how much Phase 0 actually migrated

This section exists because a future session could read the completed capability
specs and reasonably conclude the work is done. It is not. Phase 0 established
the shape, proved it once, and enumerated everything else.

| Quantity | Phase 0 planned (design.md D10) | Phase 0 actual | Note |
| --- | --- | --- | --- |
| Share of `ui/` + `model/` migrated | 4,762 of 79,017 lines ≈ 6% | unchanged: the exemplar workflow's censused `ui/` + `model/` footprint is 4,762 lines | ≈6% is the workflow's footprint, not the diff size; the other ~94% is untouched |
| Policy modules relocated | 2 of 8 | 2 relocated (`model/search_flight.rs`, `model/search_retirement.rs` → `ui/search_panel/policy.rs`); `model/` went 29 → 27 files | the denominator is better stated as **2 of 7 relocation candidates**: 4 of the 8 mechanism modules are cross-cutting and stay, and the census found 3 previously unlisted single-workflow modules (`workspace_scan`, `workspace_persistence`, `workspace_search`) |
| Test seams addressed | 48 of 639 | the exemplar workflow held exactly **48** of the 639 `#[cfg(feature = "test-utils")]` sites, so the planned figure was right; it now holds 40, and its 23 `*_for_test` functions became 7. Repo-wide: 639 → 631 sites, 351 → 335 `*_for_test` functions | the drop is smaller than the seam count because consolidation replaced 23 scattered getters with one typed surface plus one test-policy module, both still feature-gated |
| Long signatures reified | 2 of 90 | **1 seam** reified (`ReplacePreviewTicket` + `ReplacePreviewFacts`), and **0** long signatures shortened | census finding 3: this workflow contains exactly one function with ≥6 non-receiver parameters and it is `SearchResultItem::new_match`, a row-item constructor the convention exempts. The programme's real unit is *seams reified*, not signatures shortened. Later changes MUST report seams, and may report signatures only as a secondary number. |

**Reconciling the exemplar's seam figures**, because three different numbers
describe them and a slot-2 author sizing its own evidence work must not mix them.
The workflow held **23 `*_for_test` functions** before the migration. They went:
8 inspection getters retired into `evidence.rs` (which is the matrix Product
Matrix cell's "8 inspection fns"), 5 configuration setters plus 3 configuration
readers collapsed into `SearchPanelTestPolicy`, and **7 remaining** — 5 actuation
seams on the replace/undo transaction and 2 accessibility probes. 8 + 5 + 3 + 7 =
23. Separately, the matrix's "Migrated Workflow Roles" prose mentions "23
observation getters" moved out of `mod.rs`; that counts observation accessors
relocated into `evidence.rs`, which is a different population from the 23
`*_for_test` functions. For an **unmigrated** workflow, size evidence work from
the Product Matrix's per-kind `Seams (i/c/a/p)` tuple, which is the population
consolidation acts on. `WFR-SEARCH-REPLACE`'s own cell no longer holds a tuple —
once a row is migrated the cell records what the migration did instead, which is
why the reconciliation above exists.

### Baseline after slot 2a

| Quantity | After slot 1 | After slot 2a |
| --- | --- | --- |
| Workflows migrated | 1 (`WFR-SEARCH-REPLACE`, search/preview half) | 2 (plus `WFR-COMMAND-PALETTE`) |
| Share of `ui/` + `model/` migrated | 4,762 of 79,017 censused lines ≈ 6% | 4,762 + 3,282 = 8,044 ≈ 10%. The palette's censused `ui` + `model` footprint is 3,282 (ui 2,528 + model 754); as with slot 1 this is the *workflow's footprint*, not the diff size |
| Policy modules relocated | 2 of 7 relocation candidates | still 2 of 7. The palette had **no** `model/` relocation candidate: its pure logic was inline in the GTK adapter, so `ui/command_palette/policy.rs` is newly extracted rather than moved, and its mutation coverage is a gain from zero rather than a parity claim |
| Test seams addressed | repo-wide 639 → 631 sites, 351 → 335 `*_for_test` functions | the palette held 12 `*_for_test` functions and 22 gate-attribute sites in `ui/command_palette/**`; 5 inspection functions and 2 configuration setters retired, 1 renamed actuation seam added where tests previously reached through `imp()` untracked, so the directory now holds 6 |
| Seams reified | 1 (`ReplacePreviewTicket` + `ReplacePreviewFacts`) | 2 (plus `FileIndexMutationTicket` + `FileIndexMutationFacts`). The query seam needed **no** new type: its coordinator owns the generation and exposes `is_current`, which the convention accepts. Long signatures shortened: still 0 — the palette had no function with ≥6 non-receiver parameters, so the secondary figure remains uninformative and *seams reified* stays the primary unit |
| Automation projections | 1 (`window.content_search`) | 2 (plus `window.command_palette` and both palette readiness blockers), now gated by an implemented evidence-to-snapshot drift check |
| Facade line budget | measured 350, not declared | **declared at 370 and enforced** |

### Baseline after slot 2b

| Quantity | After slot 2a | After slot 2b |
| --- | --- | --- |
| Workflows migrated | 2 | still 2, but `WFR-SEARCH-REPLACE` is now migrated **end to end** rather than for its search/preview half only |
| Share of `ui/` + `model/` migrated | 8,044 of 79,017 censused lines ≈ 10% | unchanged at ≈10%. Slot 2b finished a row already counted; the censused footprint is per workflow, not per half |
| Policy modules relocated | 2 of 7 relocation candidates | **still 2, and the denominator drops to 6.** `model/workspace_search.rs` was resolved as domain and stays, so it is no longer a candidate. Slot 2b's own policy work was an extraction from an unscoped GTK adapter into `ui/search_panel/policy.rs`, like the palette's — a coverage gain from zero, not a relocation |
| Test seams addressed | palette directory holds 6 `*_for_test` functions | `ui/search_panel/**` unchanged at 7 (5 replace/undo actuation seams plus 2 accessibility probes). Slot 2b retired **no** inspection function, because slot 1 already retired all eight, and it added no new seam, no new override static, and no new `.imp()` reach-through. Instead it added 10 fields to the existing evidence surface and migrated 10 widget-test waits off direct `search_backup` reads |
| Seams reified | 2 | **3** (plus `UndoRestoreClaim`, the panel/window undo-restore claim seam). Long signatures shortened: still 0 — no function in this half has ≥6 non-receiver parameters, so *seams reified* remains the primary unit and the receiver-counted 88 / strict 43 signature figures stay uninformative here |
| Automation projections | 2 | still 2, by design. Slot 2b's obligation was no-widening, proved by a zero-difference before/after `content_search` and readiness diff |
| Facade line budget | declared at 370 and enforced | **held on its third test, with 1 line to spare.** The search facade grew from 350 to **369** while gaining a fifth Replace All stage, two module names, and the six Replace All inversions it had not been narrating. Reaching 369 took folding module-ownership detail into the role table, compressing every inversion bullet, and delegating the options-row reveal out of the facade; the first honest narration measured 379. **The 20 lines of headroom slot 2a declared are now 1**, which the matrix's budget section records as its own "real evidence" trigger firing — slot 3 must plan against 1 line |
| Bounded coordination role names | `admission`, `execution`, `retirement`, `watch` | **plus `journal`**, the one amendment the convention sanctions. Both migrated rows re-checked as confirmations |

Slot 2a also closed one Phase-0 gap that was specified but never built: the
evidence-to-snapshot drift check `workflow-evidence-surfaces` requires of
`make check-automation-docs`. Slot 1 could leave it unnoticed with one
projection; making projections plural made it load-bearing.

Two further Phase 0 facts a later session should not have to rediscover:

- **The exemplar's facade measures 350 physical lines** (75 module-doc narration
  lines, 166 non-comment non-blank), narrating 6 inversions across two stage
  orders, down from a 578-line `mod.rs` that also held the accessibility
  projection and 23 observation getters. It measured 357 when the migration
  landed and 350 after the result-cap fix delegated one more stage. **The
  normative facade line budget is declared and active at 370 physical lines**,
  set by slot 2a from that measurement plus modest headroom; see the matrix's
  "Facade size budget" section for the derivation, the stated headroom risk, and
  the machine-readable declaration the gate reads. A later slot must treat 370 as
  settled convention rather than re-deriving it.
- **A pre-existing product defect was found and fixed while proving behavior
  equivalence** (commit `f0ab1d9`): capped workspace-search results were
  silently discarded because the service's result cap wrote the caller's
  `cancel` flag, so the panel's `if !cancelled` tick arms dropped the
  `ResultCap` notice and every buffered match. Fixed at the content-search
  boundary with a private `WalkStop` value separating caller cancellation from
  service termination. **Slot 2 must not re-plan this.** The matrix records the
  stop semantics under `WFR-SEARCH-REPLACE`.

### Baseline after slot 3a

| Quantity | After slot 2b | After slot 3a |
| --- | --- | --- |
| Workflows migrated | 2 | **3** (plus `WFR-DOCUMENT-SAVE`). Counted as workflow *halves*, four are now migrated: search/preview, replace/undo, the palette, and save |
| Share of `ui/` + `model/` migrated | 8,044 of 79,017 censused lines ≈ 10% | 8,044 + 2,132 = 10,176 ≈ **13%**. The save workflow's censused `ui` footprint is 2,132; the row's old 6,672 figure double-counted two whole shared service files and the load half of `load_save.rs`, and slot 3a corrected that cell |
| Policy modules relocated | 2 of 6 relocation candidates | **3 of 6.** `save_admission.rs` is the **third** relocation and the first since slot 1; `model/` went 27 → 26 files. Slot 3a *also* extracted new pure decisions from the GTK adapter into the same `policy.rs`, which is a coverage gain from zero and is reported separately from the relocation's parity numbers |
| Test seams addressed | `ui/search_panel/**` at 7, palette directory at 6 | the save workflow retired **every** inspection seam it had — 4 call surfaces over 3 mechanisms — and added **no** per-field getter. 3 editor-side actuation seams preserved plus 3 chooser-bound Save As seams, all programme-level deferrals. **1 new actuation seam, counted and justified** (`expire_close_save_session_for_test`), the first in the programme; 4 of the 5 ungated `imp()` write sites in the widget tests became real drives of the workflow instead |
| Seams reified | 3 | **4** (plus `QueuedSaveTicket` + `QueuedSaveFacts`). **Long signatures shortened: 1 — the programme's first.** `begin_admitted_save` went from 8 parameters to 5 and lost the only non-catalog `#[expect(clippy::too_many_arguments)]`, so the workspace count drops 2 → 1 and the survivor is the exempt domain catalog constructor. The secondary signature figure is finally informative: under the strict non-receiver definition this workflow held one function at 8 parameters, and it is now gone |
| Automation projections | 2 | **3** (plus `tabs[].saving`, projecting from `SaveEvidence` and now covered by the Evidence Projection Map drift gate). The `save` readiness blocker reads the same state through the facade's cheap `is_saving()` accessor instead, because building a whole surface per editor per readiness poll to read one bool is waste — the two are identical by construction. The exported names, types, and semantics are unchanged |
| Facade line budget | held at 369, 1 line spare | **held comfortably: the save facade measures 223 of 370.** See the friction note below — a one-stage-order facade is not the case that tests this number |
| Bounded coordination role names | `admission`, `execution`, `retirement`, `watch`, `journal` | unchanged. `journal` was checked against save and **rejected**; see the friction note |
| Permitted role homes | flat, workflow-scoped names in a shared directory | **plus the per-workflow subdirectory** (`ui/editor_page/save/`), for directories hosting several workflows where the fixed `policy.rs` / `evidence.rs` names collide. Both migrated rows re-checked as confirmations, zero renames |

### Baseline after slot 3b

| Quantity | After slot 3a | After slot 3b |
| --- | --- | --- |
| Workflows migrated | 3 | **4** (plus `WFR-DOCUMENT-LOAD`). Counted as workflow *halves*, five are now migrated: search/preview, replace/undo, the palette, save, and load. **Slot 3 is complete in both halves** |
| Share of `ui/` + `model/` migrated | 10,176 of 79,017 censused lines ≈ 13% | 10,176 + 1,635 = 11,811 ≈ **15%**. The load workflow's censused `ui` footprint is 1,635 — the post-3a `load_save.rs` residual (1,212) plus the retired `load_runtime.rs` (423). The row's old 5,301 figure pooled window files this row only *calls* and the whole of `services/editor_io.rs`, which it shares with save and every other read path; slot 3b corrected that cell |
| Policy modules relocated | 3 of 6 relocation candidates | **3 of 6, unchanged — and the candidate denominator drops again.** `model/file_load.rs` is resolved as **domain and staying**, joining `workspace_search.rs`. Slot 3b extracted new pure decisions from the GTK adapter into `ui/editor_page/load/policy.rs`, which is a coverage **gain from zero** (44 generated, 41 killed, 3 unviable, 0 missed) and is reported with no parity claim attached, because nothing moved |
| Test seams addressed | save retired 4 call surfaces over 3 mechanisms | the load workflow retired **10 inspection surfaces** and added **no** per-field getter; no `*_for_test` inspection function remains on the load path. 2 configuration seams collapsed into **1** test-policy value behind `#[cfg(feature = "test-utils")]`. **Actuation seams fell 8 → 7 and none was added** — `load_runtime::reset_for_test` folded into the editor-page seam rather than surviving as a second surface, which is the first time the programme *reduced* the actuation count. 16 ungated `imp()` sites were catalogued; the 5 load-state writes became real drives of the workflow, and the 11 document-metadata writes are recorded as cross-cutting and handed on |
| Seams reified | 4 | **5** (plus `LoadRequestTicket`). Two of the four originally-unreified seams remain: `WorkspaceWatchTicket` and `NotesBrowserTicket`, both slot 5. **Long signatures shortened: 1, unchanged** — this workflow introduced no `#[expect(clippy::too_many_arguments)]` and the workspace count holds at 1 (the exempt domain catalog constructor). The secondary figure under the strict non-receiver definition is unchanged at 43 |
| Automation projections | 3 | **4** (plus `tabs[].load_state`, projecting from `LoadEvidence`). The `file-load` readiness blocker and its **six** documented predicates read the same `load_state` cell through the cheap lifecycle accessor, identical by construction, rather than building a surface per tab per poll. Exported names, types, and semantics are unchanged, including that a failed load reports `workflow-failure` rather than readiness. **The drift gate itself had to grow**: two workflow surfaces now project into one snapshot object (`tabs`), so `check-automation-docs.py` attributes a projected field by the binding it is read through and keys the documented map by evidence type. Without that, each surface would silently appear to project the other's fields and a real rename would pass |
| Facade line budget | held; save at 223 of 370 | **held; load at 253 of 370.** No escalation, no parked state, and the budget line was not edited. The load facade narrates two more inversions and four more entry points than save's and still has 117 lines spare |
| Bounded coordination role names | unchanged | unchanged. Load is the first workflow to use **three** coordination modules (`admission`, `execution`, `retirement`), and `retirement` fit without stretching: the module gives back a `DisposalOwned` payload, an admission charge, a partial buffer, and the load identity. `journal` was checked first and rejected on slot 3a's reusable test |
| Permitted role homes | plus the per-workflow subdirectory | unchanged. `ui/editor_page/load/` is its **second adopter**, and the nested `ui/**/policy.rs` glob was re-verified reachable after the move — one adopter proves the glob resolves, two prove it is not a special case |
| Evidence-surface conventions | visibility rule settled | **plus the reentrancy constraint**, promoted from a per-workflow module note into stated convention with a required proof test. This is the first amendment whose obligation was real work rather than a confirmation: `WFR-COMMAND-PALETTE` lacked the proof and slot 3b wrote it |

### Convention friction slot 3b hit, recorded for slots 4 through 7

**Four rows are migrated after this change, so the retroactive-amendment rule now
costs four per-row re-checks — and slot 4 migrates four more rows, which would
take it to eight.** Slot 3b is the change that made that cost concrete rather
than theoretical: its amendment's content was a *proof obligation*, so
"the constraint already held" was not a discharge, and one of the three
already-migrated rows genuinely lacked the test. The same amendment landed after
slot 4 would have cost up to five tests instead of one. **Slots 4 through 7
should treat a convention correction as urgent, not as tidy-up.**

- **The promoted reentrancy constraint was *not* already satisfied everywhere.**
  Two of three migrated rows had the proof test in the right shape
  (`WFR-SEARCH-REPLACE` from slot 2b, `WFR-DOCUMENT-SAVE` from slot 3a);
  `WFR-COMMAND-PALETTE` had only a teardown-observation test, which proves a
  different property. Slot 4 should expect the same when it promotes anything:
  **check each row individually and do not accept "it must already hold".**
- **The per-workflow role home read well on a second adopter, and its one real
  cost is documentation links, not structure.** `ui/editor_page/load/` needed no
  new decisions. But a narrative facade lives in a `pub` module and naturally
  wants to name its own private coordination modules and `pub(crate)` seam types,
  and every such intra-doc link is a `rustdoc::private_intra_doc_links` error
  that **`make check` does not run**. Slot 3a shipped that failure to CI; slot 3b
  fixed it and recorded the gate command in `.agents/rules/build.md`. **Slot 4
  should run the rustdoc gate before shipping a facade.**
- **The 370-line budget was never in danger, and the reason is now settled.** The
  load facade narrates one stage order with seven inversions and seven distinct
  entry points and measures 273. Two data points now agree that **stage-order
  count is what stresses the budget** — not inversions, not entry points, not the
  risk tier. Slot 6 (minimap) is still the slot most likely to prove the number
  wrong, and slot 3b supplies no evidence either way.
- **The bounded role set covered cancellation and abort without an amendment, and
  `retirement` earned its third module.** The judgement that mattered was *not*
  splitting the install state machine: `execution` owns the forward phases and
  the session type, `retirement` owns the abort transitions and the
  cancelled-clear phase, and the dispatcher hands one phase across the boundary.
  Slot 4 will face the same call for draft and session restore; the test that
  worked here is **"is the job cohesive enough that a reader would look for it
  under its own name"**, not "are these functions adjacent".
- **The `data-safety` pass produced one confirmed finding, and it was a
  pre-existing one the refactor made visible.** Finalization dropped a parked
  request's background planning owner instead of carrying it forward or releasing
  it, which would strand whoever waited on that terminal — the session-restore
  sequencer counts exactly those releases to decide when to open the next
  document. It was unreachable in practice, and it was still fixed in this change
  rather than recorded as debt, per `.agents/rules/preexisting-blockers.md`.
  **Slot 4 owns the session-restore sequencer and inherits the fixed contract:
  every load terminal now either carries the planning owner into a restart or
  releases it, and no path drops it.**
- **Census inversion counts are floors: fourth confirmation.** The trace recorded
  four; the code has seven. Slot 4 should narrate from the code and budget for
  roughly double the census figure.
- **Row-scoped seam and size counts were wrong again, in both directions.** The
  size cell over-counted by pooling shared service files and called window files;
  the consumer count for `model/file_load.rs` under-counted (4 against a real 6),
  and *gained* a consumer since the census because slot 3a's relocation added
  one. **Re-derive, and expect the answer to move in either direction.**
- **One outright census gap was found, not just a miscount.**
  `ui/open_popover/**` and `ui/window/recent_open.rs` appear in **no** matrix
  row's file set. They are now assigned to `WFR-SHELL-LAYOUT` (slot 7), split
  from the load workflow along the coordination/presentation line. Slot 7
  inherits an assignment rather than a rediscovery.

### Convention friction slot 3a hit, recorded for slots 3b through 7

**Four workflow halves are migrated after this change, so the
retroactive-amendment rule now costs three per-row re-checks.** The window for
correcting the convention cheaply is narrowing exactly as section 8 predicted.

- **The bounded role set needed no addition, and `journal` was the near miss the
  record predicted.** Slot 2b warned it would look applicable in slot 3, and it
  does: a save writes durably and irreversibly. It was rejected on a test worth
  reusing — **`journal` requires that a later stage of the *same* workflow reads
  the record back**, with startup recovery. A save replaces the user's file
  bytes and no later save stage restores from them; the record that protects an
  unsaved buffer is the draft, which is `WFR-DRAFT-RECOVERY`'s (slot 4). Pulling
  draft persistence in to justify the name would have been the overload the
  bounded set exists to prevent. Slot 4 is where `journal` will genuinely fit.
- **The per-workflow subdirectory read well, and it was mechanically forced
  rather than stylistic.** `ui/editor_page/` hosts eight workflows; the fixed
  role names cannot be shared; and a prefixed `save_policy.rs` would have left
  the `ui/**/policy.rs` mutation scope, which is a blocking coverage regression
  rather than a naming preference. The amendment adds a permitted *location*, not
  an obligation: nothing about the two existing rows changed. **Slot 3b reuses
  this exact boundary for the load half.**
- **The 370-line budget was not tested by this facade, and that is the honest
  reading.** The save facade measures **223**, 147 under budget — but it narrates
  **one** stage order. Slot 2b's qualification stands and now has a second data
  point: what stresses the budget is the *number of stage orders*, not risk tier
  or inversion count. This workflow has five inversions, more than the palette's
  first stage order, and still fits easily. **Slot 6 (minimap) remains the slot
  most likely to prove the number wrong**, and slot 3a supplies no evidence
  either way.
- **Census inversion counts are floors — third confirmation.** The trace recorded
  four inversions; the code has five. The missing one was the mirror-back through
  the bounded buffer-replacement workflow, folded into a prose phrase as though it
  were a straight-line step. It was the most important one to name: it is where a
  clean tab and the bytes on disk are reconciled. **Write the narration from the
  code, every time.**
- **A durable path's "seam census" figures were pooled, not row-scoped.** The
  row's `Seams` cell counted `services/editor_io.rs`, `durable_write.rs`, and
  `filesystem/write.rs` seams that are shared with the load workflow and the
  fault-injection lane. Slot 2a hit the same shape with the palette. **Re-derive
  row-scoped counts before sizing evidence work**; slot 3b will find the same
  pool from the other side.
- **The reach-through census needs its own scoping pass, and one named site
  turned out to belong to another row.** The planned scope named
  `window.imp().session.save_failed` as a priority save site. It is not: that
  field is *session-file* save failure, owned by `ui/window/session_persistence.rs`
  (`WFR-SESSION-RESTORE`, slot 4). Only 5 ungated write sites are genuinely
  save-owned. **A field whose name contains "save" is not thereby save-workflow
  state.**
- **An ungated `imp()` *write* is usually a real drive in disguise.** Four of the
  five write sites became real workflow drives once an existing configuration
  seam (the save write delay) was used to hold the save in flight. Only the
  close-save session expiry genuinely needed a new named actuation seam, because
  a close session ends only when its `AdwAlertDialog` pipeline completes or is
  superseded. **Reach for an existing seam plus a real drive before adding one.**
- **An evidence surface that reads a template child is not stage-independent,
  and the widget lane is what catches it.** The convention says reading evidence
  must not require the workflow to be in a particular stage. Slot 3a's first
  surface satisfied that for timers, queues, and generations but still reached
  the editor's `GtkSourceView` **template child** to classify the buffer's
  capture mode — and GTK4 clears template children in `dispose()`, *before*
  Rust's `Drop`. A teardown test that disposes the page and then asks whether
  the save released its permit is a legitimate observation point, and it
  panicked with "Failed to retrieve template child". The fix is
  `TemplateChild::try_get()` plus an honest answer for a page with no buffer
  left. **Later slots: a disposed widget is a stage, and any evidence field
  derived from a template child needs the non-panicking accessor.** Note that
  the retired `*_for_test` getters did not have this problem, because each read
  exactly one narrow thing — consolidating into one surface is what made a
  widget read reachable from every observation point.
- **The `data-safety` pass over this durable path produced no new confirmed
  finding**, unlike slot 2b's two. That is a data point rather than a
  reassurance: this change deliberately moved the save path without re-sequencing
  it, and the ordering contract it preserves — target guard before bytes,
  `BeforeRename` versus `AfterRename` honesty, buffer-and-disk agreement before
  the tab goes clean — was already correct. **Later slots should still budget for
  findings.**

## 3. Remaining scope

Five changes remain (3b and 4 through 7). Order is by increasing risk; every
`tier-3` slot follows at least two completed lower-risk migrations. Three are
complete — slot 1's exemplar, slot 2a's palette, and slot 2b's replace/undo half
— so a slot-3 tier-3 workflow starts with one more proof than the rule requires.
The matrix's
"Migration Order And Risk Tiers" section is the authoritative per-row mapping;
this table is the change-level view.

| Slot | Scope | Workflows | Artifacts expected |
| --- | --- | --- | --- |
| 1 | **complete** — census, convention, enablers, exemplar | `WFR-SEARCH-REPLACE` search and preview half | proposal + design + tasks + 2 capability specs (`normalize-workflow-readability-boundaries`) |
| 2a | **complete** — migrated the palette, set the facade budget at 370, first automation projections beyond search (`migrate-command-palette-workflow-readability`) | `WFR-COMMAND-PALETTE`, first `WFR-AUTOMATION-SPINE` projections beyond the search fields | proposal + tasks + 2 spec deltas |
| 2b | **complete** — finished search/replace: the Replace All write path and its undo journal, added the `journal` role name, and proved no automation widening (`complete-search-replace-workflow-readability`) | `WFR-SEARCH-REPLACE` replace/undo half, continuing `WFR-AUTOMATION-SPINE` projections | proposal + tasks + 1 spec delta |
| 3a | **complete** — migrated the save workflow, added the per-workflow subdirectory role home, retired the programme's only workflow-code argument-count suppression (`migrate-document-save-workflow-readability`) | `WFR-DOCUMENT-SAVE`, continuing `WFR-AUTOMATION-SPINE` projections | proposal + tasks + 1 spec delta |
| 3b | **complete** — migrated the load workflow, dissolved `load_save.rs`, promoted the evidence-surface reentrancy constraint into stated convention, and closed the `model/file_load.rs` census decision (`migrate-document-load-workflow-readability`) | `WFR-DOCUMENT-LOAD`, continuing `WFR-AUTOMATION-SPINE` projections | proposal + tasks + 1 spec delta |
| 4 | User-content restore family | `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-BUFFER-REPLACEMENT`, continuing `WFR-AUTOMATION-SPINE` projections | proposal + tasks |
| 5 | Workspace tree and notes | `WFR-WORKSPACE-TREE`, `WFR-NOTES-BOOKMARKS` | proposal + tasks |
| 6 | Minimap | `WFR-MINIMAP` | proposal + design + tasks |
| 7 | Residual sweep | `WFR-MARKDOWN-PREVIEW`, `WFR-EDITOR-FIND`, `WFR-ENCODING`, `WFR-PRINT`, `WFR-SHELL-LAYOUT`, `WFR-STATUS-NOTIFICATIONS`, `WFR-BUFFER-SNAPSHOT`, `WFR-PLAIN-DISPOSAL`, remaining `exclude_re` entries and argument-count suppressions, matrix completion | proposal + tasks |

**Slot 2's ordering question is resolved: it was split into 2a and 2b.** The
convention requires that a `tier-3` workflow not be migrated until the shape has
been proven on **at least two completed lower-risk migrations** (matrix, "Risk
Tiers"), and slot 2 carried a tier-3 half — the Replace All write path and its
undo journal — while only one lower-risk migration was complete, slot 1's tier-2
exemplar. The two options were to sequence the tier-2 palette work first inside
one change, or to split the slot. **The split was taken**, for one reason: the
two-proof rule wants a *completed* migration, and completion is observable only
at the change boundary. Sequenced inside one change the gate would be a promise
in a task list; as two changes it is enforced mechanically, because 2b cannot
pass `make check-workflow-boundaries` until the matrix marks
`WFR-COMMAND-PALETTE` migrated and the ledger marks slot 2a complete. A tier-3
durable rewrite plus undo journal also deserves its own verification section
rather than being bundled with an 11,179-line palette migration.

Per the ledger grammar below, the split keeps the number and takes letter
suffixes. Slots 3 through 7 are **not** renumbered: their numbers are cited from
the matrix and from per-row `Slot` cells.

**Slot 3 split into 3a and 3b for four reasons**, recorded beside slot 2's so a
later session does not re-litigate it. First, slot 2 split because it *contained*
a tier-3 half; slot 3 contains **two independently tier-3 workflows**, each with
its own durable or user-visible failure mode — save replaces the user's file
bytes, and load installs decoded bytes into a live buffer while owning the
encoding-recovery and cancellation paths. Bundling them would put both proof
matrices in one change. Second, **scale**: the matrix sizes the two rows at
thousands of lines each with large seam populations, and slot 2b — one *half* of
one workflow — already needed a 954-line task list and five evidence files.
Third, the shared 1,795-line `ui/editor_page/load_save.rs` splits sequentially
safely, though **item by item rather than by line range**: the two halves
interleave, so 3a lifts the save items out and leaves a coherent load-only
residual that 3b then dissolves. Doing it the other way round would leave the
entangled half behind. Fourth, **the archetype defect is save-side** — the
renamed value crossing the admission seam, `QueuedSaveTicket`, and the
programme's one workflow-code argument-count suppression — so it should not wait
behind the larger load half. **Ordering is fixed: 3a lands before 3b**, because
they share `load_save.rs` and 3a's spec delta establishes the per-workflow role
home 3b reuses.

**Naming and finding a slot's change.** Slot 1 is the OpenSpec change
`normalize-workflow-readability-boundaries`, slot 2a is
`migrate-command-palette-workflow-readability`, and slot 2b is
`complete-search-replace-workflow-readability`. Slots 3 through 7 have no
reserved names; name them for their scope with a `-workflow-readability` suffix
and record the chosen name in the remaining-scope table above when the change is
authored, so the next cold session can find it without searching. Before
authoring, check `openspec list` and `openspec/changes/archive/` for an existing
change covering the slot.

| Slot | Change name |
| --- | --- |
| 1 | `normalize-workflow-readability-boundaries` (archived) |
| 2a | `migrate-command-palette-workflow-readability` |
| 2b | `complete-search-replace-workflow-readability` |
| 3a | `migrate-document-save-workflow-readability` |
| 3b | `migrate-document-load-workflow-readability` |
| 4–7 | not yet authored |

**3a lands before 3b.** They share `ui/editor_page/load_save.rs`, and 3a's
`gtk-adapter-module-boundaries` delta establishes the per-workflow subdirectory
role home (`ui/editor_page/save/`) that 3b reuses for the load half.

**The gate slot 2b checks before it starts.** 2b is tier-3 and needs two
completed lower-risk proofs. Both are observable mechanically rather than by
reading a task list: `docs/workflow-readability-matrix.md` must mark
`WFR-COMMAND-PALETTE` `migrated` with a complete `Migrated Workflow Roles`
subsection, the ledger below must mark `slot 2a` `complete`, and
`make check-workflow-boundaries` must pass — which it cannot do if either half of
that is a false claim.

`WFR-EDITOR-MEMORY` and `WFR-MIGRATION-LEDGER` have no slot: they are
cross-cutting policy that stays where it is. Their rows exist so a later change
cannot silently relocate them.

**Migration changes are expected to need a proposal and tasks, plus the minimum
spec delta strict validation requires.** Phase 0 holds the contract; a migration
consumes `workflow-readability-boundaries` and `workflow-evidence-surfaces` and
checks off matrix rows. The original wording said "only a proposal and tasks",
which `openspec validate <change> --strict` cannot satisfy: it fails any change
carrying no `specs/` delta with "Change must have at least one delta". Slot 2a
verified that against itself before writing its deltas (exit 1). Every migration
slot therefore carries at least one delta, and slots 3 through 7 will too.

That does **not** widen scope. The signal of an incomplete Phase-0 contract is a
delta that *adds obligations or capabilities* — a new requirement, a new
capability, or a widened role set. A delta that only restates a fulfilled
future-tense requirement in the settled tense, or closes a small adjacency the
convention already sanctions, is spec hygiene. Slot 6 remains the one slot
expected to need a **design** document, because minimap decomposition has a
genuine design question (pixel-verified geometry under animation frames). When a
migration does find it needs a new obligation or capability, the
retroactive-amendment rule in section 8 applies to the fix.

### Slot 1 residue — all six obligations discharged

Slot 1 deliberately migrated only the non-writing half of `WFR-SEARCH-REPLACE`.
That is finished: slot 2a paid two of the six obligations and **slot 2b paid the
remaining four**, so the row is `migrated` end to end. The list is kept rather
than deleted so a cold session can see which obligations existed, which change
paid each one, and — importantly — which one turned out to already be done.

| Obligation | Paid by | Outcome |
| --- | --- | --- |
| The Replace All write path and its undo journal | 2b | migrated. `ui/search_panel/journal.rs` owns the transaction gate, the generation-guarded install/clear, the worker disk save/delete, startup recovery, the capacity retry, the affordance, and the hand-back |
| `replace.rs`'s role name | 2b | split into `replace_execution.rs` (stage-order-qualified) and `journal.rs` (a new bounded role name) |
| `activate_undo_replacements` delegation | **already discharged by slot 1's own result-cap fix** | The residue text was stale. The facade already held a documented one-line call to `journal::hand_back_undo_backup` reading no transaction state and mutating no widget. **The real residual asymmetry was on the window side**, in `ui/window/search.rs`'s undo path, which claimed the transaction, re-showed the undo button on two early-return paths, reserved undo capacity, and installed the remainder backup inline. Slot 2b fixed that, giving the panel one named operation per step: `journal::begin_undo_restore` (returning the `UndoRestoreClaim` seam value, which names the transaction-busy and capacity-deferred refusals so the panel owns restoring the affordance for both) and `journal::finish_undo_restore`. Do not re-open the facade item; do not conclude the window-side work was skipped |
| `model/workspace_search.rs` | 2b | **stays in `model/`.** Its reference set is larger than the census cell recorded, and a service plus a `model/` sibling both depend on it, so a move under `ui/` would invert dependency direction. Decision closed; see the matrix |
| ~~The normative facade line budget~~ | 2a | declared at **370** physical lines |
| ~~The first `WFR-AUTOMATION-SPINE` projections beyond the search fields~~ | 2a | `window.command_palette` plus both palette readiness blockers, gated by an implemented drift check |

`WFR-AUTOMATION-SPINE` itself stays `pending`, because it continues in later
slots. Slot 2b's share of it was a **no-widening** obligation rather than a new
projection, and it was proved rather than asserted: the exported schema and the
`content_search` projection function are byte-identical to before, all ten new
evidence fields are declared on the surface and read by no projection, and a
before/after Automation1 capture of the same app state diffed the
`content_search` object and the readiness fields to zero differences.

Slot 2b did **not** re-plan the result-cap delivery fix or the `WalkStop`
stop-semantics split; both landed in slot 1.

### Slot ledger (machine-readable)

`make check-workflow-boundaries` reads the lines in this subsection and compares
them against `docs/workflow-readability-matrix.md`. Each line is exactly:

```
- slot <n> (complete|outstanding): <WFR-ID>[ (partial)][, ...]
```

`<n>` is a slot label, not strictly an integer: a slot that splits keeps its
number and takes a letter suffix (`2a`, `2b`), which the gate accepts. Splitting
a slot therefore means replacing its ledger line with two labelled lines and
splitting the row in the remaining-scope table above; it never means renumbering
slots 3 through 7, because their numbers are cited from the matrix and from
per-row `Slot` cells.

The check fails when a `complete` slot names a row the matrix does not mark
`migrated`; when an `outstanding` slot names a row the matrix marks `migrated`
without the `(partial)` marker; when a named row id is absent from the matrix;
and when a matrix row that is neither `migrated` nor `exempt` and has a slot
assigned does not appear in any `outstanding` slot. Keeping this ledger current
is part of advancing a migration, not paperwork after it.

The `(partial)` marker means "this row's scope is split across slots" and works
in both directions, which some rows need:

- On an `outstanding` line it marks a row the matrix already calls `migrated`
  that still owes work. `WFR-SEARCH-REPLACE` is the current case: migrated for
  its search and preview half, outstanding for its replace/undo half.
- On a `complete` line it marks a row whose share of *that slot* is finished
  while the row itself continues in a later slot, so the entry is exempt from the
  `migrated` requirement. `WFR-AUTOMATION-SPINE` is the case to expect: the
  matrix gives it slot "2 onward, incrementally per migrated workflow", so when
  slot 2 lands it must be written `WFR-AUTOMATION-SPINE (partial)` on the
  complete line and kept on a later outstanding line. Marking that row
  `migrated` to satisfy the gate would be a false claim.

- slot 1 (complete): WFR-SEARCH-REPLACE
- slot 2a (complete): WFR-COMMAND-PALETTE, WFR-AUTOMATION-SPINE (partial)
- slot 2b (complete): WFR-SEARCH-REPLACE, WFR-AUTOMATION-SPINE (partial)
- slot 3a (complete): WFR-DOCUMENT-SAVE, WFR-AUTOMATION-SPINE (partial)
- slot 3b (complete): WFR-DOCUMENT-LOAD, WFR-AUTOMATION-SPINE (partial)
- slot 4 (outstanding): WFR-DRAFT-RECOVERY, WFR-SESSION-RESTORE, WFR-LOCAL-HISTORY, WFR-BUFFER-REPLACEMENT, WFR-AUTOMATION-SPINE
- slot 5 (outstanding): WFR-WORKSPACE-TREE, WFR-NOTES-BOOKMARKS
- slot 6 (outstanding): WFR-MINIMAP
- slot 7 (outstanding): WFR-MARKDOWN-PREVIEW, WFR-EDITOR-FIND, WFR-ENCODING, WFR-PRINT, WFR-SHELL-LAYOUT, WFR-STATUS-NOTIFICATIONS, WFR-BUFFER-SNAPSHOT, WFR-PLAIN-DISPOSAL

### Convention friction slot 2a hit, recorded for 2b and 3 through 7

Convention corrections are cheapest while few workflows are migrated, so these are
recorded now rather than rediscovered.

- **The bounded coordination role set was sufficient, but only with
  qualification.** The palette needed two `execution` modules because it owns two
  ordered stage orders that each have a submit/dispatch/arbitrate shape. No new
  role name was needed; the fix was the stage-order qualifier
  (`query_execution.rs`, `index_execution.rs`), added to
  `gtk-adapter-module-boundaries`. **Expect this again**: any workflow with a
  visible surface plus a background reconciliation half is the same shape. Slot 2b
  should check the qualification rule against `replace.rs` before proposing a new
  role name.
- **The 370-line budget held on a second facade, with room to spare.** The palette
  facade narrates *two* stage orders and eight inversions; slot 2a measured it at
  328 lines, 42 under budget, and it measures **335** today, 35 under. The risk the
  budget section states did not materialize *for the palette*, and the reason
  slot 2a gave is still informative: what makes a facade long is stage *bodies*,
  not stage narration, so delegating aggressively keeps two stage orders inside a
  budget derived from a one-and-a-half-stage-order facade. **Slot 2b qualifies this
  conclusion**, though: finishing the search workflow's second stage order took its
  facade to 369, one line under, so "room to spare" is a property of the palette
  rather than of two-stage-order facades in general.
- **The evidence-surface visibility pattern needed no deviation.** Internal type
  plus a `#[cfg(feature = "test-utils")]` re-export worked unchanged, because the
  palette's readers are the same two the exemplar's are: `ui/automation.rs`
  in-crate and the external widget harness.
- **One thing the exemplar did not have to solve: an ungated test reach-through.**
  The palette's widget tests called `palette.imp().rebuild_results(..)` directly —
  not a `*_for_test` function, so it appeared in no seam census, and not gated, so
  it shaped a production signature. Consolidating evidence surfaced it. Later slots
  should grep their workflow's tests for `\.imp()\.` reach-through, not only for
  `_for_test`, when sizing seam work.
- **The census's inversion counts are floors, not totals.** The matrix trace said
  the palette had five inversions, all coordinator-guarded; the code has eight,
  three of them timer- or wakeup-driven. Write the facade narration from the code
  and correct the trace, as slot 2a did.

### Convention friction slot 2b hit, recorded for slots 3 through 7

Three workflows' worth of the convention now exist, so the retroactive-amendment
rule is materially more expensive than it was at slot 2. These are recorded to
keep the next amendment cheap.

- **The bounded role set needed one addition, and the escape hatch worked as
  designed.** `journal` was the first genuinely missing name: durable,
  generation-guarded persistence that a later stage reads back, with startup
  recovery. Expect this shape again in slot 3 (save) and slot 4 (drafts, session,
  local history) — those workflows all keep durable records a later stage restores
  from, and `retirement` will keep looking superficially close while meaning the
  opposite. **Check `journal` before proposing a fourth mechanism.**
- **The stage-order qualifier read well, and the "qualify only the new module"
  reading is the one to keep.** Slot 2a qualified both palette execution modules
  because it created both at once; slot 2b qualified only the new one, leaving
  `execution.rs` and `retirement.rs` alone. Renaming stable already-migrated
  modules for symmetry is churn, and on a tier-3 path it is churn next to a
  durable write. The spec's wording supports the narrow reading; keep it.
- **The 370-line budget held on a facade narrating two stage orders including a
  durable write — at 369, with exactly 1 line left.** It required real discipline:
  the first honest narration of the completed stage order measured 379, and it came
  back under budget only by folding module-ownership detail into the role table,
  compressing every inversion bullet, shortening per-method doc comments that
  duplicated the module-doc narration, and delegating the options-row reveal out
  of the facade. The budget is doing its job and was never edited — but **the
  headroom is gone**, so slot 3 must plan against 1 line rather than the 20 slot 2a
  declared, and **slot 6 (minimap) is the one most likely to prove the number
  wrong.** If an honest split genuinely cannot fit, the matrix's budget section
  says to correct the number through the spec while few workflows are migrated;
  that window is closing, since every migrated row adds re-migration cost.
- **The evidence surface's reentrancy constraint should become a stated
  convention, not a per-workflow module note.** Slot 2b added ten fields to a
  surface whose module doc records that the accessor takes shared `RefCell`
  borrows, so no field may be read from inside a `borrow_mut()`. That constraint
  is not workflow-specific — it follows from "one accessor reads the whole
  surface" plus `RefCell` — and every later slot will re-derive it. A future
  change should promote it into `workflow-evidence-surfaces`, with the
  read-inside-mutation test slot 2b wrote as the pattern.
- **An evidence surface needs a field the workflow does not yet record.** Slot 2b
  had to expose the last durable apply's counts, which the window computed and
  published to the status bar but never told the panel. The honest fix was a named
  workflow operation (`record_replace_apply_counts`) rather than a test getter that
  reaches into the window. Expect the same in save and load: the workflow's
  observable outcome is often already computed somewhere that throws it away.
- **A readability slot found two real pre-existing data-loss defects.** The
  mandatory `data-safety` pass over the replace/undo surface produced two
  confirmed findings, and `preexisting-blockers.md` made fixing them
  non-negotiable even though the change's own non-goals said "no behavior change".
  **Later slots should expect this and budget for it**: a readability slot over a
  durable path is the first time anyone reads that path end to end, which is
  exactly when such defects surface. Do not treat the non-goal as permission to
  defer one.

## 4. The unblock point

**The migration changes became authorable after sections 1 and 2 of Phase 0 —
the census and the settled conventions — not after the exemplar.** The exemplar
only refines the task template. Any of slots 2 through 7 can be authored today
without waiting for its predecessor to land, though they must *land* in order,
because each `tier-3` slot depends on the lower-risk proofs before it.

What each migration takes from the census, all already recorded in the matrix:

| Input | Where | Why it unblocks authoring |
| --- | --- | --- |
| Seam value-object name and the exact bundle it carries | matrix, "Seam Value Objects" (`QueuedSaveTicket` + `QueuedSaveFacts`, `LoadRequestTicket`, `WorkspaceWatchTicket`, `NotesBrowserTicket`) plus the per-row "Seam value object" cell | the tasks can name the type and the fields instead of discovering them |
| Per-kind test seam counts | matrix "Seams (i/c/a/p)" cell per row, plus the Test Seam Census partition | sizes the evidence-surface work and separates the deferred actuation seams |
| Risk tier and slot | matrix per-row `Risk` and `Slot` cells, and "Migration Order And Risk Tiers" | fixes the ordering constraint and the required proof depth |
| Ordered stages and every control-flow inversion | matrix "Workflow Stage Traces" | the facade's narration is written from the trace, not re-derived |
| Owned pure policy and its relocation target | matrix "Policy Module Census" | says what moves, what is cross-cutting, and what needs a decision |
| Settled conventions (role names, facade budget rule, zero argument suppressions, cross-cutting eligibility) | matrix "Settled Conventions" and the two capability specs | the shape does not have to be re-litigated per slot |

## 5. Sequencing rationale

**Why census before migration.** Enumerating every workflow first is the
mechanism that prevents "pilot, then refine". The three known outliers —
`minimap.rs` (3,779 lines with pixel-verified geometry), `editor_memory` (five
real consumers), and the freshly decomposed `markdown_preview` — were classified
as deferred, exempt, and deferred *before* the shape became normative, so no
later change has to argue them into or out of the pattern. The census also found
three unlisted single-workflow policy modules and corrected four of the
programme's own headline figures, which a pilot-first order would have
discovered mid-migration.

**Why vertical slices.** Migration proceeds one workflow at a time across all of
its layers, not one layer at a time across all workflows. The deliverable is a
workflow that reads as a story, and that only materializes when facade, seam
value object, policy, coordination, and evidence land together. Horizontal
slicing would touch every file four times and produce no readable workflow until
the final tier. Vertical slicing also makes a stalled programme coherent:
migrated workflows are readable, unmigrated ones are unchanged, and the matrix
says which is which.

**Why this risk order.** Renames and value objects are compiler-verified, policy
relocation is verified by mutation parity, and evidence consolidation is verified
by test-suite equivalence plus the automation docs gate. The exemplar was chosen
as the workflow closest to the target shape whose migrated half touches no user
data. Refactoring immediately after a robustness programme is the highest-risk
refactor category, so user-data workflows come only after the pattern has two
proofs.

## 6. Rejected alternatives

Recorded so a later session does not re-propose them. Full reasoning is in the
change's `design.md` (D1, D7).

- **A new `policy/` layer as a sibling of `model/` and `services/`.** Rejected:
  it preserves the split-brain reading experience — the workflow's logic is still
  in another directory — while adding a fourth place to look whose only
  justification is mutation tooling. The dependency data says these modules are
  not shared policy; they are the pure half of one workflow.
- **A naming-only pass: leave everything in `model/` and fix the names.**
  Rejected: it keeps the parking-lot incentive alive. The next robustness change
  would add `model/notes_admission.rs` for exactly the same tooling reason. The
  fix had to remove the incentive, which is why the mutation scope became a
  naming convention (`ui/**/policy.rs`).
- **Horizontal slicing: do facades everywhere, then value objects everywhere,
  then evidence everywhere.** Rejected: four passes over every file, no readable
  workflow until the last tier, and no coherent stopping point.

Also rejected, at the seam level: making evidence surfaces public and driving
D-Bus automation directly from them. That widens an external contract for an
internal readability goal. Automation snapshots project from evidence instead,
and the exported D-Bus contract is unchanged.

## 7. Deferred work

These are **not** in any slot. Each needs its own change with its own
justification, and neither is required for the programme to be complete.

Do not confuse this section with the matrix's row status `deferred`. That status
means "will be migrated, but later than its risk tier alone suggests", and every
row carrying it (`WFR-MINIMAP` in slot 6, `WFR-MARKDOWN-PREVIEW` in slot 7) has a
slot and is planned work. The two items below are **programme-level deferrals**:
unslotted, unscoped, and possibly permanent.

### Actuation test seams (~98 functions)

**What.** Functions such as `autosave_tick_for_test` and
`cancel_open_file_for_test` that drive a workflow rather than observing or
configuring it. Phase 0 consolidated inspection seams and collapsed
configuration seams; actuation seams were classified and left alone.

**Why deferred.** They exist because the real path runs through a
`GtkFileChooser` or `AdwAlertDialog` that headless tests cannot drive. They are
evidence of a **missing workflow/dialog-presentation boundary**, not of
misplaced test code. Removing them means introducing that boundary, which is a
design change with real behavioral risk in save, close-with-changes, and open
flows. Smuggling it into a readability sweep would put user-data dialogs at risk
for a tidiness gain.

**Justification bar.** Take it on when a change independently needs the
presentation boundary — for example a portal-first file-chooser migration, a
headless-drivable dialog contract, or repeated defects traced to dialog-coupled
workflow code — and can pay for real-session proof of every affected dialog
path. Until then, actuation seams stay, and the `gtk-testing` skill records them
as legitimate.

### State-machine reification of inverted drains

**What.** Turning workflows whose control inverts through timers, idle drains,
and worker completions into explicit state machines. `WFR-DRAFT-RECOVERY` alone
has seven distinct worker handoffs; save has four inversions across six files.

**Why deferred.** It was the highest-risk item in the exploration and may never
be justified. Phase 0's non-goals are explicit: nothing in the readability
programme alters control flow. The narrative facade is the cheap substitute — it
names each inversion and its resumption point, which was the actual readability
complaint — and it carries none of the risk of re-sequencing a durable write
path.

**Justification bar.** Take it on only if, after the facades exist, a *specific*
workflow still cannot be reasoned about from its narration, and that opacity is
causing real defects — not because state machines are tidier. Any such change
must be single-workflow, must preserve timing and persistence behavior
observably, and needs mutation plus real-session proof. "May never be justified"
is the expected outcome.

## 8. Retroactive-amendment rule

**A change that amends the convention MUST re-migrate every workflow already
marked `migrated` in the same change. Two generations of the convention MUST NOT
coexist in the tree.**

This is the rule that keeps a multi-change programme from manufacturing forked
versions of its own convention — precisely the disease being treated. It applies
to the role names, the facade budget number, the seam value-object shape, the
evidence-surface visibility rule, and anything else recorded as settled in the
matrix's "Settled Conventions" section or the capability specs.

Practical consequence: the cheapest moment to correct a convention is while
exactly one workflow is migrated. That moment is slot 2. The same rule is
recorded in `docs/workflow-readability-matrix.md`'s "Completion Rule" and in
`AGENTS.md`.

## 9. Gates

| Gate | What it protects |
| --- | --- |
| `make check-workflow-boundaries` (also in `make check-policy`) | `policy.rs` purity, mutation-scope reach, migrated-row role completeness, matrix evidence existence, facade budget when declared, and this record's slot ledger against the matrix |
| `make mutants-diff` | mutation-coverage parity for every relocated policy module |
| `make check-automation-docs` | the D-Bus contract stays unchanged while snapshots become evidence projections |
| `make check-agent-docs`, `make check-agent-skills` | standing guidance stays consistent with the convention |
| `make test`, `make test-widget-headless` | behavior and test-count equivalence; the project test count must not decrease |

`.agents/rules/documentation.md` requires updating both this record and the
matrix when workflow structure changes, so a migration cannot advance the code
while leaving the programme record stale.
