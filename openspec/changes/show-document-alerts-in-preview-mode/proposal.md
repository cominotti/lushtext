## Why

In Markdown preview-only mode (`Alt+P`, `win.toggle-preview-mode`), a file that changes on disk never shows its yellow "File Has Changed on Disk" alert. The user has to leave preview mode to see it. The alert is a `LushtextInfoBar`, a child of each `LushtextEditorPage` (`resources/ui/editor-page.blp`). Every page sits in `tab_view` inside `editor_box`, which is the `[editor]` slot of `preview_layout_view` (`resources/ui/window.blp`). The `preview` `Adw.Layout` places only the `preview` slot, and `apply_preview_shell_state` also hides `editor_box`. So the whole editor is off screen, banner included. LushText does not auto-reload, so in preview-only mode the user reads stale content and gets no signal.

Every other editor inline alert has the same blind spot: the restored-draft and stale-draft alerts, "Could Not Open File", "Loading Cancelled", "Mixed Line Endings Detected", "Restored from Local History" (Undo Restore), "Draft Not Restored", and "Automatic Recovery Paused". Every one of them describes the document, not the editor widget. The principle agreed with the maintainer: **a document-state alert is visible on every surface that shows the document.**

## What Changes

- **A second presentation of the selected tab's alert, at the top of the Markdown preview.** The preview template (`resources/ui/markdown-preview.blp`) gains a second `LushtextInfoBar` instance above the scrolled text and the placeholder. The window's notification workflow (`ui/window/notifications/`) projects the **selected** tab's current inline notification onto it, read from the same `NotificationBus::editor_info_bar_view` the editor's own bar renders from. It has the same title, body, style, and buttons. The per-editor info bar is not moved, reparented, or duplicated in state.
- **Shown only while preview-only is active.** In editor-only and side-by-side layouts the editor's own bar is on screen, so the preview copy stays hidden and the user never sees two banners. It shows whatever the preview is displaying: rendered Markdown, the "Preparing Markdown preview…" placeholder, or "Not a Markdown file".
- **Same handlers, by construction.** A button on the preview copy forwards to the selected editor's own info bar, which runs the closures `wire_info_bar` installed (Retry, Discard Changes and Reload / Discard / Normalize… / Undo Restore / Show in Local History, Save, Dismiss). The forward is guarded: a click acts only if the selected tab and its alert are still the ones that were projected. Either surface's action updates both, because both re-render from the bus.
- **Re-projection points.** Every bus publish, dismiss, resolve, and expiry sweep (the existing full projection pass), every preview-mode change (`apply_preview_shell_state`), and every selected-tab change (`refresh_selected_tab_model_projections`).
- **One screen-reader announcement per alert, not per surface.** The announcement decision moves from the widget's render call into the projection pass. A window-owned ledger records the alert key last announced for each editor. The pass announces an alert once, from the surface the user can currently perceive, when its key first appears for that editor. Re-rendering an unchanged alert, switching layouts, or switching tabs announces nothing new. This also stops a pre-existing chatter: today an `Error`-lane alert, whose lane bypasses throttling, is re-announced on every full pass, and any publish on any tab triggers one. That contradicts `A11Y-EDITOR-ERROR`'s "not repeated heartbeats".
- **Pre-existing blocker fixed in the same stream: load success clears the alert behind the bus's back.** `load::execution` calls `editor.clear_inline_notification()`, which hides the widget but leaves the bus record in place. The next projection pass would bring a resolved alert back, on the editor bar today and on the preview copy after this change. The clear is routed through the notification workflow as a `resolve` of that editor's info-bar surface.
- **Automation:** a new bounded snapshot field `surfaces.preview_document_alert_visible` (`bool`, the rendered visibility of the preview copy). Documented in `docs/automation.md` and `docs/automation-reference.md`, with a stable AT-SPI anchor row for the preview copy's alert.
- **Tests:** widget tests in `crates/lushtext/tests/widget/window_preview.rs` for the external-change and Reload flow in preview-only, exactly one visible alert in side-by-side, tab-switch re-projection and clearing, forwarded actions, and narrow-width wrapping. Unit tests for the new pure policy. An accessibility widget assertion and an accessibility-smoke case `preview-mode-document-alert`.
- **Docs:** `docs/workflow-readability-matrix.md` (`WFR-STATUS-NOTIFICATIONS` re-derived, `WFR-MARKDOWN-PREVIEW` hosting note), the notifications facade narrative, `docs/accessibility.md`, `docs/accessibility-matrix.md`, `docs/accessibility-orca-checklist.md`, `docs/end-user-coverage.md`, README (Notes, History, and Preview), and AGENTS.md (the inline-alerts and Markdown-preview design decisions).

No persisted format, GSettings key, action, D-Bus method, or readiness predicate changes. No new module file is created.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `editor-inline-alerts`: adds requirements that a document-state inline alert is also presented on the preview-only surface for the selected tab, with the same actions, consistent state across both surfaces, re-projection on tab switch, exactly one visible banner in every layout, one announcement per alert, constrained-width wrapping on the preview surface, and that a successful load resolves the alert through the notification bus.
- `external-file-monitor-coverage`: adds a requirement that an external modification is covered end to end in preview-only mode: the warning shows on the preview surface, and Reload refreshes the rendered preview.
- `dbus-automation-spine`: adds a requirement for the `surfaces.preview_document_alert_visible` snapshot field and its documentation.

## Impact

- **Code:** `resources/ui/markdown-preview.blp` and its generated `.ui`, `resources/ui/template-contract.json`; `ui/markdown_preview/imp.rs` (template child, `LushtextInfoBar::ensure_type()`) and `widgets.rs` (one accessor, no policy); `ui/info_bar/mod.rs` (split silent render from announcement; `activate(InlineAlertAction)` reusing the click path); `services/notifications.rs` (GTK-free `InlineAlertAction` vocabulary); `ui/window/notifications/{mod.rs,policy.rs,execution.rs}` (projection, ledger, forward guard); `ui/window/imp.rs` (ledger and projection-ticket state); `ui/window/preview.rs` and `ui/window/documents.rs` (re-projection call sites, clear-as-resolve wiring); `ui/editor_page/mod.rs` and `ui/editor_page/load/execution.rs` (clear routed through the window callback); `ui/automation.rs` and `model/automation.rs` (snapshot field).
- **Tests:** `crates/lushtext/tests/widget/window_preview.rs`, `window.rs` (resurrection regression), `editor_page.rs` (announcement split keeps the windowless path), `accessibility.rs`; `scripts/run-accessibility-smoke.sh` case; unit tests in `notifications/policy.rs`.
- **GTK axioms:** none added (see design.md, "GTK axioms").
- **Risk:** low. The change adds one hidden-by-default widget and one projection step. The announcement change narrows when errors are re-announced, which is intended and covered by tests.
