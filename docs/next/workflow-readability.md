# Workflow Readability — Programme Record

Status: **Phase 0 complete, slots 1 through 6 and 7a complete, slot 7b
outstanding.** Slot 7 **split** under the trigger its own proposal declared: §D1
resolved that `WFR-SHELL-LAYOUT` is not one workflow, and implementing that
outcome plus the preview facade exceeded one change's capacity. **Slot 7a**
migrated **five** rows — the four tier-1 rows plus `WFR-MARKDOWN-PREVIEW` — and
discharged one cross-cutting lane's surface obligations; **slot 7b** carries the
tier-3 disposal lane, the shell-layout hybrid, the automation spine's terminal
status, capability deltas 1 and 2, and the programme closeout. See
[§D1's resolution](#slot-7as-structural-finding-d1-resolved-the-shell-row-is-not-one-workflow),
which is 7b's primary authoring input. The convention is
normative, the census is complete, the mechanical gate is wired into
`make check-policy`, the normative facade line budget is declared and enforced,
and **eleven** workflows are migrated: `WFR-SEARCH-REPLACE` (**both halves** —
search and preview in slot 1, the Replace All write path and its undo journal in
slot 2b), `WFR-COMMAND-PALETTE` (slot 2a), `WFR-DOCUMENT-SAVE` (slot 3a, the
first tier-3 workflow migrated on its own), `WFR-DOCUMENT-LOAD` (slot 3b, the
second), slot 4's four — `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`,
`WFR-LOCAL-HISTORY`, and `WFR-DRAFT-RECOVERY` — `WFR-NOTES-BOOKMARKS` (slot 5a),
`WFR-WORKSPACE-TREE` (slot 5b, the largest row in the census), and
`WFR-MINIMAP` (slot 6, the row the census deferred longest and the only one whose
behavior contract is rendered pixels). **`model/minimap_analysis.rs` no longer
exists**: slot 6 relocated it into the minimap workflow's own `policy.rs`, the
fifth of the census's six relocation candidates to move. **`ui/editor_page/load_save.rs` no longer exists**: slot 3a lifted
the save half out and slot 3b dissolved the rest, so the programme's third
measured symptom is now history rather than a live file. Everything else in `ui/`
and `model/` is unchanged and behaviorally untouched.

**Slot 4 is the programme's largest change so far**, migrating four rows in one
pass in increasing-risk order: buffer replacement first because the other restore
paths drive their bytes through it, then session restore, then local history, then
draft recovery. It also carried one **confirmed and fixed data-safety defect** —
the draft-autosave lane had never consulted `installation_incomplete`, so a
cancelled load installation plus one keystroke could write a near-empty buffer over
a draft holding real unsaved work. Slots 5 through 7 remain authorable; two items
are deliberately deferred and may never be taken on.

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

### Baseline after slot 4

| Quantity | After slot 3b | After slot 4 |
| --- | --- | --- |
| Workflows migrated | 4 | **8** (plus `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`) — the programme doubled its migrated set in one change. Counted as workflow *halves*, nine |
| Share of `ui/` + `model/` migrated | 11,811 of 79,017 censused lines ≈ 15% | 11,811 + 1,029 + 1,962 + 2,586 + 2,578 = 19,966 ≈ **25%**, using each row's censused `ui` footprint so the fraction stays comparable across slots. **Row-scoped and production-only the four rows are 9,300 lines across 27 files** (buffer replacement 1,455/4, session restore 1,744/6, local history 2,995/8, draft recovery 3,106/9). Three of the four census `Current size` cells were far too **high** — they pooled whole shared service files, and draft recovery's `services 5,910` was almost entirely `draft_service.rs`, most of which is co-located tests — while buffer replacement's was too **low** once tests are excluded. Corrections in both directions, again |
| Policy modules relocated | 3 of 6 relocation candidates | **4 of 6.** `model/draft.rs`, `model/session.rs`, `model/local_history.rs`, and `model/buffer_replacement.rs` are all confirmed **staying**: the first three because a service depends on each of them, the fourth because it is cross-cutting. What *did* relocate is `ui/window/draft_ordering.rs`'s `DraftMutationOrder` epoch allocator, whole and with its tests, into `drafts/policy.rs`. Everything else slot 4 added to the mutation scope is **gain from zero** out of GTK adapters: 248 mutants across four new `policy.rs` modules |
| Test seams addressed | load retired 10 inspection surfaces | slot 4 retired **21 inspection functions** across its four rows (4 + 2 + 8 + 7) and **no** `*_for_test` inspection function remains on any of them. **14 configuration seams plus 9 delay/fail hooks collapsed into 2** test-policy values (`drafts/test_policy.rs`, `local_history/test_policy.rs`), each entirely behind `#[cfg(feature = "test-utils")]`. **14 actuation seams preserved and 0 added** — the programme's second slot in a row to add none. One keyed question became a named workflow operation rather than a surface field, because a field cannot take an argument: `draft_delete_is_tombstoned(&str)`. **35 ungated `imp()` reach-through sites went to 0** |
| Seams reified | 5 | **5, unchanged — and two census cells were wrong about *kind*.** Slot 4's rows already carried their seam value objects, so its seam work was auditing them against the two-boundary rule, which they all pass. Two were **reclassified**: `BufferReplacementSession` is coordination-owned GTK runtime, and `DraftCleanupContinuation` is the journal's manifest offset — neither is a seam value object. `WorkspaceWatchTicket` and `NotesBrowserTicket` remain unreified, both slot 5. **Long signatures shortened: 1, unchanged**; the workspace `#[expect(clippy::too_many_arguments)]` count holds at 1 |
| Automation projections | 4 | **5** (plus the `local_history` snapshot object). The `session-restore`, `close-safety`, and `draft-autosave` readiness blockers additionally moved onto **cheap facade accessors identical by construction** rather than field reaches. **The drift gate grew once, and was proved:** `local_history` is a third projecting surface and had to be registered, and the extension was verified by confirming it **rejects a real rename** rather than by assertion. `tabs[].draft_present` was **not** re-sourced — it is a per-tab document-identity fact and the draft surface is window-level — so the "three surfaces into one `tabs` object" contingency did **not** fire. Exported names, types, and semantics unchanged |
| Facade line budget | held; load at 253 of 370 | **held four more times, and the number is now well evidenced.** Buffer replacement 167, session restore 165, local history 216, draft recovery **310** — the programme's largest facade and its closest approach, from a workflow with three stage orders and 17 inversions. No escalation, and the budget line was not edited. `ui/search_panel/mod.rs` stayed untouched at 369, save at 223, and **load grew 253 → 271** because the data-safety fix crossed a workflow boundary and therefore crossed through a facade |
| Bounded coordination role names | unchanged | unchanged, and **`journal` was checked for all three durable rows at once and passes all three.** `gtk-adapter-module-boundaries` needed no amendment. Slot 4 is also the first to use **four** coordination modules in one workflow (draft recovery: `journal`, `admission`, and two stage-order-qualified execution modules), and the first to reject `retirement` for something that *destroys* payloads — orphan cleanup is journal maintenance, on the cohesion test |
| Permitted role homes | second adopter | **sixth adopter**, and the first three under `ui/window/`. The nested `ui/**/policy.rs` glob was re-verified after every move; `make check-workflow-boundaries` now reports **8** pure mutation-scoped policy modules, up from 4 |
| Evidence-surface conventions | plus the reentrancy constraint | unchanged, with **four new surfaces and eight new proof tests** (a reentrancy proof and a disposal proof each). The **disposed-widget rule earned its place**: it was a live hazard twice, not a formality. Buffer replacement's whole subject is the source view's buffer, and local history's first surface actually **panicked** in its disposal proof because `live_local_history_availability()` derefs a template child — the fix split out a chars-taking helper so an observer that already read through `try_get()` does not read again through the panicking accessor |

### Baseline after slot 5a

| Quantity | After slot 4 | After slot 5a |
| --- | --- | --- |
| Workflows migrated | 8 | **9** (plus `WFR-NOTES-BOOKMARKS`). Counted as workflow halves, ten. `WFR-WORKSPACE-TREE` moved to **slot 5b** — see "Why slot 5 split into 5a and 5b" |
| Share of `ui/` + `model/` migrated | 19,966 of 79,017 censused lines ≈ 25% | 19,966 + 4,977 = 24,943 ≈ **32%**, using the row's censused `ui` footprint so the fraction stays comparable. **Row-scoped and production-only the row is 4,365 lines across 4 files**, and the census cell was too **high** in a way no previous slot had seen: it was not only pooling shared services, it was pooling a file the row **does not own at all** (`ui/window/startup_data.rs`, 435 lines, decided cross-cutting) — an ownership error inside a size cell |
| Policy modules relocated | 4 of 6 relocation candidates | **4 of 6, unchanged — and the two remaining candidates are still open, because they are the tree row's.** `model/workspace_scan.rs` and `model/workspace_persistence.rs` move with slot 5b. What this slot added is **gain from zero**: 81 mutants in `ui/window/notes/policy.rs` with **78 caught, 0 missed, 3 unviable — zero survivors on the first run**, which is a programme first. All five notes domain modules are confirmed **staying**, each because a GTK-free service depends on it, and `model/sidecar_identity.rs` is recorded as a **cross-workflow kernel** with seven consuming workflows |
| Test seams addressed | 21 inspection functions retired across four rows | **4 more retired**, and **no `*_for_test` inspection function remains on this row**. One retired getter had a **side effect** — `note_save_snapshot_count_for_test` pruned the capture vector as a consequence of being read — which is the observer-changes-the-observed hazard the new delta names; the surface counts without pruning. Two parameterized tuple-returning seams became **one named operation** returning a named value. **2 configuration seams collapsed into 1** test-policy value. **0 actuation seams and 0 added on the notes row.** **2 counted, justified configuration seams added on the tree row** — the first seams this programme has added since slot 3a, and both exist because a data-safety regression test for a confirmed data-destruction defect **passed against the broken code as well as the fixed code** without them |
| Seams reified | 5 | **6** (plus `NotesBrowserTicket` + `NotesBrowserFacts`). `WorkspaceWatchTicket` remains unreified and moves to 5b, but the tree row gained an **unplanned seventh** — `FileOperationTicket` + `FileOperationFacts` — because reifying it *was* the fix for a confirmed wrong-row defect. The notes seam went further than the matrix cell asked: it is **phantom-typed by flight**, so validating one coordinator's generation against another's facts is a compile error. **`NoteSourceRefreshCoordinator` retired** onto the shared `SingleFlightCoordinator`, closing a deferral slot 2a opened and slot 4 restated. **Long signatures shortened: 1, unchanged** |
| Automation projections | 5 | **6** (plus the `notes` snapshot object, four fields). **The drift gate needed no extension**, and the reason is a decision rather than luck: the dual-bound `snapshot-field-active-document-file-backed` was resolved by making **neither** object project it, because it is the active document's identity rather than either workflow's state. The rule is recorded above the map for slots 6 and 7. `window.workspace` remains unprojected — slot 5b's |
| Facade line budget | held four more times; drafts at 310 | **held again at 178 of 370**, the programme's densest narration (five stage orders) and nowhere near the ceiling. **No escalation and no split**; the budget line was not edited. `ui/search_panel/mod.rs` is still exactly 369. **Three of eight recorded facade sizes were stale** and were re-measured and corrected: buffer replacement 167 → 168, local history 216 → 215, drafts **310 → 289** |
| Bounded coordination role names | unchanged | unchanged and **not widened**. `journal` was checked per stage order for four durable records and fits **one**: the migration ledger, the strongest fit in the programme (a record read back **in a later process run**). Note and bookmark sidecars, the format-upgrade backup, and `workspaces.json` were each rejected explicitly with the reading stage named. This row uses **two stage-order-qualified `execution` modules** (`source_execution`, `query_execution`) for one workflow's two browser stage orders |
| Permitted role homes | sixth adopter | **seventh adopter, and the first *flat* home under `ui/window/`.** The three migrated siblings there all took per-workflow subdirectories, which left `policy.rs` and `evidence.rs` free in `ui/window/notes/`. The nested home (c) has **no adopter yet**; 5b will be its first. `make check-workflow-boundaries` reports **10** pure mutation-scoped policy modules, up from 8 |
| Evidence-surface conventions | plus the reentrancy constraint | plus **no-materialization** and **bounded-honest child collections**, both now stated convention. One new surface with **three** proof tests. The **disposed-widget rule earned its place for the third time, and in a new way**: the panic came from a *transitively* reached template child (`active_editor()` → `imp.tab_view`), which reads as an ordinary window operation at the call site rather than as a template-child deref |
| Called presentation surfaces | undefined category, used by six rows as "adapter detail" | **defined, and the label retired.** Statement (b) scopes the five-name taxonomy so a widget-projection module is outside it, carries no role, and owns no `policy.rs` or `evidence.rs`. The eight-row re-check found **three of eight** rows recording nine such modules under the undefined "adapter detail" label; all are now classified in both their module doc and their matrix row |
| Confirmed pre-existing data-safety defects fixed | 1 (draft autosave's `installation_incomplete`) | **7**, out of **11** found by one explicit pass — the most any slot has found, including a **normal-usage data-destruction bug**: inline rename silently replaced an existing file. Each of the 7 has a regression test **proved to fail without its fix**. The other 4 are recorded with severity, site, and owning row rather than dropped |

### Convention friction slot 5a hit, recorded for slots 5b, 6 and 7

**The data-safety pass is not a formality, and it can consume a slot.** Slot 5's
proposal budgeted for findings on the evidence that four consecutive slots each
found one. The pass found **eleven**, and the most severe was a normal-usage
data-destruction bug in the exact code the migration was about to restructure.
`.agents/rules/preexisting-blockers.md` has no exceptions, so seven fixes landed
first — and the tree row's structural migration did not. **Slots 5b, 6, and 7
should plan the data-safety pass as a first-class work item with its own budget,
not as a gate to pass.** The pass is aimed at the code the migration is about to
touch; finding a lot is the expected outcome, not the surprising one.

**Two data-safety regression tests passed against the broken code.** Both raced a
worker the fixed and unfixed versions both won. Discovering that required
*deliberately reverting each fix and re-running* — which task 7.5's phrase "proven
to fail without the fix" demands and which is easy to skip. The remedy cost **two
counted configuration seams**, the programme's first added seams since slot 3a,
each documented with why it was necessary. **A test that cannot fail is worse than
no test**, because it reads as coverage. Budget the revert-and-rerun step.

**The retroactive-amendment cost is now nine per-row re-checks, and the
not-a-confirmation streak is three.** Slot 3b's amendment found one of three rows
lacking its proof; slot 4's found two of four non-compliant; slot 5a's found **three
of eight** rows recording nine widget-projection modules under **"adapter
detail"** — a
label used in two facade role tables and this matrix and **defined nowhere**. It
was doing precisely the job the new statement names, under a name no gate and no
reader could check. **Look for the undefined label, not only the missing one**: the
gap was not that rows had unclassified modules, it was that they had
*confidently* classified them into a category that did not exist.

**Census cells can be wrong about *ownership* inside a *size* cell.** Previous
slots found size cells pooling shared service files. This one found a size cell
pooling a file the row **does not own at all** — `ui/window/startup_data.rs`, 435
lines, which orders **five** workflows and is owned by none. Slot 4 flagged
ownership corrections as more dangerous than count corrections because a name
invites trust; this is that hazard hiding inside a number.

**Inversion floors were wrong again, by 7x on both rows.** The notes row's census
cell said four inversions; the code has 28 resumption points across five stage
orders. The tree row's said five; the code has 38 across eleven stage orders. Two
whole *stage orders* were unnamed on the tree row (the workspace scope filter and
focused-folder drilldown) while their primitives were already inside the counted
total — so the arithmetic looked consistent while the narration was missing.
**Reconcile subtotals against the total**; a trace whose parts do not sum is
hiding a stage order.

**A `--file`-focused mutation run does not carry the field-deletion floor.** Slot
4's note that `cargo-mutants` 27's `--re` filter misses struct-field-deletion
mutants is correct and is easy to over-apply: it is a property of `--re`, not of
focused runs generally. `MUTANTS_SMOKE_FILE` uses `--file`, and all 81 mutants
were verifiably in the target file.

**Local mutation runs can fail before testing anything, with a message that says
nothing useful.** `Disk quota exceeded` while copying
`.flatpak-builder/build/*/target/debug/incremental/...`: `flatpak-builder` leaves
**nested git repositories** in an otherwise-gitignored tree, cargo-mutants' ignore
walker treats each as its own repo, and 97 GB gets copied into a 47 GB tmpfs.
`TMPDIR` on a large filesystem is the working fix. Full diagnosis in
`evidence/mutation-notes-policy.md`.

**`--all-features` hides a default-feature break, and an evidence surface is
exactly where it hides.** The notes surface carries a `#[cfg(feature =
"test-utils")]`-gated snapshot type in a field that is **not** gated, which
compiles under `--all-features` and fails under default features. The break did
not surface in `cargo check --workspace --all-targets` either, because the type
was reachable through a gated re-export at the time. **The mutation run's
unmutated-baseline build is what caught it**, three hours in. Check both feature
configurations after every re-export or surface change, as slot 4 recorded — and
know that an evidence surface's fields are the shape most likely to break it.

**Three of eight recorded facade sizes were stale**, all in the safe direction
(drafts by twenty-one lines). Task 9.3 says *re-measure*, not *confirm*, and this
is why: a budget claim checked against a stale number is not a check.

**A confirming full-suite run is where an undersized wait budget surfaces, and
one site is never the whole problem.** The final widget lane reported one
`FLAKY:` — a Save As / rename / delete row-state test whose three
`spawn_blocking_then` completions were budgeted at **3s** where
`.agents/rules/widget-wiring.md` requires **5-10s** for async waits. Fixing only
the site that fired would have left six more Save-As-completion waits in the same
module at 2-3s; all seven were raised together, with the reason recorded once at
the first site. The second half of the cause was this change's own: the note
resolution completion re-armed the bookmark debounce even when the live set and
the loaded sidecar were both empty, adding a timer and a worker to **every** file
load and Save As to write nothing. **Look for the shared budget population, and
check whether the change added avoidable work to the path the wait covers.**

**A `journal` verdict is per record, and three of four records were rejected.**
Note and bookmark sidecars are the workflow's *authoritative user content*, not a
generation-guarded recovery record; the format-upgrade backup is read back only by
manual recovery; and `workspaces.json` is *loaded* at next launch the way any
settings file is. Only the migration ledger has a generation, an attempt cap,
stale-record cleanup, and a later stage that restores **from a failure**. The test
that separates them: *does a later stage read the record back **as recovery***,
not *is it read back at all*.

### Convention friction slot 4 hit, recorded for slots 5 through 7

**The retroactive-amendment cost is now five per-row re-checks, and slot 4's own
re-check was not a confirmation.** Amendment (b) — that a migration re-derives its
row's measured cells — found **two of four** migrated rows non-compliant:
`WFR-SEARCH-REPLACE`'s size cell still carried the census figure with a pooled
`services 5,895` subtotal, and `WFR-COMMAND-PALETTE`'s kept `services 7,897`
pooled with no sharing rows named. Both were re-derived and filled in this change
(5,527 and 2,534 production lines respectively). That is **two consecutive
amendments** where "it must already hold" was not a discharge. Assume the next one
is also real work.

**The `policy: none` allowance exists but has no first user, and the probe is
why.** The proposal expected `WFR-BUFFER-REPLACEMENT` to be the programme's first
row complete without a `policy.rs`. The spec delta's own requirement — that the
workflow's pure logic be *entirely* cross-cutting — forced a probe of the GTK
adapter's candidate decisions, and the probe found **five** separable pure
decisions, four of them deciding whether a partially mutated buffer can be seen or
whether a caller learns the truth about its terminal. So the allowance ships
stated and unexercised. **The lesson generalises: "the domain module stays" does
not imply "the workflow owns no policy".** Slot 3b's `file_load.rs` showed the
same thing from the other side.

**Census inversion counts were floors again, by a wider margin than ever.** Slot 4
derived all four rows' traces from the code before touching anything: session
restore has **7** deferred inversions against a recorded 1, local history **16**
against 6, draft recovery **17** against 7. Buffer replacement's recorded 1 was
**correct** — the only cell of the four that needed no correction. The 26 the
census missed are all the same shape: it counted `spawn_blocking_then` sites and
missed timers, main-loop polls, disposal capacity wakeups, chunked buffer
snapshots, and buffer-replacement terminals. **A slot that counts only worker
handoffs will undercount by roughly two thirds.**

**A census cell was wrong about *ownership*, not just about counts.** The
`Policy Module Census` entry for `buffer_replacement.rs` read
`2 (WFR-LOCAL-HISTORY, Replace All undo)` and was wrong in both halves: the count
was a consuming-file count, and **Replace All undo is not a consumer at all** —
`BufferReplacementWorkflow::LocalHistoryUndo` is local history's own undo
affordance. A wrong *name* in a census cell is more dangerous than a wrong number,
because a number invites re-derivation and a name invites trust.

**A readability slot over a durable path found another real defect — third slot
running.** 2b found two, 3b found one, and slot 4 found one **confirmed
recovery-data-loss defect** in the draft-autosave lane, which had never consulted
`installation_incomplete`. The pattern is now four for four: reading a durable
path end to end for the first time is when its defects surface. **Budget for
findings, and do not treat the "no behavior change" non-goal as permission to
defer one.**

**One shared imp state group split three ways, not two, and one task's hypothesis
was wrong.** `SessionState` holds genuine session fields, the tab workflow's
aggregate projection counter, migrated save's close-save identity pair, **and**
`close_safety_inflight`/`close_safety_bypass` — which the task list hypothesised
belonged to the session row and which the code shows are genuinely **shared**
between the draft and session rows, exactly as their own doc comments said. The
generalisable move: when a field's doc comment names two workflows, believe the
comment until the code contradicts it.

**A file the task list assumed was shared between two rows turned out to belong to
neither.** `ui/window/startup_data.rs` is the startup format-upgrade gate, whose
census home is `WFR-NOTES-BOOKMARKS`; it *calls* both slot-4 startup entry points
from one function and shares no state group with either. "Shared or owned?" is
sometimes the wrong question.

**An assertion that compares a value against the constant it came from cannot
detect the constant changing — and this is now the programme's single most common
mutation survivor.** Slot 4's four rows left 33 survivors on the first
diff-scoped run, and the largest share were of exactly this shape:
`assert_eq!(delay, MAX_BACKOFF)` still holds when `15 * 60` becomes `15 + 60`,
because both sides move together. `PREVIEW_RESERVATION_BYTES`'s `64 * 1024 *
1024` had the same gap, four mutants wide. **Pin policy constants to concrete
literals, in the units a reader would sanity-check** (fifteen minutes, 64 MiB),
with the user-facing reason recorded beside them. Slots 5 through 7 should expect
to write these assertions for every constant a new `policy.rs` owns.

**A relocated module's "parity" figure may legitimately be zero-before, and
saying so is more honest than inventing one.** Both of slot 4's relocations —
the session admission policy and the draft mutation-intent allocator — moved from
files that were **outside** `examine_globs`, because they were not named
`policy.rs`. There was no before-count to be at parity with. The relocation's
behaviour parity is carried by its co-located tests passing unchanged; its
*coverage* figure is a gain. **Check whether the source location was in scope
before promising parity numbers.**

**Extracting a decision does not automatically test it, and the gap is
predictable.** Slot 4's relocated `SessionRestorePolicy` arrived with five tests
that drove it end to end and asserted its outputs — and left **19 survivors** on
its accessors, guards, and no-return-value helpers, all of which those tests
exercised only indirectly. The rule that falls out: **after extracting, run
mutation on the new module before writing the row's evidence file**, and expect
to add direct tests for every `-> bool` predicate, every generation accessor, and
every method whose only observable effect is a side effect.

**Moving an `if` condition into a `match` scrutinee silently extends a borrow's
lifetime, and this convention's mechanical work does exactly that.** Extracting a
decision into a policy function turns `if cell.borrow().is_some() { ... }` into
`match policy_fn(cell.borrow().is_some()) { ... }` — and a `match` scrutinee's
temporaries live for the **whole match**, while a plain `if` condition's drop
before the block. Slot 4 introduced one such latent `BorrowMutError` and caught it
by re-reading the diff, on the path where a superseded buffer replacement
terminated inside its own turn. The pre-existing widget test would have caught it,
but only because that path happened to be covered. **Read every `borrow()` the
extraction moved, and bind the value to a local before the `match`.**

**A "tautological" extraction is a smell, not a win.** Slot 4's first cut
extracted `terminal_is_complete(cancellation) -> bool`, which is
`cancellation.is_none()`. It proved nothing to the compiler and forced a *dead
default* at the call site. Removing it and using an exact `match` improved the
code and cost 2 mutants. **Count the decisions a reader could get wrong, not the
functions you can name.**

**Four facades in one change against one budget: held, with the ceiling now
genuinely tested.** 167, 165, 216, and **310** of 370. The 310 is draft recovery —
three stage orders and 17 inversions — and reaching it needed exactly the sequence
slot 2b recorded: delegate every stage body, compress each inversion to one line,
and fold module-ownership detail into the role table and the shared-state table.
**No escalation, and the budget line was not edited.** Slot 4's evidence is that
the number survives the programme's hardest row, so slots 5 through 7 should plan
against it rather than expect it to move.

**A workflow spanning two directories is resolvable, and the resolution has a
shape worth reusing.** `WFR-LOCAL-HISTORY` cannot own `policy.rs` and
`evidence.rs` in both `ui/window/` and `ui/editor_page/`. The split that worked is
the **coordination/presentation line** slot 3b used for recent documents: the
window half is the canonical role home, the editor-page half is a **called
surface** with its ownership recorded in its own module doc, and — the part that
makes it honest rather than nominal — **the called surface imports its freshness
tickets from the canonical `policy.rs` instead of defining private copies.** If a
future row cannot do that last step, it does not have a clean split.

**One workflow needed four coordination modules, and the bounded set absorbed
it.** Draft recovery uses `journal`, `admission`, and two stage-order-qualified
execution modules. The qualifier rule worked as written: both execution modules
were new, so neither was a stable sibling renamed for symmetry. Slot 4 also found
the first case of **rejecting `retirement` for something that destroys
payloads** — orphan cleanup deletes files, but it reloads the journal's own record
under the journal's own lock and merges back into it, so on the cohesion test it
is journal maintenance. `retirement` in this codebase means the disposal lane.

**The disposed-widget rule earned its place twice, and once it actually
panicked.** Local history's first evidence surface called
`live_local_history_availability()`, which derefs the source-view template child,
and the **required disposal proof test caught the panic** rather than a user
finding it. The fix split out a chars-taking helper so an observer that already
read the buffer through `try_get()` does not read it again through the panicking
accessor. **Write the disposal proof before believing the surface is safe.**

**Mutter's Wayland socket path is length-limited, and a scratch worktree can
exceed it.** Slot 4's automation no-widening proof needs a **baseline tree** to
diff against, and the first attempt put that worktree under the session scratch
directory. `make automation-smoke` failed there with
`libmutter-ERROR: Failed to create socket` — a message that says nothing about
path length. Re-created at `/tmp/lt-base`, it worked. The build rules already warn
that smoke runtime directories must stay short; **a comparison worktree is one of
them.**

**`cargo-mutants` 27's `--re` filter does not apply to its struct-field-deletion
mutants.** A "focused" run of 21 policy mutants actually ran 53, the extra 32
being every field-deletion mutant in scope regardless of the regex. 16 of those
survived, all pre-existing, in `services/draft_service.rs` and
`services/file_tree.rs` — the latter being slot 5's. Expect focused runs to carry
that floor, and do not attribute its survivors to your change.

**Untracked files are invisible to the diff-aware policy gates, so a migration
that adds whole new directories can pass them while they are blind to the bulk of
the change.** Every slot in this programme adds a new per-workflow role directory,
and until that directory is tracked, `git diff origin/main` does not mention a
single file inside it. Slot 4 hit this twice with different symptoms. `make
mutants-diff` needed the documented `git add -N` workaround to see the new policy
modules at all — that one is already recorded. The second is quieter and worse:
`make check-visual-proof-policy` **passed** while its "changed files" list omitted
all 24 new files, and it only started failing once `git add -N` made them visible,
reporting `summary changed-files digest does not match current visual-sensitive
diff`. The fix is to keep the files visible and re-run the lane, **not** to reset
the index and take the green.

So: run `git add -N` on new directories **early**, before the first diff-aware
gate, and treat a green diff-aware gate on a slot with untracked new files as
unproven. The gates are correct; the input was incomplete.

**Isolating an app's state does not isolate its window, and the live-run gate is
therefore not an agent-runnable lane.** Slot 4's live run was launched under fully
isolated `XDG_*` and `LUSHTEXT_DATA_DIR` directories, after confirming through
`flatpak ps` and `busctl --user list` that no LushText instance was running — and
it still interrupted the user's active fullscreen desktop session, because a real
Wayland launch maps a real surface and takes focus regardless of where its state
lives. The user stopped it. **Treat the `make run` paned-warning proof as
requiring scheduled user availability**, plan the slot without it on the critical
path, and mark it `[~]` deferred rather than ticking it or quietly dropping it.
Everything else display-dependent has a headless path already: the smoke lanes
run isolated `mutter --headless`, and `scripts/run-widget-tests.sh --headless`
self-supervises into one.

Two smaller lessons from the same episode. **Synthetic global input is the wrong
tool for driving the app**: `ydotool` types into whatever the compositor focuses,
so it is both hazardous in a live session and unverifiable — slot 4's first
attempt landed nowhere and that only surfaced because the fixture file and the
`modified` flag were checked afterwards. Use targeted AT-SPI
(`Atspi.EditableText.insert_text`), which is what `crash-recovery-smoke` and
`accessibility-smoke` already do. And **an instance launched with `nohup` from a
tool call may be reaped when that call ends**, exiting cleanly with no diagnostic;
that is a shell-lifecycle artifact and not an application failure, so do not chase
it as a crash.

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

### Baseline after slot 5b

Slot 5b **landed in full**: `WFR-WORKSPACE-TREE` is **migrated**, so the workflow
count moves to ten. An earlier revision of this section described a narrower
boundary — evidence surface not landed, seams unretired, the budgeted seam unspent,
the automation reach-throughs still open — that the change then exceeded. Those
sentences were retracted rather than left standing, because a superseded scope
statement in this record is how a later session concludes finished work is still
pending. The rows below describe what actually landed.

| Quantity | After slot 5a | After slot 5b |
| --- | --- | --- |
| Workflows migrated | 9 | **10** (plus `WFR-WORKSPACE-TREE`). The Completion Rule is satisfied on every axis: the narrative facade, the nested coordination roles, the single `policy.rs`, the single `evidence.rs`, the reified seam, mutation parity, and seam retirement all exist |
| Policy modules relocated | 4 of 6 relocation candidates | **6 of 6 — the count finally moves, and it is the first relocation since slot 3a.** Both workspace modules landed in `ui/sidebar/policy.rs` with **exact mutant-by-mutant parity**: 50 generated / 35 caught / 8 unviable / 7 missed before, identical after, every survivor matched at a constant **+198** line offset. `model/workspace.rs` is confirmed **domain and staying**, and the reason is now concrete rather than asserted: it has **four GTK-free `services/` consumers**, which forbids a `services -> ui` inversion outright |
| Mutation survivors in relocated policy | — | **7 → 0.** Six killed at step two of the documented order by tightening assertions that the mutants' own values had satisfied; one **provably equivalent** and excluded narrowly, with the invariant that makes it equivalent pinned by its own test so the exclusion cannot outlive its justification. Final run: **53 mutants, 44 caught, 9 unviable, 0 missed** |
| Seams reified | 6 | **7** (plus `WorkspaceWatchTicket` + `WorkspaceWatchFacts` + `WorkspaceWatchDisposition`). **The last `required` seam in the matrix is now `done`.** Its predicate is deliberately **not** a bool: a stale lifetime must *retire* while a stale target generation must *restart*, so it returns a named three-way disposition. **Long signatures shortened: 1, unchanged** |
| Seams retired | 0 of 60 | **19 of 60 inspection seams retired; population 60 fns / 111 gate sites → 41 / 93.** Five tuple-returning seams became named fields and the destructive "take touched rows" seam split into a non-destructive field plus a separate reset **drive**. The **one** new seam slot 5b budgeted is **spent**, on the load-worker delay that M-4's two driven race tests need |
| Automation projections | 6 | **7** (plus `window.workspace`, ten fields, registered in the drift gate's `EVIDENCE_PROJECTIONS`). Both production `ui/automation.rs` `.imp()` reach-throughs are retired. The contract is **unwidened**: the ten snapshot fields are byte-identical to what the reach-throughs produced |
| Data-safety defects fixed | 7 (slot 5a) | **7 more** — two from pass 1, three from pass 2, two from the fix cycle, counted once here and everywhere against the rule stated at the end of the change's `evidence/data-safety.md`. The three that most matter: a confirmed delete that removed **by path with no identity recheck** — recursively, for directories, in a file whose own contract says "never delete by path alone"; a rename that left the draft journal pointing at the vanished path, stranding the user's unsaved edits on the crash path with neither notification nor restore; and **M-4's superseded-load guard, twice** — first the guard itself, which skipped adoption and let the pending write commit one workspace over every workspace on disk, and then its fix, which was **inert** because the bit it read was set by *every* rebuild rather than by a load adoption. The remaining four are the draft re-stamp latching a clean tab's autosave flag, the dangling-symlink delete refusing forever, the confirmed delete refusing an already-vanished target, and the pass-1 rename fix's own follow-on. **Five further confirmed findings are handed on with named owning rows and durable homes** |
| Pre-existing blockers fixed | — | **2**: the **default-feature build was broken at `origin/main`** (a denied `unused_self` that `--all-features` hides), and `.agents/rules/ui.md` named a **phantom symbol** — `restore_materialized_state`, which has never existed — that the rule, slot 5a's snapshot, and slot 5b's own task list had all been citing |
| Facade budget | seventh adopter; 370 unchanged | **370 unchanged and not edited.** The sidebar facade shrank **415 → 292** — the 103-line cross-cutting width preset left, and the rest went to the coordination roles and to one named `with_first_visible_section` operation replacing four duplicated walks. That leaves **78** lines of headroom. Escalation was **not** needed: delegate-harder suffices. (An earlier revision of this row said 316, measured mid-change and before the last delegation; the figure was re-derived in the fix cycle.) `ui/search_panel/mod.rs` still sits at exactly **369** with not one line added |
| Permitted role homes | seventh adopter | **nested home (c) fully exercised, its first adopter** — the canonical role home at `ui/sidebar/` holds the facade, the single `policy.rs`, the single `evidence.rs`, `seams.rs`, and `test_policy.rs`, with `ui/**/policy.rs` confirmed to reach it **after** the move; the nested **coordination** modules live under `workspace_section/`. `make check-workflow-boundaries` reports **10** pure mutation-scoped policy modules, unchanged, because both relocations merged into the workflow's existing `policy.rs` rather than adding one |
| Convention changes | — | **2 statements across 2 capabilities**: dissolution-before-escalation (with the already-correctly-named corollary) in `gtk-adapter-module-boundaries`, and the unfilterable mutation floor plus parity-versus-gain separation in `mutation-testing`. Both verified **pure additions** — zero removed non-blank lines across all three requirements |

### Baseline after slot 6

Slot 6 **landed in full**: `WFR-MINIMAP` is **migrated**, so the workflow count
moves to eleven and the census's last deferred single row is resolved.

| Quantity | After slot 5b | After slot 6 |
| --- | --- | --- |
| Workflows migrated | 10 | **11** (plus `WFR-MINIMAP`). Every Completion Rule axis is satisfied: the narrative facade, six bounded coordination roles, the single `policy.rs`, the single `evidence.rs`, two reified seams, relocation parity reported separately from extraction gain, and seam retirement |
| Policy modules relocated | 6 of 6 relocation candidates | **6 of 6, unchanged as a fraction — and the fraction was already wrong.** `minimap_analysis.rs` was the census's own sixth candidate and it relocated here, so the "6 of 6" slot 5b recorded had counted the workspace pair as closing the list. `model/` went 26 → 25 files. Relocation parity is **exact**: 21 generated / 19 caught / 2 missed before, identical after |
| Seams reified | 7 | **9** (plus `MinimapProjectionSpace` and `MarkerProjectionSpace`, promoted from private adapter structs to pure seams in `policy.rs`). A **third** candidate was carried into implementation and **dropped** rather than built: an adjustment-facts bundle that review could not show crossing two boundaries and that `MinimapAdjustmentDiagnostics` already reified. Recording a dropped candidate is the point — a seam rule that only ever adds types is not being applied. **Long signatures shortened: 1, unchanged** |
| Seams retired | 41 fns / 93 sites (tree row) | **this row 11 fns / 21 gate sites → 2 / 15.** Seven inspection accessors and the eleven-field `MinimapAnalysisSnapshot` became one 20-field `MinimapEvidence`, with 37 widget-test call sites rewritten. **Zero new actuation seams; slot 5b's budgeted one remains unspent.** Of the row's two *existing* actuation seams, one **retired** onto a real production drive and one is **kept with its justification at its definition** — the three-way disposition the convention asks for, applied to seams a consolidation that only names inspection seams would have carried silently past |
| Automation projections | 7 | **7, and this is a result rather than an omission.** The minimap's ≥18 `visual_geometry.native_minimap` fields, four `pixel_anchors`, `surfaces.minimap_requested`, and the `minimap-refresh` blocker are **unchanged and unwidened**; what changed is that `ui/automation.rs` no longer reaches through `.imp()` to build them. **Five** reach-throughs retired, one more than the four the matrix's table catalogued — the fifth, `editor.imp().minimap.render_hold`, was found during the retirement. `ui/automation.rs` now contains **zero** `editor.imp()` reads, leaving two `window.imp().tab_view` reads for slot 7 |
| Data-safety defects fixed | 7 (slot 5a) + 7 (slot 5b) | **2 more.** One is this row's: `dispose()` cleared the minimap's template children and widget slots but left its `Debounce` and `SettleBurst` **armed**, over callbacks that reach panicking `TemplateChild` accessors. The other is in the **already-migrated load row**, and it is the more serious: three exits of `finish_chunked_install` returned without restoring the installation state they had captured, so a superseding load adopted the already-suspended values as its own baseline and faithfully restored them to *suspended* — leaving the tab read-only with local-history capture and minimap edit tracking disabled for the rest of the session. One of those exits also left `begin_irreversible_action()` unmatched, disabling undo. A migrated row is not a closed row |
| Pre-existing blockers fixed | 2 (slot 5b) | **2**: `scripts/run-performance-smoke.sh` filtered on `ui::search_panel::runtime::tests::…` after slot 2a renamed that module to `execution`, and libtest exits 0 on a filter matching nothing, so the `search_interactive_policies` lane had reported a green proof that did not run since 2026-08-25 — now re-keyed **and** given a match-count assertion so it fails loudly next time. `scripts/check-filesystem-boundary.sh` also carried `crates/lushtext/benches` as a scan root that **has never existed in repository history**; `rg` silently skips a missing path, so it protected nothing |
| Facade budget | 370 unchanged; 78 lines of headroom on the tree row | **370 unchanged and not edited — but this is the first slot to need the escalation path at all.** The first honest minimap facade measured **389**. Escalation step one, *delegate harder*, sufficed: the four widget accessors became `widgets.rs`, a called presentation surface, landing the facade at **355**; the cold read's seven fixes took it to **366 of 370**. Step two (amend the number) and the two forbidden responses were not reached. The new datum the programme should carry forward: what stressed this facade was neither stage orders (five, fewer than 5b's twelve) nor inversions (six) but the **external entry surface** — 24 operations called from 16 files, against load's seven |
| Permitted role homes | nested home fully exercised | **per-workflow subdirectory, fourth adopter in `ui/editor_page/`**, after `save/`, `load/`, and `buffer_replacement/`. `make check-workflow-boundaries` reports **11** pure mutation-scoped policy modules, up one, because this row created a `policy.rs` where none existed |
| Mutation configuration | 72 `exclude_re` entries | **the minimap's 14 entries naming 66 methods retired to 4 entries naming 0 methods**, and the hand-listed `examine_globs` entry **retired rather than re-pointed**, leaving exactly one pre-convention hand-listed UI file (`markdown_preview/inline_footnotes.rs`, slot 7's). **Slot 7a retired that last one too, and the configuration now has zero hand-listed UI entries**: the module became `ui/markdown_preview/policy.rs`, which the convention reaches by name. Unlike the minimap case this was a **rename rather than an extraction**, so the entry *did* select the file beforehand and the **relocation** is a parity claim rather than a gain from zero — **175 relocated** — but the module then **gained 12** when the facade migration moved this workflow's fuzz and property entry points into it, ending at **187**, then **177** after 10 of those 12 were excluded as unkillable by construction. Reported separately; an earlier revision of this row claimed "175 before, 175 after", which was measured before the gain and was false by the time it shipped. The six re-keyed `exclude_re` entries were therefore verified by **direct measurement** rather than by an unchanged total: **210** mutants without them, **187** with, so all six match real generated mutants and suppress **23**. Slot 7a also renamed `ui/window/adaptive_shell.rs` to `ui/window/policy.rs` (**0 mutants before, 81 after**), added four new `policy.rs` modules in rows recorded as owning `none` (+82), and retired the stale `ui/window/tabs.rs` calibration comment, which named a file the current `examine_globs` never selects. Configured total **5,216 → 5,381**; pure mutation-scoped policy modules **11 → 17**. Of the preview module's 12 gained mutants, **10 are excluded as unkillable by construction** (feature-gated fuzz/property entry points the default lane does not compile) and **2 are production policy**, one of which was a genuine untested contract now killed. **The newly-in-scope mutants were executed and triaged to zero**: 17 survivors on the first run, 3 after ten new tests, 1 on the verifying run (**162 tested / 148 caught / 13 unviable**), and 0 after that last one was killed. Every survivor was killed by a **test**; the change ends with **no new equivalence exclusion at all**. The final survivor is the instructive one: it was a second operator on `properties_inner_split_width`, the function extracted only to narrow an exclusion, and giving that named pure function a direct contract test killed all five of its mutants and let the exclusion be **deleted** rather than widened. Reading the retired entries against the tool rather than the source found **seven method names with zero definitions anywhere** and **four entries anchored to a `line:column` that matched no generated mutant** — a stale exclusion is a recorded equivalence claim that has quietly stopped protecting the mutant it names |
| Convention changes | 2 statements across 2 capabilities | **2 statements across 2 capabilities**: path-keyed mechanical gates in `workflow-readability-boundaries` (re-key or retire in the same change, in *every* implementation of the predicate, proved by running the gate rather than reading the patch), and hand-listed scope-entry retirement plus line/symbol-anchored exclusion re-verification in `mutation-testing`. The retroactive re-check across all ten previously-migrated rows found **one** real disarm and two adjacent dead keys |

### Baseline after slot 7a

Slot 7a migrated **four tier-1 rows** and discharged **one cross-cutting lane's**
surface obligations. It stopped one row short of its declared boundary; see the
remaining-scope table.

| Quantity | After slot 6 | After slot 7a |
| --- | --- | --- |
| Workflows migrated | 11 | **16** (plus `WFR-PRINT`, `WFR-EDITOR-FIND`, `WFR-STATUS-NOTIFICATIONS`, `WFR-ENCODING`, `WFR-MARKDOWN-PREVIEW`). Every Completion Rule axis satisfied on each: narrative facade, coordination role, pure policy where owned, evidence surface **or a recorded measured conclusion that the row owns none**, and mutation figures reported by kind |
| Policy modules relocated | 6 of 6 candidates | **6 of 6 candidates, plus 6 modules brought into the mutation convention that were never on the candidate list at all.** Four are new `policy.rs` modules in rows the census recorded as owning `none`; two are renames of already-pure modules that generated **zero** mutants under any scope entry. `make check-workflow-boundaries` reports **11 → 17** pure mutation-scoped policy modules |
| Mutants generated | 5,216 configured | **5,381 (+165)**, and the arithmetic closes exactly: **+82** from the four new row policies (3 print, 36 editor-find, 11 notifications, 32 encoding), **+80** from the `adaptive_shell` → `window/policy.rs` rename, and **+2** net from the preview module (12 gained, 10 excluded as unkillable by construction because they are feature-gated fuzz/property entry points the default lane does not compile). **Triaged to zero, entirely by tests**: 17 survivors, then 3, then 1, then 0 — and the change ends with **one fewer** `exclude_re` entry than it started with, because the extraction made for exclusion granularity turned out to make the exclusion unnecessary |
| Seams reified | 9 | **10** (plus `EditorNotificationTarget`, the bus owner/surface pair the census recorded as needing none and which was in fact rebuilt at four call sites from one owner id). Two rows probed and **correctly found to need none**, with the negative finding recorded rather than assumed |
| Seams retired | 2 / 15 (minimap row) | **`WFR-STATUS-NOTIFICATIONS` 1 → 0**, retired onto **production pure policy** rather than consolidated into a surface: its one seam was a `test-utils` wrapper around a pure function, so a surface would have been a surface over nothing. `WFR-BUFFER-SNAPSHOT`'s **three parallel typed observation types** became one surface with named components, eliminating a five-field duplication that had let one fact have two declarations. **Zero new actuation seams; slot 5b's budgeted one remains unspent** |
| Automation projections | 7 | **7, unchanged.** No new evidence surface projects to D-Bus: print's and buffer snapshot's surfaces are `test-utils`-gated and report no automation field, and the exported contract is untouched. The spine's own **stale cell was corrected** — it claimed four projections where seven are registered |
| Facades measured against 370 | 11 | **16.** Print **105**, notifications **153**, encoding **155** (from a 907-line file), editor-find **238** (from a 395-line `mod.rs`, written *down* to a pre-declared ≈230 target), preview **270** (from a 1,983-line `mod.rs`). **370 unchanged and not edited**; escalation step one was applied *before* a measurement forced it, by classifying encoding's six grouped-row dialogs as a called presentation surface. `ui/search_panel/mod.rs` still sits at exactly **369** |
| Data-safety defects fixed | 7 (5b) + 2 (slot 6) | **1 confirmed defect fixed**, plus a **pre-existing accessibility gap** the migration surfaced. The teardown-before-close defect — confirmed independently by slot 5a (M-3) and slot 5b (finding 4) as **one** defect — is closed by *deleting* the eager block rather than moving it, because `handle_tab_detached` was already the terminal and "move" would have duplicated the teardown. Regression test **proved to fail without the fix** by deliberate revert, needing **no new actuation seam**. An earlier revision of this row also claimed a *second* consequence (a duplicate tab from the premature `open_paths` retirement); **that claim was withdrawn after measurement** — reverting only that removal does not reproduce it, because the load-completion path's `reconcile_open_paths_from_tabs()` heals the set. The removal was redundant and its deletion is still correct, but it was not demonstrably reachable. The accessibility gap is real and fixed: the encoding dialogs had **no** accessible metadata at all, so the current encoding, line ending, or invisible-character mode reached assistive technology only as subtitle prose; option rows now carry `Radio` role and `Checked` state |
| Handed-on findings landed | 0 of 5 (5b's, un-homed for five slots) | **4 landed in `docs/next/persistent-format-hardening.md`** with severity, re-verified sites, owning rows, and close conditions, plus topical cross-references in `bookmarks.md` and `workspace-context-switching.md`; the fifth is the teardown defect, **recorded as closed**. One site had already moved under slot 5b's own dissolution, which is what a year of archived-only handoffs produces |
| Pre-existing blockers fixed | 2 (5b) | **1 gate fail-open closed and 1 existing check refined, each proved by a deliberate red.** `check-accessibility-policy` **passed when a smoke summary was absent** — so wiping the artifact root, which the lane instructions ask for, turned a hard failure into a silent pass. And the slot ledger's reconciliation **could not represent a discharged cross-cutting lane** — a refinement rather than a fail-open, because it was failing *closed*: it demanded `migrated` for any row on a complete line, a status such a row can never reach, so settled work had to be recorded as *outstanding* — which is how `WFR-PLAIN-DISPOSAL` came to be listed as unfinished in a slot that had settled it. Neither fix reduces protection; both negative arms are self-test cases |
| Convention changes | 2 statements / 2 capabilities | **1 statement in 1 capability**: `mutation-testing`'s inclusion-side discovery obligation, with role-based classification and the parity-versus-gain reporting rule. Landed **with** its mechanical check and both proof arms. Deltas 1 and 2 were **deliberately withheld** — both assert obligations only the closing change can discharge |

### Convention friction slot 7a hit, recorded for slot 7b

- **A census cell that reads as settled can be wrong in its *kind*, not just its
  number.** Four rows recorded as owning `none` pure policy each own one; the
  preview row's "evidence surface" cell names a **services-owned** type the row
  does not declare; the spine's seam cell restated a gate-site count as a
  declaration count. The lesson for 7b: re-derive the *shape* of a cell, not only
  its magnitude.
- **A terminal status label still means two things.** `cross-cutting` means both
  "resolved, nothing to do" and "resolved, and its surface obligations are
  discharged". This slot had to widen a gate to express the difference at all, and
  the vocabulary that would fix it properly is capability delta 1's — 7b's.
- **An inspection seam's disposition is not always consolidation.** The
  notifications row's one seam wrapped a *pure function*; the right disposition was
  retirement onto production policy, and consolidating it would have built a
  surface over nothing. The requirement should be read as "retire it", with
  consolidation as the usual means rather than the only one.
- **A proof's premise needs measuring too.** Two of this slot's evidence proofs
  failed on first run for premise reasons, not defect reasons: `window.close()` is
  **not** `dispose()` and leaves template children intact, and a `Dispose` snapshot
  test-edit does **not** release the session it names. Both are now recorded in the
  tests themselves so the next author does not re-derive them.
- **`git mv` plus a substring rewrite is not a rename.** Bulk-substituting an
  accessor name across a test file produced two mangled identifiers
  (`overflow_buffer_snapshot_evidence`) that only the compiler caught. Prefer
  anchored replacements over substring ones when the old name is a prefix of a
  variable.

### Convention friction slot 6 hit, recorded for slot 7

**A gate keyed on a literal path is a gate a migration disarms, and the disarm is
green.** This is the slot's central finding and it generalizes past this row.
Every prior slot's structural risk was coverage that could be *lost and measured*;
this one was protection that vanishes while every command still exits 0. Two
properties make it worse than a stale document:

- **Reviewing the edit cannot distinguish a correct re-key from a silent disarm.**
  Both look like a path being updated. Only *running* the gate against the shipped
  tree tells them apart, which is why the amended statement requires the run.
- **One predicate implemented twice can be half-fixed.** The native-minimap
  invariant lives in `scripts/check-visual-proof-policy.py` and in
  `crates/cargo-gtk-proof/src/policy.rs`, with nothing linking them. Slot 6 added a
  parity assertion to **each** and proved each by a deliberate red, because one
  assertion on one side is the half that passes while the other side is wrong.

**Slot 7 inherits this directly.** `ui/window/actions.rs` and `ui/window/imp.rs`
both appear in that same native-minimap predicate and both belong to
`WFR-SHELL-LAYOUT`, which slot 7 migrates. If either moves without re-keying both
implementations, the row's own pixel proof stops being required and the lane keeps
passing.

**A fail-open filter is the same defect wearing a different key.** The one finding
from the ten-row retroactive re-check was not a path at all: it was a *test name*
in `scripts/run-performance-smoke.sh`. `cargo test` exits 0 when a filter matches
nothing, and the lane checked only the exit status, so a renamed module turned an
advertised proof into a no-op that reported success for three days. Slot 7 should
treat every string-keyed lane filter — test names, bench group names, grepped
evidence labels — as the same hazard class, and prefer asserting a non-zero match
count over trusting an exit code. This row's own two perf-smoke widget test names
and its two `minimap-(analysis|cancellation)-evidence` labels were deliberately
preserved for exactly this reason, and the lane's summary was checked for the
grepped lines rather than the command's status.

**An evidence surface's own proofs are worth writing before you believe them.**
The disposal proof for `MinimapEvidence` **failed on first run**, and the defect
was real: `wrapped_layout_analysis_required` reached `source_view()`, a panicking
`TemplateChild` accessor, so a teardown read crashed rather than answering. The
constraint had been *stated* in the module doc a few minutes earlier and was
false. Slot 7 should expect the same: write the three proofs first, run them, and
treat a green first run as the unusual outcome.

**A migrated row is not a closed row.** The mandatory `data-safety` pass found its
more serious defect in `WFR-DOCUMENT-LOAD`, migrated by slot 3b, reached from this
row only because the minimap's `set_minimap_tracking_suspended` is one of the four
values that exit failed to restore. Nothing about the load row's own migration was
wrong; the defect simply had not been looked for from this angle.

### Convention friction slot 5b hit, recorded for slots 6 and 7

**A handed-on number is a hypothesis, not a measurement.** Slot 4 made re-derivation an
obligation and slot 5b is the strongest evidence yet for why: **five separate inherited
figures were wrong, every one in the direction of more work** — the stage trace (11/38 →
**12/44**), the materialization facts ("five" → **six**, where the source's own prose
contradicted its own table), the widget reach-through (45 → **79**, of 929 total, with
TemplateChild 113 → **179**), the `file_tree.rs` survivors (11 → **12 generated**), and
the dangling matrix pointers (5 → **12**). Only the seam census (60/111) and the size
census reproduced exactly — and the size census reproducing exactly is itself a finding
worth recording, because slot 5b's *own proposal* had first claimed it was stale by ~900
lines by comparing a production-only census against raw totals. **The unit must be stated
on every figure mechanically, not carefully**: that error happened to the very change
carrying the corrective.

**Two of the eight retroactive-re-check findings were invisible to every gate**, and both
required reading something other than the matrix. `G1` is an **undeclared role module** —
`ui/window/drafts/retirement.rs` exists and declares itself the `retirement` job, while
`WFR-DRAFT-RECOVERY` declared only four coordination modules; the boundary check
validates that *declared* paths exist, never that *existing* role modules are declared.
`G8` is **twelve dangling evidence pointers** across four archived changes, none of which
resolved on disk; the gate resolves live-form paths against the archive directory, so a
stale pointer stays green forever. A re-check budgeted as re-reading the matrix will find
neither.

**`G7` is the sharpest argument for the amendment that found it.** The live
`WFR-DRAFT-RECOVERY` cell claimed a relocation landed "with parity proved"; its own cited
evidence says the opposite — *"the old location was outside the mutation scope ... the
move is a coverage gain rather than a parity case."* That is exactly the parity/gain
conflation slot 5b's `mutation-testing` amendment forbids, found **in the live matrix
while the amendment was being written**.

**Two mutation-tooling traps that cost real time.** First, **`make mutants-diff` proves
nothing on an uncommitted worktree and exits 0 doing it**: it builds its diff with a
three-dot commit range, so working-tree edits are invisible, and `git add -N` does not
help because the problem is the range and not the index. Second, **editing any file in
the mutation scope while a run is in flight silently invalidates it** — a mid-run
`cargo fmt` shifted line numbers and produced a false MISSED. The hand-check that
disproved it nearly went wrong too, because reproducing an operator mutation requires
letting Rust's precedence apply: `a || b && c` is `a || (b && c)`, not `(a || b) && c`.
Related and useful: **`--re` does not bound a run at all** (the unfilterable floor is
**34** field-deletion mutants, measured from the tool), while **`--in-diff` does**.

**The data-safety pass forced a scope decision for the second slot running.** Slot 5's
pass over this same code found eleven findings and consumed that change's capacity; slot
5b's found **seven**, and the response this time was explicit triage rather than absorbing
all of them: two fixed, five handed on with named owning rows and durable homes, and the
decision recorded as a deviation at the head of the task list. **One of the five is a tree
file** and is deferred anyway, with its reason stated — its fix changes a persistence
invariant in the same step as relocating its owner, which is how a tier-3 workflow
acquires an unreviewable defect. The generalisable lesson for slots 6 and 7: a tier-3
pass will find several defects, and the plan needs a stated disposition rule *before* the
pass runs, not a hope that there will be one finding.

**A pre-existing blocker can hide behind the blocking gate itself.** The documented
blocking Clippy command uses `--all-features`, which **hides** a denied `unused_self` that
the default-feature build errors on — so `origin/main` did not compile under default
features while `make check` was green. Task 10.2 exists precisely for this, and it found
it on the first run. Run both configurations.

**Documentation can cite a symbol that has never existed.** `.agents/rules/ui.md` named
two deferred-restore functions; only one exists. `restore_materialized_state` appears in
the rule, in slot 5a's evidence snapshot, and in slot 5b's own task list — three
documents, two changes, and no `git grep`. The rule now names the one real site and
deliberately names **no file**, because migrations rename the owner.

**What slot 5b did not learn, and slot 6 should expect to.** The nested role home was only
**partially** exercised: the canonical home landed, the nested coordination modules did
not. The evidence surface over a lazily materialized `GtkTreeListModel` and over a
variable-sized child collection — the reason the no-materialization rule exists — is
**still unbuilt**, and its six hazards are recorded and re-verified but not discharged.
The cold read was not run, because there is no narration to read.

## 3. Remaining scope

Five changes remained at Phase 0 (3b and 4 through 7); slot 7 later split into 7a and 7b. Order is by increasing risk; every
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
| 4 | User-content restore family (`migrate-user-content-restore-workflow-readability`, **complete**) | All four rows **migrated**: `WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`, plus a further `WFR-AUTOMATION-SPINE` projection (`LocalHistoryEvidence`). Nothing from this family is outstanding. One acceptance item is deferred rather than unmet: the live-session `make run` paned-warning proof, which needs user availability — see task 10.10 and `evidence/live-run.md` | proposal + tasks + 1 spec delta |
| 5a | **complete** — migrated the notes and bookmarks family, retired `NoteSourceRefreshCoordinator` onto the shared single-flight coordinator, added the no-materialization and child-collection evidence-surface statements and the called-presentation-surface taxonomy scope, and fixed **seven confirmed pre-existing data-safety defects** including a rename that silently destroyed an existing file (`migrate-workspace-tree-and-notes-workflow-readability`) | `WFR-NOTES-BOOKMARKS`, continuing `WFR-AUTOMATION-SPINE` projections (`NotesEvidence`) | proposal + tasks + 2 spec deltas |
| 5b | **complete — `WFR-WORKSPACE-TREE` migrated**, in `migrate-workspace-tree-workflow-readability`. Facade **291 of 370** by delegate-harder alone; **three dissolutions** (`tree_loading.rs`, `tree_index.rs`, `watch_targets.rs`) plus `workspaces.rs` dissolving into four `execution` roles; **twelve** stage orders and 44 resumption points re-derived against a floor of five (**8.8x**, the programme's widest); the first **nested** role home; `evidence.rs` discharging the **no-materialization** statement with a driven collapsed-and-expanded inertness proof; both `ui/automation.rs` reach-throughs retired and `window.workspace` projected from evidence; **both relocations at exact mutant-by-mutant parity** (the first relocation since 3a) with their 7 inherited survivors triaged to 0; two convention amendments with a nine-row re-check that found **eight gaps**; and **seven confirmed data-safety defects fixed** (two from pass 1, three from pass 2, two from the fix cycle), including two CRITICAL: M-4's superseded-load guard, and that guard's own fix being inert. **Seam retirement is complete**: 60 fns / 111 gate sites → 41 / 93. **Remaining follow-up, recorded rather than hidden**: `scan_execution.rs` is ~2,000 production lines, five confirmed non-tree data-safety findings are handed on with owners, task 7.6's two-tree automation capture is unrun, and the live `make run` walkthrough awaits user availability | `WFR-WORKSPACE-TREE`, continuing `WFR-AUTOMATION-SPINE` projections | proposal + tasks + 2 spec deltas |
| 6 | **complete — `WFR-MINIMAP` migrated**, in `migrate-minimap-workflow-readability`. The one slot the record expected to need a `design.md`, and the expectation was confirmed rather than obeyed. Facade **366 of 370** after **one escalation step**: the first honest facade measured 389, and *delegate harder* sufficed — the four widget accessors became `widgets.rs`, a called presentation surface, which is where the taxonomy already put them. The budget number was not edited and the census row was not split. **Five stage orders and six resumption points** re-derived against a recorded floor of three. **Two path-keyed gates re-keyed or retired, and the disarm observed before it was fixed** — the `.cargo/mutants.toml` `examine_globs` entry **retired** (0 mutants generated after the move, still exiting 0), and the native-minimap invariant predicate re-keyed to a directory prefix in **both** implementations, each with its own parity assertion proved by a deliberate red. The Python half's self-tests were **unreachable** before this change and now run. The retroactive re-check across ten migrated rows found **one** real disarm, inherited from slot 2a: `scripts/run-performance-smoke.sh` still filtered on `ui::search_panel::runtime::tests::…` after that module was renamed to `execution`, and libtest exits 0 on a filter that matches nothing, so a green proof had not run since 2026-08-25. **Mutation configuration retired from 14 entries / 66 method names to 4 entries / 0 method names**, with seven named methods found to have zero definitions anywhere and four entries anchored to a `line:column` that matched no generated mutant. **All 12 first-run survivors triaged to zero** — nine killed by tests, three removed by extracting a block that was duplicated verbatim between two functions, which then exposed a dead cap and deleted a fourth. Final run **412 generated / 406 caught / 0 missed**. **Two confirmed data-safety defects fixed**, one of them in the already-migrated load row: a superseded chunked install returned without restoring the suspension it captured, so a following load adopted the suspended values as its own baseline and made a read-only tab with local-history capture disabled permanent for the session | `WFR-MINIMAP`, continuing `WFR-AUTOMATION-SPINE` projections | proposal + design + tasks + 2 spec deltas |
| 7a | **complete — five rows migrated and one cross-cutting lane discharged.** `WFR-PRINT` (facade 105/370), `WFR-EDITOR-FIND` (238/370, from a 395-line `mod.rs`), `WFR-STATUS-NOTIFICATIONS` (153/370), `WFR-ENCODING` (155/370, from a 907-line file), and **`WFR-MARKDOWN-PREVIEW` (270/370, from a 1,983-line `mod.rs`)** — facade and evidence only, with the topical decomposition two earlier changes paid for left untouched apart from import paths. Its recorded inversion count was **low by ~3.2x** (5 recorded, 16 resumption points derived), and **11 of its 13 tuple-returning inspection seams** retired into one named surface. **Four rows the census recorded as owning `none` pure policy all own a `policy.rs`**: probing found 5 decisions in editor-find, 6 in notifications, and the whole user-facing dialog vocabulary in encoding — **+82 mutants, all gain from zero**, none previously covered. **`WFR-BUFFER-SNAPSHOT`'s three parallel typed observation types consolidated** into one `BufferSnapshotEvidence` with named components, all three surface proofs discharged. **§D1 resolved** (the shell row is not one workflow). **Capability delta 3 landed** with its inclusion-side discovery check and two policy renames (parity 175→175; gain 0→78). **The teardown-before-close data-safety defect fixed** with a revert-proved regression test, and **`check-accessibility-policy`'s fail-open closed**. Slot 5b's four remaining handed-on findings **landed in `docs/next/`**. Deltas 1 and 2 deliberately withheld as 7b's | `WFR-PRINT`, `WFR-EDITOR-FIND`, `WFR-STATUS-NOTIFICATIONS`, `WFR-ENCODING`, `WFR-BUFFER-SNAPSHOT`, `WFR-MARKDOWN-PREVIEW` | proposal + design + tasks + 3 spec deltas (1 landed) |
| 7b | Residual close-out. `WFR-PLAIN-DISPOSAL` tier-3 surface narrowing; **the `WFR-SHELL-LAYOUT` hybrid §D1 selected**, with §D1's four contested-file findings as authoring inputs; `WFR-AUTOMATION-SPINE`'s terminal status (§D3); **capability deltas 1 and 2**, which assert obligations only the closing change can discharge; and the programme closeout with its single deferral inventory. Also outstanding: triage of the **160 newly-in-scope mutants** slot 7a generated | `WFR-PLAIN-DISPOSAL`, `WFR-SHELL-LAYOUT`, `WFR-AUTOMATION-SPINE`, matrix completion | proposal + tasks + 2 spec deltas |

### Slot 7a's structural finding: §D1 resolved, the shell row is not one workflow

Recorded here rather than only in the change directory, because a change
directory is archived and this decision outlives it — the failure mode this
programme has already suffered once.

**`WFR-SHELL-LAYOUT` is NOT one workflow.** The row's own stage trace called it
*"a residual grouping of 19 shell surfaces"* and licensed a split *"if the facade
work shows it holds more than one story."* It does. The evidence is stage-order
and shared-state evidence; **the line count supports nothing and is not offered
as support**, because a split justified by line count is the forbidden budget
response wearing the grouping clause as cover.

- **Criterion 1 (one operation, or a family sharing one ordered stage sequence)
  fails.** The row's files own **at least 12 distinct ordered stage orders** before
  the contested files, and past 19 with them: adaptive geometry 1, tab strip 1
  (+3 synchronous projections), Focus Mode 1, recent documents 2 (+1 lazy
  projection gate), shell dialogs **5**, transient dismissal 1, startup preflight
  1, `focus_indexing.rs` **3**, `window/search.rs` **4**, plus a **tenth story
  nobody had enumerated** — `mod.rs`'s `setup_theme_selector` (~100 lines), which
  appears neither in the row's story list nor in the no-coordination-tier list.
- **Criterion 2 (shared coordination state, not merely a shared `imp` struct)
  fails.** **15 of 18** `imp` state groups are touched only by one candidate's
  files. Exactly **three** genuine cross-candidate couplings exist, and none is a
  shared generation counter, admission budget, or settle gate: one read-only
  `Cell<bool>`, one call, and one `HashSet<PathBuf>` of tab identities. Meanwhile
  **six independent generation/identity mechanisms** live in the row's files, none
  shared with another candidate.
- **Criterion 3 was not reached**, 1 and 2 having already failed.

**Outcome: (c), the hybrid** — one workflow row for the adaptive shell geometry
story (which satisfies criterion 1 cleanly: seven entry points converging on one
ordered sequence, and the smallest external entry surface of any candidate),
replacement rows for the surfaces that are separate stories, and
no-coordination-tier entries for the rest. **Implementing it is outstanding work.**

Four findings the implementing change must not re-derive or inherit blindly:

1. **`dialogs.rs` is not this row's, and not a called presentation surface.** It
   owns five stage orders and three unrecorded freshness/identity values, and its
   confirmed-close coordination is consumed by the **already-migrated**
   `WFR-DOCUMENT-SAVE` (`close_save_session_is_current`) and
   `WFR-DRAFT-RECOVERY` (`clear_close_discard_drafts`) rows. Do not make it a row
   of its own without first deciding which migrated row owns those stages.
2. **`focus_indexing.rs` is three stories, not two** — its own doc says so, and
   the largest, ~590 lines of **editor-memory eviction orchestration** with its own
   generation counter and 8 test seams, is owned by **no story anywhere**
   (`WFR-EDITOR-MEMORY` is `exempt` and covers only `model/editor_memory.rs`). The
   other two belong to the migrated command-palette row and to the geometry
   candidate.
3. **`ui/window/search.rs` (955 lines) was attributed to nothing**, and is
   `WFR-SEARCH-REPLACE`'s window-side surface — but it is *more* than a called
   presentation surface: it holds two of that workflow's ordered coordination
   stages plus one coordination job of its own.
4. **`transient_surfaces.rs` does not belong on the no-coordination-tier list.**
   It has a strictly ordered dismissal ladder *and* a one-tick idle latch, so the
   list's "no ordered stages" preamble is false for it. And **`actions.rs` is not
   demotable**: it contains two of the geometry candidate's stages verbatim, and
   demoting it would be a route to demoting its pixel-proof obligations.

**The binding constraint on any implementation (§D6): a split MUST NOT reduce
protection.** `ui/window/actions.rs` and `ui/window/imp.rs` are literal path keys
in **three** predicates in **each** of `scripts/check-visual-proof-policy.py` and
`crates/cargo-gtk-proof/src/policy.rs`, plus six further literal `imp.rs` keys in
those implementations' own self-tests. The geometry candidate's stage 1 lives in
`actions.rs` and its clamp/breakpoint path lives in `imp.rs`. Moving either into a
new module no predicate names would disarm two named pixel invariants and the
sidebar animation matrix **while every gate exited 0**. The
`crates/lushtext-core/src/ui/window/` prefix re-key is **forbidden**: it would
demand those proofs of four migrated per-workflow role homes that no predicate has
ever protected. This is why the first attempt's one shell-side rename
(`adaptive_shell.rs` → `ui/window/policy.rs`) was **same-directory**: no geometry
code moved, so no predicate's file set changed and no re-key was required.

### Why slot 5 split into 5a and 5b

Slot 5's proposal argued explicitly against a split and recorded the reason so it
would not be re-litigated: no convention deliverable of the change is a
prerequisite of its own second half, and the two rows share the
rename→sidecar-migration boundary and the `services/palette/notes.rs` population,
both of which a split would force to be decided twice. **That reasoning held, and
the split happened anyway, for a reason the proposal did not anticipate.**

The proposal's own task 0.6 required a `data-safety` pass in explicit mode before
implementing, and budgeted for findings on the evidence that four consecutive
prior slots each found one. This slot's pass found **eleven**, of which the most
severe is a normal-usage data-destruction bug: inline rename validated only
empty-or-unchanged, and the platform rename silently replaces a regular
destination, so renaming a file to the name of an existing sibling destroyed that
file's contents with no prompt, no warning, and no undo.
`.agents/rules/preexisting-blockers.md` is binding and has no exceptions, so those
fixes came first — and seven of them landed, each with a regression test proven to
fail without its fix.

That consumed the change's capacity before `WFR-WORKSPACE-TREE`'s structural
migration began. The choice was then between a **partially** migrated tier-3 row
— facade rewritten, 60 seam functions half-retired, a matrix row claiming roles
that do not all exist — and an honestly unmigrated one. The convention's own
Completion Rule answers that: a row may be marked `migrated` only when every role,
seam value object, evidence surface, and parity claim exists. A half-migration of
the workflow that renames and deletes the user's documents is the worst available
outcome, so the row stays `pending`.

**What 5a nevertheless leaves in place for 5b**, so the split is not pure loss:

- `ui/sidebar/policy.rs`, `ui/sidebar/seams.rs`, and `ui/sidebar/test_policy.rs`
  exist, are pure, and are inside the `ui/**/policy.rs` mutation scope. They were
  created *by* the data-safety fixes rather than ahead of them: the rename
  refusal, the unique-name sequence, and the prefix-matching rule are pure
  decisions, and the wrong-row defect was fixed by reifying exactly the
  `FileOperationTicket` + `FileOperationFacts` seam the row owed.
- The row's two counted, justified test-policy configuration seams
  (`set_workspace_rename_worker_delay_for_test`,
  `set_workspace_placeholder_cleanup_delay_for_test`) exist and are documented
  with why each was necessary: without them, two of the data-safety regression
  tests passed against the broken code as well as the fixed code.
- The census re-verification, the reconciled stage trace (**11 stage orders, 27
  primitives, 11 non-primitive callback resumptions** against a census floor of
  5), the role-home collision analysis, the facade budget measurement, and the
  `journal` verdict for workspace persistence are all recorded as evidence, so 5b
  starts from decisions rather than from questions.

**The lesson for slots 5b, 6, and 7**: a tier-3 migration slot must budget for
the data-safety pass finding *more* than one defect, because the pass is aimed at
exactly the code the migration is about to restructure. Slot 5's eleven findings
in one pass is not an outlier to discount; it is what auditing a workflow's file
operations and its sidecar family *together* produces.

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
| 4 | `migrate-user-content-restore-workflow-readability` — **complete**: all four tier-3 rows landed (`WFR-BUFFER-REPLACEMENT`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`) |
| 5–7 | not yet authored |

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
- slot 4 (complete): WFR-BUFFER-REPLACEMENT, WFR-SESSION-RESTORE, WFR-LOCAL-HISTORY, WFR-DRAFT-RECOVERY, WFR-AUTOMATION-SPINE (partial)
- slot 5a (complete): WFR-NOTES-BOOKMARKS, WFR-AUTOMATION-SPINE (partial)
- slot 5b (complete): WFR-WORKSPACE-TREE, WFR-AUTOMATION-SPINE (partial)
- slot 6 (complete): WFR-MINIMAP, WFR-AUTOMATION-SPINE (partial)
- slot 7a (complete): WFR-PRINT, WFR-EDITOR-FIND, WFR-STATUS-NOTIFICATIONS, WFR-ENCODING, WFR-BUFFER-SNAPSHOT, WFR-MARKDOWN-PREVIEW
- slot 7b (outstanding): WFR-PLAIN-DISPOSAL, WFR-SHELL-LAYOUT, WFR-AUTOMATION-SPINE

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
