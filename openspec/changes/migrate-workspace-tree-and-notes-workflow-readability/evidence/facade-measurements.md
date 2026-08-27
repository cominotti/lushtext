# Facade measurements (tasks 9.2, 9.3)

Metric: **physical lines**, which is what the declared budget of 370 is stated
against.

## This change's facade

| Facade | Physical lines | Budget | Headroom |
| --- | --- | --- | --- |
| `ui/window/notes/mod.rs` (`WFR-NOTES-BOOKMARKS`) | **178** | 370 | 192 |

It narrates **five stage orders** and **nine inversions**, one line each, plus a
role table and a "State this workflow shares with others" table. Five stage
orders in 178 lines is the programme's densest narration so far and it fits
comfortably, which is worth stating because it is evidence about the budget:
**stage-order count alone does not stress 370 — the exemplar's two stage orders
sit at 369 because its narration carries twelve inversions in prose plus a large
value-type surface in the same file.** The notes facade holds no value types at
all: `NotesBrowserState`, `ActiveNotesBrowser`, `OpenEditorNoteSnapshots`, the
four preview constants, and the eleven budgets all moved to `browser.rs` and
`policy.rs`.

## The other facades, re-measured (task 9.3)

Every migrated facade was re-measured at the end of this change, not read from the
matrix.

| Facade | Matrix figure | Measured now | Verdict |
| --- | --- | --- | --- |
| `ui/search_panel/mod.rs` | 369 | **369** | unchanged. **No physical line was added**, as required: the only edit replaced one role-table row's label text (`adapter detail` → `called presentation surface (no role)`) in place. |
| `ui/command_palette/mod.rs` | 335 | **335** | unchanged, same one-line in-place label replacement |
| `ui/editor_page/save/mod.rs` | 223 | **223** | unchanged |
| `ui/editor_page/load/mod.rs` | 271 | **271** | unchanged |
| `ui/editor_page/buffer_replacement/mod.rs` | 167 | **168** | **the matrix figure was stale by one line before this change.** `git diff` confirms this change did not touch the file. Corrected in the matrix. |
| `ui/window/session_restore/mod.rs` | 165 | **165** | unchanged |
| `ui/window/local_history/mod.rs` | 216 | **215** | **the matrix figure was stale by one line.** Untouched by this change. Corrected. |
| `ui/window/drafts/mod.rs` | 310 | **289** | **the matrix figure was stale by twenty-one lines.** Untouched by this change; the matrix records it as "the programme's largest facade and its closest approach to the ceiling", which is still true of the ordering but not of the number. Corrected. |

**Three of eight recorded facade sizes were wrong, all in the safe direction**
(the real files are the same size or smaller than claimed). That is a smaller drift
than the census cells have shown, but it is the same class, and it is the reason
task 9.3 says *re-measure* rather than *confirm*: a budget claim checked against a
stale number is not a check. Worth carrying to slots 5b–7: **re-measure every
facade at the end of the change, from the tree.**

## The `WFR-WORKSPACE-TREE` budget decision (task 2.6), and what became of it

Task 2.6 required the one-row-versus-split and delegate-versus-escalate decisions
**before** any facade text was written, against task 0.4's reconciled trace. Both
were made and recorded in `shared-ownership-decisions.md` §2.6: **one row, one
facade, delegated hard, no escalation and no split**, projected at ≈351 of 370
after moving `WorkspaceSidebarWidthPreset` (103 lines, cross-cutting to
`WFR-SHELL-LAYOUT`), `SidebarFileRowStateSnapshot` (36, → `seams.rs`), and
`WorkspacePersistenceFlushError` (21, → `seams.rs`) out of `ui/sidebar/mod.rs`,
and delegating the five section-fan helpers.

**That facade was not written**, because the tree row's structural migration moved
to slot 5b (see `docs/next/workflow-readability.md`, "Why slot 5 split into 5a and
5b"). `ui/sidebar/mod.rs` therefore still measures **415** physical lines — nine
more than the 406 recorded at authoring: the `pub mod policy;`, `mod seams;`, and
`#[cfg] mod test_policy;` declarations plus the re-exported test-policy setter
pair the data-safety regression tests required. The decision and its arithmetic stand
recorded for 5b, which inherits a projection rather than a question.

**The budget line was not edited**, and no escalation was proposed.
