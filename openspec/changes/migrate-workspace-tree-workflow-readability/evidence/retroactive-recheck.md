# Retroactive amendment re-check (task 1.3)

Discharges `openspec/specs/workflow-readability-boundaries/spec.md`'s
"Convention amendments are applied retroactively" for the two amendments this
change lands:

- **(a)** `gtk-adapter-module-boundaries` — before amending the bounded
  coordination role set for a pre-convention module that no listed role name
  describes, a migration MUST first determine whether that module is **one
  coordination job at all**; where it is not, it is **dissolved** across the
  existing roles and each part's destination is recorded in the matrix row.
  Escalation by amendment is reserved for a genuinely novel single job. The
  stage-order qualification rule applies to modules a migration **creates or
  renames**; a module already carrying a correct bounded role name MUST NOT be
  renamed or qualified for symmetry.
- **(b)** `mutation-testing` — a focused mutation run MUST state the
  unfilterable mutant floor its filter cannot exclude (measured for this repo:
  **34 struct-field-deletion mutants** across the configured `examine_globs`
  scope, of which **12** are in `services/file_tree.rs`); and a change that
  **both** relocates existing pure policy **and** extracts new pure policy from
  a GTK adapter MUST report relocation **parity** separately from extraction
  **gain from zero**.

Nine rows were `migrated` when this change started, so nine per-row re-checks are
owed. Slot 4's cost note projected eight; the ninth is `WFR-NOTES-BOOKMARKS`,
migrated by slot 5a after that note was written.

## Method

Per row: read the row's `### <ROW>` subsection under
[Migrated Workflow Roles](../../../../docs/workflow-readability-matrix.md), then
read every module on disk under `crates/lushtext-core/src/ui/` that the row owns —
not only the ones the row declares. Three sweeps were run against the code rather
than against the matrix, because a matrix-only reading cannot see an **undeclared**
module (`make check-workflow-boundaries` validates that declared role paths exist;
no gate detects a role module the row failed to declare):

1. **Module census.** `wc -l` over each role home, compared module-for-module
   against the row's declared roles, called presentation surfaces, seam value
   objects, and test policy. Nine rows, 79 modules.
2. **Cohesion read.** The module doc of every declared coordination module (29
   modules), looking for a doc that names more than one job, and for type/const
   declarations that belong to another role. Proxy greps: `struct|enum` and
   `const [A-Z]` per coordination module; `Evidence` types outside `evidence.rs`.
3. **Provenance.** `git log --diff-filter=A` per role home, to separate a module
   a migration **created** from one it **renamed**, which is the distinction
   amendment (b)'s second sentence turns on.

For Q3, the three changes' `evidence/mutation-*.md` files were read in full under
`openspec/changes/archive/`, and each recorded figure was compared against this
change's measured floor.

## Nine-row verdicts

| Row | Q1 — non-cohesive or topic-named coordination module | Q2 — sibling renamed/qualified for symmetry | Evidence pointer |
| --- | --- | --- | --- |
| `WFR-SEARCH-REPLACE` | **no.** Four coordination modules, all bounded-named, each one job. `execution.rs` (streaming search), `retirement.rs` (bounded disposal), `replace_execution.rs` (preview + checked apply), `journal.rs` (undo journal). No pure decision, evidence type, or seam value object is declared in any of them — the workflow's tickets are in `policy.rs`. `retirement.rs` does define `SearchRetirementSliceObservation`, which `evidence.rs` aggregates; see the adjacent finding below | **no — and this row is the precedent amendment (b) codifies.** Slot 2b explicitly declined to rename `execution.rs` to `search_execution.rs` when it created `replace_execution.rs`, and declined to rename `retirement.rs` for symmetry, with the reasoning recorded in the row | `ui/search_panel/{mod,execution,retirement,replace_execution,journal}.rs`; matrix L1596, L1631-1636 |
| `WFR-COMMAND-PALETTE` | **no.** Four coordination modules, all bounded-named plus stage-order qualifiers, each one job | **partial gap — G3.** No rename occurred (`git log`: all four created by `cf941215`, `runtime.rs` deleted). But `index_admission.rs` carries a stage-order qualifier on a bounded name that **was never spent** — the palette owns exactly one admission module. The row's note justifies leaving `retirement.rs` unqualified with "only one stage order retires anything" and never applies that same test to `index_admission`. Amendment (b) forbids fixing this by rename, so the fill is the missing justification | `ui/command_palette/index_admission.rs`; matrix L2061, L2078-2082 |
| `WFR-DOCUMENT-SAVE` | **no.** `admission.rs` = everything before document text is copied, `execution.rs` = everything after; both docs state the single job. `SaveCompletionTicket` is declared in `execution.rs` rather than a `seams.rs`; recorded, not a gap | **no.** Both created by `5072f517`; the workflow owns one stage order and neither name is qualified | `ui/editor_page/save/{admission,execution}.rs`; matrix L1682, L1704-1707 |
| `WFR-DOCUMENT-LOAD` | **no.** Three coordination modules; `admission.rs`'s doc enumerates four ordered sub-stages of **one** reserve-then-settle job, which is cohesion, not mixing. `journal` was checked and rejected on slot 3a's reusable test | **no.** All created by `d5f6c1db`; one stage order, no qualifiers | `ui/editor_page/load/{admission,execution,retirement}.rs`; matrix L1772, L1806-1808 |
| `WFR-BUFFER-REPLACEMENT` | **no.** One coordination module, and the row records why one is right. It declares eight types, including the seam value object `BufferReplacementRequest` and `BufferReplacementState`; a `seams.rs` was not created. Recorded, not a gap — the convention reifies seam value objects, it does not mandate a file | **no.** Created by `833c22ec`; single unqualified `execution.rs` | `ui/editor_page/buffer_replacement/execution.rs`; matrix L1985, L1990-1994 |
| `WFR-SESSION-RESTORE` | **no.** Three coordination modules, all bounded-named, each one job; `journal`'s two-readers justification is recorded | **no.** All created by `833c22ec`; no qualifiers | `ui/window/session_restore/{journal,admission,execution}.rs`; matrix L1857 |
| `WFR-LOCAL-HISTORY` | **no.** `journal.rs` owns reading the record plus keeping it consistent with its files, which is one job; it does carry two user-facing message constants that could be `policy` copy — noted, below the gap threshold. `LocalHistoryReplacementTicket` is declared in `restore_execution.rs` | **no.** All created by `833c22ec`. `preview_execution` and `restore_execution` are **both** qualified and both new, which is the qualification rule's intended case | `ui/window/local_history/{journal,preview_execution,restore_execution}.rs`; matrix L1900 |
| `WFR-DRAFT-RECOVERY` | **GAP — G1.** Not a non-cohesive module: an **undeclared** one. `ui/window/drafts/retirement.rs` exists (142 lines, created by the same slot-4 commit `833c22ec`) and its own module doc opens "The `retirement` coordination job for `WFR-DRAFT-RECOVERY`". The row's coordination cell names **four** modules; there are **five**. Worse, the row's prose reads as a denial: "`journal` absorbed the one the task list expected to be `retirement`" and "`retirement` was checked against it and **rejected**". Both statements are true *of orphan cleanup* and false *of the row*. Slot 5a's own (b) table counted "ten modules, all roles plus `seams.rs` and `test_policy.rs`" — which only adds up if `retirement.rs` is a role — so the matrix already contradicts itself | **no.** `autosave_execution` and `restore_execution` are both new (`833c22ec`), as the row states | `ui/window/drafts/retirement.rs:1-14`; `ui/window/drafts/mod.rs:138`; matrix L1939, L1944-1946, L2442 |
| `WFR-NOTES-BOOKMARKS` | **partial gap — G2.** `bookmark_execution.rs` (1,035 lines) opens "**Two of** `WFR-NOTES-BOOKMARKS`'s ordered stage orders live here" — editor note resolution and bookmark lifecycle. The module records its cohesion basis (a shared live `GtkSourceMark` projection and sidecar identity/generation), so it survives amendment (a)'s "is it one coordination job at all" test — but the **matrix cell** states it as a bare conjunction, "bookmark lifecycle and editor note resolution", with no cohesion determination recorded. Amendment (a) makes that determination a required, recorded step. Compare `WFR-DRAFT-RECOVERY`, whose row *does* record it ("share their shape exactly"). Also noted: `editor_execution.rs` carries dialog geometry, response ids, and placeholder copy alongside coordination while the row owns `chrome.rs`; judged in-role, since coordination may mutate widgets | **no.** `bookmarks.rs` and `editors.rs` (created `1f0aff3c`, 2026-07-13) were pre-convention **topic** names, so renaming them to `bookmark_execution.rs` / `editor_execution.rs` was required, not symmetry churn. `execution` is contested by four stage orders here, so every qualifier is earned; `journal.rs` is correctly unqualified | `ui/window/notes/bookmark_execution.rs:3-14`; matrix L2050 |

**Q1 across nine rows: one gap (G1), one partial (G2), seven clean.** No row owns
a coordination module whose *name* states a pre-convention topic — that half is a
confirmation across all nine, and it is the half most likely to have failed, since
five of the nine rows dissolved a `runtime.rs`, a `replace.rs`, a `bookmarks.rs`,
or an `editors.rs`. No coordination module in any of the nine declares an
`*Evidence` surface type.

**Q2 across nine rows: one partial (G3), eight clean.** Amendment (b)'s second
sentence is close to a confirmation, and the reason is provenance, not luck:
`git log --diff-filter=A` shows every qualified coordination module in the nine
rows was **created** by its migration, except the two notes modules renamed from
topic names. The only row that had a stable correctly-named sibling available to
churn is `WFR-SEARCH-REPLACE`, and it declined in writing — which is the behaviour
(b) now makes normative.

## Q3 — the three changes that relocated pure policy or ran a focused mutation run

| Change | (i) states the unfilterable field-deletion floor | (ii) separates relocation parity from extraction gain |
| --- | --- | --- |
| **slot 3a** — `archive/2026-08-26-migrate-document-save-workflow-readability/evidence/mutation-parity-save-policy.md` | **GAP — G4. No floor is stated, and the phenomenon is misdiagnosed.** Both runs were focused with `MUTANTS_RE`, and the file explains the extra mutants as: "`MUTANTS_RE` matches against the `--list` output, and the regex is **not anchored**, so both runs also swept a small number of unrelated `services/` mutants that happened to match." That is the wrong cause. The cause amendment (b)(i) names is that cargo-mutants 27's `--re` filter **does not apply to struct-field-deletion mutants at all**; they run regardless of the regex, anchored or not. The file also calls the population "a small number", where the measured floor is 34 | **compliant, and the programme's reference implementation.** "Two claims, reported separately, because mixing them makes both unreadable." Numerically split: 42 relocated (0 lost, 29/12/1 before and after — "parity holds exactly at the moment of the move") and 16 gained (14 caught, 0 missed, 2 unviable), with per-survivor disposition for each |
| **slot 4** — `archive/2026-08-27-migrate-user-content-restore-workflow-readability/evidence/mutation-{buffer-replacement,session-restore-policy,draft-recovery-policy,local-history-policy}.md` | **GAP — G5, stale figure.** A floor **is** stated, correctly diagnosed, in `mutation-buffer-replacement.md`: "cargo-mutants 27's `--re` filter does not apply to its struct-field-deletion mutants, so **32** of those ran regardless of the regex... a 'focused' run is focused plus that floor", of which "**16 survived** — **11** in `services/file_tree.rs` and **5** in `services/draft_service.rs`". `mutation-draft-recovery-policy.md` repeats a run-local form ("The 16 misses are the documented pre-existing floor"). This change's measurement supersedes those figures: **34** total, **12** in `file_tree.rs`. The stated floor is also a *survivor* count in the draft file and a *generated* count in the buffer file, which is the ambiguity (b)(i) exists to remove | **GAP — G6, prose-only for two of four rows.** `mutation-buffer-replacement.md` and `mutation-local-history-policy.md` correctly declare that **no** relocation parity numbers are owed. But `mutation-session-restore-policy.md` and `mutation-draft-recovery-policy.md` each cover a change half that relocated *and* half that extracted, and they separate the two **in prose only**: each reports a single pooled generated count for the whole `policy.rs` (**83** and **54**), with the relocated portion's share unstated. Slot 3a split 42/16; these do not split. Both then close on a figure pooled across **all four** rows ("246 mutants tested... across all four slot-4 policy modules"), which is the opposite of row-scoped |
| **slot 5a** — `archive/2026-08-27-migrate-workspace-tree-and-notes-workflow-readability/evidence/mutation-notes-policy.md` | **compliant, and the best instance in the tree.** It has a dedicated section, "The field-deletion floor, stated so it is not misattributed", which states the tooling fact, states that **the floor did not apply to this run** because it was focused with `--file` (`MUTANTS_SMOKE_FILE`) rather than `--re`, verifies all 81 mutants were in the target file, warns that "a future `MUTANTS_RE` run on this module *will* carry the floor", and hands the `file_tree.rs` survivors to slot 5b unattributed. Its **11** is superseded by the measured **12**, same as G5 | **compliant.** "Reported as a **gain from zero** with **no parity claim attached**, because nothing relocated into this module: all five notes domain modules stay in `model/`." The absence is stated as a conclusion with its reason, which is what (ii) asks of a change with only one of the two halves |

**Q3: two of three changes non-compliant on (i), one of three on (ii).**

## Gaps found and how each is filled

Eight gaps. Each correction lands in the **live** matrix cell with a pointer
naming the archived figure it supersedes; the archived evidence files are history
and are **not** rewritten.

### G1 — `WFR-DRAFT-RECOVERY` omits a fifth coordination module

Matrix L1939, replacing the coordination line:

> - coordination: `crates/lushtext-core/src/ui/window/drafts/journal.rs`, `crates/lushtext-core/src/ui/window/drafts/admission.rs`, `crates/lushtext-core/src/ui/window/drafts/autosave_execution.rs`, `crates/lushtext-core/src/ui/window/drafts/restore_execution.rs`, `crates/lushtext-core/src/ui/window/drafts/retirement.rs`

and appending to the row's `journal`-absorption paragraph (L1944-1946):

> **The row owns five coordination modules, not the four slot 4 declared.**
> `retirement.rs` was created by slot 4 and its module doc has always opened "The
> `retirement` coordination job for `WFR-DRAFT-RECOVERY`", but the row's
> coordination cell listed four. The `retirement`-was-rejected finding above is
> about **orphan cleanup** and stands unchanged: cleanup reloads and merges *this*
> record and therefore belongs to `journal`. What `retirement.rs` owns is a
> different job — releasing the eager startup **preload bodies** to a worker while
> keeping the compact markers the lazy admission queue still needs — which is
> `retirement` in this codebase's exact sense: off-GTK destruction of an in-memory
> payload the workflow is finished with. Slot 5a's amendment table already counted
> ten modules "all roles plus `seams.rs` and `test_policy.rs`", which only sums if
> `retirement.rs` is a role; this corrects the coordination cell to agree.
> Supersedes the four-module coordination cell recorded at slot 4.

### G2 — `WFR-NOTES-BOOKMARKS` records no cohesion determination for a two-stage-order module

Matrix L2050, replacing the `bookmark_execution.rs` parenthetical:

> `ui/window/notes/bookmark_execution.rs` (**two stage orders in one module, and
> the determination is recorded rather than implied**: editor note resolution and
> the bookmark lifecycle share the editor's live `GtkSourceMark` projection and one
> sidecar identity/generation, so a resolution completion and a lifecycle write
> validate against the same freshness state. Dissolving them across the row's other
> roles was considered under amendment (a) and rejected: the split would put one
> generation guard in two files. Contrast `WFR-DRAFT-RECOVERY`'s
> `autosave_execution`, whose two stage orders share their *shape*; these share
> their *state*)

### G3 — `WFR-COMMAND-PALETTE` never justifies a qualifier on an unspent name

Matrix, appended to the note at L2078-2082:

> **`index_admission.rs` is qualified although `admission` was never spent, and
> that is deliberate.** The palette owns exactly one admission module, so the
> collision test that leaves `retirement.rs` unqualified would leave this one
> unqualified too. It keeps the qualifier because the module admits *the file-index
> mutation queue specifically* — its 75 ms debounce, bounded retention,
> disposal-capacity retry, and flush gate are all the index stage order's — and an
> unqualified `admission.rs` beside `index_execution.rs` would read as the query
> flight's admission, which does not exist. Amendment (b) forbids renaming a
> correctly-named module for symmetry, and it equally forbids renaming this one
> *away* from symmetry: the fix owed here was the missing justification, not churn.

### G4 — slot 3a states no floor and misdiagnoses the cause

Matrix L1685, replacing the mutation-parity line:

> - mutation parity: `openspec/changes/archive/2026-08-26-migrate-document-save-workflow-readability/evidence/mutation-parity-save-policy.md` — the programme's **reference implementation of the two-claim split** (42 relocated at exact parity, 16 gained from zero, reported separately). **One correction, superseding that file's account of its own focused run:** it explains the extra `services/` mutants its `MUTANTS_RE` runs swept as an unanchored regex matching unrelated files. The real cause is that cargo-mutants 27's `--re` filter does not apply to struct-field-deletion mutants at all, so every focused `--re` run carries them; the floor is **34 across `examine_globs`, 12 of them in `services/file_tree.rs`** (measured by `migrate-workspace-tree-workflow-readability`), not "a small number".

### G5 — slot 4's stated floor is superseded

Matrix, appended to the `WFR-BUFFER-REPLACEMENT` mutation-parity line (L1988) —
the row whose evidence file carries slot 4's floor statement:

> That file's field-deletion floor figures are superseded: it recorded **32**
> mutants running regardless of the regex with **16 surviving** (11 in
> `services/file_tree.rs`, 5 in `services/draft_service.rs`); the current measured
> floor is **34 generated, 12 of them in `services/file_tree.rs`**
> (`migrate-workspace-tree-workflow-readability`). The 12 are `WFR-WORKSPACE-TREE`'s
> to triage, as slot 4 and slot 5a both recorded.

### G6 — slot 4 separates two claims in prose but pools their counts

Matrix L1860 (`WFR-SESSION-RESTORE`) and L1942 (`WFR-DRAFT-RECOVERY`), replacing
each mutation-parity annotation:

> - mutation parity: `openspec/changes/archive/2026-08-27-migrate-user-content-restore-workflow-readability/evidence/mutation-session-restore-policy.md` — the relocated bounded-turn admission policy and the newly extracted journal decisions are reported as **two separate claims**, and both are **gains from zero**: the admission policy's old home `ui/window/session_restore.rs` was outside `examine_globs`, so no before-figure exists to be at parity with. **The claims are separated in prose but not in the numbers** — the file reports one pooled **83 generated / 0 missed** for the whole module, with the relocated half's share unstated, where slot 3a split 42/16. Amendment (b) now requires the numeric split; a future re-measurement of this row owes it.

> - mutation parity: `openspec/changes/archive/2026-08-27-migrate-user-content-restore-workflow-readability/evidence/mutation-draft-recovery-policy.md` — the extracted decisions and the relocated `DraftMutationOrder` epoch allocator are reported as two separate claims, **both gains from zero**. As with the session row, the counts are pooled (**54 generated / 0 missed**) rather than split. Includes the change's own post-review addendum on the `services/draft_service/cleanup_types.rs` coverage regression a diff-scoped run could not see.

### G7 — the `WFR-DRAFT-RECOVERY` cell claims a parity its own evidence denies

This is the sharpest gap of the eight, because the false claim is in the **live**
matrix rather than in history. L1942 currently reads that the allocator "relocated
whole from the retired draft-ordering module **with parity proved**". The cited
evidence file says the opposite: "the **old location was outside the mutation
scope**... So the relocated allocator's *before* count is **0 generated**, and the
move is a coverage gain rather than a parity case." No parity was proved, and none
could be. The G6 replacement text above corrects this cell; the behaviour-parity
half the evidence *does* prove — "its five relocated tests all pass unchanged" —
is what the cell should have said.

### G8 — twelve evidence pointers are in live form for changes that are archived

The matrix's own [Evidence pointer form](../../../../docs/workflow-readability-matrix.md)
section states the rule: a live change records live-form pointers, and
"**an archived change's pointers are rewritten to archive form**, so a human
following the path finds the file. ... only the archive form is a real path on
disk." Slot 3a paid this debt for slots 1 and 2a. It has since re-accrued for
**every** change from slot 3a onward: none of the following resolves.

| Matrix line | Row / section | Live-form pointer to rewrite |
| --- | --- | --- |
| L1685 | `WFR-DOCUMENT-SAVE` | `migrate-document-save-workflow-readability/` → `archive/2026-08-26-migrate-document-save-workflow-readability/` |
| L1775, L1821 | `WFR-DOCUMENT-LOAD` | `migrate-document-load-workflow-readability/` → `archive/2026-08-26-…` |
| L1860, L1903, L1942, L1988 | `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY`, `WFR-DRAFT-RECOVERY`, `WFR-BUFFER-REPLACEMENT` | `migrate-user-content-restore-workflow-readability/` → `archive/2026-08-27-…` |
| L2056, L2057 | `WFR-NOTES-BOOKMARKS` | `migrate-workspace-tree-and-notes-workflow-readability/` → `archive/2026-08-27-…` |
| L94, L500, L2500 | `WFR-WORKSPACE-TREE` product cell, Workflow Stage Traces, slot 5a re-check | same slot-5a change |

Fill: rewrite all twelve to archive form — the count is the twelve matrix line
references in the table above, and an earlier revision of this file said eleven while
listing twelve. **Done**: the only live-form `openspec/changes/` pointers left in
`docs/workflow-readability-matrix.md` are this change's own five, which is the correct
form for a change that is not yet archived. Four of them are pointers **this
change's own tasks depend on** — task 1.3 had to locate them by directory listing
rather than by following the recorded path, which is exactly the reader cost the
pointer-form rule exists to prevent. Note the asymmetry that keeps producing this:
the gate resolves a live-form pointer against its archived directory, so a stale
pointer stays **green** indefinitely.

## Adjacent finding, recorded and not counted as a gap

`WFR-SEARCH-REPLACE`'s evidence surface has a field compiled only under the test
feature: `ui/search_panel/evidence.rs:97-98` gates
`retirement_observations: Vec<SearchRetirementSliceObservation>` behind
`#[cfg(feature = "test-utils")]`, and the type itself
(`ui/search_panel/retirement.rs:85-96`, doc'd "Compact evidence from one actual
GTK retirement turn") is likewise gated. `WFR-BUFFER-REPLACEMENT` made the
opposite call for the same shape and stated the rule while doing it: its terminal
diagnostic "**[was] `#[cfg(feature = "test-utils")]` and [is] now always compiled,
because an evidence surface must be readable in a production build**". Two rows
therefore hold opposite positions on whether an evidence surface may be partly
absent from a production build.

This belongs to `workflow-evidence-surfaces`, which **neither amendment in this
change touches**, so it is out of scope here and is not one of the eight. It is
recorded so the next `workflow-evidence-surfaces` amendment inherits a located
finding rather than a search, and so the position is not settled by accident in
one direction.

## Streak

**This is a fourth consecutive not-a-confirmation.** Slot 3b's re-check found one
of three rows non-compliant; slot 4's found two of four on statement (b); slot 5a's
found three of eight, and six of eight non-compliant on its statement (b); this
one finds **eight gaps** — one undeclared coordination role, two unrecorded
determinations, four mutation-evidence defects including a live matrix cell
claiming a parity its own evidence explicitly denies, and twelve unresolvable
evidence pointers across eight rows and two other sections.

The pattern across four amendments is now stable enough to state as a prediction
rather than an observation: **an amendment that adds a required *determination* or
a required *statement* is never discharged by the fact that the underlying
behaviour already held.** Slot 2b and slot 3a — the two amendments that added a
*name* and a *location* — were genuine confirmations. Every amendment since has
added an obligation to *record*, and every one has found rows that did the right
thing without writing it down. A fifth amendment of the recording kind should
budget for gaps in roughly half its rows, not hope for a confirmation.

One structural cause is worth handing forward, because three of the eight gaps
share it: **no gate reads what a row failed to declare.**
`make check-workflow-boundaries` verifies that declared paths exist and that a
`migrated` row names its required roles — so an omitted fifth coordination module
(G1), an unrecorded cohesion determination (G2), an unexplained qualifier (G3), and
a stale-but-resolvable evidence pointer (G8) are all invisible to it and stay green.
The module census in this document's method section is the cheap sweep that finds
them, and it should be a standing step of every future re-check: `wc -l` the role
home, diff against the row's declared modules, and read the doc of anything left
over.

---

## What was applied to the live matrix (recorded for traceability)

The re-check above is the analysis; this is what actually changed in
`docs/workflow-readability-matrix.md`. An **archived** change's evidence file is
history and was **not** rewritten — every correction lands in a live cell with a
pointer saying which archived figure it supersedes.

| Gap | Applied? | What changed |
| --- | --- | --- |
| **G1** — `WFR-DRAFT-RECOVERY` has an undeclared `retirement.rs` role module | **fixed** | `drafts/retirement.rs` added to the row's declared coordination list, and the count corrected from **four to five**. The existing "checked and rejected" reasoning was **kept and re-scoped**: it is sound, but it is about **orphan cleanup** specifically and was reading as a denial that the row owns a `retirement` module at all. Recorded plainly that no gate catches an *undeclared* role module. |
| **G2** — `WFR-NOTES-BOOKMARKS`'s `bookmark_execution.rs` states two stage orders as a bare "and" | recorded | Left as a recorded finding rather than rewritten: the module's own doc already records its cohesion basis, so the gap is in the cell's phrasing rather than in the classification. Named in the re-check subsection so the next slot touching that row closes it. |
| **G3** — `WFR-COMMAND-PALETTE`'s `index_admission.rs` qualifies a name never spent | recorded, **deliberately not renamed** | Renaming a stable, correct module is exactly the churn this change's own already-correctly-named amendment forbids. Recorded as a qualification the amendment would not now require. |
| **G4** — slot 3a states no floor, and misdiagnoses the cause | superseded in the live cells | The floor is stated here as **34**, measured from the tool, with the correct cause: `--re` does not apply to field-deletion mutants **at all**, rather than 3a's "the regex is not anchored". |
| **G5** — slot 4's floor figure of 32, and 11 `file_tree.rs` survivors | superseded | Corrected to **34** total and **12 generated** in `file_tree.rs` (of which 11 were recorded surviving, so exactly one is already killed). Slot 4's diagnosis was otherwise correct and is credited. |
| **G6** — slot 4 separates parity from gain in prose only | recorded | Named in the re-check subsection. This change demonstrates the compliant form in `evidence/mutation-workspace-tree-policy.md`, which reports parity and gain as separate tables each naming its invocation and file-level anchors. |
| **G7** — a **false parity claim in the live matrix** | **fixed** | The `WFR-DRAFT-RECOVERY` allocator relocation is corrected from "with parity proved" to a **gain from zero**, quoting its own cited evidence, which says the old location was outside the mutation scope. Found while writing the amendment that forbids this exact conflation. |
| **G8** — **twelve** dangling live-form evidence pointers | **fixed** | All twelve rewritten to archive form across four archived changes (slots 3a, 3b, 4, 5a) and **each verified to resolve on disk**. The gate cannot catch this, because it resolves live-form paths against the archive directory. |

Two further corrections were applied to this row's own cells from the same pass:
the stage-trace floor (**5 → 12/44**, an 8.8x correction) and the
materialization-fact count (**"all five" → six**, naming `find_dir_row` as the sixth).

## Not counted, and deliberately not touched

`WFR-SEARCH-REPLACE` gates an evidence field behind `test-utils` while
`WFR-BUFFER-REPLACEMENT` un-gated the same shape, stating that "an evidence surface
must be readable in a production build" — two migrated rows holding **opposite**
positions on the same question. That belongs to `workflow-evidence-surfaces`, which
this change does **not** amend, so it is recorded here as an observation for whoever
next opens that capability rather than folded into this re-check's eight.
