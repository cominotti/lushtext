# A FLAKY in the final-tree `make test`, and a Replace All refusal path it may have taken

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

## A refusal path, reproduced deterministically

`window/search.rs`'s Replace All callback reserves the undo journal's
`MAX_REPLACE_UNDO_RETAINED_BYTES` (64 MiB) in the process-wide disposal lane
(128 MiB). If the reservation is refused it publishes "Replace All deferred
while memory pressure clears; try again shortly", finishes the transaction
and starts no run, and nothing retries. By then Confirm has already consumed
the preview.

A scratch probe (below, not committed) filled the lane's remaining capacity
with `fill_disposal_capacity_for_test()` after the preview completed,
confirmed, then released the fill:

```
PROBE: status while full 'Replace All deferred while memory pressure clears; try again shortly';
run started after release false; preview_mode false preview count 0 transaction pending false;
files ["needle\n", "needle\n", "needle\n"]
```

**Observed:** with the lane full at the instant of Confirm, Replace All is
refused, never retried after the lane frees, and the preview and its checked
selection are discarded, although the message says to try again. No file
changes, so no data is lost.

**Inferred, not shown:**

- That the failed attempt took this path. The confirm path has other silent
  returns (a stale preview ticket at the selection worker's completion, a
  claimed transaction), and the failure printed only the timeout.
- That one 64 MiB holder is enough. This is arithmetic from the constants
  (64 + 64 + any small reservation > 128); the probe filled the whole lane.
- That a holder was present. A palette file-index rebuild reserves
  `MAX_FILE_INDEX_RETAINED_BYTES` (64 MiB, `window/palette_shell.rs`), which
  the attention refresh schedules 300 ms after a window's
  `notify::is-active`, and an incremental index update reserves the same.
  Whether either ran in that test at that moment was not checked.

## What this change does about it, and what it leaves

- Nothing in the tree. Any edit under `crates/lushtext/tests/widget/` owes
  `make accessibility-smoke` and `make visual-smoke`
  (`make check-accessibility-policy`), which are outside this change's
  gates, so the diagnostic below is for the follow-up change to carry.
- The production fix is not made here. It changes the Replace All
  transaction contract (`stream-replace-all`'s spec): the confirm path would
  keep the selection and the claimed transaction and retry the reservation on
  the lane's capacity wakeup, as the preview, the persisted-undo load and the
  index flush already do. It touches data-safety code, so it needs its own
  change in the ledger (`parallel-agent-work.md`, "Merging": "or gets its
  own change in the ledger"). Until then the test can flake under the same
  coincidence.

### Diagnostic for `replace_all_held_at_second_file` (`crates/lushtext/tests/widget/search_panel.rs`)

With it, a recurrence names its path (7 tests use the helper):

```diff
@@ -4729,9 +4729,26 @@ fn replace_all_held_at_second_file(
         evidence.replace_preview_count == 3 && !evidence.replace_preview_pending
     });
     panel.activate_confirm_replacements();
-    wait_until(Duration::from_secs(20), || {
+    // A confirm that starts no run returns silently (a stale ticket, a
+    // refused reservation, a claimed transaction), so a timeout names the
+    // state that tells those apart.
+    if !crate::common::wait_until_or_false(Duration::from_secs(20), || {
         panel.evidence().replace_in_flight
-    });
+    }) {
+        let evidence = panel.evidence();
+        panic!(
+            "no Replace All run started within 20s: status '{}', preview_mode {}, \
+             preview count {} pending {}, transaction pending {} generation {:?}, \
+             last apply {:?}",
+            status_text(window),
+            panel.imp().preview.preview_mode.get(),
+            evidence.replace_preview_count,
+            evidence.replace_preview_pending,
+            evidence.replace_transaction_pending,
+            evidence.replace_transaction_generation,
+            evidence.last_apply_counts,
+        );
+    }
     (panel, files)
 }
 
```

### The scratch probe

Appended to `crates/lushtext/tests/widget/search_panel.rs`, run with
`scripts/run-widget-tests.sh --headless -- --exact search_panel::test_probe_confirm_under_transient_disposal_pressure`.
It prints today's behaviour and asserts nothing; the follow-up turns it into
a regression test that expects the run to start once the lane frees:

```rust
/// SCRATCH PROBE (not committed): transient disposal-lane pressure at the
/// instant of Confirm.
#[test]
fn test_probe_confirm_under_transient_disposal_pressure() {
    ensure_gtk_init();
    let _data = isolated_data_dir();
    let window = test_window();
    present_window(&window);
    crate::common::wait_for_startup_data_flow(&window);
    let dir = tempfile::tempdir().expect("expected operation to succeed");
    let files: Vec<_> = (0..3)
        .map(|index| {
            let path = dir.path().join(format!("file_{index}.txt"));
            fixture::write_text(&path, "needle\n");
            path
        })
        .collect();
    let panel = window.imp().search_panel.get();
    panel.set_workspace_folders(vec![dir.path().to_path_buf()]);
    panel.start_search(&search_spec("needle"));
    wait_until(Duration::from_secs(15), || {
        !panel.imp().runtime.searching.get() && panel.imp().runtime.total_matches.get() == 3
    });
    panel.enter_preview_mode("thread");
    wait_until(Duration::from_secs(15), || {
        let evidence = panel.evidence();
        evidence.replace_preview_count == 3 && !evidence.replace_preview_pending
    });
    wait_for_journal_disk_idle(&panel);
    // A palette index rebuild holds MAX_FILE_INDEX_RETAINED_BYTES (64 MiB)
    // of this lane while it runs; this fill stands in for it.
    let fill = lushtext_core::ui::plain_disposal::fill_disposal_capacity_for_test();
    panel.activate_confirm_replacements();
    flush_after_delay(Duration::from_millis(500));
    let status_while_full = status_text(&window);
    drop(fill);
    let started = crate::common::wait_until_or_false(Duration::from_secs(5), || {
        panel.evidence().replace_in_flight || panel.evidence().last_apply_counts.is_some()
    });
    let evidence = panel.evidence();
    eprintln!(
        "PROBE: status while full '{status_while_full}'; run started after release {started}; \
         preview_mode {} preview count {} transaction pending {}; files {:?}",
        panel.imp().preview.preview_mode.get(),
        evidence.replace_preview_count,
        evidence.replace_transaction_pending,
        files.iter().map(|path| std::fs::read_to_string(path).unwrap_or_default()).collect::<Vec<_>>(),
    );
}
```
