## Why

This is **slot 7b, the programme's closing change** (`docs/next/workflow-readability.md`).
Slot 7 split under the trigger its own proposal declared: §D1 resolved that
`WFR-SHELL-LAYOUT` is not one workflow, and implementing that outcome alongside the
preview facade exceeded one change's capacity. Slot 7a migrated five rows and
discharged one cross-cutting lane. **Three rows remain non-terminal**, two capability
deltas remain unlanded because they assert obligations only the closing change can
discharge, and the programme has no completion record.

Slot 7b's contract is what slot 7's was, narrowed to what is left: leave the
programme with **nothing outstanding**. Every row terminal with its probe evidence,
both withheld deltas landed and discharged, every inherited handoff item disposed,
every remaining deferral inventoried in one place with its gating condition and its
owner, and the programme record closed out against its own baseline.

**Baseline.** This change starts from a **committed** base: slot 7a landed as
`a3b1743a` and was archived as `4c39c58a`, so its five migrations, its delta 3 sync
into `openspec/specs/mutation-testing/spec.md`, and its matrix and record edits are all
in the tree rather than in an uncommitted worktree. Every figure below was measured
against that base. This matters mechanically as well as narratively: `mutants-diff` and
the diff-aware gates now have a real base ref to diff against, which slot 5b's warning
says they otherwise do not.

Nine findings, measured at authoring rather than inherited, shape the change.

### Finding 0 — the two withheld deltas were relocated into this change at authoring

Slot 7a's task 0.14a states the rule that produced this: *"Do not ship the delta in
one change asserting an obligation only the other change can discharge."* Its
`specs/` directory nonetheless still carried all three delta files while its tasks
recorded deltas 1 and 2 as withheld — an authoring record, not a shipping intent, but
one that a reader running `openspec show` would have read as 7a's contract.

Both files were **moved into this change** as the first authoring act:
`specs/workflow-readability-boundaries/spec.md` (183 lines) and
`specs/workflow-evidence-surfaces/spec.md` (169 lines). Slot 7a retains only
`specs/mutation-testing/spec.md`, the delta it actually discharged, and its proposal
carries a one-line note recording the relocation rather than a rewritten history.

**Both were re-based against the live specs and needed no edit**, which was measured
rather than assumed: each delta's `### Requirement:` header matches a live requirement
title exactly, and `diff` against the live requirement body shows each delta is a
strict superset with **zero** modified and **zero** removed lines —
`workflow-readability-boundaries` **+108 lines / +9 scenarios** and
`workflow-evidence-surfaces` **+46 / +3**, counting each requirement block *including*
the blank separator line that terminates it; **+107** and **+45** excluding it. The
predicate is stated because it is the only reason two honest counts differ by one, and
this change's whole method is to state the predicate rather than the number alone.

Slot 7a's delta 3 was **committed and synced** into
`openspec/specs/mutation-testing/spec.md` (`4c39c58a`), so the two withheld deltas were
re-based against post-sync live text, not against a worktree. Neither quotes anything
that sync touched.

### Finding 1 — the shell split needs seven replacement rows, and only four were candidates

§D1's verdict is recorded and is this change's primary authoring input:
`WFR-SHELL-LAYOUT` is **not one workflow** — criterion 1 fails on ≥12 distinct
ordered stage orders (past 19 with the contested files), criterion 2 fails on 15 of 18
`imp` state groups being touched by exactly one candidate's files, and criterion 3 was
never reached. Outcome **(c), the hybrid** was selected. **The line count supports
nothing and is not offered as support**, here or anywhere in this change.

The design's candidate table is the **maximum** under consideration. It has **six
rows** — five naming a candidate story, plus one naming the no-coordination tier; the
word "five" appears nowhere in it and the count is this change's reading of its
structure, not a quotation. One of the five — shell dialogs — §D1 resolved as **not a
row at all**. So **four** candidate rows survive, and this change writes **six new
facades** against them. That arithmetic is stated plainly because it is the departure,
and burying it would be the same move as arguing from line count.

§D1's contested-file verdicts create **three further attributions the table did not
anticipate**, each forced by the coverage proof rather than chosen. Its record uses
unnumbered bullets, so each is cited by the surface it names rather than by a
finding number that does not exist:

| Replacement row | Source | Why it is a row |
| --- | --- | --- |
| `WFR-SHELL-GEOMETRY` | candidate table | §D1: satisfies criterion 1 **cleanly** — seven entry points converging on one ordered sequence, and the smallest external entry surface of any candidate |
| `WFR-TAB-STRIP` | candidate table | one stage order plus three synchronous projections; owns tab close, where slot 7's confirmed data-safety defect lived |
| `WFR-RECENT-DOCUMENTS` | candidate table | two stage orders plus one lazy projection gate; slot 3b already split it from the load row along the coordination/presentation line |
| `WFR-FOCUS-MODE` | candidate table | one stage order: reversible chrome suppression with fullscreen ownership and preview compatibility |
| `WFR-TRANSIENT-DISMISSAL` | §D1's `transient_surfaces.rs` verdict | it *"does not belong on the no-coordination-tier list"* — a strictly ordered dismissal ladder **and** a one-tick idle latch, so the list's "no ordered stages" preamble is false for it. Note it was never on the **live** tier list; the design proposed it as a tier candidate and §D1 rejected the proposal, so nothing is being removed |
| `WFR-EDITOR-MEMORY-EVICTION` | §D1's `focus_indexing.rs` verdict | eviction orchestration with its own generation counter, a bounded idle continuation, 8 test seams, and two race-injector hooks, **owned by no story anywhere**. Size restated in Finding 1b: **~406 production lines**, not the ~590 the handoff carries |
| `WFR-STARTUP-PREFLIGHT` | coverage proof | `startup_data.rs` is cross-cutting and owned by none (slot 5a, re-confirmed by §D1), ordering five workflows; it needs a terminal row, not a facade |

Exceeding a stated maximum needs its own justification, and this is it: the maximum
bounded **the candidates the table listed**, and three of these seven are attributions
§D1's verdicts created after the table was written. Each is argued on criterion 1 or on
cross-cutting grounds in `design.md` §E1, per surface, with its evidence. **If the
stage trace supports more than seven, that is a signal to re-read the trace rather
than to add rows** — the design's own constraint, carried forward unchanged. The
reciprocal test applies too and is easy to skip: a 202-line surface that gains a
facade, a `policy.rs`, an `evidence.rs`, and a `test_policy.rs` may end up **larger
after migration than before**, which is the thin-row failure the design warns about
from the other direction.

### Finding 1b — `focus_indexing.rs` holds four stories, not three, and the inherited decomposition is wrong in three ways

§D1's verdict that the file is *"three stories"* is the handoff this change inherits,
and re-derivation against the code at authoring falsifies all three of its parts. The
file is 856 physical lines and its own module doc names *"focus restoration,
editor-memory orchestration, and palette indexing"* — three subjects, but not three
stage orders:

| Story | Functions | Measured span | What the handoff got wrong |
| --- | --- | --- | --- |
| **Editor-memory eviction** | `track_editor_memory` (`:56`), `untrack_editor_memory` (`:79`), `mark_editor_memory_accessed` (`:108`), `update_editor_memory_record` (`:128`), `schedule_editor_memory_evaluation` (`:163`), `reload_if_evicted` (`:380`), `maybe_evict_background_tabs` (`:393`), `evaluate_editor_memory_budget` (`:407`) | **~406 production lines** across two discontiguous ranges (`:56`–`:190` and `:380`–`:667`) | the inherited **~590** absorbs the **~129 lines of focus restoration** that sit between them at `:255`–`:380`. Two stories were counted as one because they are adjacent |
| **Palette overlay control** | `toggle_command_palette` (`:190`), `close_command_palette` (`:209`), `set_command_palette_actions_enabled` (`:218`), `set_command_palette_query` (`:229`), `set_command_palette_mode` (`:237`), `refresh_command_palette_sources` (`:807`), `open_file_palette_entries` (`:817`), `command_palette_workspace_group_label` (`:843`) | **~104** | "the palette story" is **two** stage orders, not one |
| **Palette file-index build** | `start_file_index_build` (`:667`), `retry_file_index_admission` (`:746`), `finish_cancelled_file_index_admission` (`:753`), `finish_file_index_build` (`:770`), `retire_file_index_outcome` (`:852`) | **~171** | it has **its own coordinator and generation counter** — a separate ordered stage order from overlay control, and the one that actually crosses to background work |
| **Focus restoration** | `focus_selected_editor_after_action` (`:255`), `restore_focus_after_secondary_pane_close` (`:304`), `restore_focus_after_breakpoint_collapse` (`:325`), `restore_saved_focus` (`:362`) | **~129** | the handoff calls this *"the geometry story"*. **It contains no geometry code at all.** Two of its four functions are merely geometry-*triggered* — a pane close and a breakpoint collapse call them — which is a caller relationship, not ownership. Assigning it to `WFR-SHELL-GEOMETRY` on that basis would leave a story unowned while every gate exits 0 |

**Consequences this change must carry**, not treat as detail:

- The eviction facade is re-projected from **~406**, not ~590.
- The palette reassignment (0.5c) lands **two** stage orders in
  `WFR-COMMAND-PALETTE`, and that row's cells stale accordingly — the file-index
  build's coordinator is coordination, not presentation.
- **Focus restoration needs an explicit owner** and currently has none. Candidates:
  its own replacement row, a coordination role of `WFR-SHELL-GEOMETRY` argued on
  behavior rather than on the caller relationship, or a called presentation surface of
  whichever row owns editor focus. Task 0.4a decides; leaving it implicit is the exact
  failure §D1 was run to prevent.
- **A handed-on decomposition is a hypothesis in the same way a handed-on number is.**
  Seven slots of evidence say re-derive the cell; this is the first instance where the
  thing to re-derive was the *story boundary*, and it was wrong in three independent
  ways.

### Finding 1c — the retired `WFR-SHELL-LAYOUT` row has no lawful disposition today

Retiring the row is what §D1's outcome requires, and neither available mechanism is
legal:

- **Delete it.** `record_findings` in `check-workflow-boundaries.py` fails when a
  ledger slot names a row id with no matrix row, and
  `docs/next/workflow-readability.md:1231` reads
  `- slot 7b (outstanding): WFR-PLAIN-DISPOSAL, WFR-SHELL-LAYOUT, WFR-AUTOMATION-SPINE`.
  Deleting the row fires the gate.
- **Keep it with a terminal status.** `KNOWN_STATUS_LABELS` offers `pending`,
  `migrated`, `partially-conforming`, `exempt`, `deferred`, `cross-cutting`. Delta 1
  forbids the three transitional ones after the closing slot; the row is not
  `migrated` (it was replaced, not converted), not `exempt` (it *was* forced into the
  convention, by decomposition), and not `cross-cutting` (it was not shared
  coordination — it was a grouping error).

So the row needs a vocabulary that does not exist. Two candidate resolutions, decided
in task 0.4b rather than discovered mid-implementation: add a terminal
**retirement/superseded** label to the vocabulary — which is a **delta 1 text edit**,
paid inside this change with its retroactive re-check — or retain the row with a
documented terminal status plus a `superseded by` pointer to the seven rows, and argue
which existing label that is. Either way the ledger line must be edited in the same
change, because a ledger naming a deleted row and a matrix hiding a retired one are the
same defect from two sides.

Four surfaces are **reassigned to rows this change does not migrate**, which is the
other half of the split and the half the coverage proof depends on:

- **`dialogs.rs` (861/862)** — §D1: *"not this row's, and not a called presentation
  surface."* Its confirmed-close coordination is consumed by the **already-migrated**
  `WFR-DOCUMENT-SAVE` (`close_save_session_is_current`) and `WFR-DRAFT-RECOVERY`
  (`clear_close_discard_drafts`). Deciding which migrated row owns its five stage
  orders **stales those rows' measured cells**, which delta 1's own cross-row staling
  statement requires this change to re-derive rather than leave.
- **`ui/window/search.rs` (955/928)** — `WFR-SEARCH-REPLACE`'s window side, and *"more
  than a called presentation surface: it holds two of that workflow's ordered
  coordination stages plus one coordination job of its own."* That row's cell reads
  "all under `ui/search_panel/**`", which is now false by 928 production lines.
- **`focus_indexing.rs`'s two palette stage orders** (Finding 1b) —
  `WFR-COMMAND-PALETTE`'s. That row's cell already says the file *"stays window code"*,
  which the reassignment contradicts and this change must reconcile. **Plus its focus
  restoration story, which has no owner at all.**
- **`mod.rs`'s `setup_theme_selector` (~100 lines)** — §D1 found *"a tenth story
  nobody had enumerated"*, in neither the row's story list nor the tier list. It needs
  a home, and which home is a decision, not an assumption.

### Finding 2 — `WFR-PLAIN-DISPOSAL` has three parallel observation values, not one, and the obvious narrowing is wrong

Measured at authoring, with the predicate stated on every figure because this row is
the census's only user of the dual gate:

- `ui/plain_disposal.rs` — **1,542 physical / 1,344 production**; `model/plain_disposal.rs`
  — **692 physical / 465 production**. **8 `*_for_test` declarations**; **17**
  `cfg(feature = "test-utils")` sites plus **13** `cfg(any(test, feature = "test-utils"))`
  sites in the `ui` half, **0** of either in the `model` half.
- **Three parallel typed observation values**, reached through **five** accessors.
  Stating the predicate, because a count of "observation values" is exactly the kind of
  figure two readers get differently: an *observation value* is a type returned by a
  gated accessor for the purpose of being read, and the two lanes are two *instances*
  of one type, not two types.

  | Type | Accessors | Sites |
  | --- | --- | --- |
  | `PlainDisposalLimits` | `limits_for_test`, `progress_limits_for_test` | `:1013`, `:1020` |
  | `PlainDisposalSnapshot` | `lane_snapshot_for_test`, `progress_lane_snapshot_for_test` | `:1027`, `:1034` |
  | `DisposalPressureEvidence` | `aggregate_pressure_evidence_for_test` | `:1145` |

  The row's remaining **three** gated declarations — `hold_disposal_capacity_for_test`,
  `fill_disposal_capacity_for_test`, `hold_progress_disposal_capacity_for_test`,
  returning `DisposalCapacityHold` / `ProgressDisposalCapacityHold` — are **actuation**
  holds, not observation, and §E4 excludes them from the surface. That accounts for all
  8 declarations: 5 observation + 3 actuation.

  Slot 7a's Finding 4 named three parallel types for the buffer-snapshot lane; this
  lane also has three, and the census cell named one.

**The narrowing task inherited from slot 7a is stated in a form that would be wrong to
execute literally.** Slot 7a's task 6.8 says *"narrow `DisposalPressureEvidence` from
`pub` to its readers' visibility"*, and its B.6 records the narrowing as outstanding.
Measured at authoring: the type is already `#[cfg(feature = "test-utils")]`-gated, and
its **only** reader is `crates/lushtext/tests/widget/plain_disposal.rs:16` — a
**different crate**. `pub` under a test-utils gate *is* the narrowest visibility a
cross-crate reader permits. Executing the inherited instruction would break the widget
lane and call it a narrowing.

The real obligation is delta 2's, and it is the consolidation: **one** typed surface
replacing three parallel values and five accessors, at whatever visibility its measured
reader set actually requires, with the visibility conclusion **recorded with its
reader measurement** either way. Task 3.2 states the reader set before choosing.

### Finding 3 — the row's status label is already terminal and its obligations are not

`WFR-PLAIN-DISPOSAL` carries `cross-cutting`, which the Status Labels section defines
as terminal, while the slot table gives it slot 7b and the tier-3 surface narrowing.
Slot 7a hit the same collision from the gate side and recorded it as friction:
*"`cross-cutting` means both 'resolved, nothing to do' and 'resolved, and its surface
obligations are discharged'. This slot had to widen a gate to express the difference
at all; the vocabulary that would fix it properly is capability delta 1's — 7b's."*

`WFR-BUFFER-SNAPSHOT` already carries the resolved form —
`cross-cutting — surface obligations **discharged**` — so the target text exists and
the work is to earn it, not to invent it.

### Finding 4 — `WFR-AUTOMATION-SPINE`'s three sources disagree three ways, not one

Slot 7a's Finding 5 named one disagreement. Measured at authoring there are three:

1. The matrix `Slot` cell reads `2a onward, incrementally per migrated workflow;
   terminal status decided in 7b`; the ledger's explanatory text quotes it as
   `"2 onward, ..."` — a different slot label and a dropped clause.
2. **Slot 7a's `complete` ledger line is the only one from 2a onward that omits
   `WFR-AUTOMATION-SPINE (partial)`**, while 7a migrated five rows. Its baseline row
   says *"7, unchanged"*, which is a defensible reason for the omission and is
   nowhere stated as one.
3. The Migration Order table's slot-7b row lists the spine; the row's own `Slot` cell
   defers only its terminal status to 7b.

Slot 7a's ledger line also lists `WFR-BUFFER-SNAPSHOT` on a `complete` line without
`(partial)`, which works only because the matrix marks that row discharged — the exact
gate refinement 7a had to make. All of it must **agree** after this change; today
neither source is marked authoritative over the other.

Two related cells are stale and one is confirmed correct:

- The row's **evidence cell was corrected by slot 7a** (four → seven projections).
  Task 6.3 re-derives the count rather than inheriting the correction, because a
  correction is a measurement and this change re-derives measurements.
- `MinimapEvidence` is still **not** registered in `EVIDENCE_PROJECTIONS`. Slot 6
  called that *"a result rather than an omission"*; slot 7a did not verify it. The
  Completion Rule's last bullet says *"any automation snapshot field for this workflow
  projects from the evidence surface"*, so the verdict is verified here, not inherited
  twice.

### Finding 5 — the automation ratchet row is stale about its *occurrence* but right about its *expression*, and it cannot be struck

The matrix's `Production cross-widget reach-throughs still open, by owning row` table
records **two** open entries at `ui/automation.rs:517`/`:518` reading
`window.imp().tab_view`, both owned by `WFR-SHELL-LAYOUT`. Measured at authoring, the
answer is **not** the clean retirement the change was drafted around:

- **The recorded occurrence is gone.** `current_readiness_failure` now iterates
  `window.open_editors()`, and slot 7a's B.5 records that retirement and its corrected
  attribution — the load story reading the tab collection, not tab-strip state.
- **The reading expression persists at eight sites**: `:585`, `:586`, `:776`, `:777`,
  `:836`, `:837`, `:843`, and the chained read spanning `:2003`–`:2005`. Seven match
  `imp.tab_view` directly after a local `let imp = window.imp()`; the eighth reads
  through a multi-line `.imp()` chain, which a single-line grep misses. The table's own
  instruction is *"match on the reading expression rather than the line"*, and under
  that predicate the row is **not** retired.

So the row **must not be struck `~~RETIRED~~`**. The correct disposition is to record
it as **occurrence-retired, expression persisting**, with a **new owning row** —
`WFR-SHELL-LAYOUT` is being retired, and these eight sites read the tab collection, so
the owner becomes whichever row task 0.5 gives tab enumeration. Task 6.4 then surfaces
the remaining sites as ratchet entries rather than letting a fixed line number stand in
for a fixed pattern.

This is the mirror of Finding 6's lesson and worth naming as its own class: **the
predicate a ratchet row is checked under decides whether it is closed.** A row recorded
by line number reads as retired the moment the code moves; a row recorded by expression
stays honest. `ui/automation.rs` holds **15** `.imp()` reads in total; how many cross a
workflow boundary is a third, separate measurement, and task 6.4 must not conflate it
with either of the first two.

### Finding 6 — six pieces of engineering debt from slot 7a's review pass reached no artifact

Slot 7a's review and simplify passes produced findings that appear in **no** OpenSpec
artifact, no `docs/next/` record, and no rules file. A grep across `.md` for their
anchors returns zero hits. This is Finding 6 of slot 7a's own proposal recurring one
change later, in the change whose job is to leave nothing outstanding — and it is
worse than a handoff to an archived directory, because these had no directory at all.

All six were **re-verified against the code at authoring**, and two of the inherited
figures are wrong:

| Item | Verified state at authoring | Disposition |
| --- | --- | --- |
| **The rustfmt gate hole** | confirmed and **larger than reported**. `crates/lushtext/tests/widget.rs` reaches its 18 test modules through `include!(concat!(env!("OUT_DIR"), "/widget_test_registry.rs"))`, so `cargo fmt` cannot discover them and `cargo fmt --all --check` **passes while formatting nothing under `tests/widget/`**. The inherited figure is 171 hunks; re-derived per file with `rustfmt --edition 2024 --emit stdout \| diff`, it is **411 hunks across 18 files** (largest: `workspace_section.rs` 129, `window.rs` 75, `markdown_preview.rs` 70) | task 8.1: decide fix-or-record on the evidence, with the reformat isolated from every semantic change so review can separate them |
| **A proof whose comment describes a step it never runs** | confirmed, `tests/widget/window.rs`. The print-disposal proof asserts `print_evidence(&window).document.is_some()` at `:14566`, then again at `:14575`–`:14578` under a comment claiming *"Verified: the surface still answered `Some` after `close()`"* — **the test never calls `close()`**. The comment reports a verification that did not happen and the second assert is a duplicate of the first | task 8.2: fix. A proof that misdescribes its own premise is the honesty class this programme keeps naming, not a comment nit |
| **Dead tuple ladders in `tests/widget/markdown_preview.rs`** | confirmed: **8** destructuring sites bind **20** `_` placeholders, **6** of them over seven-element tuple literals and **2** over five-element ones, left behind by slot 7a's retirement of 11 tuple-returning seams into `MarkdownPreviewEvidence` (`:1208`, `:1303`, `:1322`, `:1385`, `:1415`, `:2223`, `:2413`, `:2454`) | task 8.3: re-derive the line count under a stated predicate — the inherited "~70 lines" is untraceable to any measurement — then read the fields from the surface directly |
| **`encoding/dialogs.rs` "near-duplicate dialog builders"** | **the named target does not exist.** `append_action_row` (`:342`) is a **one-expression default-argument wrapper** whose entire body is `append_action_row_with_sensitivity(group, title, subtitle, true, window_weak, dialog, action);` — the idiomatic way to express a default in a language without them, not duplication. And the file has **five** row builders (`dialog_row`, `static_dialog_row`, `append_action_row`, `append_action_row_with_sensitivity`, `append_choice_row`), not four | task 8.4: **re-scope or record the negative.** Look for real duplication in the six `present_*` shapes over `build_dialog`, and if none is found, **"no near-duplicate exists" is the recorded outcome.** Do not refactor a correct wrapper to satisfy an inherited claim |
| **`git_lines` is dead** | confirmed. `scripts/accessibility_source_fingerprint.py:142`–`:143` is a one-line wrapper over `git_lines_checked` with **zero** callers | task 8.6: remove |
| **Three residual ledger-check holes** | the inherited label "S12" appears nowhere in the repository, but **all three holes reproduce**, and two of them sit *around* the four documented conditions rather than inside them — they are **fail-opens in the parsing path**, which is why counting the documented conditions would have missed them (see Finding 6b) | task 8.5, **re-framed** as a fail-open audit of the ledger-and-slot-column parsing path, with the three reproduced holes as its starting findings rather than its conclusion |

Slot 7a's appendix also contradicts itself about a seventh item: **A.6 finding 1**
records `check-accessibility-policy`'s summary-absence fail-open as **fixed** and
proved by deliberate red, while **A.13a** records it as *"Not fixed here"*. Both
cannot be true. Task 8.7 resolves it against the script.

### Finding 6b — the three ledger-check holes are fail-opens in the *parsing* path, not gaps in the checks

The inherited item names three holes and no evidence. All three reproduce, and their
shape changes what task 8.5 has to be: `check-workflow-boundaries.py` documents **four**
ledger failure conditions and implements them correctly. The holes are in the code that
decides **what the conditions see**, so an audit that enumerated the four conditions
would have found nothing and reported the gate sound.

1. **A renamed `Slot` header silently disables the slot-dependent half of the matrix
   check.** `parse_matrix_rows` locates the column by name — deliberately, *"so the
   `Slot` column is located by name rather than by a positional guess that a later
   column insertion would silently shift"* — by setting `slot_index` only when a header
   row contains both `row id` and `slot`. Rename or reword that header and `slot_index`
   stays `None`, so **every** row's `slot` is `None`, every slot-dependent finding is
   skipped, and the gate **exits 0**. The defence against a positional shift became a
   fail-open against a rename.
2. **A typo'd ledger verb silently drops that slot's claim.** `SLOT_LEDGER_RE` requires
   the literal `complete` or `outstanding`. A line that fails the match is `continue`d,
   and the only guard is the *all-lines-failed* case (`if not claims`). One malformed
   line among eleven is invisible — and a dropped `outstanding` line is precisely how a
   row with remaining work reads as settled.
3. **A `Slot` cell of `none` exempts a `pending` row from the outstanding-slot rule.**
   `none` is a real value meaning "never migrated", which is correct for
   `WFR-EDITOR-MEMORY`; it is not correct for a row that is `pending`, and nothing
   distinguishes the two.

All three are the same class this programme keeps naming — **a step that quietly
succeeds against the wrong input** — and it is the class delta 3 was written for, slot
6 fixed twice, and slot 7a fixed twice more. Finding it in the *reconciliation gate the
closeout depends on* is why task 8.5 is not a paperwork item: the closeout's central
claim is that the matrix and the ledger agree, and two of these holes let them disagree
while the gate passes.

### Finding 7 — the matrix's facade table is stale again, in the change that must replace it

Slot 7a replaced a four-row table headed "after slot 3b" with an eleven-row table
headed *"all eleven, re-measured in slot 7"*. **Sixteen rows are `migrated` today**:
the table omits all five of slot 7a's own facades. Its surrounding prose is
anachronistic in the same way — *"only two workflows are migrated today"* and
*"slot 3 must plan against 1 line"* both still stand.

Slot 7a's A.11 recorded that the replacement *"travels with the change that writes new
facades"*. This change writes six. The table is replaced with every migrated facade
measured in this change, and the prose is re-based, because **a budget claim checked
against a stale number is not a check** and this is the third consecutive slot to
inherit that lesson.

The programme record has the mirror defect: its status line still says **"eleven"**
workflows are migrated where its own slot-7a baseline row says **16**, still lists
rows only through slot 6, and still reads *"Slots 5 through 7 remain authorable"*. Its
change-name table still reads `| 5–7 | not yet authored |`, and its §7 parenthetical
still calls two migrated rows deferred.

### Finding 8 — thirteen mutation survivors became tracked debt in slot 7a, and 160 mutants were never triaged

Slot 7a's B.3 recorded two ratchet rows in `docs/mutation-testing.md` for findings
outside its own boundary: **8 survivors** deleting bounded-scan telemetry fields from
the published `DirectoryScan` (`WFR-WORKSPACE-TREE`'s), and **5 survivors** deleting
orphan-cleanup continuation fields from the published plan and outcome (the draft
row's). Both are the same operator class and the same shape — *"bounded-work counters
that no test asserts, in two rows whose whole point is bounded work"* — and neither
file had a ratchet row before, so the survivors *"were not tracked debt; they were
invisible"*.

**Slot 7a contradicts itself about whether those 160 mutants were triaged, and the
inherited half is the stale one.** Its B.6 "explicitly not claimed" list says the
**160 newly-in-scope mutants are untriaged** — *"`make mutants-diff` was not run, and
the figures reported are generation counts, not kill counts"* — while its **A.12**
records an actual run: **130 caught / 13 unviable / 17 missed**, with the 17 since
triaged to a residue of roughly 0–2. Both cannot be true, and B.6 is the section that
was written first and not revised when A.12's run landed.

This matters more than the arithmetic. A closing change that budgets for triaging 160
untriaged mutants is budgeting for work largely already done, and would then either
report a fictional accomplishment or quietly discover the discrepancy and have no
record of it. Task 9.4a resolves the contradiction against the tool **before** task 9.5
scopes anything, and 9.5 is scoped to the **actual surviving set** — which may be two
mutants, not 160.

Neither of the two ratchet rows is inherited as a number either: task 9.6 re-derives
their current survivor counts from the tool.

### Inheritances this change is the named recipient of

Verified against the code at authoring rather than copied from the handoff:

| Inherited | From | Status at authoring |
| --- | --- | --- |
| §D1's resolution, the ≥12 stage orders, the 15-of-18 state groups, and the candidate table **as a maximum** | 7a A.4, mirrored into `docs/next/workflow-readability.md` | **recorded in the programme record deliberately**, *"because a change directory is archived and this decision outlives it"*. Consumed, not re-derived |
| The four contested-file verdicts | 7a B.0 item 2 | all four confirmed present at authoring; `dialogs.rs` 861, `focus_indexing.rs` 856, `window/search.rs` 955, `transient_surfaces.rs` 202 |
| §D6's constraint, **re-proved intact** by 7a | 7a B.0 item 3 | **confirmed by count**: `actions.rs` and `imp.rs` are literal keys in three predicates in each of two implementations (`check-visual-proof-policy.py:163`/`:164`, `:190`/`:191`, `:209`/`:210`; `policy.rs:823`/`:824`, `:852`/`:853`, `:879`/`:880`), plus **six** self-test keys (`:594`, `:786`, `:808`; `:69`, `:228`, `:254`). 7a's proposal says "five" in one place and six in another; **six** is correct |
| `ui/window/policy.rs` landed, gain from zero | 7a, delta 3's rename | present, **813 physical / ~287 production**. The gain figure disagrees between sources — the matrix says **80 mutants** with 15 survivors triaged to zero. Task 2.4 re-derives it from the tool; every copied-forward figure is suspect, *"including this change's own"* |
| The census coverage proof is stale | 7a A.2f | stale, and **by how much is not inherited**: the matrix's proof still states 198/195, slot 7a recorded 266, and a count at authoring gives 286. Three figures for one denominator is itself the finding — task 5.10 re-derives **both** numerator and denominator under a stated predicate rather than adopting any of them |
| Remaining argument suppressions | matrix's slot-7b ledger row | **one exists**, at `model/action_catalog.rs:178` — the exempt domain catalog constructor, whose parameters each name a documented external contract field, which `.agents/rules/rust.md` places outside the seam rule. Slot 7a confirmed zero `#[allow]` of the lint anywhere. The ledger item is therefore discharged by **recording** that the single instance is the sanctioned exception, not by removing it — task 11.12 |
| Facade budget 370, tightest repo margin **1** | 2a, re-confirmed 3a/3b/5b/6/7a | `ui/search_panel/mod.rs` still exactly **369**. Plan against 1 line. 7a's newest facades landed 105/153/155/238/270, none near the ceiling — do not read that as headroom |
| One unspent actuation-seam budget | 5b, unspent by 6 and 7a | this change plans to spend **zero** and says so in task 4.6 |
| `mutants-diff` proves nothing on an uncommitted worktree | 5b | task 9.5 generates the diff and passes it explicitly |
| Run the rustdoc gate by hand; `make check` does not | 3a shipped the failure, 3b fixed it, 5b/6/7a re-warned | task 9.3, before shipping any facade. This change ships **six** new facades in new `pub` role homes, the exact shape that trips it |
| The `[~]` reconciliation: **23** literal markers, **16** of them slot 5a's and closed by 5b, **seven** genuinely open | 7a B.3 | consumed as stated; **plus 7a's own two** (10.22, 10.23) makes **nine**. State 23/16/9 together or a reader who greps concludes sixteen items were abandoned |
| Slot 6's `minimap_work_pending` candidate, **conditionally cleared** by 7a | 7a task 8.7 | the clearance is conditional on no `mark-set` handler reading readiness; both handlers in the tree reach only scrolling and menu refresh. Task 6.5 re-checks the condition and records it as a standing condition rather than a closed item |
| Task 7.6's unbounded startup activation-open queue, **un-homed** | 5a via 7a B.4 | belongs to `startup_data.rs`, which §D1 confirmed cross-cutting and owned by none. Task 5.7: bounded-work assessment plus a budget, or a `docs/next/` home with its gating condition |
| `scan_execution.rs` ~2,000 production lines | 5b | not absorbed; named in the closeout inventory as a recorded size follow-up |
| **Twelve dangling evidence pointers across four archived changes**, and *"the gate cannot catch this"* | matrix finding G8 | this is the population the inherited "seven stale archive-form pointers" refers to. Task 7.7 re-derives the current dangling set rather than inheriting either figure |
| Evidence pointers stay live-form until archive time | 5b, *"the step four prior changes missed"* | task 10.7 states it as an archiving step, not a body edit |

**Explicitly not inherited**, confirmed by path in task 0.9 rather than assumed: the
nine open `[~]` items across slots 4, 5a, 5b, 6, and 7a. They are **user-gated**, they
stay user-gated, and this change's contribution is to inventory them in one place —
the programme's single deferral inventory — rather than leave the only outstanding work
distributed across five archived directories.

### Facade budget: per-row projections and the escalation path, declared before writing

**No amendment is proposed and the budget line is not to be edited by default.** All
sixteen migrated facades are re-measured in task 7.3 — verb *re-measure*, not
*confirm*, because slot 5a found three of eight stale and slot 7a found the load
facade 18 lines off.

Projections for this change's six new facades, stated as predictions task 0.6 measures
and may falsify:

| Row | Current entry file | Projection | Worst case | Basis |
| --- | --- | --- | --- | --- |
| `WFR-TRANSIENT-DISMISSAL` | `transient_surfaces.rs` (202/203) | **≈120** | 170 | one ordered ladder, one idle latch, one inversion |
| `WFR-FOCUS-MODE` | `focus_mode.rs` (354/355) | **≈150** | 210 | one stage order; fullscreen ownership and preview compatibility are its only cross-surface obligations |
| `WFR-EDITOR-MEMORY-EVICTION` | new, from `focus_indexing.rs` (**~406** of 856 — Finding 1b, not the inherited ~590) | **≈160** | 230 | one stage order with its own generation counter and a bounded idle continuation, over 31% fewer source lines than the handoff implied |
| `WFR-RECENT-DOCUMENTS` | `ui/open_popover/mod.rs` (424/425) | **≈250** | **>370** | **the escalation candidate.** Two stage orders plus a lazy projection gate, and the matrix's measured stressor is stage-order count, not inversions or entry points. It also retires 26 declarations / 37 sites, more than every other shell surface combined |
| `WFR-SHELL-GEOMETRY` | new `ui/window/geometry/mod.rs` | **≈190** | 260 | one ordered sequence, seven entry points, **the smallest external entry surface of any candidate** (§D1) |
| `WFR-TAB-STRIP` | `tabs.rs` (634/588) + `documents.rs` close half | **≈220** | 300 | one stage order plus three synchronous projections |

`WFR-PLAIN-DISPOSAL` and `WFR-STARTUP-PREFLIGHT` are projected as **no facade**: a
lane and a cross-cutting orderer own no user-initiated operation (§D2, §E1).

**Focus restoration is deliberately unprojected**, because task 0.4a has not yet given
it an owner (Finding 1b). If it becomes its own row, its facade is projected there
before any code moves; if it becomes a coordination role or a called presentation
surface of an existing row, it needs no facade. Projecting a number now would prejudge
the ownership decision, which is the mistake §D1 exists to prevent — the same reason
slot 7 declined to project a single facade for the whole shell row.

**Escalation path, declared now so it is not invented under pressure**, unchanged from
slot 6 and 7a because step one has been sufficient six times:

1. Exceeding 370 is answered first by **delegating harder** — extracting called
   presentation surfaces and pushing stage bodies into coordination roles.
2. If delegating harder cannot reach 370 without moving *narration* into a
   coordination module, the change **amends the budget number** and pays the
   retroactive re-check across every migrated row in the same change. At sixteen
   migrated rows that cost is at its programme maximum, which argues for step 1 and
   does not prohibit step 2.
3. **Not available**: splitting a row to make two smaller facades, or hiding stage
   narration behind a helper module whose only job is to shorten `mod.rs`. The shell
   split is licensed by §D1's stage-order evidence and by nothing else — if any
   replacement row's only support is its line count, it is this forbidden response
   wearing the grouping clause as cover.

### Data safety: tier-3 twice over, and the pass is a first-class work item

Slot 5a's lesson is binding and has now held for seven consecutive slots, every one of
which found at least one confirmed defect. This change carries **two tier-3 rows** —
the disposal lane and the tab-strip replacement row — plus one that inherits a
tier-3 contract it must not perturb.

Six places to look, with the reason each is on the list:

- **`WFR-PLAIN-DISPOSAL` is the retirement lane for every workflow's document-sized
  payloads**, called by 21 files across 10 workflows. A dropped terminal strands
  whoever waited on it — slot 3b fixed exactly that shape in the load row — and a
  mis-accounted permit lets the next admission overshoot the budget.
- **`WFR-TAB-STRIP` owns tab close and delete-driven close**, where slot 7's confirmed
  teardown-before-close defect lived. Two independent passes reached that defect and
  **neither reached it from the tab-pin or bulk-close paths in `tabs.rs`**, so those
  are the unexamined neighbours, and they are inside this change's files.
- **`dialogs.rs`'s close-coordination contract** must hold *exactly* after
  reassignment: input rejected across the selected-save pipeline and later
  draft/session yields, discarded editor identity/content-generation/modified/path
  state fingerprinted at confirmation, active saves and freshness rechecked before
  destruction, retryable drafts restored on every aborted close.
- **`startup_data.rs` gates the format-upgrade preflight**, and slot 5b handed on a
  confirmed fail-open finding (M-5) in the format gate. Slot 7a's task 0.10 left the
  question of whether M-5's site is inside this row's files unresolved.
- **`WFR-EDITOR-MEMORY-EVICTION` evicts live editor state** under its own generation
  counter with two race-injector hooks. Eviction that races a save or a draft write is
  a data-safety defect regardless of how tidy the row becomes.
- **`WFR-SHELL-GEOMETRY` persists to GSettings**, and `.agents/rules/ui.md` forbids
  persisting from allocation and notify paths. A geometry extraction that moves a
  clamp across that boundary is a live-warning regression the widget lane cannot see.

**Disposition rule, stated before the pass runs.** A confirmed finding whose fix is
inside this change's files is fixed here with a regression test **proved to fail
without the fix** by deliberate revert-and-rerun. A confirmed finding in an
already-migrated or out-of-scope row is fixed here if its fix is independent of that
row's structure, and otherwise landed in a `docs/next/*.md` record with severity, site,
owning row, and close condition — **and named in the closeout inventory**, because
this is the last change and "handed on" has no recipient. Nothing is recorded as
accepted debt; `.agents/rules/preexisting-blockers.md` has no exceptions.

### One change, and the split path if it is needed

**This is authored as one change**, matching the ledger's existing
`- slot 7b (outstanding):` line, with sections ordered by increasing risk and proof
cost:

`WFR-TRANSIENT-DISMISSAL` → `WFR-FOCUS-MODE` → `WFR-PLAIN-DISPOSAL` →
`WFR-EDITOR-MEMORY-EVICTION` → `WFR-RECENT-DOCUMENTS` → `WFR-SHELL-GEOMETRY` →
`WFR-TAB-STRIP` → `WFR-STARTUP-PREFLIGHT` → `WFR-AUTOMATION-SPINE` + closeout.

The two smallest replacement rows first because they prove the shape of a
single-stage-order shell facade; the lane surface next because it has no structural
move at all; the two-stage-order row and the two pixel/data-safety-heaviest rows after
that; the cross-cutting resolutions and the closeout strictly last, because the
closeout asserts that nothing is outstanding and can only be written truthfully once
everything else has landed.

**Split path, named in advance.** Six new facades exceeds slot 7a's five, and 7a split.
If the data-safety pass, the recent-documents seam retirement, or §D6's re-key consumes
this change's capacity, the split boundary is **after `WFR-SHELL-GEOMETRY`**:

- **7b** — the six replacement rows' migrations, `WFR-PLAIN-DISPOSAL`'s discharge,
  `WFR-STARTUP-PREFLIGHT`, every reassignment with its receiving row's cells
  re-derived, the coverage proof, the gate re-key, and the inherited debt.
- **7c** — `WFR-AUTOMATION-SPINE`'s terminal status, both capability deltas, and the
  programme closeout with its single deferral inventory.

**Taking the split forces a delta decision, and it is finer-grained than "move them
both."** Slot 7a's rule is that a delta must not ship in a change that cannot discharge
its obligation — but the two deltas are not each single obligations, and their clauses
do not all land on the same side of the boundary:

| Clause | Dischargeable in 7b? | Why |
| --- | --- | --- |
| Delta 1(a) cross-row cell staling | **yes** | 7b performs all four reassignments and re-derives every receiving row's cells |
| Delta 1(c) provisional grouping rows | **yes** | 7b implements the split and re-derives the coverage proof |
| Delta 1(b) terminal status + probe evidence + matrix/ledger reconciliation | **no** | requires the spine terminal, which is 7c's |
| Delta 1(d) closeout record | **no** | 7c's by construction |
| Delta 2 (cross-cutting lane surface) | **yes** — but its own scenario says the obligation *"is discharged by the change that closes the migration programme"* | 7b discharges the lane; 7c would be the closing change. The scenario's wording and the work would separate |

So the split has three lawful resolutions, and task 0.13 selects one rather than
discovering the problem mid-split: **(i)** move both deltas to 7c and accept that 7b
performs work its own spec text does not yet require; **(ii)** split delta 1 along the
table, shipping (a) and (c) in 7b and (b) and (d) in 7c — which is more faithful and
costs a second delta file; or **(iii)** keep delta 2 in 7b and reword its discharge
clause from "the closing change" to "the change that discharges the lane", a
one-sentence delta edit paid with its retroactive re-check.

Taking the split also means replacing the ledger's `slot 7b` line with `slot 7b` and
`slot 7c` lines and splitting the remaining-scope row. It does **not** renumber
anything, and a partially migrated row is never an acceptable outcome.

## What Changes

- **Implement §D1's hybrid outcome**: retire `WFR-SHELL-LAYOUT` and replace it with
  seven rows that each name one workflow or one cross-cutting orderer, plus
  no-coordination-tier entries for the surfaces that have neither — with each row's
  criterion-1 evidence recorded per surface, the three rows outside the candidate
  table's maximum justified individually, and **`transient_surfaces.rs` removed from
  the tier list** and `actions.rs` explicitly not demoted.
- **Migrate six replacement rows**: a narrative facade each measured against 370, role
  homes chosen and recorded, coordination roles from the bounded set, called
  presentation surfaces classified in both their own module doc and the row, one test
  policy per row, and — for every row the census would have said needs no seam value
  object or evidence surface — **probe first and record the negative finding**.
- **Reassign four surfaces to rows this change does not migrate** (`dialogs.rs`,
  `ui/window/search.rs`, `focus_indexing.rs`'s two palette stage orders and its
  unowned focus-restoration story, `mod.rs`'s theme selector), and **re-derive every receiving row's staled measured cells** in the same
  change, which is delta 1's own cross-row staling statement applied to the change
  that authored it.
- **Re-derive the census coverage proof** under a stated predicate rather than
  adopting any of the three figures in circulation (the matrix's 198, slot 7a's 266, and
  286 at authoring), so
  matrix still states, so no file loses attribution at the moment the programme claims
  completeness.
- **Give `WFR-SHELL-LAYOUT` a lawful retirement** (Finding 1c) — either a terminal
  retirement label added to the status vocabulary as a delta 1 edit, or the row retained
  with a documented terminal status and a `superseded by` pointer — and **rewrite the
  nine stale `WFR-SHELL-LAYOUT` owner pointers** across seven source and doc files that
  no gate can see.
- **Re-derive `focus_indexing.rs`'s four stories from the code** before reassigning any
  of them, and **give focus restoration an explicit owner**, since the inherited
  three-story decomposition mislabels it as geometry, absorbs its 129 lines into the
  eviction figure, and collapses two palette stage orders into one.
- **Audit the ledger-and-slot-column parsing path for fail-opens** (Finding 6b) with
  the three reproduced holes as the starting point — a renamed `Slot` header silently
  disabling half the matrix check, a typo'd ledger verb silently dropping a claim, and a
  `Slot` cell of `none` exempting a `pending` row — because the closeout's central claim
  is that the matrix and the ledger agree, and two of these let them disagree at exit 0.
- **Resolve slot 7a's own mutation-triage contradiction** (B.6's "160 untriaged" against
  A.12's recorded 130 caught / 13 unviable / 17 missed) before scoping any triage, and
  scope the run to the **actual** surviving set.
- **Discharge `WFR-PLAIN-DISPOSAL`'s surface obligations**: three parallel typed
  observation values and five accessors consolidated into one surface at its measured
  readers' visibility, all three mandated proofs plus the no-materialization statement
  discharged with the lane **quiesced** so a worker-thread atomic cannot make the
  reentrancy assertion unsound, the `DisposalProducer` family's 12 default-feature
  `never used` items resolved, and the row advanced to
  `cross-cutting — surface obligations discharged` with `model/plain_disposal.rs`
  unmoved and out of GTK Lush scope.
- **Resolve `WFR-AUTOMATION-SPINE` to a terminal status** after probing
  `ui/automation.rs` for separable pure decisions and recording the finding either
  way, reconcile all three disagreeing sources, verify rather than inherit the
  `MinimapEvidence` non-registration verdict, and **record the automation ratchet row
  as occurrence-retired with its reading expression persisting at eight sites** under a
  new owning row — not struck retired, which its own predicate forbids.
- **Re-key the path-keyed gates §D1's outcome moves** — six literal `actions.rs`/
  `imp.rs` pairs across three predicates in each of two implementations, plus six
  self-test keys — to the narrowest key that still selects exactly the protected code,
  with the `ui/window/` prefix **forbidden**, each half carrying a parity assertion
  proved by deliberate red, and the disarm **observed before it is fixed**.
- **Land both withheld capability deltas** with their retroactive re-checks across
  sixteen migrated rows, and **implement the mechanical half delta 1 still owes**:
  fail when a row carries a transitional status while the ledger has no outstanding
  slot.
- **Dispose the six pieces of slot 7a review debt** that reached no artifact, each
  re-verified and two of them re-measured against wrong inherited figures, plus the
  A.6/A.13a contradiction about the accessibility gate.
- **Triage slot 7a's 160 newly-in-scope mutants** to zero or to narrow documented
  equivalences, and re-derive the two ratchet rows' current survivor counts.
- **Advance every matrix row to a terminal status, close the slot ledger, and write
  the programme completion record** — measured outcomes against the section 2
  baseline, the full re-measured facade table replacing the stale eleven-row one, the
  refreshed Measurement Definitions denominators, and **one** inventory of all nine
  remaining user-gated deferrals with each one's gating condition and owner — with the
  live-run and manual-Orca gaps recorded as **awaiting the user's decision** and not
  written as accepted on this change's authority.

## Capabilities

### New Capabilities

None. Phase 0 holds the contract; this change consumes and closes it.

### Modified Capabilities

Both deltas were authored in slot 7 and **relocated into this change** at split time
(Finding 0). Both add obligations, so both carry the retroactive-amendment cost across
**sixteen** migrated rows — the most expensive re-check the programme has paid, accepted
deliberately because this change's own sweep *is* the re-check.

- **`workflow-readability-boundaries`** — four statements on the census requirement.
  (a) **Cross-row cell staling**: where a change resolves a census gap by *assigning*
  files to a row it does not migrate, the receiving row's measured cells become stale
  at that moment and the assigning change either re-derives them or records in the row
  that they are stale and why. **Two notes on how this change reads its own delta**,
  because both are easy to over-read. First, the clause grants a **disjunction** —
  re-derive *or* record-stale — and this change's decision to always re-derive is a
  **change-level choice**, not something the delta compels; it is taken because a
  closeout that records four rows' cells as stale would be asserting completeness over
  known-stale measurements. Second, the clause's trigger is a row the assigning change
  *"does not otherwise touch"*, and 7b **does** touch `WFR-DOCUMENT-SAVE`,
  `WFR-DRAFT-RECOVERY`, `WFR-SEARCH-REPLACE`, and `WFR-COMMAND-PALETTE` — so the clause
  arguably does not fire at all here and the obligation is self-imposed. Recorded so a
  later reader does not cite this change as precedent for a stricter rule than the text
  states. (b) **Terminal status at programme close**: when the
  final slot lands, every row carries `migrated`, `exempt`, or `cross-cutting`;
  `pending`, `deferred`, and `partially-conforming` do not survive; a row resolved as
  non-migrating records the **probe evidence** for that conclusion; and where the
  matrix and the ledger disagree, the closing change makes them agree. (c) **Residual
  grouping rows**: such a row is provisional, its migration derives its stage orders,
  resumption points, shared coordination state, and external entry surface first, and
  where the grouping is not one workflow it is replaced by rows that each name one
  workflow with the coverage proof re-derived — and a split justified by line count is
  the forbidden budget response wearing the grouping clause as cover. (d) **Closeout
  record**: the programme record carries a completion section stating measured outcomes
  against its baseline and listing every remaining deferral in one place with its
  gating condition and owner, including deferrals recorded only inside archived change
  directories, and it must not record an unmet acceptance gate as accepted on the
  change's own authority.
- **`workflow-evidence-surfaces`** — one statement extending the evidence-surface
  requirement from *migrated workflows* to **cross-cutting coordination lanes**. A lane
  that is not a workflow but exposes observable state through test-only inspection
  seams holds **one** typed surface under the same visibility, reentrancy,
  non-materialization, and bounded-child rules with the same three proofs; parallel
  typed observation values over one lane's state are the duplication the requirement
  already forbids; consolidation must not move or duplicate a limit the lane shares
  with a workflow that calls it; the surface's file may keep the lane's own name
  because `evidence.rs` is a workflow **role** name and a lane carries no roles; and
  because no migration event will ever fire for such a lane, the obligation is
  **discharged by the change that closes the programme** rather than deferred to it.

## Impact

**Affected code**, physical/production lines measured at authoring:

- `crates/lushtext-core/src/ui/window/imp.rs` (1,675) — the geometry half moves; a
  **visual-sensitive** path requiring two named pixel invariants and the
  workspace-sidebar animation matrix.
- `crates/lushtext-core/src/ui/window/actions.rs` (863) — two of the geometry
  candidate's stages, **not demotable**, and a literal key in three predicates per
  implementation.
- `crates/lushtext-core/src/ui/window/policy.rs` (813 / ~287) — the geometry row's
  pure policy, landed by slot 7a and moving into whichever role home §E1 selects.
- `crates/lushtext-core/src/ui/window/tabs.rs` (634/588) and `documents.rs` (1,181) —
  the tab-strip row, including the tab-pin and bulk-close neighbours.
- `crates/lushtext-core/src/ui/window/focus_indexing.rs` (856) — **four** stories
  (Finding 1b): editor-memory eviction (~406), palette overlay control (~104), palette
  file-index build (~171, with its own coordinator), and focus restoration (~129, with
  **no owner**).
- **Stale `WFR-SHELL-LAYOUT` pointers in production source and docs**, which the row's
  retirement invalidates and which no gate can find because they are prose in doc
  comments: `ui/sidebar/mod.rs:25`, `:68`, `:85`; `ui/sidebar/AGENTS.md:21`, `:43`;
  `ui/sidebar/width_preset.rs:9`; `ui/sidebar/filter_execution.rs:20`;
  `ui/editor_page/restore_position.rs:14`; and `docs/automation-reference.md:681`. Nine
  sites across seven files. Each names the retired row as an **owner** — of the width
  preset, of the sidebar show/hide animation, of the readiness blocker, of the
  open/active file-row projection — so leaving them behind would point a reader at a
  row that no longer exists, from files the seven replacement rows now own. Task 11.5a
  re-derives the set and rewrites each to its new owner.
- `crates/lushtext-core/src/ui/window/focus_mode.rs` (354),
  `transient_surfaces.rs` (202), `startup_data.rs` (435), `zoom.rs` (156),
  `workspace_scope.rs` (48), `mod.rs` (269), `recent_open.rs` (282).
- `crates/lushtext-core/src/ui/window/dialogs.rs` (861) and `search.rs` (955/928) —
  reassigned to migrated rows, not moved into a new row.
- `crates/lushtext-core/src/ui/open_popover/**` — 3 files, 1,422, carrying the row's
  26 declarations and 37 gate sites.
- `crates/lushtext-core/src/ui/sidebar/width_preset.rs` (125) and
  `ui/properties_panel/**` (333) — the geometry row's and the tier list's respectively.
- `crates/lushtext-core/src/ui/plain_disposal.rs` (1,542/1,344) and
  `crates/lushtext-core/src/model/plain_disposal.rs` (692/465) — surface only; over the
  1,000-line production target, recorded rather than split blind.
- `crates/lushtext-core/src/ui/automation.rs` (2,214/2,084) — probed, not restructured.

**Affected configuration and gates:** `.cargo/mutants.toml`,
`scripts/check-workflow-boundaries.py`, `scripts/check-visual-proof-policy.py`,
`crates/cargo-gtk-proof/src/policy.rs`, `scripts/check-automation-docs.py`,
`scripts/accessibility_source_fingerprint.py`, `scripts/run-performance-smoke.sh`,
and whatever task 8.1 selects for the rustfmt reach.

**Affected documentation:** `docs/workflow-readability-matrix.md`,
`docs/next/workflow-readability.md`, `docs/mutation-testing.md`,
`docs/next/persistent-format-hardening.md`, `docs/automation.md`,
`docs/automation-reference.md`, `docs/accessibility-matrix.md`,
`docs/end-user-coverage.md`, `AGENTS.md`, `README.md`,
`crates/lushtext-core/src/ui/window/AGENTS.md`, `.agents/rules/rust.md`,
`.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, `.agents/rules/build.md`,
`.agents/rules/documentation.md`, and any `.agents/skills/*/references/*.md` naming a
moved path.

**Affected tests:** `crates/lushtext/tests/widget/{window,open_popover,plain_disposal,
markdown_preview,properties_panel,status_bar,preferences,editor_page}.rs` and the
co-located test modules that follow the production code they cover.

**Not affected, and deliberately so:** the exported D-Bus automation contract, every
visual geometry invariant id, `model/plain_disposal.rs`'s location and GTK Lush
exclusion, `char_count_requires_chunked_snapshot`, and the behavior of the sixteen
already-migrated workflows. Where a task's proof is that something did *not* change,
it is a measured diff rather than an assertion.
