# Task 7.4 — per-site categorization of the 35 `search_backup` widget-test reaches

`crates/lushtext/tests/widget/search_panel.rs` reached around the widget into the
`search_backup` service at **35 sites: 16 `load`, 4 `save`, 15 `delete`**, which
matched the authoring inventory exactly. Task 7.4 asked which subset should read
the evidence surface instead, and required stating the category per site.

## The categorization rule, and one premise correction

Task 7.4 predicted the migration population would be "predominantly the `load`
and `delete` assertions". **The `delete` half of that prediction is wrong**, and
it is recorded here rather than quietly worked around:

- All **15 `delete` sites** are `let _ = search_backup::delete(&data_dir);` —
  every one is fixture **arrange/teardown**, clearing real on-disk state before or
  after a test. None is an assertion. Evidence cannot replace them: no evidence
  read deletes a file. **0 of 15 migrate.**
- All **4 `save` sites** are **arrange**, seeding a durable journal a test then
  loads or recovers from. Task 7.4 explicitly says to keep these. **0 of 4
  migrate.**
- The **16 `load` sites** split by *what the site is for*, not by function name:
  - **10 are waits** — `wait_until(.., || load(..) == expected)`. The question
    they actually ask is "has the workflow's journal disk work landed", which is a
    workflow question the evidence surface answers directly. Polling the journal
    directory is a weaker proxy: it can observe a partially written directory, and
    it cannot distinguish "not written yet" from "written differently". **All 10
    migrate.**
  - **6 are assertions** — `assert_eq!(load(..), expected)` or
    `assert!(load(..).is_empty())`. The question is genuinely "what bytes are on
    disk". **All 6 stay.**

**Migrated: 10 of 35.** Every migration also *strengthens* its test rather than
weakening it. `wait_until` panics with a generic "condition was not met" message
on timeout; the replacement is `wait_for_journal_disk_idle(&panel)` followed by an
explicit `assert_eq!` / `assert!` on the disk bytes, so the wait becomes the
workflow question it always was and the disk check becomes a real assertion that
prints the actual bytes on failure.

## Per-site table

Line numbers are pre-migration. Category is one of **wait→evidence** (migrated),
**disk assertion** (stays), or **arrange/teardown** (stays).

| # | Line | Call | Test | Category |
| --- | --- | --- | --- | --- |
| 1 | 1920 | `delete` | `test_clear_results_preserves_undo_backup` | arrange/teardown |
| 2 | 1928 | `load` | `test_clear_results_preserves_undo_backup` | **wait→evidence** |
| 3 | 1941 | `load` | `test_clear_results_preserves_undo_backup` | disk assertion — the journal survived a new search |
| 4 | 1945 | `delete` | `test_clear_results_preserves_undo_backup` | arrange/teardown |
| 5 | 1952 | `delete` | `test_search_panel_restores_active_persisted_undo_backup_on_construction` | arrange/teardown |
| 6 | 1959 | `save` | same | arrange — seeds the journal construction must recover |
| 7 | 1967 | `load` | same | disk assertion — recovery did not mutate the journal |
| 8 | 1971 | `delete` | same | arrange/teardown |
| 9 | 1978 | `delete` | `test_clearing_persisted_undo_cancels_capacity_retry_before_it_can_reload` | arrange/teardown |
| 10 | 1980 | `save` | same | arrange |
| 11 | 1997 | `load` | same | **wait→evidence** (`.is_empty()`) |
| 12 | 2007 | `delete` | `test_persisted_undo_load_resumes_after_disposal_capacity_clears` | arrange/teardown |
| 13 | 2009 | `save` | same | arrange |
| 14 | 2024 | `delete` | same | arrange/teardown |
| 15 | 2032 | `delete` | `test_search_panel_close_preserves_durable_undo_backup` | arrange/teardown |
| 16 | 2038 | `load` | same | **wait→evidence** |
| 17 | 2046 | `load` | same | disk assertion — panel close preserved the journal |
| 18 | 2050 | `delete` | same | arrange/teardown |
| 19 | 2078 | `delete` | `test_set_undo_backup_updates_ui_before_delayed_disk_save` | arrange/teardown |
| 20 | 2090 | `load` | same | disk assertion — **mid-flight**: the disk is still empty while the delayed save sleeps. Evidence cannot express this; it is the point of the test |
| 21 | 2096 | `load` | same | **wait→evidence** |
| 22 | 2099 | `delete` | same | arrange/teardown |
| 23 | 2108 | `delete` | `test_clear_undo_backup_updates_ui_before_delayed_disk_delete` | arrange/teardown |
| 24 | 2114 | `load` | same | **wait→evidence** |
| 25 | 2125 | `load` | same | disk assertion — **mid-flight**: the journal still exists while the delayed delete sleeps |
| 26 | 2130 | `load` | same | **wait→evidence** (`.is_empty()`) |
| 27 | 2142 | `delete` | `test_clear_after_delayed_undo_backup_save_keeps_disk_empty` | arrange/teardown |
| 28 | 2153 | `load` | same | **wait→evidence** (`.is_empty()`) |
| 29 | 2165 | `delete` | `test_save_after_delayed_undo_backup_clear_keeps_newer_disk_backup` | arrange/teardown |
| 30 | 2171 | `load` | same | **wait→evidence** |
| 31 | 2181 | `load` | same | **wait→evidence** |
| 32 | 2191 | `delete` | `test_reserved_replace_generation_blocks_stale_delete_after_service_commit` | arrange/teardown |
| 33 | 2197 | `load` | same | **wait→evidence** |
| 34 | 2205 | `save` | same | arrange — simulates a committed service journal |
| 35 | 2210 | `load` | same | disk assertion — the newer journal is the one on disk |

## Totals

| Category | Sites | Migrated |
| --- | --- | --- |
| wait→evidence | 10 | 10 |
| disk assertion (including 2 mid-flight) | 6 | 0 |
| arrange/teardown (`delete` 15, `save` 4) | 19 | 0 |
| **Total** | **35** | **10** |

No `*_for_test` accessor was added for any of this. The one field the surface
lacked — an in-flight journal disk-job count — was added to the surface itself
(`SearchPanelEvidence::journal_disk_jobs_in_flight`), which is what task 7.4
requires when a test needs a fact the surface does not expose.
