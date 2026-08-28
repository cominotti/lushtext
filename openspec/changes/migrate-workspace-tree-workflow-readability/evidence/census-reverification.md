# Census re-verification — `WFR-WORKSPACE-TREE` (slot 5b)

**Method, stated before any figure, because this change's own proposal committed a
units error by not doing so.** Size figures are **production lines only**:
`#[cfg(test)]` modules are excluded, including a co-located test module that lives
in its own file behind `#[cfg(test)] mod tests;`, which a naive per-file scan
counts as production. Where a raw total is given it is labelled **raw** and is
present only because a file being moved wholesale is moved at its raw size. Seam
figures are counted as **`*_for_test` function definitions** and, separately, as
**`#[cfg(feature = "test-utils")]` gate sites**; the two are different metrics and
both are named. Consumer counts are counted as **owning workflows**, established
by `use` import rather than by substring.

Measurement is row-scoped: only the 23 files this row owns are counted. No shared
service file, cross-cutting module, or neighbouring called file is pooled in.

## 1. Size — every figure exactly reproducible, correction is to the *matrix* cell

Counted at this change's implementation against the tree at `f1515574`.

| File | Production | Raw | 5a's production figure | Growth |
| --- | --- | --- | --- | --- |
| `ui/sidebar/mod.rs` | 415 | 415 | 406 | **+9** |
| `ui/sidebar/workspaces.rs` | 864 | 864 | 843 | **+21** |
| `ui/sidebar/imp.rs` | 246 | 246 | 246 | 0 |
| `ui/sidebar/callbacks.rs` | 219 | 219 | 219 | 0 |
| `ui/sidebar/dialogs.rs` | 205 | 205 | 205 | 0 |
| `ui/sidebar/policy.rs` | 134 | 261 | — (new with 5a) | **+134** |
| `ui/sidebar/seams.rs` | 92 | 133 | — (new with 5a) | **+92** |
| `ui/sidebar/test_policy.rs` | 88 | 88 | — (new with 5a) | **+88** |
| `workspace_section/tree_loading.rs` | 1,269 | 1,269 | 1,269 | 0 |
| `workspace_section/tree_index.rs` | 844 | 969 | 844 | **0** |
| `workspace_section/folders.rs` | 835 | 835 | 835 | 0 |
| `workspace_section/context_menus.rs` | 809 | 809 | 809 | 0 |
| `workspace_section/dnd.rs` | 769 | 769 | 769 | 0 |
| `workspace_section/mod.rs` | 744 | 744 | 744 | 0 |
| `workspace_section/peek.rs` | 728 | 728 | 728 | 0 |
| `workspace_section/actions.rs` | 707 | 707 | 534 | **+173** |
| `workspace_section/refresh.rs` | 666 | 702 | 666 | **0** |
| `workspace_section/watch.rs` | 593 | 606 | 583 | **+10** |
| `workspace_section/imp.rs` | 508 | 508 | 508 | 0 |
| `workspace_section/row_factory.rs` | 463 | 463 | 463 | 0 |
| `workspace_section/watch_targets.rs` | 264 | 337 | 264 | **0** |
| `workspace_section/row_accessibility.rs` | 199 | 199 | 199 | 0 |
| `workspace_section/icon_presentation.rs` | 80 | 155 | 80 | **0** |
| **Total (23 row files)** | **11,741** | 12,231 | 11,214 | **+527** |

`ui/sidebar/file_tree_item.rs` (150 production / 151 raw) is **excluded** and
confirmed still listed under the matrix's *Surfaces With No Coordination Tier*.
It is nonetheless crossed by this row's stage trace: it owns the `pending_rename`
one-shot flag (`:35`) that `workspace_section/actions.rs:75` sets and
`workspace_section/row_factory.rs:308-320` consumes and clears.

**The total is checkable two ways, and both agree**: 5a's 11,214 plus 527 of
attributed growth equals 11,741, and the direct per-file production sum equals
11,741.

### Growth attribution, in production units

| Source | Lines | Cause |
| --- | --- | --- |
| `workspace_section/actions.rs` | +173 | 5a's rename/cleanup data-safety fixes |
| `ui/sidebar/workspaces.rs` | +21 | the M-4 load-generation guard |
| `workspace_section/watch.rs` | +10 | the watch-target repair operation |
| `ui/sidebar/mod.rs` | +9 | module declarations and one re-export pair |
| three new files | +314 | `policy.rs` 134, `seams.rs` 92, `test_policy.rs` 88 |
| **total** | **+527** | |

### Four of 5a's per-file figures are exactly reproducible with **zero** production growth

This is recorded affirmatively, because recording that 5a was *right* is what
keeps a later slot from "re-correcting" a correct cell:

| File | 5a production | Now | `#[cfg(test)]` opens at | Raw growth is test-only |
| --- | --- | --- | --- | --- |
| `tree_index.rs` | 844 | 844 | `:845` | 969 − 844 = 125 |
| `watch_targets.rs` | 264 | 264 | `:265` | 337 − 264 = 73 |
| `icon_presentation.rs` | 80 | 80 | `:81` | 155 − 80 = 75 |
| `refresh.rs` | 666 | 666 | `:667` | 702 − 666 = 36 |

**Slot 5a's census cell was correct.** This change's own proposal first draft
claimed it had gone stale by ~900 lines; that claim was false and its cause was
comparing a production-only census against raw file totals. The correction below
is therefore to the *matrix cell*, which still records 5a's pre-growth figure —
not to 5a's method or its arithmetic.

### Corrections, with direction

| Cell | Old | New | Direction |
| --- | --- | --- | --- |
| Row `Current size` | 20 files, 11,214 production | **23 files, 11,741 production** | **upward**, +527 across 3 more files |
| Owned pure policy: `policy.rs` size | **190** | **134 production / 261 raw** | the old figure is neither unit; genuinely stale |
| Census-cell legacy figure `28 files, 16,947 lines (ui 11,682 / model 1,368 / services 3,897)` | — | superseded; the `services` subtotal pooled `services/file_tree.rs`, `workspace_manager.rs`, `workspace_watch.rs`, `file_peek.rs` **whole**, all shared and none owned | pooled population named |

## 2. Seams — 60 functions across 111 gate sites, byte-exactly matching the matrix cell

| File | `*_for_test` fns | `test-utils` gate sites |
| --- | --- | --- |
| `workspace_section/mod.rs` | 15 | 23 |
| `workspace_section/watch.rs` | 15 | 24 |
| `workspace_section/refresh.rs` | 9 | 10 |
| `ui/sidebar/dialogs.rs` | 6 | 6 |
| `workspace_section/dnd.rs` | 6 | 12 |
| `workspace_section/tree_loading.rs` | 4 | 10 |
| `workspace_section/tree_index.rs` | 3 | 3 |
| `ui/sidebar/test_policy.rs` | 2 | 1 |
| `workspace_section/imp.rs` | 0 | 8 |
| `workspace_section/watch_targets.rs` | 0 | 7 |
| `workspace_section/folders.rs` | 0 | 3 |
| `workspace_section/actions.rs` | 0 | 2 |
| `ui/sidebar/mod.rs` | 0 | 2 |
| **total** | **60** | **111** |

Still the largest seam population in the programme by more than double; slot 4's
largest single row held 28 functions across 55 gate sites.

## 3. Two ungated `_for_benchmark` seams **outside** that census

Neither is behind `test-utils`, so no gate-site grep finds either. Both confirmed
at the exact lines the proposal names:

| Seam | Definition | Bench use | Owning row |
| --- | --- | --- | --- |
| `child_cache_rebuild_operation_evidence_for_benchmark` | `ui/sidebar/workspace_section/mod.rs:608`, `pub fn` | `crates/lushtext-core/benches/benchmarks.rs:95` (`use`) | **this row** |
| `merge_backend_result_for_benchmark` | `crates/lushtext-core/src/services/workspace_watch.rs:267`, `pub fn` | `benches/benchmarks.rs:3113` | **the service**, not this row |

An ungated `pub` bench seam is the same class of invisible seam as a test-only
field on a production struct. Disposition is decided in task 6.1; recorded here so
the population is complete.

## 4. Test-only override storage — still **no module statics**

Confirmed: this row's configuration overrides are test-only *fields on production
state structs*, which no `static` grep finds.

`crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`:

| Field | Line | Owner struct |
| --- | --- | --- |
| `test_reconcile_batch_delay` | `:164` | `RefreshRuntimeState` |
| `test_scan_delay` | `:167` | `RefreshRuntimeState` |
| `test_empty_probe_reads` | `:170` | `RefreshRuntimeState` |
| `test_start_delay` | `:197` | `WatchRuntimeState` |
| `test_drop_delay` | `:200` | `WatchRuntimeState` |
| `test_worker_starts` | `:203` | `WatchRuntimeState` |
| `test_last_poll_notices` | `:206` | `WatchRuntimeState` |
| `test_disabled` | `:209` | `WatchRuntimeState` — the **permanent restart-suppression flag** whose meaning must be preserved exactly |

Plus, outside those structs:

- `workspace_section/tree_loading.rs:109-111` — a `thread_local!`
  `DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT: Cell<usize>` counter.
- `workspace_section/watch_targets.rs:56` — `touched_rows: usize`, accumulated at
  `:138`, `:204`, `:216` through `record_touched_rows` (`:231`) and read
  **destructively** at `:237` `take_touched_rows` via `std::mem::take`. This is
  the destructive-read seam whose reset task 6.4 must separate from its
  observation.

Also recorded, because it is adjacent and is *not* test-only storage:
`tree_loading.rs:77-78` holds `ACTIVE_WORKSPACE_SCAN_TASKS` and
`WORKSPACE_SCAN_TASK_HIGH_WATER` as `AtomicUsize` **process-global** statics. They
are production admission accounting, not overrides — and their process-global
scope is the honesty problem task 6.1 must decide.

## 5. Pure policy consumer counts, by import

| Module | Raw size | Importers (by `use`) | Owning workflows | Verdict |
| --- | --- | --- | --- | --- |
| `model/workspace_persistence.rs` | 338 | `ui/sidebar/imp.rs:9`, `ui/sidebar/workspaces.rs:21` | **1** | relocates; the programme's cleanest relocation — **no** `services`, **no** `model`, **no** bench consumer |
| `model/workspace_scan.rs` | 231 | `workspace_section/tree_loading.rs:10`, `workspace_section/folders.rs:15`, `workspace_section/imp.rs:19`, **plus** `benches/benchmarks.rs:57` (`use lushtext_core::model::workspace_scan::WorkspaceScanFlight`) and `:3198` (fully-qualified return type `model::workspace_scan::WorkspaceScanFlightMetrics`) | **1** owning workflow + a bench path | relocates; the move is a **public-path break for the bench target** (task 3.4) |

Substring false positives named, so a later re-derivation does not re-count them:
grepping `workspace_persistence` also matches the sidebar's own method names
`publish_workspace_persistence_message`, `flush_workspace_persistence`,
`workspace_persistence_pending`, `workspace_persistence_inflight`, the
`ui/window/dialogs.rs:715` close-time flush **call**, three `ui/automation.rs`
projection **calls** (`:759`, `:925`, `:926`), and a widget test name — none of
which is an import of the model module. Likewise grepping `workspace_scan` matches
`workspace_scan_admission_source`, `try_acquire_workspace_scan_permit`,
`arm_workspace_scan_admission_retry`, and two `*_for_test` counter reads, none of
which is an import.

Confirmed: **no `services` consumer and no `model` consumer** for either module,
which is what makes the relocation permissible at all.

`model/workspace.rs` (799 raw) is confirmed **domain and staying**; its owning-workflow
count is re-derived in task 3.7.

## 6. Summary of directions

- Size: **upward** correction to the matrix cell (+527 / +3 files); 5a's method and
  every one of its 23 per-file production figures reproduce exactly.
- `policy.rs` size cell: genuinely **stale in both units**; corrected.
- Seams: **unchanged** at 60/111 — byte-exact against the matrix cell, plus **two
  newly named ungated bench seams** that were outside the census entirely.
- Override storage: **unchanged** — still no module statics; eight test-only fields
  re-confirmed at their current lines.
- Consumer counts: **unchanged** in count, with the bench path break named
  explicitly rather than discovered by the compiler.

---

## 7. Row size **after** this change, and why it grew

Re-measured in the same production units, so the two figures are comparable.

**23 row files, 12,279 production lines** (up from 11,741, **+538**).

`ui/sidebar/file_tree_item.rs` (150) is still excluded as not this row's, and the new
`ui/sidebar/width_preset.rs` (125) is **also excluded**, for the same kind of reason:
it is cross-cutting and belongs to `WFR-SHELL-LAYOUT`. So the file count is unchanged
at 23 even though the directory now holds one more file.

| File | Before | After | Delta | Cause |
| --- | --- | --- | --- | --- |
| `ui/sidebar/policy.rs` | 134 | **588** | **+454** | the two relocations, plus the extracted `confirmed_delete_verdict` |
| `ui/sidebar/seams.rs` | 92 | **224** | **+132** | `WorkspaceWatchTicket`/`Facts`/`Disposition` plus the two generation newtypes |
| `workspace_section/actions.rs` | 707 | **764** | **+57** | the delete identity recheck and its typed outcome |
| `workspace_section/watch.rs` | 593 | **609** | **+16** | the ticket wiring |
| `workspace_section/watch_targets.rs` | 264 | **244** | **−20** | the two newtypes left |
| `workspace_section/imp.rs` | 508 | **507** | **−1** | import reshuffle |
| `ui/sidebar/mod.rs` | 415 | **316** | **−99** | `WorkspaceSidebarWidthPreset` left as cross-cutting |
| all others | — | unchanged | **0** | untouched by this change |

**The growth is the row taking ownership of policy it already depended on, not bloat.**
+454 of the +538 is two `model/` modules moving *in* — the row's footprint grew by
exactly the amount that `model/` shrank, and the mutation-scoped policy count stayed at
**10** because both merged into the workflow's single existing `policy.rs`.

Netting the relocations out, this change's own **new** production code is about **84
lines**: the delete fix (+57), the seam wiring (+16), the extracted verdict, and the
draft re-stamp, against **−99** for the facade shrinking. The row's *own* adapter code
therefore got slightly smaller.

**That table is a mid-change measurement, superseded below.** It was taken after the
relocations and the two pass-1 data-safety fixes but **before** the structural migration
and the seam retirement, and an earlier revision of this file left it as the final word.
The authoritative figures are in section 3.

**A note for the next re-derivation**: `policy.rs` was **588 production / 1,086
raw** at that mid-change point. The raw figure exceeds the ~1000-line target, but the target counts production
lines and excludes `#[cfg(test)]` — roughly half that file is now co-located tests,
which is what absorbing two fully-tested modules looks like and what made the
mutant-by-mutant parity proof possible. Do not "correct" it to the raw number.


## 3. Final re-derivation — the authoritative figures (fix cycle)

Re-derived **after** the last code change and after every lane re-ran, per the matrix's
own timing rider: a cell measured mid-change is stale by construction. Same method as
section 1 — production lines only, `#[cfg(test)]` modules excluded, row-scoped to the 27
files this row owns.

| File | Production | Raw |
| --- | --- | --- |
| `ui/sidebar/callbacks.rs` | 228 | 228 |
| `ui/sidebar/dialogs.rs` | 214 | 214 |
| `ui/sidebar/evidence.rs` | 544 | 544 |
| `ui/sidebar/filter_execution.rs` | 161 | 161 |
| `ui/sidebar/imp.rs` | 269 | 269 |
| `ui/sidebar/list_execution.rs` | 379 | 379 |
| `ui/sidebar/membership_execution.rs` | 322 | 322 |
| `ui/sidebar/mod.rs` | 292 | 292 |
| `ui/sidebar/persist_execution.rs` | 220 | 220 |
| `ui/sidebar/policy.rs` | 690 | 1,318 |
| `ui/sidebar/seams.rs` | 294 | 411 |
| `ui/sidebar/test_policy.rs` | 136 | 136 |
| `ui/sidebar/workspace_section/context_menus.rs` | 817 | 817 |
| `ui/sidebar/workspace_section/file_execution.rs` | 805 | 805 |
| `ui/sidebar/workspace_section/folder_execution.rs` | 850 | 850 |
| `ui/sidebar/workspace_section/icon_presentation.rs` | 87 | 162 |
| `ui/sidebar/workspace_section/imp.rs` | 681 | 681 |
| `ui/sidebar/workspace_section/mod.rs` | 788 | 788 |
| `ui/sidebar/workspace_section/peek_execution.rs` | 741 | 741 |
| `ui/sidebar/workspace_section/refresh_execution.rs` | 632 | 668 |
| `ui/sidebar/workspace_section/reorder_execution.rs` | 834 | 834 |
| `ui/sidebar/workspace_section/row_accessibility.rs` | 208 | 208 |
| `ui/sidebar/workspace_section/row_factory.rs` | 477 | 477 |
| `ui/sidebar/workspace_section/scan_admission.rs` | 129 | 129 |
| `ui/sidebar/workspace_section/scan_execution.rs` | 1,982 | 2,107 |
| `ui/sidebar/workspace_section/watch.rs` | 550 | 563 |
| `ui/sidebar/workspace_section/watch_targets.rs` | 269 | 342 |
| **Total (27 row files)** | **13,599** | — |

`ui/sidebar/file_tree_item.rs` (150) and `ui/sidebar/width_preset.rs` (125) are excluded
as before, the first as a surface with no coordination tier and the second as
cross-cutting `WFR-SHELL-LAYOUT` state.

**Corrections this forced to the matrix row**, each in the direction of *more* code than
the mid-change figures said:

| Cell | Said | Is | Note |
| --- | --- | --- | --- |
| row total | 12,958 across 27 files | **13,599** | measured before the role modules finished gaining doc narration |
| `policy.rs` | 612 production / 1,110 raw | **690 / 1,318** | the third confirmed-delete verdict and its two tests landed after |
| `evidence.rs` | 269 production | **544** | the surface's own module doc and per-field docs are most of it |
| facade `mod.rs` | 291 of 370 | **292 of 370** | one line; recorded because an off-by-one in a *budgeted* figure is exactly what the budget exists to catch |

The four cells moved in one direction, which is the point of the timing rider: the
mid-change snapshot was taken while the narration this convention asks for was still
being written, so it understated the very growth the convention causes.

**Growth from the pre-slot-5b row**: 11,741 → **13,599** production across 23 → 27
files, of which **380** is the two `model/` relocations moving in
(`workspace_persistence.rs` 219, `workspace_scan.rs` 161) and the rest is role modules
gaining doc narration plus `evidence.rs`.
