# Test counts (task 10.3)

The count must not decrease. Counting method: `cargo nextest run --workspace
--all-features -E 'not binary(widget)'` for the non-widget total, and the widget
harness's own reported test count for the widget total. Both are stated because
they are measured differently and mixing them has produced wrong figures before.

## Non-widget

| Tree | Tests run | Skipped |
| --- | --- | --- |
| `dfc42b6` (before), measured in a detached worktree | 1,670 | 11 |
| after | **1,713** | 11 |

**+43**, all newly added and all passing. Measured, not inferred: the baseline was
run from a `git worktree add --detach dfc42b6`, because a count derived by adding
up one's own new tests is exactly the arithmetic that has been wrong before.

Where they are: `ui/window/notes/policy.rs` and `ui/window/notes/seams.rs` hold
**29** between them (23 policy, 6 seams), `services/migration_ledger.rs` gained
**2** (the operation-lock singleton and the serialization proof, on top of its
existing 6), and `services/document_note_service.rs` gained **1** (the per-item
isolation regression, on top of its existing 11). The remaining **11** are the
`ui/sidebar/policy.rs` and `ui/sidebar/seams.rs` tests that landed with the
data-safety fixes. No test was deleted, weakened, or renamed away.

The four retired notes inspection functions had **no tests of their own** — they
were read by other tests, whose assertions moved to the evidence surface
unchanged — so the consolidation cost zero tests, which is what the
`workflow-evidence-surfaces` "No observable field is lost" scenario requires.

## Widget

Widget tests added by this change: **9**.

- `window::test_notes_evidence_reads_stay_side_effect_free_across_mutation`
- `window::test_notes_evidence_read_materializes_nothing_and_advances_no_generation`
- `window::test_notes_evidence_answers_honestly_for_a_disposed_window`
- `window::test_pending_bookmark_write_is_flushed_when_a_tab_detaches`
- `workspace_section::test_inline_rename_refuses_to_replace_an_existing_sibling`
- `workspace_section::test_inline_rename_completion_ignores_a_row_retargeted_mid_flight`
- `workspace_section::test_cancelled_new_item_cleanup_never_deletes_a_replacement_file`
- `workspace_section::test_inline_rename_of_a_symlink_onto_its_target_refuses_without_hanging`
- `window::test_bookmark_writes_wait_until_the_sidecar_has_been_read_back`

None removed. Four are **regression tests for confirmed pre-existing defects,
each individually proved to fail without its fix**; one
(`..._symlink_onto_its_target_refuses_without_hanging`) is a regression test for a
**deadlock this change's own first fix introduced**, found by the
post-implementation pass. See `data-safety.md`.

**Seven wait budgets corrected, not added.** The confirming full widget lane
reported one `FLAKY:` on
`window::test_workspace_row_state_window_updates_save_as_rename_and_delete`,
whose three Save As completions waited on `spawn_blocking_then` with a **3s**
budget where the documented floor for async waits is **5-10s**. All seven
Save-As-completion waits in `window.rs` were raised to 10s together — fixing only
the one that fired would have left six identical latent fragilities in the same
module. The change's own contribution to the timing was also removed: the note
resolution completion no longer re-arms the bookmark debounce when the live set
and the loaded sidecar are both empty, which had added a timer and a worker to
every file load and Save As in order to write nothing. Re-run in isolation and
then as a full suite: **zero `FLAKY:` lines, no retry relied upon.**
