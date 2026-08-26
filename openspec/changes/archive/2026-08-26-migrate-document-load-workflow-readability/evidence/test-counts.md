# Test counts before and after

The completion rule requires that a migration not decrease the project test
count. Consolidating scattered `*_for_test` getters into one evidence surface
retires *functions*, not *tests*, so the count must hold or rise.

## Counting method

The same three independent measures slot 3a used, so the two changes are
comparable and a movement in one can be cross-checked against the others:

1. **Executed non-widget tests** — `cargo nextest run -p lushtext-core
   --all-features`, reading the run summary.
2. **Declared `#[test]` attributes in `lushtext-core`** —
   `grep -rh '#\[test\]' crates/lushtext-core/src crates/lushtext-core/tests | wc -l`.
3. **Declared `#[test]` attributes in the widget harness** —
   `grep -rh '#\[test\]' crates/lushtext/tests/ | wc -l`.

**"Before" was measured in a throwaway `git worktree` at `HEAD`**, not by
stashing the working tree. This change adds whole new directories
(`ui/editor_page/load/`) and deletes two files, so a stash would have had to move
untracked directories and staged deletions together; a detached worktree at the
same commit gives the same answer with no risk to the in-flight tree. Both sides
used the same toolchain.

## Results

| Measure | Before | After | Delta |
| --- | --- | --- | --- |
| Executed `lushtext-core` tests | 1,389 | 1,400 | **+11** |
| Declared `#[test]` in `lushtext-core` | 1,390 | 1,401 | **+11** |
| Declared `#[test]` in the widget harness | 1,198 | 1,209 | **+11** |

No measure decreased.

## Where the movement came from

**+11 in `lushtext-core`**, all in the new `ui/editor_page/load/policy.rs`, which
holds exactly 11 `#[test]` functions. None moved from anywhere: this module is a
coverage **gain from zero**, because `model/file_load.rs` stays in `model/` and
nothing relocated.

| Test | Covers |
| --- | --- |
| `ticket_is_current_only_when_generation_token_and_cancellation_all_agree` | all three clauses of the reified freshness seam, each failing on its own |
| `ticket_reports_cancellation` | the ticket's own view of its token |
| `installation_freshness_ignores_token_identity` | the deliberately weaker per-slice predicate, which must **not** compare token identity |
| `chunked_install_triggers_on_either_side_of_the_swap` | the install threshold from both the incoming payload and the existing buffer, including the exact-threshold boundary and a negative character count |
| `the_clear_slice_budget_matches_the_shared_replacement_budget` | pins the clear budget against `model/buffer_replacement`'s, closing the one mutation survivor |
| `clear_slices_are_bounded_and_stop_on_paragraph_boundaries` | the clear-slice budget and all four paragraph-extension cases |
| `slice_action_orders_terminal_disposal_and_staleness` | the scheduled-slice decision, including that a cancelled-clear phase is not re-aborted for being stale |
| `abort_action_protects_terminal_finalizing_and_repeated_cancellation` | every abort classification, including that disposal still retires a session already clearing |
| `a_failed_reload_over_loaded_content_keeps_the_loaded_state` | the failure-state rule for all four prior states |
| `user_cancellation_is_published_only_for_a_visible_interrupted_load` | the cancellation-publication rule across all four inputs |
| `load_outcome_defaults_to_none` | the terminal-outcome default |

**+11 in the widget harness**, and the split matters because only two of them are
convention bookkeeping:

**The reentrancy convention this change promoted (3 tests).**

- `editor_page::test_load_evidence_reads_stay_side_effect_free_across_load_mutation`
  — this workflow's required proof. It drives the workflow through each operation
  taking a mutable borrow of the state the accessor reads (identity rotation, the
  installation session, the parked request, cancellation), reads **after** each
  one, and asserts repeated reads of unchanged state are identical.
- `editor_page::test_load_evidence_reads_survive_widget_disposal` — the
  teardown-observation test the three sibling surfaces already had. No
  `LoadEvidence` field derives from a `TemplateChild`, and this proves it rather
  than asserting it.
- `command_palette::test_evidence_reads_stay_side_effect_free_across_palette_mutation`
  — written here to discharge the **retroactive-amendment** obligation. The
  palette had only a teardown test; see the matrix's slot 3b amendment re-check.

**Load behavior equivalence (8 tests).** These are the acceptance criterion for a
tier-3 migration whose non-goal is behavior change, so each asserts the
user-visible outcome and the resulting buffer content:

- `test_small_file_takes_the_direct_install_and_publishes_its_content`
- `test_empty_file_loads_to_an_empty_buffer_without_slicing`
- `test_a_paragraph_larger_than_the_slice_budget_installs_in_one_turn` — the
  paragraph-boundary contract, measured; see `install-slicing-linearity.md`
- `test_undecodable_bytes_in_the_requested_encoding_report_a_decode_failure`
- `test_missing_and_unreadable_paths_fail_without_publishing_content` — uses a
  directory as the unreadable target, so the failure path stays deterministic
  without depending on ambient Unix permissions, which CI may run as root and
  bypass
- `test_reopen_with_a_different_encoding_replaces_the_loaded_content`
- `test_a_newer_load_of_a_different_path_refuses_the_older_completion` — one of
  the three cases `LoadRequestTicket` protects
- `test_a_load_whose_editor_is_disposed_before_the_worker_returns_publishes_nothing`
  — another, and it also asserts the lane does not keep the disposed request's
  charge

3 + 8 = 11, and no assertion was folded into an existing test to make the
arithmetic work.

The remaining two cases from the verification matrix were already covered and
were confirmed rather than duplicated: a file large enough to require chunked
installation
(`test_large_unicode_load_installs_in_exact_bounded_slices`), and a load
cancelled mid-install whose partial content is cleared and payload retired
(`test_chunked_load_cancellation_clears_partial_text_and_releases_admission`).
Both pass unchanged, which is itself behavior-equivalence evidence.

## What was retired without losing a test

Ten `*_for_test` inspection surfaces were removed and **51 call sites** migrated
to fields on `LoadEvidence`; one actuation surface
(`load_runtime::reset_for_test`) was folded into the editor-page seam that shares
its mechanism; and two configuration statics became one test-policy value while
keeping both public setter names. **No assertion was dropped and no test was
deleted**, which is why the widget-harness count moves by exactly the 11 tests
added rather than falling. Details in
`widget-test-load-site-migration.md`.
