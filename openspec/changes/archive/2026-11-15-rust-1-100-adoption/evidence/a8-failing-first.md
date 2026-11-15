# A8 failing-first evidence

Tree: branch `rust-1-100-adoption` at `dadf35cc` plus the tests below and the
test-only hooks they need (task 1.1's `ErrorKind` parameter on
`fail_next_parent_sync_for_test`, and `next_no_replace_unsupported_for_test`,
`pub(in crate::services)`), with **no** production change. The tests were
written against the unfixed signatures (`io::Result<()>`,
`Result<(), RenameFailure>`), so they fail by assertion, not by compilation.

Command (through `scripts/heavy.sh`, CPUs 8-23):

```
cargo nextest run -p lushtext-core --lib --no-fail-fast -E 'test(...) | ...'
```

## Before the fix

| Test | Result on the unfixed code | What it shows |
| --- | --- | --- |
| `services::filesystem::write::tests::an_unsupported_sync_after_the_rename_is_not_a_name_collision` | **FAIL**: `assertion left != right failed: entity already exists; left: AlreadyExists` | the defect: an `Unsupported` directory sync after a completed rename comes back as `AlreadyExists` |
| `services::draft_service::set_aside::tests::a_move_whose_directory_sync_is_unsupported_keeps_the_first_name` | **FAIL**: `failed to set aside draft …/drafts/a8-draft.draft as …/set-aside/a8-draft.42-1.draft: No such file or directory` | set-aside took the false `AlreadyExists` for a collision, tried the next name with the source already gone, and failed |
| `ui::sidebar::workspace_section::file_execution::tests::a_rename_whose_directory_sync_fails_is_a_completed_rename` | **FAIL**: `Unsupported: refused: A file named 'new.txt' already exists in this folder` and `Other: reported as failed: injected parent directory sync failure` | both sidebar gaps: the false "already exists" warning, and the wider one where any post-rename durability error is projected as a failed rename |
| `services::filesystem::write::tests::any_sync_failure_after_the_rename_reports_the_moved_file` | PASS | the old `io::Result` cannot express the class, so this case's failing-first proof is the sidebar test's `Other` row; after the fix it asserts `AfterRename` |
| `services::filesystem::write::tests::the_fallback_rename_classifies_its_sync_failure` | PASS | as above: the old fallback leg returned the plain sync error; after the fix it asserts `AfterRename` |
| `services::filesystem::write::tests::an_existing_destination_is_refused_by_either_route` | PASS | **regression test**, expected to pass before and after |
| `services::durable_write::tests::move_durable_keeps_its_source_when_the_destination_is_not_durable`, `atomic_write_parent_sync_failure_is_after_rename` | PASS | existing callers of the injection, now passing `ErrorKind::Other` |

Summary line: `8 tests run: 5 passed, 3 failed`.

## After the fix (task 2.8)

Same tests, with the tests retargeted at the classified types
(`DurableWriteError::AfterRename`/`BeforeRename`,
`Ok(RenameDurability::Unconfirmed(_))`, the sync's own kind kept), plus the
new core and policy tests. Command: `cargo nextest run -p lushtext-core --lib`
over `services::filesystem::`, `services::durable_write::`,
`services::draft_service::`, `ui::sidebar::`, `local_history_service::` and
`content_search::replace::`.

`Summary: 379 tests run: 379 passed, 1733 skipped`, including:

- `services::filesystem::write::tests::an_unsupported_sync_after_the_rename_is_not_a_name_collision` — PASS (`AfterRename(Unsupported)`)
- `services::filesystem::write::tests::any_sync_failure_after_the_rename_reports_the_moved_file` — PASS (`AfterRename(Other)`)
- `services::filesystem::write::tests::the_fallback_rename_classifies_its_sync_failure` — PASS (`AfterRename(Unsupported)` through the fallback leg)
- `services::filesystem::write::tests::an_existing_destination_is_refused_by_either_route` — PASS (`BeforeRename(AlreadyExists)`, both files untouched)
- `services::draft_service::set_aside::tests::a_move_whose_directory_sync_is_unsupported_keeps_the_first_name` — PASS
- `ui::sidebar::workspace_section::file_execution::tests::a_rename_whose_directory_sync_fails_is_a_completed_rename` — PASS (both kinds `Unconfirmed`)
- `services::filesystem::write_protocol::tests::{a_rename_syncs_each_directory_it_mutated, a_failed_rename_step_is_before_rename, a_failed_directory_sync_after_a_rename_is_after_rename}` — PASS
- `ui::sidebar::policy::tests::{a_confirmed_rename_owes_no_warning, an_unconfirmed_rename_warns_with_the_new_name}` — PASS
