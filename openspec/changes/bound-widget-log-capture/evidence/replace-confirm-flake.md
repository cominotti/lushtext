# A FLAKY in the final-tree `make test`: Replace All refused under transient lane pressure

The rerun of `scripts/heavy.sh --envelope 16G -- make test` on `96dcae3e`
(2026-11-19 02:00–02:21, load 5.7 before, 7.8 after) passed every test but
printed one `FLAKY:` line:

```
[surfaces] test search_panel::test_an_apply_while_another_window_runs_is_refused ...
[surfaces] thread 'main' panicked at …/widget_mod_search_panel.rs:4592:5:
[surfaces] condition was not met within 20s
…
FLAKY: [surfaces] search_panel::test_an_apply_while_another_window_runs_is_refused
```

The timeout is the wait in `replace_all_held_at_second_file` for
`replace_in_flight` after Confirm. The test came from `stream-replace-all`
(2026-11-18) and its code is untouched here. This is its first failure in
any recorded run: it passed in stream-replace-all stages 3 and 4 and in
slice-large-directory-collapse rev6 and rev7. It does not involve this
change's runner: the timeout is the test process waiting on its own panel
state.

## Not reproduced by repetition

- Isolated (`run-widget-tests.sh --headless -- --exact …`): 8 of 8 passed.
- 12 copies in parallel, 4 rounds: 48 of 48 passed.
- The whole `search_panel::` module, 4 copies in parallel: 4 × 149 passed.
- Storage was not the cause: `btrfs-stall-report` for 02:09–02:21 shows one
  15 s episode (02:17:33–02:17:48, commit 212 ms, dirty-page throttling of
  another build's linker), which cannot hold a 20 s wait.

## The mechanism, reproduced deterministically

`window/search.rs`'s Replace All callback reserves the undo journal's
`MAX_REPLACE_UNDO_RETAINED_BYTES` (64 MiB) in the process-wide disposal lane
(128 MiB). If the reservation is refused it publishes "Replace All deferred
while memory pressure clears; try again shortly", finishes the transaction
and starts no run, and nothing retries. By then Confirm has already consumed
the preview. The same lane is held, 64 MiB at a time, by a palette file-index
rebuild (`MAX_FILE_INDEX_RETAINED_BYTES`, `window/palette_shell.rs`), which
the attention refresh schedules 300 ms after a window's `notify::is-active`,
and by an incremental index update. Which holder was present at 02:1x was
not recorded.

A scratch probe (not committed; its text is in the stage's handoff) filled
the lane's remaining capacity with `fill_disposal_capacity_for_test()` after
the preview completed, confirmed, then released the fill:

```
PROBE: status while full 'Replace All deferred while memory pressure clears; try again shortly';
run started after release false; preview_mode false preview count 0 transaction pending false;
files ["needle\n", "needle\n", "needle\n"]
```

So transient lane pressure at the instant of Confirm refuses the user's
Replace All for good and discards the checked preview, although the message
says to try again. No file changes, so no data is lost; the preview and its
selection are.

## What this change does about it, and what it leaves

- Nothing in the tree. A diagnostic for the helper's wait (panic with the
  status message, the preview state and the transaction state, so a
  recurrence names its path; 7 tests use the helper) is in the stage's
  handoff as a patch. It is not committed here because any edit under
  `crates/lushtext/tests/widget/` owes `make accessibility-smoke` and
  `make visual-smoke` (`make check-accessibility-policy`), which are outside
  this change's gates; the follow-up change carries it.
- The production fix is not made here. It changes the Replace All
  transaction contract (`stream-replace-all`'s spec): the confirm path would
  keep the selection and the claimed transaction, and retry the reservation
  on the lane's capacity wakeup, as the preview, the persisted-undo load and
  the index flush already do. It touches data-safety code, so it needs its
  own change in the ledger (`parallel-agent-work.md`, "Merging": "or gets
  its own change in the ledger"). Until then the test can flake under the
  same coincidence.
