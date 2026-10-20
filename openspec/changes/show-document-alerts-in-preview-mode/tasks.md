## 1. Failing-first tests (no production behaviour change)

- [ ] 1.1 In `crates/lushtext/tests/widget/window_preview.rs` (already in a widget shard, so `scripts/widget-shards.py` needs no edit), add `test_external_change_in_preview_only_shows_alert_on_preview_surface`. Open a Markdown file, enter preview-only through `win.toggle-preview-mode`, write the file with the existing `write_external_change_after_mtime_tick` pattern from `window.rs` (move it to `common.rs` if both files need it; do not copy it), and wait on a predicate with a timeout for the preview surface's title to read "File Has Changed on Disk". Record it failing on the current tree (the surface does not exist yet, so it fails to compile first). Then check it fails for the right reason once the template child exists.
- [ ] 1.2 In `crates/lushtext/tests/widget/window.rs`, add the D6 resurrection regression: publish an inline alert for an editor, complete a load of that document through `load_file_async` without dismissing, publish an alert on a second tab, and assert the first editor's bar stays hidden. Record it failing on the current tree, with the observed state, in the task log.
- [ ] 1.3 Audit every caller of `LushtextEditorPage::clear_inline_notification` and every inline-alert producer for an alert published before a load success and expected to survive it (design D6). Record the result, including "none found" if that is the outcome, in this file under task 1.3.

## 2. Shared vocabulary and info-bar presentation surface

- [ ] 2.1 Add `InlineAlertAction { Retry, Primary, Secondary, Dismiss }` to `services/notifications.rs` with a doc comment naming which button each variant is. GTK-free.
- [ ] 2.2 In `ui/info_bar/mod.rs`, make `render_notification` silent. Add `announce(&self, &InlineActionNotification)` holding the current announcement code, plus the production read-only counter `announcements_emitted() -> u64`. Add `pub(crate) fn activate(&self, InlineAlertAction)` and route the four `connect_clicked` closures in `imp.rs` through it, so a click and an `activate` run one code path (Dismiss still renders `None` before its callback).
- [ ] 2.3 Update the windowless fallback in `LushtextEditorPage::emit_inline_notification` and `emit_inline_notification_with_warning_action` to call `render_notification` and then `announce`. Confirm `test_inline_alert_announcements_use_shared_throttling_policy` and the other `editor_page.rs` inline-alert tests pass unchanged.

## 3. Preview template hosts the second instance

- [ ] 3.1 Add `$LushtextInfoBar document_alert {}` as the first child of `resources/ui/markdown-preview.blp`. Run `make blueprint-generate` and refresh `resources/ui/template-contract.json`, then `make check-blueprint`.
- [ ] 3.2 In `ui/markdown_preview/imp.rs`, add the `TemplateChild` and call `LushtextInfoBar::ensure_type()` in `class_init` before `bind_template()`. In `widgets.rs`, add `pub(crate) fn document_alert(&self) -> &LushtextInfoBar` and nothing else.
- [ ] 3.3 Add a widget test proving `clear()`, `clear_source_snapshot()`, `show_content_placeholder()`, and a full render do not change the instance's visibility or labels.

## 4. Notification workflow: projection, forward, announcement

- [ ] 4.1 In `ui/window/notifications/policy.rs`, add `DocumentAlertProjectionTicket`, `DocumentAlertFacts`, `document_alert_projection_is_current`, `plan_preview_document_alert`, `InlineAlertAnnouncer`, `inline_alert_announcer`, and `inline_alert_announcement_due`, with no GTK imports. Add unit tests: each ticket/facts field mismatch is stale, a matching pair is current, the plan is `Some` only for preview-only with an alert, the announcer is the preview surface only for preview-only plus selected, and announcement is due on first appearance, on a changed key, and on reappearance after clearing, but not on an unchanged key.
- [ ] 4.2 In `ui/window/imp.rs`, add `preview_document_alert_ticket: RefCell<Option<DocumentAlertProjectionTicket>>` and the `InlineAlertAnnouncementLedger` state, grouped with `notification_bus`.
- [ ] 4.3 In `ui/window/notifications/execution.rs`: render every surface silently, add the preview step (design D3), reconcile the ledger against visited tabs (dropping closed or alert-free owners), and call `announce` on exactly one surface per due alert (design D5). When the preview copy hides while a button on it had focus and preview-only is active, move focus to the preview content (design D4).
- [ ] 4.4 In `ui/window/notifications/mod.rs`, add `render_preview_document_alert` and `wire_preview_document_alert` (the four forwarding callbacks guarded by the ticket). Update the facade narrative: stage 2 now projects three surfaces, the announcement sub-step, the forward and its resumption point (the click closure), and the called-presentation-surface list now names the preview-hosted instance. Keep the facade within 370 lines.
- [ ] 4.5 Call `wire_preview_document_alert` from the window's `constructed`. Call `render_preview_document_alert` from `apply_preview_shell_state` (`ui/window/preview.rs`, after the layout and visibility writes) and from `refresh_selected_tab_model_projections` (`ui/window/documents.rs`).
- [ ] 4.6 Route `clear_inline_notification` through the window: install `connect_inline_notification_cleared` in `wire_info_bar` so it calls `resolve_editor_inline_notification`, and keep the direct render as the windowless fallback (design D6). Confirm tasks 1.2 and 1.1 now pass.

## 5. Automation snapshot

- [ ] 5.1 Add `preview_document_alert_visible: bool` to `AutomationSurfaceSnapshot` (`model/automation.rs`, including its default/fixture constructor) and derive it in `ui/automation.rs::surface_snapshot` from the rendered widget state (design D7).
- [ ] 5.2 Document the field in `docs/automation.md` and `docs/automation-reference.md` (`snapshot-field-preview-document-alert-visible`), add the AT-SPI anchor row `atspi-anchor-preview-document-alert-alert-could-not-open-file`, and run `make check-automation-docs`. Run `make automation-client-self-test` only if the client or its fixtures changed.

## 6. Widget and accessibility tests

- [ ] 6.1 `window_preview.rs`: the reload flow, where "Discard Changes and Reload" on the preview surface reloads disk bytes, the rendered preview contains the new heading, and both surfaces are cleared.
- [ ] 6.2 `window_preview.rs`: side-by-side with an alert shows the editor bar and keeps the preview copy hidden, and exactly one visible widget in the window shows the alert title. Editor-only is checked the same way.
- [ ] 6.3 `window_preview.rs`: tab-switch re-projection (show for an alert tab, clear for a non-alert tab, reappear on reselect), closing the selected tab, and a stale forward (switch tabs, then `activate` the preview copy in the same turn: no handler runs, as observed through the editor's buffer, load state, and bus views).
- [ ] 6.4 `window_preview.rs`: dismiss and Save from the preview surface (restored-draft fixture) run the editor's handlers. A non-Markdown document's alert shows above the "Not a Markdown file" placeholder. Focus lands on the preview content after the alert resolves.
- [ ] 6.5 `window_preview.rs`: at a narrow preview-only window (create the window at the target size; see the GTK4 resize note in AGENTS.md), the action group is allocated below the message and Discard and dismiss stay adjacent. At a wide window they share one row. Reuse the geometry assertions the existing narrow inline-alert tests in `editor_page.rs` use.
- [ ] 6.6 `accessibility.rs` (or `window_preview.rs` if the accessibility helpers are shared): in preview-only mode the preview copy has `Group`/`Alert` roles, a labelled description, and reachable buttons, while the editor's bar is not exposed. Announcements are counted with `announcements_emitted()`: one per appearance on the perceivable surface, none on entering preview-only mode, none for an error on unrelated publishes, and one for a changed alert.
- [ ] 6.7 Add the `preview-mode-document-alert` case to `scripts/run-accessibility-smoke.sh` (design D7), mapped to `A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, and `A11Y-ERROR-SURFACES`, and register it in the case list and matrix mapping.

## 7. Documentation

- [ ] 7.1 `docs/workflow-readability-matrix.md`: re-derive `WFR-STATUS-NOTIFICATIONS`'s measured cells (row-scoped, excluding `#[cfg(test)]`), and record the new pure policy (mutant gain), the new seam value object `DocumentAlertProjectionTicket` + `DocumentAlertFacts`, the unchanged zero seam count, and the preview-hosted instance under called presentation surfaces. Update its Migrated Workflow Roles entry. Add a hosting note to `WFR-MARKDOWN-PREVIEW` (file set unchanged; the instance is not a preview role). Run `make check-workflow-boundaries`.
- [ ] 7.2 `docs/accessibility.md` (preview-only section and inline alerts), `docs/accessibility-matrix.md` (`A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, `A11Y-SHELL-ERROR-STATUS`, `A11Y-MARKDOWN-CONSTRAINED`, `A11Y-ERROR-SURFACES`), `docs/accessibility-orca-checklist.md` (one workflow row for the preview-only alert), and `docs/end-user-coverage.md` (the new smoke case).
- [ ] 7.3 README "Notes, History, and Preview": one sentence saying document alerts (changed on disk, restored drafts, load errors) stay visible above the preview in preview-only mode.
- [ ] 7.4 AGENTS.md: amend the "Inline alerts" and "Markdown preview" key design decisions (the second instance, preview-only only, forward-by-`activate`, one announcement per appearance, clear-as-resolve). Mirror the facade's surface list in `.agents/rules/ui.md` and `widget-wiring.md` where they describe inline alerts: the `EditorInfoBar` surface carries document-state alerts only.

## 8. Verification

- [ ] 8.1 `make check` (fmt, all-feature Clippy, policy audits) and `make check-blueprint`.
- [ ] 8.2 `cargo nextest` for `lushtext-core` unit tests covering `notifications/policy.rs` and `services/notifications.rs`.
- [ ] 8.3 The widget shards owning `window_preview`, `window`, `editor_page`, and `accessibility` (`make test-widget-shard WIDGET_SHARD=<shard>`). Run every new and touched test in isolation as well. Any `FLAKY:` is a blocker under `preexisting-blockers.md`.
- [ ] 8.4 `make accessibility-smoke` for the new case (skips explicitly when AT-SPI is unavailable), and a `make visual-smoke` screenshot of the preview-only alert in light and dark themes, confirming the copy stays opaque at low Background Opacity.
