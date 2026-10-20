## 1. Failing-first tests and audits (no production behaviour change)

- [x] 1.1 In `crates/lushtext/tests/widget/window_preview.rs` (module of the `surfaces` shard, so `scripts/widget-shards.py` needs no membership edit), add `test_external_change_in_preview_only_shows_alert_on_preview_surface`. Open a Markdown file and enter preview-only through `win.toggle-preview-mode`. Write the file with the existing `write_external_change_after_mtime_tick` pattern from `window.rs` (move it to `common.rs` if both files need it; do not copy it). Wait on a predicate with a timeout for the preview surface's title to read "File Has Changed on Disk". Record it failing on the current tree. The surface does not exist yet, so it fails to compile first. Check that it fails for the right reason once the template child exists.
  - Log: on the pre-change tree the test cannot compile (no `preview_document_alert`), as anticipated. With the template child in place but the preview projection step rendering nothing, it fails at the wait for the preview surface's "File Has Changed on Disk" title (`condition was not met within 5s`), the right reason; it passes with 4.2. `write_external_change_after_mtime_tick` moved to `common.rs`, as did `rect_within`, `seed_restored_window_size`, and `test_window_with_restored_size`, instead of being copied.
- [x] 1.2 In `crates/lushtext/tests/widget/window.rs`, add the D7 resurrection regression. Show "File Has Changed on Disk" on an unmodified tab. Reload it through one production path that does not dismiss first: the Replace All target reload, the eviction reload on reselect, or reopen with an encoding. Then publish a status message and assert the editor's bar stays hidden. Record it failing on the current tree, with the observed state, in the task log.
  - Log: `test_eviction_reload_resolves_external_change_alert_for_good` (eviction reload on reselect) failed on b7e030f0 with "the reload superseded the external-change alert": the reload hid the bar but the bus record survived, so the next full pass would bring it back. Passes after 4.7.
- [x] 1.3 In `window.rs`, add the D7 survival regression. Show "Draft Not Restored" on an unmodified tab, evict it, then select it again so it reloads. Assert the alert is still shown after the reload terminal and after a later full pass. Record its result on the current tree. It passes there only through the resurrection, so it must keep passing after 4.7.
  - Log: `test_draft_not_restored_alert_survives_eviction_reload` failed on b7e030f0 with "the recovery alert must still be shown after the reload terminal": the reload hid the bar, and only a later full pass (the resurrection) would have shown it again. It passes after 4.7 without the resurrection, because "Draft Not Restored" is emitted as `InlineAlertLifetime::OutlivesLoad`.
- [x] 1.4 In `window.rs`, add the D4 Normalize regression. Load a mixed-line-ending file, type into the buffer, and activate "Normalize…". Close the line-ending dialog without choosing, then activate "Normalize…" again. Assert the dialog opens again and the buffer still holds the typed text. Record it failing on the current tree, where the second click reloads from disk.
  - Log: `test_cancelled_normalize_dialog_keeps_normalize_bound_and_unsaved_edits` failed on bd53fc09 with "the second Normalize… click must reopen the line-ending dialog": the discard closure had consumed `PendingWarningAction::NormalizeLineEndings`, so the second click took the default dismiss-and-reload path. Passes after 4.8.
- [x] 1.5 In `crates/lushtext/tests/widget/status_bar.rs` (or `window.rs` if the window is needed), add the D6 regression. Publish an `Error` status, publish and dismiss an inline alert on another tab, and assert the status lane announced once. Count the announcements through the status bar's throttler probe on the current tree and through `NotificationEvidence` after 4.3. Record it failing on the current tree.
  - Log: added in `window.rs` as `test_status_error_is_announced_once_across_unrelated_passes`, counting through `NotificationEvidence`. The status bar has no counter on the old tree, so the failing run reinstated the old behaviour inside the pass (announce the visible status on every render, counted): it failed with `left: 6, right: 1`. The companion `test_inline_alert_is_announced_once_when_first_perceivable` failed the same way with "no announcement at raise time" (`left: 1, right: 0`). Both pass with 4.2 and 4.3.
- [x] 1.6 Audit every inline-alert producer and assign each one to a column of the D7 table (superseded by the next successful load, or outlives a load). Audit every caller of `clear_inline_notification`. Record both results in this file under task 1.6, including any producer the table does not list.
  - Log (producers, every `emit_inline_notification*` call in `crates/lushtext-core/src`; the table lists all of them, none is missing):
    - superseded by the next successful load: `editor_page/monitor.rs` "File Has Changed on Disk"; `editor_page/load/execution.rs::publish_load_error` "Could Not Open File"; `editor_page/load/retirement.rs::finish_user_cancelled_load` "Loading Cancelled"; `editor_page/load/execution.rs::complete_loaded_installation` "Mixed Line Endings Detected"; `window/drafts/restore_execution.rs::finish_applied_draft` "Draft Changes Restored" / "Document Restored"; `window/drafts/restore_execution.rs::show_automatic_recovery_limit` "Automatic Recovery Paused"; `window/local_history/restore_execution.rs` "Restored from Local History".
    - outlives a load: `window/drafts/restore_execution.rs` "Draft Restore Failed" (the `Err` arm of the guarded restore), `show_stale_draft_skipped` "Draft Not Restored" (both preservation variants), `show_oversized_draft_skipped` "Draft Not Restored".
    - `window/local_history/restore_execution.rs` also calls `set_pending_warning_action(Some(UndoLocalHistoryRestore))` when an undo is cancelled; that re-arms the action of the alert already shown and emits nothing.
  - Log (`clear_inline_notification` callers): exactly one, `editor_page/load/execution.rs::complete_loaded_installation`. No test calls it. It is removed by 4.7.
- [x] 1.7 Audit every path that renders or publishes a `Warning` or `Error` status outside `render_notifications_for_status_update`, such as progress updates (design D6). Record each one, and how it keeps its announcement after 4.3, in this file under task 1.7.
  - Log: the bus is written in exactly three places outside `ui/window/notifications/`. (1) `search_progress_execution.rs::update_search_progress_status_message` publishes through `update_progress` and already renders through `render_notifications_for_status_update`, so it keeps its acknowledgement-time announcement; its messages are `Info` today, which the status lane never speaks. (2) `finish_search_progress_tracking` resolves the progress record and (3) the progress heartbeat renews it; both render through `render_notifications()`, publish no new text, and so correctly lose the re-announcement. Every `Warning` and `Error` status goes through `publish_status_message`, which renders through `render_notifications_for_status_update` and is announced in stage 3 when it wins the lane. `window/mod.rs`'s construction-time `render_notifications()` runs before anything is published. `announce_workflow_update` callers announce explicitly and are unchanged. No caller loses its only announcement.

## 2. Shared vocabulary and presentation surfaces

- [x] 2.1 Add `InlineAlertAction { Retry, Primary, Secondary, Dismiss }` to `services/notifications.rs` with a doc comment naming which button each variant is. GTK-free.
- [x] 2.2 In `ui/info_bar/mod.rs`, make `render_notification` silent. Add an unthrottled `announce(&self, &InlineActionNotification)` (one announcement on `alert_title`, lane and text from `notifications/policy.rs`). Keep a throttled entry point holding today's `AnnouncementThrottler` path for the windowless fallback. Add `pub(crate) fn activate(&self, InlineAlertAction)` and route the four `connect_clicked` closures in `imp.rs` through it, so a click and an `activate` run one code path (Dismiss still renders `None` before its callback). Add no counter to the widget (design D8).
  - Log: the announcement entry points are `announce_alert` (unthrottled) and `announce_throttled` (windowless). The first name was `announce`, which `make check-accessibility-policy` rejects as a direct `.announce(` call and which shadowed GTK's own `Accessible::announce`.
- [x] 2.3 Update the windowless fallback in `LushtextEditorPage::emit_inline_notification` and `emit_inline_notification_with_warning_action` to render and then call the throttled announcement. Confirm `test_inline_alert_announcements_use_shared_throttling_policy` and the other `editor_page.rs` inline-alert tests pass unchanged.
  - Log: the fallback calls `LushtextInfoBar::announce_throttled`; `test_inline_alert_announcements_use_shared_throttling_policy` and the other `editor_page.rs` inline-alert tests pass unchanged apart from the explicit `InlineAlertLifetime` argument added in 4.7.
- [x] 2.4 In `ui/status_bar/mod.rs`, make `render_message` silent. Expose the existing throttled `announce_visible_status` for stage 3 to call (design D6).
  - Log: `announce_visible_status` is `pub(crate)` and returns whether it spoke. `status_bar::test_visible_status_rendering_does_not_announce_info_or_flood_repeated_warnings` asserted that `render_message` announces a warning, which is the defect; it became `test_visible_status_rendering_is_silent_at_every_severity`.

## 3. Window template hosts the second instance

- [x] 3.1 In `resources/ui/window.blp`, make the `"tabs"` `StackPage` child a vertical `Box` holding `$LushtextInfoBar preview_document_alert {}` first, then `preview_layout_view` (keep its `vexpand`/`hexpand`). Run `make blueprint-generate`, refresh `resources/ui/template-contract.json`, then `make check-blueprint` and `make lint-blueprint`.
  - Log: `make lint-blueprint` already failed on main, on an unclassified `scrollable_parent` finding in `workspace-section.blp` (the `ViewportSliceBin` host, a deliberate scroll host inside the sidebar's outer scroller). Fixed under the pre-existing-blockers rule by classifying it in `scripts/blueprint-templates.sh`. The new `$LushtextInfoBar` child adds one `scrollable_parent` finding of the same shell-child class, so `window.blp`'s classified ceiling went from 8 to 9.
- [x] 3.2 In `ui/window/imp.rs`, add the `TemplateChild` and call `LushtextInfoBar::ensure_type()` in `class_init` before `bind_template()`. Add `preview_document_alert_ticket: RefCell<Option<DocumentAlertProjectionTicket>>`, the `InlineAlertAnnouncementLedger`, and the announcement counts, grouped with `notification_bus`.
- [x] 3.3 Confirm that no `ui/markdown_preview/**` file and no `markdown-preview.blp` change is needed. Confirm that `window.rs`'s existing `preview_layout_view` and shell-geometry tests pass unchanged.
  - Log: no `ui/markdown_preview/**` or `markdown-preview.blp` change. The shell-geometry and preview tests (`preview` filter, 186 tests) pass unchanged.

## 4. Notification workflow: projection, forward, announcement, load resolve

- [x] 4.1 In `ui/window/notifications/policy.rs`, add `DocumentAlertProjectionTicket`, `DocumentAlertFacts`, `document_alert_projection_is_current`, `plan_preview_document_alert`, `InlineAlertSurface`, `inline_alert_perceivable_on`, and `inline_alert_announcement_due`, with no GTK imports. Add unit tests:
  - each ticket/facts field mismatch is stale, and a matching pair is current;
  - the plan is `Some` only for preview-only with an alert;
  - perceivability is the preview surface for preview-only plus selected, the editor bar for selected without preview-only, and none for a non-selected editor;
  - an announcement is due on first appearance, on a changed key, and on reappearance after clearing, but not on an unchanged key.
- [x] 4.2 In `ui/window/notifications/execution.rs`:
  - render every surface silently and add the preview step (design D3);
  - reconcile the ledger against visited tabs, dropping closed or alert-free owners;
  - for each visited editor, announce only where `inline_alert_perceivable_on` is `Some` and the key is due, on that one surface, then record it (design D5), leaving the ledger untouched when the editor is not perceivable;
  - count each announcement by surface;
  - hand focus off when the preview copy hides while focus is inside it and preview-only stays active (design D4).
  - Log: the focus handoff is applied on both surfaces. On the editor's own bar it was needed on the previous tree too: GTK leaves focus on a focused Dismiss button after it hides itself (task 6.4).
- [x] 4.3 In stage 3 (`project_notification_surfaces` with `Some(expected)`), call the status bar's announcement beside the pulse when `status_update_is_visible` holds, and count it (design D6). Apply the task-1.7 audit result.
  - Log: done with the 1.7 audit result; no caller needed a change.
- [x] 4.4 Add `ui/window/notifications/evidence.rs` with `NotificationEvidence` and one `notification_evidence()` accessor (design D8). Read `preview_document_alert` through `try_get()`, and drop every `Ref` before building the value. Add the three driven proofs: side-effect freedom across publish, dismiss, a selection change, and a mode change; disposal honesty; and materialization neutrality, stated as vacuous with the reason.
  - Log: the three proofs are `window::test_notification_evidence_reads_stay_side_effect_free_across_mutation`, `window::test_notification_evidence_survives_window_disposal`, and `window_preview::test_preview_surface_evidence_reads_stay_side_effect_free` (the preview fields); materialization neutrality is stated as vacuous in the module doc.
- [x] 4.5 In `ui/window/notifications/mod.rs`, add `wire_preview_document_alert` (the four forwarding callbacks guarded by the ticket; a stale ticket re-projects through `render_notifications()`) and `notification_evidence`. Update the facade narrative:
  - stage 2 projects three surfaces and announces once per perceivable alert;
  - the forward and its resumption point (the click closure);
  - stage 3 announces the status lane;
  - the module-roles table gains `evidence`;
  - the called-presentation-surface list names the window-hosted instance;
  - the D7 superseded-by-load table;
  - the D1 rule (`EditorInfoBar` carries document-state alerts only).

  Replace the "What a test reads" section's no-surface conclusion. Keep the facade within 370 lines.
- [x] 4.6 Call `wire_preview_document_alert` from the window's `constructed`. Call `render_notifications()` from `apply_preview_shell_state` (`ui/window/preview.rs`, after the layout and visibility writes) and from `refresh_selected_tab_model_projections` (`ui/window/documents.rs`). Drop the closed editor's ledger entry in `handle_tab_detached` (`ui/window/tab_strip/close_execution.rs`) beside `dismiss_editor_notifications`.
  - Log: `wire_preview_document_alert` is called from `constructed`, before the first `apply_preview_shell_state`.
- [x] 4.7 Implement D7:
  - give the editor a superseded-by-load flag that every emit sets or resets, beside `PendingWarningAction`;
  - have each producer pick its emit variant per the task-1.6 audit;
  - replace `complete_loaded_installation`'s `clear_inline_notification()` with `resolve_inline_notification_superseded_by_load()`, routed through a `connect_inline_notification_superseded` callback installed in `wire_info_bar` to `resolve_editor_inline_notification`, with the direct render as the windowless fallback.

  Confirm tasks 1.1, 1.2, and 1.3 pass.
  - Log: the flag is `InlineAlertLifetime { SupersededByLoad, OutlivesLoad }` in `editor_page/imp.rs`, stored beside `PendingWarningAction` and set by every emit, which now takes it as an explicit argument. Tasks 1.2 and 1.3 pass; 1.1 is confirmed with the preview surface (task 4.5).
- [x] 4.8 Implement the D4 non-consuming warning action: add `pending_warning_action()` (read only), switch `wire_info_bar`'s discard closure to it, and remove `take_pending_warning_action` if nothing else calls it. Confirm task 1.4 passes.

## 5. Automation snapshot

- [x] 5.1 Add `preview_document_alert_visible: bool` to `AutomationSurfaceSnapshot` (`model/automation.rs`, including its default/fixture constructor), projected in `ui/automation.rs::surface_snapshot` from `NotificationEvidence` (design D8, D9).
- [x] 5.2 Document the field in `docs/automation.md` and `docs/automation-reference.md` (`snapshot-field-preview-document-alert-visible`), add the AT-SPI anchor row `atspi-anchor-preview-document-alert-alert-could-not-open-file`, and run `make check-automation-docs`. Run `make automation-client-self-test` only if the client or its fixtures changed.
  - Log: three anchor rows (the alert and its Retry and Dismiss buttons). `make check-automation-docs` passes; the client and its fixtures did not change.

## 6. Widget, accessibility, and visual tests

- [x] 6.1 `window_preview.rs`: the reload flow. "Discard Changes and Reload" on the preview surface reloads disk bytes, the rendered preview contains the new heading, and both surfaces are cleared.
- [x] 6.2 `window_preview.rs`: side-by-side with an alert shows the editor bar and keeps the preview copy hidden, and exactly one visible widget in the window shows the alert title. Editor-only is checked the same way. At the narrowest supported window in side-by-side, the editor bar's allocation does not intersect the preview pane's.
  - Log: the narrowest window is 640 px wide (the window's minimum). There the side-by-side pane and the editor bar do not intersect.
- [x] 6.3 `window_preview.rs`: tab-switch re-projection (show for an alert tab, clear for a non-alert tab, reappear on reselect), closing the selected tab, an action after an ordinary tab switch acting on the newly shown alert, and the stale forward. For the stale forward, change the selection inside a deferred tab-projection batch, then `activate` the preview copy before the batch's terminal rebuild. No handler runs, as observed through the editor's buffer, load state, and bus views, and the copy is re-projected.
  - Log: the stale forward is driven by raising `tab_projection_refresh_defer_depth` around the selection change, as a session-restore batch or bulk close does.
- [x] 6.4 `window_preview.rs`: dismiss and Save from the preview surface (restored-draft fixture) run the editor's handlers. A non-Markdown document's alert shows above the "Not a Markdown file" placeholder. Escape leaves the alert shown. Focus:
  - after Dismiss and after Discard-reload on a Markdown document, focus is on the preview `text_view`;
  - after Dismiss on a non-Markdown document, the window has no focus widget;
  - after "Normalize…" then a line-ending choice, record where Libadwaita returns focus when the dialog closes and the alert has hidden the originating button; if focus is stranded on the sidebar or a hidden widget, close it with the D4 rule, and if the result decides the design, record it as a GTK axiom with a probe;
  - measure the same Dismiss case on the editor's own bar on the current tree, and if focus is stranded, apply the D4 rule there (the selected editor's `source_view`) and test it.
  - Log (measured on GTK 4.22 / Libadwaita 1.9): after Dismiss and Discard-reload on a Markdown document focus is on the preview `text_view`; after Dismiss on a non-Markdown document the window has no focus widget. After "Normalize…" and a line-ending choice Libadwaita returned focus to the preview `text_view` (`GtkTextView`), so no extra rule was needed. On the editor's own bar a focused Dismiss left focus on the hidden button (`GtkButton`) on the previous tree; the D4 rule now moves it to the editor's `source_view` (`window::test_editor_alert_dismiss_does_not_strand_focus`). Neither result became a GTK axiom: the handoff checks whether focus is inside the hidden bar, so it holds whichever way GTK behaves. The Normalize-dialog test lives in `window.rs`, which owns the alert-dialog helpers.
- [x] 6.5 `window_preview.rs`: in a narrow preview-only window (create the window at the target size; see the GTK4 resize note in AGENTS.md), the action group is allocated below the message, and Discard and dismiss stay adjacent. In a wide window they share one row. Reuse the geometry assertions the existing narrow inline-alert tests in `editor_page.rs` use.
  - Log: deviation recorded in design.md (D2) and the spec. The preview column is never narrower than about 600 px (640 px minimum window), and at the default text size the message wraps its own text first, so the action group never left the row at any supported window size (measured: 596–976 px columns). The narrow case is therefore enlarged text (`gtk-xft-dpi` at twice the default) in the 640 px window, where the group wraps as one row; the wide case is 1400 px at the default size.
- [x] 6.6 `window_preview.rs` or `multi_window_drafts.rs`: two windows, one in preview-only and one in editor-only, each project and announce only their own selected tab's alert.
- [x] 6.7 `accessibility.rs` (or `window_preview.rs` if the accessibility helpers are shared): in preview-only mode the preview copy has `Group`/`Alert` roles, a labelled description, and reachable buttons, and the editor's bar is not exposed. Count announcements through `NotificationEvidence.announcements`:
  - one per alert, on the perceivable surface;
  - none on entering preview-only mode after the alert was announced;
  - none at raise time for a background-tab alert, and one when that tab is selected;
  - none when switching back to an announced alert;
  - none for an error on unrelated publishes;
  - one for a changed alert.
  - Log: in `window_preview.rs` (`test_preview_surface_accessibility_and_announce_once`), with `AccessibleAudit` for the roles and the labelled description.
- [x] 6.8 Add the `preview-mode-document-alert` case to `scripts/run-accessibility-smoke.sh` (design D9), mapped to `A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, and `A11Y-ERROR-SURFACES`, and register it in the case list and the matrix mapping.
  - Log: the full lane first failed in `editor-save-completion`: the new host `Box` deepened the accessible tree by one level and `capture-lushtext-mutter.py` still ran `atspi-set-text.py` at its default depth of 30, so the editor text view (now depth 31) was out of reach. All its AT-SPI helpers now share `ATSPI_MAX_DEPTH = 44`, the tree dump's depth. The new case's deliberate permission-denied load of `unreadable-load-target.md` was added to `scripts/accessibility_warning_allowlist.py` beside its `.txt` twin.
  - Log: `scripts/run-accessibility-smoke.sh --case preview-mode-document-alert` passes, asserting exactly one showing "Could Not Open File" alert plus Retry and Dismiss.
- [x] 6.9 Add the `preview-mode-document-alert` scenario to `scripts/run-visual-smoke.sh` (light and dark, and low Background Opacity), with its matrix row ids, confirming the copy stays opaque and sits above the preview without overlap.
  - Log: three cases, `preview-mode-document-alert` (light), `-dark`, and `-transparent` (Background Opacity 0.35). Each asserts the snapshot field and finds one opaque error-coloured band (dominant colour share 0.97) above an uncovered preview.

## 7. Documentation

- [x] 7.1 `docs/workflow-readability-matrix.md`: update `WFR-STATUS-NOTIFICATIONS`. Re-derive its measured cells (row-scoped, excluding `#[cfg(test)]`), and declare `evidence.rs` in the file set and the evidence cell, replacing the "none, and that is the recorded conclusion" cell. Record the new pure policy (mutant gain) and the new seam value object `DocumentAlertProjectionTicket` + `DocumentAlertFacts`. Record the unchanged zero seam count, and add the window-hosted instance under called presentation surfaces. Update its Migrated Workflow Roles entry. `WFR-MARKDOWN-PREVIEW` is unchanged. Run `make check-workflow-boundaries`.
  - Log: re-derived as 2,966 physical / 2,031 production (ui 1,580 / services 451); policy mutants 11 → 29; seam count still zero; the row left the "no value object" list. `make check-workflow-boundaries` passes.
- [x] 7.2 `docs/accessibility.md` (the preview-only section, inline alerts, and status announcements), `docs/accessibility-matrix.md` (`A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, `A11Y-SHELL-ERROR-STATUS`, `A11Y-MARKDOWN-CONSTRAINED`, `A11Y-ERROR-SURFACES`, plus both new smoke-case mappings), `docs/accessibility-orca-checklist.md` (one workflow row for the preview-only alert and one for the background-tab announcement on selection), and `docs/end-user-coverage.md` (both new smoke cases).
- [x] 7.3 README "Notes, History, and Preview": one sentence saying document alerts (changed on disk, restored drafts, load errors) stay visible above the preview in preview-only mode.
- [x] 7.4 AGENTS.md:
  - module layout: `ui/window/notifications/` gains `evidence.rs`, and `ui/info_bar/` names the window-hosted preview-only instance;
  - amend the "Inline alerts", "Status bar notification lifecycle", and "Markdown preview" key design decisions with the second instance (preview-only only, hosted by the window), forward-by-`activate`, the one announcement when an alert first becomes perceivable, status announcement on acknowledgement, and the superseded-by-load resolve.

  In `.agents/rules/ui.md`'s Inline Alerts section, amend the "above each editor page" bullet for the preview-only instance and add the D1 rule and the D7 table. Mirror them in `widget-wiring.md` where it describes inline alerts.
  - Log: `widget-wiring.md` has no inline-alert section, so there was nothing to mirror; `ui.md`'s widget hierarchy also gained the host `Box`.
- [x] 7.5 If the new `window_preview` and `window` tests move either shard's measured wall time materially, re-record `ci_minutes`/`measured_in` in `scripts/widget-shards.py` from the CI measurement artifacts, and keep each shard within 20 minutes (`make check-widget-shards`).

## 8. Verification

- [x] 8.1 `make check` (fmt, all-feature Clippy, policy audits), `make check-blueprint`, and `make check-workflow-boundaries`.
- [x] 8.2 `cargo nextest` for `lushtext-core` unit tests covering `notifications/policy.rs` and `services/notifications.rs`.
- [x] 8.3 The widget shards owning `window_preview`, `window`, `editor_page`, `status_bar`, and `accessibility` (`make test-widget-shard WIDGET_SHARD=<shard>`). Run every new and touched test in isolation as well. Any `FLAKY:` is a blocker under `preexisting-blockers.md`.
- [x] 8.4 `make accessibility-smoke` and `make visual-smoke` for the new cases. Each skips explicitly when host support is unavailable.
