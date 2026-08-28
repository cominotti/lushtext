# Slot 5a's decisions: confirmations, and the two re-measurements (section 2)

Framing, per the task list: **inherited decisions are verified, not re-litigated.**
Each item below is a confirmation with its evidence, or a recorded deviation with its
reason. None is "decide again".

## 2.1 Facade budget — RE-MEASURED

Recorded in full in `evidence/facade-measurements.md`. Summary: the three recorded
line ranges were stale by **exactly +9** each while every one of their **sizes was
unchanged**; the fourth subtrahend (the `:102-178` inspection and focus block, 77
lines reducing to ~24) had no recorded range and was measured here for the first
time. Independent re-projection gives **≈349 of 370**; 5a's figure re-based onto the
now-confirmed twelfth stage order gives **≈358–360**. Both fit. **Escalation path
step 1 (delegate harder) is sufficient; no in-change escalation, no census-row
split.**

## 2.2 Role home and module classification — CONFIRMED, with one addition

**Confirmed: option (2), nested.** Canonical role home `ui/sidebar/` holding the
facade, the single `policy.rs`, and the single `evidence.rs`; bounded coordination
role modules nested inside `ui/sidebar/workspace_section/`. This change is the
**first adopter** of the nested role home statement (c) that slot 5a landed.

Re-confirmed module by module against the current code. 5a's classification stands
for every module it named, with these deltas:

| Delta | Detail |
| --- | --- |
| **`tree_loading.rs` was never classified** | 5a's map — under a heading reading "Every module classified" — gives `scan_admission.rs` and `scan_execution.rs` no named source and never classifies `tree_loading.rs` at all, the row's **largest** file at 1,269 lines. This change classifies it as a **third dissolution** (task 5.3). |
| **Folder membership needs its own coordination module** | Task 0.4's verdict (below) makes workspace folder add/remove its **own** stage order, so `list_execution.rs` must **not** own folder membership. A separate `execution` module owns it, and it is the named destination for both routes — the header dialog route (`dialogs.rs:71`) and the row request route (`workspace_section/mod.rs:315`). This is a genuine correction to 5a's map, which had only `list_execution.rs`. |
| **`watch.rs` keeps its name** | Already a correct bounded role name. This change's own amendment forbids renaming it for symmetry with the eight newly named `*_execution.rs` siblings; recorded explicitly so a reader does not read the asymmetry as an oversight. |
| **Called presentation surfaces: nine, not six** | `ui/sidebar/{callbacks.rs, dialogs.rs, imp.rs}` plus `workspace_section/{mod.rs, imp.rs, row_factory.rs, context_menus.rs, row_accessibility.rs, icon_presentation.rs}`. 5a's summary implied six; the map itself lists nine. Task 5.4 counts them in the artifact so the matrix row states the number that is actually true. |

`row_factory.rs` is classified a called presentation surface **precisely so** no role
move touches the `GtkTreeExpander` internal-gesture disable at `:324-343` — the
three-iteration lesson `.agents/rules/ui.md` records.

## 2.3 The `journal` verdict for workspace persistence — CONFIRMED

Slot 3a's test re-applied: *does a later stage of the same workflow read the record
back, and is that read-back recovery from a failure or an ordinary next-launch load?*

`workspaces.json` is **not** a journal. Confirmed from the code: there is no
generation in the file, no stale-record cleanup, a failed write leaves the previous
file intact, and the retry ladder awaits **explicit** user retry rather than
recovering a record. The read-back is an ordinary next-launch load. The role is
therefore `execution` with latest-generation supersession, named
**`persist_execution.rs`**. Verdict unchanged; not re-litigated.

Applied once to the **expansion state**: it is **not** persisted with the workspace
file. `expanded_paths` is in-memory live state on the section imp struct
(`workspace_section/imp.rs:263`) and does not survive process exit, so the `journal`
question does not arise for it. Recorded so a reader does not go looking.

## 2.4 Excluded scope — CONFIRMED

| Boundary | Confirmation |
| --- | --- |
| `WFR-SHELL-LAYOUT` (slot 7) keeps the sidebar show/hide animation and its `workspace-sidebar-animation` blocker | Confirmed. The blocker **follows the animation, not the row name** — 5a's reusable form of "a field whose name contains *save* is not thereby save-workflow state". |
| `WorkspaceSidebarWidthPreset` is that row's value and leaves this facade | Confirmed; 103 lines at `mod.rs:180-282`, moving to `ui/sidebar/width_preset.rs` as **cross-cutting**. Three consumers outside this row are re-pointed by a **path edit proved by compilation**: `ui/preferences/imp.rs` (4 references), `ui/window/adaptive_shell.rs` (12), `ui/window/imp.rs` (6). `adaptive_shell.rs` is `WFR-SHELL-LAYOUT`'s own file, so the touch stays a path edit and is not a restructuring. |
| `ui/sidebar/file_tree_item.rs` has no coordination tier | Confirmed still listed under the matrix's *Surfaces With No Coordination Tier*. Its 150 production lines are **not** counted in this row. It is nonetheless crossed by the row's stage trace via `pending_rename` (`:35`). |
| `WFR-MINIMAP` (slot 6) keeps its four `ui/automation.rs` reach-throughs | Confirmed; re-derived line numbers recorded in task 6.7's evidence rather than copied from 5a's handoff. |
| `ui/window/startup_data.rs` stays **cross-cutting** (5a's option (3)) | Confirmed, even though it calls `sidebar.load_workspaces()`. Not absorbed. |
| The ten shared services **stay**; a `services -> ui` relocation is **forbidden outright** | Confirmed, not re-opened. `services/file_tree.rs`, `workspace_manager.rs`, `workspace_watch.rs`, `file_peek.rs`, `migration_ledger.rs`, `single_flight.rs` keep their behavior and their homes. |

## 2.5 Closed boundaries — CONFIRMED NOT RE-OPENED

Recorded so a reader does not think a question is open:
`model/workspace_search.rs`, `model/file_load.rs`, `model/buffer_replacement.rs`,
`model/editor_memory.rs`, `model/migration_ledger.rs`, `ui/plain_disposal.rs`,
`ui/buffer_snapshot.rs`, `services/single_flight.rs`, and `services/sync.rs` are all
untouched by this change and their ownership is unchanged.

`model/workspace.rs` (799 raw) is confirmed **domain and staying**.

## 2.6 Task 0.4's stage-trace verdict, because it changes section 5

Recorded in full in `evidence/stage-traces.md`. The three consequences the task list
predicted all materialized:

1. **The floor correction is wider than inherited.** Re-derived: **12 stage orders,
   28 deferral primitives, 16 non-primitive callback resumptions = 44 resumption
   points**, against the matrix's `Workflow Stage Traces` floor of **five
   inversions** — a correction factor of **8.8×** (5a computed 7.6× from 38), still
   the widest in the programme.
2. **The facade narration budget moves**, folded into `facade-measurements.md`.
3. **`list_execution.rs` does not own folder membership** — see 2.2's delta table.

Six places slot 5a's attribution was wrong, all in the direction of **more**, are
enumerated in the stage-trace evidence. Both of 5a's named reconciliation moves are
**confirmed** (`tree_loading.rs:143` is genuinely the DnD hover shield;
`folders.rs:471` is correctly named once as shared — though shared across four
orders, not two).

## 2.7 A contract finding, fixed in-stream

`.agents/rules/ui.md` named **two** deferred-restore functions that must read
`expanded_paths` at apply time. Only one exists:
**`restore_materialized_state` has never existed in the codebase.** `git grep` over
tracked sources returned the rule itself, slot 5a's archived snapshot, and this
change's own task list — a phantom symbol propagated across at least two changes
without anyone grepping for it. The nearest real name,
`refresh_materialized_view` (`workspace_section/refresh.rs:303`), is **synchronous**
and never touches `expanded_paths`.

Per `.agents/rules/preexisting-blockers.md` this is fixed **in this change**:
`.agents/rules/ui.md`'s Live expansion state bullet now names the one real
implementing site, states the placement obligation (the `expanded_paths` borrow lives
**inside** the deferred closure, not in the scheduling scope), and deliberately names
no file, because the owning coordination module is renamed by workflow migrations.
The real obligation is **satisfied** by the current code at
`tree_loading.rs:1244`.

Two task premises were corrected from the same pass: the second rename-entry cleanup
loop is in **`connect_unbind`** (`row_factory.rs:391-406`), not `connect_bind`, and
its six extra resets are the **unbind-side reset of state `connect_bind` sets
affirmatively** — so a move must preserve the asymmetry rather than "fix" it.

## 2.8 Task 0.10 — no other slot's deferred work has migrated here

Confirmed by path: slot 4's three B.3 simplify candidates live in
`ui/window/drafts/journal.rs`, `ui/window/local_history/preview_execution.rs`, and
`ui/window/imp.rs`; `git grep -l current_window_width -- crates/lushtext-core/src/ui/sidebar/`
returns nothing. Slot 4's two `[~]` items and slot 5a's `[~]` live and manual proof
remain theirs or user-gated. **Neither ticked nor re-planned here.**

---

# Implementation outcome: what the structural migration actually did (section 5)

## Deviations from the inherited module map, each with its reason

Three, all found by reading code the map described rather than by re-planning:

### 1. `tree_index.rs` dissolves to **one** destination, not two

**Recorded**: pure index arithmetic → `policy.rs`; child-store lookup and cache
maintenance → `scan_execution.rs`.

**Actual**: **entirely → `scan_execution.rs`.** Re-reading the file found **no pure
arithmetic left in it**. The splice planning, the reconciliation plan, and the common
prefix/suffix logic already live in `services::file_tree`; what remained was one
`impl LushtextWorkspaceSection` block of GTK-touching cache and expansion-set
maintenance (`ItemLocation`, `gio::ListStore`, `TreeListRow`) plus three free helpers.

The recorded destination was written from the module's *name* and doc comment, which
described "path/index bookkeeping" — accurate, but bookkeeping over GTK collections is
not pure arithmetic. **The lesson is narrow and reusable: a dissolution plan derived
from a module's own doc must be re-derived from its imports before it is executed.**

### 2. `watch_targets.rs` is **partially** dissolved, and is recorded as such

Its two generation newtypes moved to `seams.rs` — which also closed an encapsulation
gap, because the mirror bookkeeping had been advancing a generation by writing its tuple
field directly, and the move made that a privacy error resolved with a `next()` method.

Its **mirror arithmetic and snapshot stayed.** Moving 244 production lines of
incremental splice/contribution accounting into `policy.rs` would have pushed that file
further past its size target for no readability gain, and the snapshot's only consumer
is `watch.rs`. Recorded as **partially dissolved** rather than claimed as done.

### 3. `workspaces.rs` dissolved too — an addition, not a correction

The inherited map named `list_execution.rs`, `persist_execution.rs`, and
`filter_execution.rs` at the canonical home without saying what became of the 864-line
`workspaces.rs` they came from. The answer is that it dissolves **entirely** into
**four** `execution` roles: those three plus `membership_execution.rs`, which task 0.4's
twelfth-stage-order verdict requires. 800 of its 865 lines partitioned cleanly across
the four with **zero overlap**, verified programmatically before the split ran; the
remaining 65 were its header, imports, and `impl` wrapper.

## What the nested role home was like to adopt, as its first user

**It read well, and the reason is specific**: the canonical home holds the things a
reader needs *once* (the facade's narration, the one `policy.rs`, the one `evidence.rs`,
the seam types), while the nested directory holds the things a reader needs *per
section*. The split matches how the code is actually read.

Two frictions worth handing on:

1. **Visibility churn.** Seam and evidence types at the canonical home are reached from
   the nested directory, and `pub(super)` inside `seams.rs` means "visible to
   `ui::sidebar`" — which descendants can then see, so `pub(super)` is usually right and
   `pub(crate)` is usually over-wide. Two of the moved items needed `pub(crate)` only
   because a *test* crate reads them; `WorkspaceTreeEvidence` needed full `pub` for the
   same reason, matching `NotesEvidence`'s precedent.
2. **`super::` path renames dominate the diff.** Renaming five nested modules produced
   ~25 mechanical `super::old::` → `super::new::` edits across siblings. They are noise
   in review, and they are why `row_factory.rs`'s diff had to be checked explicitly to
   confirm the `GtkTreeExpander` block itself was untouched.

## One size regression, recorded rather than hidden

`workspace_section/scan_execution.rs` is **2,000 production lines** — twice the ~1000
target and the worst readability outcome in this change.

It is accepted debt for a stated reason: the alternative was inventing a role name for
`tree_index.rs`'s pre-convention *topic* (an `index` role), which **this change's own
amendment forbids**. Splitting the cache maintenance away from the child-store
materialization that owns it would also separate the deferred expansion restore from the
expansion set it restores, on a `tier-3` workflow, for a line count.

Reducing it is named follow-up in the matrix row and on the slot ledger. The most
promising split, if a later slot wants it, is the expansion-state authority group
(`derive_expanded_paths_from_model`, `save_expanded_paths`,
`record_row_expansion_transition`, `reconcile_expanded_subtree_from_model`,
`rename_expanded_subtree`, ~160 lines) into `folder_execution.rs`, which already owns
folder rows and drilldown — but that separates the set from its deferred restore, so it
needs the behavior-equivalence battery run first, not after.
