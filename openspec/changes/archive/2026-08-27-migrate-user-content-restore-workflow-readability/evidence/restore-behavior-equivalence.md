# Restore behavior equivalence (task 10.5)

Every case task 10.5 lists, with the test that asserts the user-visible outcome.
**Every test name below was verified to exist** rather than recalled.

The strongest evidence for a pure restructuring is that the behaviour assertions
are the *same* assertions. Almost every test here predates this change and was
modified only mechanically, where a retired inspection seam became its
evidence-surface equivalent. New tests are marked **new**; the one deliberate
behaviour change is marked as the carved-out fix.

## Crash recovery

| Case | Covering test (`crates/lushtext/tests/widget/window.rs` unless noted) | Status |
| --- | --- | --- |
| crash recovery of a **file-backed** draft | `test_startup_restore_applies_matching_file_backed_draft`; `test_failed_file_retry_reads_durable_draft_after_preload_release` | pre-existing, unchanged |
| crash recovery of an **untitled** draft | `test_startup_restore_keeps_untitled_draft_behavior`; `test_ordinary_untitled_restore_rejects_edit_and_preserves_recovery` | pre-existing, unchanged |
| a draft large enough to require **chunked installation** | `test_document_sized_preloaded_draft_publishes_only_after_bounded_install` | pre-existing, unchanged |
| a draft whose largest **paragraph exceeds the slice budget** | `lushtext-core` unit `insertion_policy_installs_oversized_paragraphs_atomically`, plus the `properties file_load::install_boundaries_*` pair | unchanged and byte-identical: `model/buffer_replacement.rs` was **not edited**, and `delete_one_slice` diffs **IDENTICAL** against its pre-migration text |
| a **stale** file-backed draft is skipped, once | `test_startup_restore_skips_stale_file_backed_draft_once` | pre-existing, unchanged |
| restore refuses a **reopened or moved** tab | `test_file_restore_rejects_reload_and_path_change`; `test_file_restore_rejects_manifest_replacement_and_closed_editor` | pre-existing, unchanged — these are the `draft_restore_is_current` contract |
| a restored draft seeds the local-history **baseline** | `test_local_history_startup_restore_uses_restored_draft_as_baseline` | pre-existing, unchanged |
| grouped **recovery diagnostics** reach the user | `test_startup_restore_surfaces_grouped_recovery_diagnostics` | pre-existing; the message text now comes from `session_restore::policy::startup_recovery_status_message`, which is mutation-tested |

## Autosave

| Case | Covering test | Status |
| --- | --- | --- |
| **first-dirty** autosave, small buffer | `test_first_dirty_autosave_writes_small_buffer_before_periodic_tick` | pre-existing, unchanged |
| first-dirty autosave, **chunked** buffer | `test_first_dirty_autosave_large_buffer_snapshots_across_main_loop_chunks` | pre-existing, unchanged |
| a **superseded** autosave | `test_draft_autosave_marks_pending_when_editing_during_inflight_batch` | pre-existing, unchanged. It pins the mark-pending-not-queue rule `policy::autosave_admission` now owns |
| an autosave that **fails** | `test_first_dirty_autosave_failure_keeps_editor_retry_eligible`; `test_injected_draft_stage_failures_remain_retryable` | pre-existing, unchanged |
| a **mutated** snapshot retried with the latest body | `test_draft_pipeline_mutated_snapshot_retries_once_with_complete_latest_body` | pre-existing, unchanged — the `policy::captured_snapshot_is_current` contract |
| a **partial** failure must not publish an authoritative subset | `test_draft_pipeline_partial_body_failure_does_not_publish_an_authoritative_subset` | pre-existing, unchanged |
| the pipeline never holds more than **one complete body** | `test_draft_pipeline_retains_at_most_one_complete_body_across_many_tabs`, plus the new evidence proof asserting `max_retained_complete_bodies <= 1` | pre-existing test unchanged; the counters are now compiled in **production** rather than test-gated, so the invariant is observable where it matters |
| an over-limit buffer stays retryable, then clears | `test_draft_pipeline_limit_stays_retryable_then_clears_after_acceptance` | pre-existing, unchanged |
| **an autosave whose editor holds a partially installed buffer** | `test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft` | **new**, and the one deliberate behaviour change. Proven to fail without the fix: the draft body becomes `Some("x")` instead of the preserved work |

## Close flush

| Case | Covering test | Status |
| --- | --- | --- |
| close writes every dirty tab, skipping explicit discards | `test_flush_dirty_drafts_skips_close_discarded_editors` | pre-existing, unchanged |
| a manifest failure **blocks** the close | `test_flush_dirty_drafts_fails_when_manifest_cannot_be_saved` | pre-existing, unchanged |
| an unconfirmed snapshot **blocks** the close and stays retryable | `test_close_draft_snapshot_mutation_blocks_close_and_keeps_retryable_state`; `test_draft_pipeline_close_blocks_and_preserves_retry_state_over_limit` | pre-existing, unchanged |
| an **intentionally empty** modified draft is still persisted | `test_close_flush_persists_intentionally_empty_modified_draft` | pre-existing, unchanged |
| cancel preserves, discard cleans | `test_close_modified_untitled_cancel_preserves_and_discard_cleans_draft` | pre-existing, unchanged |

## Deletes and orphan cleanup

| Case | Covering test | Status |
| --- | --- | --- |
| a failed body deletion keeps an **explicit retry tombstone** | `test_file_backed_draft_body_delete_failure_keeps_an_explicit_retry_tombstone` | pre-existing; its read migrated to `draft_delete_is_tombstoned` |
| a later **edit after a delayed delete** creates newer recovery | `test_edit_after_delayed_delete_creates_newer_recovery` | pre-existing, unchanged — the `DraftMutationOrder` epoch-equality contract |
| a **save tombstone** wins over a delayed autosave | `test_save_tombstone_wins_over_delayed_autosave_body_and_manifest` | pre-existing, unchanged |
| cleanup where the **inode matches**, and where an autosave replaced the body between inspection and execution (must **not** delete) | `lushtext-core` `services::draft_service` orphan-cleanup suite, plus `properties draft_orphan_cleanup` | pre-existing, unchanged — `git diff --stat -- crates/lushtext-core/src/services/` is **empty**, so the guard/recheck ordering recorded in `durability-contracts.md` is preserved by non-edit |
| timers **coalesce** and workers **serialize** | `test_orphan_cleanup_coalesces_timers_and_serializes_workers` | pre-existing; its reads migrated to the evidence surface, including `cleanup_workers_high_water` |
| bounded continuation and **backoff** | `drafts::policy::tests::orphan_cleanup_follow_up_*` (4) and `cleanup_backoff_doubles_and_then_caps` | relocated; the cap assertion was **strengthened** to a concrete duration to kill a real survivor |

## Session restore

| Case | Covering test | Status |
| --- | --- | --- |
| **zero** descriptors | `session_restore::policy::tests::total_descriptors_counts_what_the_generation_started_with`'s empty-generation case | policy case **new** (killed a survivor); the adapter's empty path is unchanged |
| **one** descriptor | `session_restore::policy::tests::a_permit_and_its_policy_report_the_generation_they_were_created_for` | **new** (killed four survivors) |
| **many** descriptors across bounded turns | `test_bounded_session_restore_preserves_order_selection_and_one_terminal_projection` (12 descriptors); `policy::tests::high_tab_count_is_admitted_in_order_across_bounded_turns` (11) | pre-existing, unchanged; both assert `max_pages_in_one_turn <= 4` and `max_inflight_file_plans == 2` |
| a **cancelled** restore | `test_session_restore_cancellation_clears_pending_permits_source_and_projection_deferral`; `policy::tests::cancellation_drops_pending_and_active_ownership_once` | pre-existing, unchanged |
| a restore whose **editor closes mid-turn** | `test_session_restore_editor_and_window_teardown_release_all_planning_ownership` | pre-existing, unchanged — the "no planning terminal is ever dropped" contract slot 3b handed here |
| **delayed** completions must not disturb the active or modified page | `test_delayed_session_restore_completions_preserve_active_and_modified_pages` | pre-existing, unchanged |
| a **user selection** during restore wins | `test_newer_tab_selection_survives_later_session_restore_turns` | pre-existing, unchanged — the user-first settle rule |
| **pinned** tabs stay ahead | `test_session_restore_keeps_pinned_tabs_ahead_of_unpinned_tabs` | pre-existing, unchanged |
| a **malformed** session file | `lushtext-core` `services::session_service` recovery suite; `test_saved_search_recovery_surfaces_visible_warning` for the adjacent surface | pre-existing, unchanged |
| a close **during** a still-running restore preserves unreached descriptors | `test_close_session_snapshot_preserves_not_yet_mounted_restore_descriptors`; `test_close_before_startup_descriptors_preserves_persisted_session`; `test_close_before_startup_descriptors_merges_new_untitled_recovery` | pre-existing, unchanged |
| a close **aborts** when recovery evidence cannot be preserved | `test_close_aborts_when_pending_session_evidence_cannot_be_preserved` | pre-existing, unchanged |
| a session-save failure stays retryable and warns | `test_sync_session_save_failure_keeps_retry_state_and_warns_user` | pre-existing; its `save_failed` reads migrated to the evidence surface |
| the **merge bound** and the empty-draft-ID identity guard | `policy::tests::a_persisted_active_index_past_the_merged_end_is_dropped`; `an_empty_draft_id_is_not_a_merge_identity` | **new** (killed four survivors) |

## Local history

| Case | Covering test | Status |
| --- | --- | --- |
| **baseline** capture, and its retry budget | `test_local_history_baseline_retry_is_bounded_and_releases_permit`; `test_local_history_baseline_transient_failure_retries_original_clean_text` | pre-existing, unchanged |
| a **stale** baseline rejected by its ticket | `test_failed_local_history_baseline_cannot_replace_newer_saved_cycle`; `..._does_not_cross_path_generation`; `..._drops_after_editor_disposal` | pre-existing, unchanged |
| **periodic** capture, and its one-latest timer | `editor_page::test_periodic_local_history_clean_dirty_cycles_own_one_latest_timer` | pre-existing; its reads migrated to the evidence surface |
| a periodic capture **cancelled by an edit** mid-snapshot | `editor_page::test_periodic_local_history_edit_cancels_chunked_snapshot_without_persistence` | pre-existing, unchanged |
| capture **policy** across Full / SaveOnly / Unavailable | `test_local_history_capture_policy_respects_full_save_only_and_unavailable_modes` | pre-existing, unchanged |
| a **preview install**, sliced, with supersession | `test_local_history_preview_supersedes_reads_and_unicode_install_slices` | pre-existing; its counters migrated to `local_history_preview_install_evidence` |
| a preview **deferred** by disposal pressure | `test_local_history_preview_resumes_after_disposal_capacity_clears` | pre-existing, unchanged |
| a **restore** and its **undo** | `test_local_history_browser_collapses_and_restore_can_be_undone`; `test_document_sized_local_history_restore_and_undo_are_bounded_and_exact` | pre-existing, unchanged |
| a restore **deferred** by progress pressure | `test_local_history_restore_defers_compactly_until_progress_capacity_clears` | pre-existing, unchanged |
| a restore whose **safety snapshot** was mutated is discarded | `test_local_history_restore_discards_mutated_chunked_safety_snapshot` | pre-existing, unchanged — this is what makes a restore non-destructive |
| which snapshots the user is **shown** | `test_local_history_browser_hides_legacy_empty_baseline_noise`; and the new `policy::tests::the_periodic_count_requires_a_periodic_snapshot_with_real_content` | the new policy test killed three survivors that would have hidden a user's only "before edits" snapshots |
| a **repaired** lineage warns | `test_local_history_browser_warns_and_shows_repaired_snapshot` | pre-existing, unchanged |
| the browse **action's** enablement | `test_local_history_action_requires_saved_eligible_document` | pre-existing, unchanged |

## Buffer replacement

| Case | Covering test (`editor_page.rs`) | Status |
| --- | --- | --- |
| cancelled **mid-slice** | `test_bounded_buffer_replacement_stale_partial_body_is_cleared_not_published` | pre-existing, unchanged |
| **caller gone** when a slice resumes | `test_bounded_buffer_replacement_disposal_terminal_releases_source_and_body` | pre-existing, unchanged |
| **supersession** publishes only the latest body | `test_bounded_buffer_replacement_supersession_publishes_only_latest_body` | pre-existing, unchanged. It is also what caught the latent `BorrowMutError` recorded in `mutation-buffer-replacement.md` |
| a **synchronous buffer signal** superseding a live replacement, both paths | `test_first_synchronous_delete_signal_can_supersede_buffer_replacement`; `..._insert_signal_...` | pre-existing, unchanged |
| unicode preserved, guard restored | `test_bounded_buffer_replacement_preserves_unicode_and_terminal_guard_cleanup` | pre-existing, unchanged |

## State extremes the UI rules require for collection surfaces

| Surface | no items | one | many / awkward |
| --- | --- | --- | --- |
| local-history browser | `test_local_history_dialog_shows_empty_state_without_snapshots`; `test_local_history_browser_explains_empty_snapshot_and_disables_copy` for the empty-*snapshot* case | the single-snapshot browser in `test_local_history_browser_collapses_and_restore_can_be_undone` | `test_local_history_dialog_scales_from_parent_and_keeps_preview_dominant`; the unicode sliced-preview test's large body |
| session restore | zero descriptors (policy, above) | one descriptor (policy, above) | 12 descriptors across bounded turns; the **20,001-tab** merge in `policy::tests::session_merge_indexes_large_descriptor_sets_and_overlays_current_pages` |
| draft manifest | `manifest_entries == 0` in the draft evidence proof's idle read | one draft, in every autosave test | `test_draft_pipeline_retains_at_most_one_complete_body_across_many_tabs`; the cleanup pagination suite |
| preview install | an **empty** snapshot gets its own explanatory status page and keeps Copy disabled while Restore stays enabled — asserted both in `policy::tests::preview_install_separates_empty_from_merely_small` and in `test_local_history_browser_explains_empty_snapshot_and_disables_copy` | direct install | sliced install |
| accessibility of the dense browser | — | — | `test_local_history_browser_controls_expose_accessibility_roles` |

Long paths and deep names are covered by the 20,001-tab merge's
`/persisted/{index}.txt` fan-out and by the browser's wrapping subtitle, which the
empty-state dialog test asserts does not introduce a scrollbar.
