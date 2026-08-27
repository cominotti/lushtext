# Behavior equivalence and regression proof, `WFR-WORKSPACE-TREE` (task 4.9, partial)

**Scope, stated first so this file is not read as more than it is.** The workspace
tree's *structural migration* moved to slot 5b, so task 4.9's full state-extreme
battery is **not** discharged here. What this file records is the behavior proof
for the **four tree-side data-safety fixes that did land**, plus the equivalence
argument for the parts of the row this change touched at all.

The row's own file operations were the only tree behavior this change altered.
Everything else in `ui/sidebar/**` is untouched: `mod.rs` gained three module
declarations and one re-export pair, `workspace_section/mod.rs` widened one
method's visibility, and `workspace_section/watch.rs` gained one named repair
operation. `git diff` on `workspaces.rs` was empty until the M-4 fix below.

## The four landed fixes, each proved to fail without its fix

Method: apply the test, run it headless at `--retries 0`, then revert **only the
fix** (leaving the test) and re-run. A test that passes both ways is not coverage,
and two of these initially did — see the seam note at the end.

| Id | Test | With fix | Fix reverted |
| --- | --- | --- | --- |
| **C-1** rename silently destroyed an existing file | `workspace_section::test_inline_rename_refuses_to_replace_an_existing_sibling` | **pass** | **fail** — `condition was not met within 10s`: no refusal is ever published, because the rename succeeded and destroyed `final.md` |
| **M-1** completion re-read the live `context_target` | `workspace_section::test_inline_rename_completion_ignores_a_row_retargeted_mid_flight` | **pass** | **fail** — `the retargeted row must keep its own path`: the renamed path was written onto the bystander row's item |
| **M-2** detached path-only placeholder delete | `workspace_section::test_cancelled_new_item_cleanup_never_deletes_a_replacement_file` | **pass** | **fail** — the file renamed onto the placeholder's name is unlinked |
| **P2-1** two-guard self-deadlock introduced by C-1's own fix | `workspace_section::test_inline_rename_of_a_symlink_onto_its_target_refuses_without_hanging` | **pass** | (pre-fix code deadlocked a worker permanently; the test's second, ordinary rename is the pool-exhaustion detector) |

**H-5** (rename and delete bypassing `TargetWriteGuard`) is proved **by
construction plus existing coverage**, not by a race test: `rename_target_guarded`
and the delete worker now acquire the guard, and `services/durable_write.rs`'s own
guard tests cover the exclusion contract. Recorded honestly as covered-by-construction
rather than claimed as a driven race.

**M-4** (workspace load clobbering a pre-load mutation) landed with the
load-generation guard in `ui/sidebar/workspaces.rs`. Its proof is the guard's own
shape — the adoption is skipped when `requested_generation()` moved between
dispatch and completion — plus the unchanged pass of the existing workspace
lifecycle suite. **No driven race test**: forcing a "New Workspace" between the
load dispatch and its completion needs a load-worker delay seam, and this change
had already added two counted seams. Recorded as the highest-value remaining test
for 5b.

## Equivalence for the untouched tree behavior

| Case | Evidence |
| --- | --- |
| The `GtkTreeExpander` internal-gesture disable for file rows | `row_factory.rs:325-343` byte-identical, verified by diff. The module is classified a **called presentation surface** precisely so no role move touches it |
| The peek key controller's `Capture` phase and its `focus_allows_peek_shortcuts()` gate | `peek.rs` unchanged |
| Scan flight, watcher mirror, mailbox cap, DnD shield, expansion authority | all five `ui/sidebar/AGENTS.md` local contracts unchanged; files untouched |
| Create's unique-name policy, rename's empty/unchanged cancellation, the focus-out double-fire guard, prefix matching for directory ops | relocated into `policy.rs` as pure functions with the same literals, unit-tested across the arities; the guard and the prefix call sites are unchanged |
| Workspace persistence state machine, debounce window, close-time flush | `model/workspace_persistence.rs` unchanged and **not relocated** — the relocation is 5b's |
| Focused workspace file-row state | `make test-workspace-row-states` clean |
| Full tree widget coverage | `crates/lushtext/tests/widget/workspace_section.rs` (123 pre-existing tests plus 4 new) passing at `--retries 0` |

## The two counted seams, and why they were unavoidable

`set_workspace_rename_worker_delay_for_test` and
`set_workspace_placeholder_cleanup_delay_for_test`. Without them the M-1 and M-2
tests **passed against the broken code**: the worker won the race a headless test
could set up, so neither test could distinguish fixed from unfixed. Both live in
`ui/sidebar/test_policy.rs`, entirely behind `#[cfg(feature = "test-utils")]`,
each documented with that justification at its definition.
