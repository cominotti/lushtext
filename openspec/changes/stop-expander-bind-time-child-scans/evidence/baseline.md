# Failing-First Baseline Measurement (Task 1.4)

Recorded on `feat/stop-expander-bind-time-child-scans` (based on `origin/main` `0aadb77e`) **before** any production fix, running headless under `mutter --headless` via:

```bash
./scripts/heavy.sh --envelope 16G -- ./scripts/run-widget-tests.sh --headless -- --exact workspace_section::test_scrolling_and_collapsing_300_directories_starts_no_bind_time_child_scans --nocapture
```

Fixture: 1 workspace folder (`root`) containing 300 sibling subdirectories (`dir-000`..`dir-299`), each containing 1 file (`file.txt`). `workspace-empty-folder-lookahead-cap = 100` (yielding 100 `Some(false)` hinted subdirectories and 200 `None` unhinted subdirectories). Window size `1200x480`.

## Pre-Fix Measurements (Attempt 1 / Attempt 2)

| Step | `child_model_creations` | `scan_submissions` | `registered_child_stores` | `dir_stores.len()` | `live_dir_stores` (settled) | `AccessibleState::Busy` observed | `process_scan_task_high_water` | Max main-loop iteration (µs) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1. First bind (after root expand) | 207 | 207 | 207 | 205 | 1 | `false` | 4 | 36 / 46 (`bind elapsed_us`: 1,388,031 / 1,399,624) |
| 2. Scroll to end | 343 (+136) | 343 (+136) | 331 (+124) | 301 | 1 | **`true`** | 4 | 12,361 / 17,356 |
| 3. Scroll back | 467 (+124) | 467 (+124) | 411 / 409 (+80 / +78) | 301 | 1 | **`true`** | 4 | 13,159 / 18,327 |
| 4. Expand 1 hinted dir, then collapse | 470 (+1 expand, +2 collapse) | 470 (+1 expand, +2 collapse) | 413 / 411 | 301 | 1 | — | 4 | 643 / 814 (collapse) |
| 5. Expand 1 unhinted (`None`) dir | 471 (+1) | 471 (+1) | 414 / 412 | 301 | 2 | — | 4 | — |

Test verdict before fix: **FAILED** at step 1 assertion (`left: 207, right: 1: first bind of 300 collapsed subdirectories must submit only the workspace root scan (got 207)`).

## Hypotheses H1–H5 Disposition

- **H1 (Confirmed):** First bind submitted 206 unrequested subdirectory scans (`207` total minus `1` for `root`). Scrolling to the end submitted `+136` more scans, and scrolling back submitted `+124` more (`467` total across bind and scroll).
- **H2 (Confirmed):** Collapsing the expanded directory added `+2` scan submissions (`468` after expand → `470` after collapse).
- **H3 (Confirmed):** `dir_stores` accumulated `301` entries (`root` plus all 300 never-expanded subdirectories). While a probe scan is in flight in `start_child_scan`, `spawn_blocking_then` captures a strong `gio::ListStore` reference, keeping the orphan store alive and upgradeable in `dir_stores` until completion.
- **H4 (Confirmed):** `registered_child_stores` (`child_store_paths.len()`) reached `409`–`411` after bind and scroll (and `412`–`414` by step 5), proving probe registrations outlive their discarded stores until GLib happens to reuse an address.
- **H5 (Confirmed):** Bind/scroll probes saturated all `4` permits of the process-wide `WORKSPACE_SCAN_TASK_LIMIT` (`process_scan_task_high_water = 4`), delayed initial bind settling to `~1.39 s`, pushed scroll-loop GTK iterations to `12.4–18.3 ms` (exceeding `B_INTERACT = 8 ms` and `B_SETTLE = 16 ms`), and flipped the file tree's `AccessibleState::Busy` to `true` during mere scrolling.

## Post-Fix Measurements (Task 4.1)

Command (post-Group 3, same test `workspace_section::test_scrolling_and_collapsing_300_directories_starts_no_bind_time_child_scans` in `crates/lushtext/tests/widget/workspace_section.rs`):

```
./scripts/heavy.sh --envelope 16G -- ./scripts/run-widget-tests.sh --headless -- --exact workspace_section::test_scrolling_and_collapsing_300_directories_starts_no_bind_time_child_scans --nocapture
```

Budget reference (`crates/lushtext-model/src/responsiveness_budget.rs`): `B_INTERACT_US = 8_000` µs (8 ms), `B_SETTLE_US = 16_000` µs (16 ms).

| Step | Event / Assertion | `creations` | `probes_discarded` | `materializations` | `pending_mat` | `scan_submissions` | `registered_stores` | `dir_stores_len` (`live`) | `process_hw` | `busy` | Timing (µs) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| 1 | Bind 300 collapsed subdirs | 207 | 206 | 1 | 0 | 1 | 1 | 1 (1) | 1 | `false` | `elapsed_us=367715`, `max_iter_us=275` |
| 2 | Scroll to end | 343 | 342 | 1 | 0 | 1 | 1 | 1 (1) | 1 | `false` | `max_iter_us=14954` |
| 3 | Scroll back to top | 467 | 466 | 1 | 0 | 1 | 1 | 1 (1) | 1 | `false` | `max_iter_us=15741` |
| 4 | Expand hinted + collapse | — | 468 | 2 -> 2 | — | 2 -> 2 (+0) | 1 | — | — | — | `max_collapse_iter_us=917` |
| 5 | Expand unhinted (`None`) | 471 | 468 | 3 | 0 | 3 | 2 | 3 (2) | — | — | — |

Key deltas (baseline -> post-fix):

- **Bind (`step1`)**: `scan_submissions` `207 -> 1`, `registered_stores` `207 -> 1`, `dir_stores_len` `205 -> 1`, `process_hw` `4 -> 1`, bind `max_iter_us` `275` µs (within `B_INTERACT_US = 8_000` µs), `elapsed_us` `1,388,031 -> 367,715` µs (`-73.5%`).
- **Scroll round-trip (`step2`–`step3`)**: `scan_submissions` added `260 -> 0` (stayed at `1`), `registered_stores` added `204 -> 0` (stayed at `1`), accessible `Busy` `true -> false` throughout. Each `adjustment.set_value` jump still measures a pure GTK `GtkListView` widget-recycle/layout dispatch (`max_iter_us` `14,954` / `15,741` µs vs `17,356` / `18,327` µs baseline, above `B_INTERACT_US` `8_000` µs and within `B_SETTLE_US` `16_000` µs), now with zero child scans or store registrations occurring during those dispatches.
- **Collapse (`step4`)**: extra `scan_submissions` on collapse `2 -> 0`, `max_collapse_iter_us = 917` µs (within `B_INTERACT_US = 8_000` µs).
- **Expand unhinted (`step5`)**: `materializations` `+1`, `scan_submissions` `+1`, `registered_stores` `1 -> 2` (expanded child directory only).

Every assertion in the D4 test (`test_scrolling_and_collapsing_300_directories_starts_no_bind_time_child_scans`) passes.

## Task 1.4 Verification of Hypotheses

Aligned with `design.md` (`Hypotheses the measurement must confirm or refute`):

- **H1** (*Each bind of a collapsed `Some(false)` or `None` directory row submits one child scan, and each recycled rebind submits another*): **Confirmed** on baseline (`207` bind submissions, `+136` on scroll-end, `+124` on scroll-back); **Eliminated** post-fix (`scan_submissions = 1` across bind and scroll round-trip; `206` bind probes and `466` cumulative bind+scroll probes discarded).
- **H2** (*Collapsing an expanded, bound directory submits a scan via `notify::children`*): **Confirmed** on baseline (`+2` scan submissions on collapse); **Eliminated** post-fix (`+0` scan submissions on collapse; `2` collapse probes discarded).
- **H3** (*While a probe scan is in flight, `dir_stores[path]` upgrades to an orphan store no row shows*): **Confirmed** on baseline (`dir_stores.len() = 301`); **Eliminated** post-fix (`dir_stores.len() = 1` across bind and scroll; zero never-expanded subdirectories in `dir_stores`).
- **H4** (*Probe registrations in `child_store_paths`, `child_store_refs`, and `child_scan_flights` outlive their stores until address reuse or teardown*): **Confirmed** on baseline (`registered_child_stores = 409–414`); **Eliminated** post-fix (`registered_child_stores = 1` across bind and scroll, root only).
- **H5** (*Probe scans compete with real expansions and emptiness probes for process-wide scan admission*): **Confirmed** on baseline (`process_scan_task_high_water = 4`, bind `1.39 s`, scroll `AccessibleState::Busy = true` and `12.4–18.3 ms` max dispatches); **Eliminated** post-fix (`process_scan_task_high_water = 1`, bind `0.37 s` / `0.33 s`, `AccessibleState::Busy = false` across bind and scroll).
