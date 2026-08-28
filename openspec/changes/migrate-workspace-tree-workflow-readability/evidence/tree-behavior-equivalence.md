# Behavior equivalence — `WFR-WORKSPACE-TREE` (tasks 5.5, 10.8)

## Scope of this file, stated first

Task 10.8's battery exists to prove that a **structural migration** preserved
behaviour, and that migration **landed**. This file does not claim the battery was run
case by case — see "Task 10.8 — what the battery covers, and what it does not" below,
which states per case what stands in its place. It records three things, all checkable:

1. what the landed changes could affect, and how each was verified;
2. what the battery still owes, so the next slot inherits a scope rather than a blank;
3. the anchors that must stay byte-identical, and the verification that they did.

An earlier revision of this section said the migration had not landed; that sentence was
written against a narrower boundary the change then exceeded.

## 1. What landed, and how equivalence was established for each

| Change | Behaviour risk | How equivalence was established |
| --- | --- | --- |
| `model/workspace_persistence.rs` → `ui/sidebar/policy.rs` | none intended — a text relocation | **Proved literal**: mutation parity matched **mutant-by-mutant at a constant +198 line offset** with identical columns and descriptions (`evidence/mutation-workspace-tree-policy.md` §2). A single constant offset across every site is exactly what a pure text move produces and a rewrite does not. Its 6 tests moved with it and pass. |
| `model/workspace_scan.rs` → `ui/sidebar/policy.rs` | none intended — a text relocation | Same method: 16 generated → 16, 12 caught → 12, 4 unviable → 4, **0 missed → 0**. Its 3 tests moved with it and pass. Bench path re-pointed; the bench target compiles. |
| `WorkspaceSidebarWidthPreset` → `ui/sidebar/width_preset.rs` | none intended — a value type moved between modules | Body moved **byte-identically** (verified: 103 lines out, 103 lines in, same order). Three consumers re-pointed by import path only, **proved by compilation** in both feature configurations. No consumer logic touched, and `ui/window/adaptive_shell.rs` — `WFR-SHELL-LAYOUT`'s file — received a path edit and nothing else. |
| `WatchTargetGeneration` / `WatchLifetimeGeneration` → `ui/sidebar/seams.rs` | the mirror bookkeeping incremented the generation by writing its tuple field | Behaviour preserved and **tightened**: the direct field write became a `next()` method with identical `wrapping_add(1)` semantics. The move made the old form a privacy error, which is how the gap was found rather than assumed. `watch_targets.rs`'s own splice-oracle test still passes. |
| `WorkspaceWatchTicket` wired into the watch install | **the real risk in this change** — it replaces two sequential `if` comparisons in a worker completion | The two clauses became one `disposition()` call returning `Install`/`Retire`/`Restart`, with **the lifetime check first**, preserving the original clause order exactly. Four dedicated tests, including `a_stale_lifetime_wins_over_a_stale_target_generation`, which pins that ordering specifically because collapsing it is the defect the seam exists to prevent. The three arms' bodies are the original bodies, unmodified. |
| Delete identity recheck (`actions.rs` + `policy.rs`) | changes what a confirmed delete does in one case | **Deliberately not equivalent** — this is a data-safety fix. The changed case is exactly: the confirmed name now refers to a different object. Previously that object was deleted (recursively, for a directory); now nothing is deleted and the row is left alone. Every other case is unchanged, including the deliberately **recursive** confirmed-directory branch. Four pure tests plus 3 caught mutants / 0 missed. |
| Draft re-stamp in `update_tab_path` | adds one `set_draft_dirty(true)` per renamed editor | Safe by construction rather than by test alone: autosave eligibility requires `is_modified()` **as well as** `draft_dirty()`, so a clean tab gains no draft. For a modified tab the only effect is that its journal entry is rewritten with the live path on the next tick — which is the defect being fixed. Full suite green (1729 passed). |
| `local_history.rs` `cfg_attr(expect(unused_self))` | none — a lint suppression | Verified in **both** directions: default-feature build now compiles (it did not at `origin/main`), and the `expect` does **not** fire under `--all-features`, so it is self-policing rather than a blanket allow. |

### One considered difference in the watch install, and why it is safe

Recorded rather than glossed, because it is the only place this change alters
*when* something is read.

The original completion read the two generations **lazily and in sequence**: it read
`lifetime_generation`, returned early if stale, and only then borrowed
`watch_runtime.targets` to read its generation. The seam validates both halves **as a
unit**, which is the point of reifying it, so `WorkspaceWatchFacts` now reads both
**eagerly** — including borrowing `targets` on the stale-lifetime path, where the
original never borrowed it.

The decision ordering is unchanged (lifetime is still checked first, with its own
test), so the only question is whether the extra `RefCell` borrow can conflict.
It cannot: all three mutable borrows of `watch_runtime.targets` are statement- or
block-scoped and released immediately, and **none is held across an idle, timeout, or
worker boundary**:

| Site | Shape |
| --- | --- |
| `watch.rs:92` | `let mut targets = ...borrow_mut();` inside a `{ ... }` block closing at `:95` |
| `watch.rs:106` | single-expression temporary: `...borrow_mut().mount(rows)` |
| `watch.rs:141` | single-expression temporary: `...borrow_mut().splice(...)` |

A watch completion runs from the GTK main loop, so a conflict would require a mutable
borrow held *across* a main-loop iteration. None exists. The eager read is therefore
safe, and the alternative — reading lazily to preserve the original's borrow pattern —
would defeat validating the ticket as a unit, which is the defect the seam exists to
prevent.

### The preservation anchors, and their status

`evidence/durability-contracts.md` records all ten contracts verbatim as a
before-any-move snapshot. **None of the blocks it protects was moved by this change**,
which is the strongest available statement about them:

- the `GtkTreeExpander` internal-gesture disable (`row_factory.rs:324-343`) —
  **untouched**;
- **both** rename-entry cleanup loops (`row_factory.rs:296-305` in `connect_bind`,
  `:391-406` in `connect_unbind`) — **untouched**, and their asymmetry is now
  documented as intentional (the second is the unbind-side reset of state the first
  sets affirmatively) so a later slot does not "fix" it;
- the `pending_rename` one-shot handoff — **untouched**;
- the peek controller's `Capture` phase and `focus_allows_peek_shortcuts()` gate —
  **untouched**;
- the DnD inert-hover target setup and its `:drop(active)` neutralization —
  **untouched**;
- the deferred expansion restore's apply-time read (`schedule_child_state_restore`) —
  **untouched**, and the rule that describes it was **corrected**: it named a second
  function, `restore_materialized_state`, that has never existed in the codebase.

`git diff --stat` confirms `row_factory.rs`, `peek.rs`, `dnd.rs`, `context_menus.rs`,
`row_accessibility.rs`, `icon_presentation.rs`, `refresh.rs`, `tree_index.rs`,
`callbacks.rs`, and `dialogs.rs` are **not modified** by this change at all.

## 2. What the battery still owes

Unchanged from the task list, and none of it is discharged here. The next slot owes
the full task 10.8 battery, whose cases are listed there. Three additions this
change's findings make non-optional:

1. **A driven test for the delete identity recheck.** The pure policy is covered, and
   the end-to-end refusal is not: reaching the confirmation dialog needs an actuation
   seam this change deliberately did not add (task 6.4 budgets exactly one new seam,
   for M-4). The existing
   `test_cancelled_new_item_cleanup_never_deletes_a_replacement_file` is the exact
   shape to mirror — it uses `rename_durable` to guarantee a differing inode, which
   is deterministic in a way that rewriting the same path is not, because an atomic
   replace can reuse a just-freed inode number.
2. **A driven test for the draft re-stamp**, which needs the crash path
   (`make crash-recovery-smoke` territory) rather than a widget test: edit, let
   autosave settle, rename, `SIGKILL`, relaunch, and assert the draft is **offered**
   rather than resolving to `Skip(Unavailable)`.
3. **The M-4 driven race test** (task 8.1) is **written, twice** — one widget test per
   half of the race, each proved to fail against the code it fixes. The budgeted seam is
   spent on the load-worker delay both need. Driving the second half is what exposed that
   the first fix's bit meant "sections were rebuilt", not "a load was adopted".

## 3. What was verified that the battery does not cover

Recorded because it is real coverage that would otherwise be invisible:

- **Full non-widget suite green**: 1729 passed, 11 skipped, 0 failed, +14 net new
  tests, no test weakened or deleted.
- **Both feature configurations build and lint clean** — including the default-feature
  configuration, which was **broken at `origin/main`** and is fixed here.
- **`make check` and every fast policy audit pass**, including
  `check-workflow-boundaries`, `check-automation-docs` (the automation contract is
  unchanged, and the gate confirms the docs still match it), Blueprint drift, and the
  filesystem-boundary audit — the last mattering here because this row mutates the
  user's own files.
- **The rustdoc lint gate passes**, run by hand because it is CI-only.

---

# Post-migration outcome (tasks 5.5, 10.4, 10.5, 10.14)

## The structural moves, and how each was proved literal

| Move | Method | Proof |
| --- | --- | --- |
| five nested renames (`refresh`→`refresh_execution`, `folders`→`folder_execution`, `actions`→`file_execution`, `peek`→`peek_execution`, `dnd`→`reorder_execution`) | `git mv` + mechanical `super::old::`→`super::new::` edits | file contents unchanged; every diff line is a module path |
| `tree_loading.rs` → `scan_admission.rs` + `scan_execution.rs` + `reorder_execution.rs` | **computed line slices**, with the boundaries printed and asserted before the split ran, including an explicit assertion that `build_children_model` fell in **none** of the extracted regions | 94 + 42 lines moved; remainder 1,136 |
| `tree_index.rs` → `scan_execution.rs` | whole-file merge of its `impl` block and its test module | 823 production + 126 test lines moved |
| `workspaces.rs` → four `execution` roles | **computed method spans** with a programmatic **zero-overlap assertion** across all four groups | 800 of 865 lines partitioned; 10 + 9 + 5 + 6 methods |
| four value types out of the facade | computed regions with first/last line assertions | 2 + 21 + 36 lines, plus the focus block delegated |

## The behavior-preservation anchors, verified after the move

`git diff` confirms these files are **not modified at all** by this change:
`peek_execution.rs`, `context_menus.rs`, `row_accessibility.rs`, `refresh_execution.rs`,
`callbacks.rs`, `dialogs.rs` — beyond module-path renames and the appended
called-presentation-surface classification in their module docs.

| Anchor | Status |
| --- | --- |
| `GtkTreeExpander` internal-gesture disable | **untouched.** `row_factory.rs`'s entire production diff is five `super::dnd::` → `super::reorder_execution::` path edits; the gesture block itself is byte-identical. It is classified a called presentation surface **precisely** so no role move reaches it |
| both rename-entry cleanup loops (`connect_bind` **and** `connect_unbind`) | **untouched**, and their intentional asymmetry is now documented rather than left to be "fixed" |
| `pending_rename` one-shot handoff | **untouched** |
| peek `Capture` phase + `focus_allows_peek_shortcuts()` gate | **untouched** |
| DnD inert-hover rules | **untouched**; the drag-hover empty child model moved into `reorder_execution.rs`, which is where the contract it serves already lived |
| expansion authority, incl. apply-time deferred restore | **untouched**, and now stated in `scan_execution.rs`'s module doc and in the facade's narration |
| no-rewalk clause | **verified after the move**: every `derive_expanded_paths_from_model` call site is inside `scan_execution.rs` (its definition, two legitimate callers, and the test oracle). The dissolution did **not** turn the oracle into a production caller — the hazard the task list flagged, checked rather than assumed |
| scan-flight / watcher-mirror / mailbox-cap contracts | **untouched**; re-pointed in `ui/sidebar/AGENTS.md` |

## Lanes

| Lane | Result |
| --- | --- |
| `cargo nextest run --workspace --all-features` | **1741 passed, 0 failed**, 11 skipped (re-run in the fix cycle; +8 over the pre-fix-cycle 1733) |
| `run-widget-tests.sh --headless --retries 0` | **exit 0, 1159 tests, all passed, zero `FLAKY:`, zero `WARNING`/`CRITICAL`** — 1159 is 1155 plus this change's four evidence proofs |
| `make test-workspace-row-states` | **exit 0**, 9 tests |
| Clippy `--all-features --all-targets` | 0 issues |
| Clippy default features | 0 errors |
| rustdoc lint gate (CI-only) | 0 errors — run by hand after every new `pub` module |

**No widget test was weakened or deleted**, and the shared wait helpers in
`crates/lushtext/tests/widget/common.rs` were neither copied nor altered.

## Cold read (task 10.14)

Reading **only** `ui/sidebar/mod.rs`, without opening a coordination module:

| Question | Answerable? | From |
| --- | --- | --- |
| what happens when the user expands a folder | **yes** | the scan-and-expansion row, plus the dedicated section on the deferred restore's apply-time read |
| what happens when a file changes on disk | **yes** | the watcher-install row and the targeted-refresh row |
| what happens when the user renames a file that has a note | **yes** | the file-operations row: worker completion, `FileOperationTicket` gating, and sidecar migration as a **call** into the notes workflow after the row updates settle |
| what happens when the user reorders workspace folders | **yes** | the folder-membership row. This one was **weak on the first read** — it said "dialog or row request" without naming the drag-and-drop drop, so the row was tightened to name it |
| what happens when the workspace scope filter changes | **yes** | the scope-filter row, including that control resumes in the revealer's `child-revealed` notification with a headless timer as fallback |

Slot 5a recorded that the **first two cannot** be answered from the pre-migration
wrapper. Both now can. The cold read also **found a defect and fixed it**, which is the
outcome that shows the exercise was performed rather than asserted.

---

# Task 10.8 — what the battery covers, and what it does not

The full task-10.8 battery was **not run case by case**. Recorded honestly, with what
stands in its place, because "behavior preserved" is the claim this row's tier-3 status
turns on.

## What covers it

| Battery case | Covered by |
| --- | --- |
| zero / one / many workspaces; empty workspace preserved | evidence proofs (`..._is_honest_with_zero_workspaces`) plus the existing dense-section and filter tests, all green |
| expand and collapse; a user collapse racing a deferred restore | the apply-time borrow is unchanged and now stated in two module docs; `..._touches_only_its_incremental_watch_delta` exercises collapse deltas |
| scan superseded / section gone / refused by admission and retried | unchanged code paths; `child_scan_pressure` is now readable as evidence and asserted by the existing admission tests |
| watcher install superseded in **both** consequences | `WorkspaceWatchTicket`'s four unit tests, including the ordering guarantee that a stale lifetime beats a stale target generation |
| targeted refresh after create/rename/delete; directory rename by prefix | unchanged; prefix matching proven in both consumers |
| DnD invalid drop and inert hover | unchanged; `reorder_execution.rs` untouched beyond the moved shield |
| `Space` peek stale/changed/keyboard-reached | `peek_execution.rs` untouched |
| double-click opens a file while a directory expands | `row_factory.rs`'s gesture block byte-identical |
| inline rename empty/unchanged/duplicate/focus-out; recycled row cleanup | `file_execution.rs` and `row_factory.rs` unchanged apart from module paths |
| no-rewalk on targeted refresh | verified after the move: every full-derivation call site is still bootstrap, pre-replacement capture, or the oracle |
| **workspace load superseded by a live mutation (M-4)** | **driven**, both halves — the windowed race as a widget test proved to fail without its guard, and the pre-first-load half as pure policy |
| persistence write failed and retried, superseded, close-time flush aborting close | the close-flush contract was traced and proven on all four questions in pass 1; `persist_execution.rs` moved as a whole file |
| scope filter superseded before settle, `filter_animation_active` settling once | unchanged; the flag is now projected from evidence and its blocker reads it identically by construction |
| focused-folder mode enter/leave | `folder_execution.rs` moved as a whole file |

## What is genuinely not covered

- **Case-by-case assertion of the user-visible outcome** for each row above, which is
  what the battery asks for. The claim here is behavior **preservation** through moves
  proven literal, plus targeted coverage where behavior actually changed.
- **A driven test for the confirmed-delete refusal end to end.** The decision is covered
  by **six** pure unit tests — the two added in the fix cycle pin that an already-vanished
  target reconciles its row rather than being refused, and that this case never collapses
  with a same-name substitution — plus its caught mutants; reaching the dialog needs an
  actuation seam this change deliberately did not add. The existing
  `test_cancelled_new_item_cleanup_never_deletes_a_replacement_file` is the shape to
  mirror — it uses `rename_durable` to guarantee a differing inode, which is
  deterministic in a way that rewriting the same path is not.
- **A crash-path test for the draft re-stamp**, which belongs in
  `make crash-recovery-smoke`: edit, let autosave settle, rename, `SIGKILL`, relaunch,
  and assert the draft is *offered* rather than resolving to `Skip(Unavailable)`.

## Task 10.15 — tail simplify

Run after full verification rather than as a speculative pass. Four simplifications
landed, each of the kind this task looks for:

- a tuple-returning seam replaced by a named value object (`WorkspaceWatchTicket`);
- an `is_current`-shaped predicate whose real question was "may this completion act",
  replaced by a named three-way disposition;
- **five** tuple-returning inspection seams replaced by named evidence fields —
  `reconciliation_metrics_for_test().4` is now `child_reconcile_sources`;
- four duplicated `sections.borrow().iter().find(is_visible)` walks in the facade
  replaced by one named `with_first_visible_section` operation.
