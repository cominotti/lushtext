## Context

### Where the alert lives today

- `LushtextInfoBar` (`ui/info_bar/`) is the first child of every `LushtextEditorPage` (`resources/ui/editor-page.blp`). It is a `GtkRevealer` wrapping an `AdwWrapBox` (`content_wrap`). The message box and the action group (`retry_button`, `discard_button`, `save_button`, `dismiss_button`) are the wrap box's two children.
- The page sits in `tab_view` inside `editor_box`, which is the `[editor]` child of `preview_layout_view` (`resources/ui/window.blp`). The `"editor"` `Adw.Layout` places `editor` in the `AdwOverlaySplitView` content slot and `preview` in its sidebar slot. The `"preview"` layout places only `preview`. `LushtextWindow::apply_preview_shell_state` (`ui/window/preview.rs`) also sets `editor_box` invisible in preview-only mode. In preview-only mode nothing in the editor is on screen or in the accessibility tree, and that includes the banner.
- `preview_layout_view` is the `"tabs"` page child of `content_stack`. The window's other notification surface, the status bar, is also in `window.blp`.

### Who owns the state: one source of truth already exists

- **Owner:** the window's `NotificationBus` (`services/notifications.rs`), a GTK-free reducer, one per window. Editor alerts are records on `NotificationSurface::EditorInfoBar(owner_id)` owned by `NotificationOwner::Editor(owner_id)`. Publishing an inline alert replaces the previous record for the same owner and surface, so an editor has at most one. `owner_id` is `LushtextEditorPage::notification_owner_id()` (the object address). `ui/window/notifications/policy.rs::EditorNotificationTarget` derives both halves once. The answer for a surface is `NotificationBus::editor_info_bar_view(owner_id) -> Option<InlineActionNotification>`.
- **Publish path:** producers call `editor.emit_inline_notification[_with_warning_action]`. The window installed `connect_inline_notification` in `documents.rs::wire_info_bar`, so the call becomes `publish_editor_inline_notification`. That is stage 1, followed by `render_notifications()`.
- **Projection:** `notifications/execution.rs::project_notification_surfaces` does a full pass. It renders the status lane, then every tab's `editor_info_bar_view` onto that tab's `info_bar()`. The pass is bounded by the tab count.
- **Per-editor side state that is not on the bus:** `PendingWarningAction` (which flow the warning-primary button runs) and `is_draft_restored()` live on the editor. Every emit resets or sets `PendingWarningAction`, so it always belongs to the editor's latest alert. The handlers read it when a button is clicked, not when it is rendered. Because the preview surface forwards to the same handlers, they keep reading the same state.

### Pre-existing defects found while tracing the state

Each one was checked against the current tree. All four are fixed in this change, under `.agents/rules/preexisting-blockers.md`.

1. **Load success hides the alert behind the bus's back.** `ui/editor_page/load/execution.rs::complete_loaded_installation` calls `editor.clear_inline_notification()`, which renders `None` straight into the widget. The bus record survives, and the next full pass brings it back. Any status message or publish on any tab runs a full pass. This is not latent. Three production paths reach load success without dismissing first:
   - the eviction reload when an evicted tab is selected again (`editor_memory_eviction::reload_if_evicted`);
   - the Replace All target reload of an open, unmodified file (`window/search.rs`);
   - reopen with a chosen encoding (`encoding/execution.rs::reopen_now`).

   Example: a "File Has Changed on Disk" alert on an unmodified tab, then Replace All rewrites that file. The reload hides the alert, then the "Replaced …" status message runs a pass and the stale alert comes back over current content. (Retry and Discard dismiss before reloading. The draft check runs after load success, so first opens are not affected.)
2. **Inline alerts re-announce on unrelated passes.** `render_notification` announces inside the widget through a per-widget `AnnouncementThrottler`. `AnnouncementLane::Alert` bypasses throttling, so every full pass re-announces every error alert. Warnings use `StatusUpdate`, which has a 2 s cooldown, so an unchanged warning is re-announced by any pass more than 2 s after the last one. This happens on background tabs too, from unmapped widgets.
3. **The status lane has the same defect.** `StatusBar::render_message` announces on every render, and an `Error` status maps to the `Alert` lane. So a visible error status ("Could not open …") is re-announced by every pass until it expires 10 s later. That contradicts `A11Y-ERROR-SURFACES` ("high-priority alerts bounded and throttled"). It is in the same pass this change extends. D3 also runs the pass at every tab and mode change, which would make this worse.
4. **A cancelled "Normalize…" turns the button into a silent reload.** `wire_info_bar`'s discard closure *takes* `PendingWarningAction`. For `NormalizeLineEndings` it opens the line-ending dialog and returns, and the alert stays. If the user cancels the dialog, the action is gone. The next click on the same "Normalize…" button falls through to the default path: dismiss, then `load_file_async`. That reloads the file from disk and drops any unsaved edits without asking. The preview surface would expose the same button, so the bug now has two entry points.

### Action wiring

`documents.rs::wire_info_bar` installs four closures per editor on that editor's bar: `connect_retry`, `connect_discard` (dispatching on `take_pending_warning_action()` and then `is_draft_restored()`), `connect_save` (`window.save_current()`, which saves the **selected** editor), and `connect_dismissed`. The dismiss button first renders `None` on itself, then runs `dismissed_callback`.

### The preview widget

`LushtextMarkdownPreview` (`resources/ui/markdown-preview.blp`) is a vertical `Box` holding `scrolled_window` (the `text_view`) and an `Adw.StatusPage placeholder`. It is not only the window's preview. The notes browser (`window/notes/browser.rs`) and every note editor (`window/notes/editor_execution.rs`) build their own instances.

### The side-by-side split never collapses

`preview_split_view` is an `AdwOverlaySplitView`. Neither the template nor any breakpoint sets its `collapsed`; the only `collapsed` setters target `workspace_split_view`. `geometry/execution.rs` pins its sidebar, and its width is `clamped_preview_width`, at most one third of the available content width (Kani-proved in `ui/markdown_preview/policy.rs`). So in side-by-side the preview is always a pane beside the editor column and never an overlay over it. The editor's own banner, at the top of that column, stays on screen.

## Goals / Non-Goals

**Goals:**

- A document-state inline alert is visible in preview-only mode, for the selected tab, with the same text, style, and actions as the editor's bar, running the same handlers.
- Exactly one banner is visible in every layout: editor-only, side-by-side, and preview-only.
- Either surface's action updates both. Switching tabs re-projects or clears the preview copy.
- An alert is announced exactly once, when it first becomes perceivable on any surface (D5).
- The projection stays inside the notification workflow (`WFR-STATUS-NOTIFICATIONS`). `ui/info_bar/` stays a called presentation surface. The Markdown preview widget is not touched.

**Non-Goals:**

- Moving or reparenting the per-editor info bar, or merging the bars into one widget.
- Editor-state messages: the find bar's counter, minimap warnings, and read-only overlays. They are not inline alerts and are not projected.
- Auto-reload on external change.
- Projecting alerts of non-selected tabs. Their banners appear on the tab once selected, as they do today.
- Changing alert copy, buttons, styles, or `InlineActionNotification`'s shape.
- Adding inline alerts to the Escape dismissal ladder. Alerts are persistent document state, not transient surfaces, on either surface. Escape behaviour is unchanged.
- The workspace sidebar overlay at narrow widths (`workspace_split_view` collapsed). The user opens it, and it covers the editor and the preview in the same way.

## Decisions

### D1. Every `EditorInfoBar` record is document-state, and the bus surface is the classifier

Each current inline-alert producer describes the document or its recovery: "File Has Changed on Disk" (`editor_page/monitor.rs`), "Could Not Open File" (`load/execution.rs`), "Loading Cancelled" (`load/retirement.rs`), "Mixed Line Endings Detected" (`load/execution.rs`), "Draft Changes Restored" / "Document Restored", "Draft Not Restored" (both variants), "Draft Restore Failed", "Automatic Recovery Paused" (`window/drafts/restore_execution.rs`), and "Restored from Local History" (`window/local_history/restore_execution.rs`). So no per-notification flag is added for *visibility*. The rule recorded in `ui.md` and the facade is: **`NotificationSurface::EditorInfoBar` carries document-state alerts only.** A future editor-state message must use a different surface.

*Alternative rejected:* a `scope: Document | Editor` field on `InlineActionNotification`. Nothing would set it to `Editor` today, it widens a struct every producer builds, and a default would be a silent policy.

### D2. Host the second `LushtextInfoBar` in the window template, above `preview_layout_view`

`window.blp`'s `"tabs"` `StackPage` child becomes a vertical `Box` holding `$LushtextInfoBar preview_document_alert {}` first, then `preview_layout_view` (`vexpand`). The `[preview]` slot child stays `markdown_preview`, and every site that addresses `markdown_preview` or `preview_layout_view` by template id is unchanged. `window/imp.rs` adds the `TemplateChild` and calls `LushtextInfoBar::ensure_type()` in `class_init`.

In preview-only mode the `"preview"` layout makes the preview the whole content of `preview_layout_view`, so the alert sits directly above the preview and spans its full width. That is the same column the editor bar would have had. The alert shows above rendered Markdown and above every placeholder state ("Preparing Markdown preview…", "Not a Markdown file", "Open a Markdown file…"). In the other layouts the projection keeps it hidden (D3). It lives next to the status bar, the workflow's other window-owned surface.

Reusing the widget gives the agreed narrow-width behaviour without a copy: the `AdwWrapBox` (`justify: spread`, the action group as one atomic child), wrapped button labels (`wrap_button_label`), the `SizeGroup` balancing Discard and Save, the `Group`/`Alert` roles, the `.editor-inline-alert` CSS scoping, and the `GtkRevealer` slide.

*Alternatives rejected:*
- **A child of `markdown-preview.blp`** (the original draft of this design). `LushtextMarkdownPreview` is also built by the notes browser and by every note editor. Each of those would carry a dead, unwired info bar. The preview's `clear()`, `clear_source_snapshot()`, `show_placeholder()`, and `show_content_placeholder()` would also have to be proven not to touch it, and `WFR-MARKDOWN-PREVIEW` would host a surface of another workflow. The window-level host avoids all of that. The preview widget is not edited.
- **Wrap the `[preview]` slot child in a new `Box`.** The slot child would then no longer be `markdown_preview`. About 37 window-side sites address that widget as the slot occupant, including `accessibility::set_hidden(&*imp.markdown_preview, …)`. Wrapping `preview_layout_view` instead changes no slot.
- **A new, lighter "preview banner" widget.** It would fork the wrap layout, button wrapping, roles, and CSS: two presentations that drift.
- **Reparent the editor's bar in preview-only mode.** Excluded by the maintainer, and reparenting a template child of another composite widget breaks `TemplateChild` ownership.

### D3. Projection: one full pass, run from every re-projection point

`execution::project_notification_surfaces` gains a step after the per-tab loop, `project_preview_document_alert`:

1. Read `preview_only = imp.preview_mode.get()` and the selected editor (`tab_view.selected_page()` downcast to `LushtextEditorPage`, the lookup `refresh_preview` uses).
2. `let view = selected.and_then(|e| bus.editor_info_bar_view(e.notification_owner_id()))`.
3. Pure policy `policy::plan_preview_document_alert(preview_only, selected_owner, view.as_ref()) -> Option<DocumentAlertProjectionTicket>` returns `Some` only when preview-only is active and the selected tab has an alert.
4. Render `view` when the plan is `Some`, else `None`, into `imp.preview_document_alert`. Store the ticket in `imp.preview_document_alert_ticket` (window-owned coordination state).

There is **one** projection entry point. The re-projection points call the existing `render_notifications()`:

- every bus publish, dismiss, resolve, and expiry sweep (already the case);
- `apply_preview_shell_state` (`ui/window/preview.rs`), after the layout name and visibility are applied. That covers `Alt+P`, the target-state automation actions, Focus Mode's preview toggling, `exit_preview_only_mode_now`, the deferred side-by-side show, and the empty-tab-stack exit;
- `refresh_selected_tab_model_projections` (`ui/window/documents.rs`). That covers selection change, tab close/detach, and the deferred tab-projection batch's terminal rebuild (session restore, bulk close), which already calls it once.

The original draft had a second, narrow `render_preview_document_alert` path for the last two points. It is dropped. It would have been a second projection that has to agree with the full pass. It also could not announce (D5): a background-tab alert becomes perceivable exactly at a selection change, and so does an unannounced alert at a mode change. A full pass costs one bus lookup and one idempotent widget render per tab. That is already paid on every publish and status message, and a selection change happens far less often. During a deferred batch, selection changes do not re-project (the existing early return). The batch's terminal rebuild re-projects once, and the D4 guard covers a click in between. Running the pass this often is safe only because it no longer announces as a side effect (D5, and the status-lane fix in D6).

*Alternative rejected:* render from `refresh_preview`. That is the Markdown-preview workflow's content path, with a 300 ms debounce, an early return when no preview is visible, and a Markdown-only branch. Putting a notification decision there would make the preview workflow own notification policy.

### D4. Actions forward to the selected editor's own bar, guarded by a projection ticket

`services/notifications.rs` gains the GTK-free vocabulary `InlineAlertAction { Retry, Primary, Secondary, Dismiss }`, beside `InlineActionNotification`, so both surfaces import it and neither defines a private copy (the honesty test in the facade). `LushtextInfoBar` gains `pub(crate) fn activate(&self, action: InlineAlertAction)`. It runs exactly what the matching click runs: the stored callback, and for `Dismiss` the `render_notification(None)` first. The `connect_clicked` closures call `activate`, so there is one code path.

The window wires `preview_document_alert` once, at construction (`notifications/mod.rs::wire_preview_document_alert`, called from the window's `constructed`). Each of its four callbacks resolves the forward at click time:

```text
ticket  = imp.preview_document_alert_ticket            (captured at projection)
facts   = DocumentAlertFacts { preview_only, selected_owner, current_alert_key }
if policy::document_alert_projection_is_current(&ticket, &facts)
    selected_editor.info_bar().activate(action)        (the closures wire_info_bar installed)
else
    window.render_notifications()                      (stale: re-project, act on nothing)
```

`DocumentAlertProjectionTicket { owner_id, alert_key }` and `DocumentAlertFacts` follow the repo's `*Ticket` + `*Facts` + `*_is_current` seam shape. The ticket is built once at projection. It crosses into the window's state and then into the click closure (two boundaries), and it is validated as a unit. `alert_key` is `inline_alert_announcement_key(notification)`, the existing semantic key, and it is not renamed while crossing.

**When a ticket can be stale.** Every bus change and every non-deferred selection change re-projects synchronously, so the ticket normally matches what is on screen. It goes stale in one case: the selection changed while tab-derived projections were deferred (session-restore batches and bulk close). Then the copy can still show the previous tab's alert until the batch's terminal rebuild. Without the guard, "Save…" would run `save_current()` on a different document, and "Discard Changes and Reload" would reload one. The guard also turns the forward into a no-op if the window's state ever drifts from the screen.

So the agreed rule holds by construction. Retry dismisses and reloads, Discard runs the pending warning action or the draft discard and reload, Save runs `save_current()`, and Dismiss runs `dismiss_editor_notifications`. Each changes the bus, and the full pass then re-renders **both** surfaces. Mnemonics (`_Retry`, `_Discard…`, `Show in _Local History`) do not collide: in preview-only mode the editor's bar is unmapped, and in the other layouts the preview copy is hidden.

**`PendingWarningAction` is read, not consumed, on click** (pre-existing defect 4). The discard closure reads it with a new non-consuming `pending_warning_action()`. Every emit already sets or resets it, so it always belongs to the editor's latest alert, and nothing else has to clear it. A cancelled "Normalize…" then reopens the dialog on the next click instead of reloading. A second "Undo Restore" click is already harmless: `undo_local_history_restore` finds no undo text and publishes a warning status. `take_pending_warning_action` is removed if nothing else calls it.

**Focus.** When the projection hides `preview_document_alert` while preview-only is active and the window's focus widget is inside it, the projection moves focus to the preview's `text_view` if it is visible. Otherwise it calls `set_focus(None)`, the last step of the existing `restore_saved_focus` order. It does not let GTK fall back to the first focusable widget, which is a sidebar button. `.agents/rules/ui.md`'s focus-restoration rule requires this. The `text_view` stays visible during a reload because the "Preparing Markdown preview…" state is a content placeholder. It is hidden behind the `StatusPage` for non-Markdown documents. Actions that open a dialog ("Normalize…", "Save _As…") move focus into the dialog before the alert resolves. When that dialog closes it hands focus back to a hidden button. Task 6.4 records where focus lands on the current Libadwaita and closes the gap with the same rule if it is stranded. The editor's own bar has the same gap today (a focused Dismiss hides its own button). Task 6.4 measures it and applies the same rule there (the selected editor's `source_view`) if focus is stranded.

*Alternative rejected:* refactor the four closures into window methods and wire both bars to them. It works, but it touches every handler, and "the same handler" would then rest on two registrations staying identical. Forwarding through `activate` makes it a single registration.

### D5. Announce once, when the alert first becomes perceivable

The rule:

- An alert is **perceivable** on exactly one surface or on none. If its editor is the window's selected tab and preview-only is active, it is perceivable on the preview surface. If its editor is selected and preview-only is not active, it is perceivable on the editor's own bar. If its editor is not selected, it is perceivable on no surface. D2 and the Context section show the editor bar is on screen in side-by-side, so side-by-side needs no special case. (Whether the window is the active one is not considered. Each window announces for its own selected tab, as each does today.)
- An alert is announced exactly once, at the first projection where it is perceivable. That covers three cases: it appears on the visible editor; preview-only is entered while the alert is still unannounced; a background tab whose alert was raised while hidden is selected.
- An alert raised on a non-selected tab is **not** announced when it is raised. An announcement from an unmapped widget is either undelivered or confusing, because it names a document the user cannot see.
- Re-rendering an unchanged alert, switching layouts or tabs after it was announced, and another tab's activity announce nothing. An alert whose key changes, or that reappears after its record was cleared, is a new alert.

Mechanism:

- `LushtextInfoBar::render_notification` becomes **silent**. It sets labels, buttons, style, reveal, and the labelled description. `LushtextInfoBar::announce(&self, &InlineActionNotification)` emits one announcement on `alert_title` with `inline_alert_announcement_text` and `_lane`. It is **unthrottled**, because the window's ledger is the deduplication. A second, throttled entry point keeps today's per-widget `AnnouncementThrottler` path. The windowless fallback in `LushtextEditorPage::emit_inline_notification*` (no callback connected) calls it after rendering, so the windowless behaviour and `test_inline_alert_announcements_use_shared_throttling_policy` are unchanged.
- The window owns `InlineAlertAnnouncementLedger`, a `HashMap<owner_id, alert_key>` in `imp`, as `execution` coordination state. Each full pass reconciles it against the tabs it visits. An editor with no alert, or no longer open, loses its entry. `handle_tab_detached` also drops the entry explicitly, because `owner_id` is an object address that a later editor can reuse. The ledger never outgrows the tab count.
- Pure policy: `inline_alert_perceivable_on(preview_only, is_selected) -> Option<InlineAlertSurface>` (`EditorInfoBar` or `PreviewSurface`) and `inline_alert_announcement_due(announced: Option<&str>, current: &str) -> bool`. For each visited editor with an alert, the pass announces only when both the surface is `Some` and the announcement is due, on that one surface. Then it records the key. When the surface is `None` the ledger is left alone, so the alert stays due until it is perceived.
- The lane still comes from `inline_alert_announcement_lane`, so errors keep `Alert` urgency and warnings keep `StatusUpdate`. Each lane is now spoken once per appearance instead of on every pass.

*Alternatives rejected:* share one throttler between the two instances. That still re-announces `Alert`-lane errors on every pass, and it couples two widgets' private state. *Let both surfaces announce and rely on AT dropping announcements from unmapped widgets:* that relies on an unpinned toolkit behaviour (see "GTK axioms"). *Keep announcing background-tab alerts at raise time (the original draft):* rejected by the maintainer's rule above.

### D6. Status lane: announce on acknowledgement, not on render (pre-existing defect 3)

`StatusBar::render_message` becomes silent. The status announcement moves into stage 3, **Acknowledge**, beside the pulse. It is emitted when `policy::status_update_is_visible` holds on the publish/update path (`render_notifications_for_status_update`), through the status bar's existing throttled `announce_visible_status`, with its existing lanes and keys. Expiry sweeps, resolves, heartbeats, inline-alert publishes, and the new selection and mode re-projections then never re-announce the status lane. A status message is announced when it is published and wins the lane, as the pulse already is. A message uncovered when a higher-priority one expires is not re-announced. Before switching, the implementer audits every caller that renders or publishes a `Warning` or `Error` status outside `render_notifications_for_status_update` (for example progress updates), so none loses its only announcement. The result goes in the task log.

### D7. Load success resolves only the alerts the loaded bytes supersede (pre-existing defect 1)

`complete_loaded_installation` stops calling the widget-only `clear_inline_notification()`. It calls a new `LushtextEditorPage::resolve_inline_notification_superseded_by_load()`. With a window connected, that goes through a second callback installed by `wire_info_bar` (`connect_inline_notification_superseded`) to the existing stage-1 `resolve_editor_inline_notification(editor)`. Without a window it renders `None` directly, as today. The order inside the terminal stays *resolve, then publish the mixed-line-endings warning*.

It resolves only when the editor's current alert was emitted as **superseded by a load**. The editor records that flag at emit time, beside `PendingWarningAction`, and every emit resets it. Callers pick the emit variant explicitly:

| Superseded by the next successful load | Outlives a load |
| --- | --- |
| "Could Not Open File", "Loading Cancelled" (the load workflow's own failures) | "Draft Not Restored" (both variants), "Draft Restore Failed": they point at a preserved recovery body that is still on disk after the reload |
| "File Has Changed on Disk" (the reload installed the current bytes) | |
| "Mixed Line Endings Detected" (re-derived and re-published from the new bytes in the same terminal) | |
| "Draft Changes Restored" / "Document Restored", "Restored from Local History", "Automatic Recovery Paused" (they describe buffer content the load replaced) | |

Why not resolve everything: today a recovery alert on an unmodified tab outlives the eviction reload, though only because of the resurrection. The tab is evictable, so selecting it again reloads it. Resolving that alert would silently remove the only in-place pointer to where the user's unsaved edits were preserved. Why not resolve nothing and just stop clearing: that would keep a stale "File Has Changed on Disk" over current content after every non-dismissing reload, which is defect 1's visible form.

A failing-first regression test proves the resurrection on the current tree through one of the three production paths in the Context section. A second test proves that "Draft Not Restored" survives an eviction reload.

### D8. Evidence surface

The original draft argued that `WFR-STATUS-NOTIFICATIONS` still needed no evidence surface. That no longer holds. The row's recorded reason for having none was that "every stage's result is observable through the bus's own production views." After this change three facts live outside the bus: the announcement ledger, the projection ticket, and how many announcements each surface emitted. The convention says such facts are read through one typed `evidence.rs`, not through per-field counters. The draft's `announcements_emitted()` on the info bar was such a counter, on a called presentation surface that must not own evidence. It is dropped. The convention also says automation snapshot fields project from the surface.

So `ui/window/notifications/evidence.rs` adds `NotificationEvidence`, read through one `notification_evidence()` accessor:

- `preview_document_alert_visible` (the rendered visibility: `GtkRevealer` revealing, the widget visible, and the widget mapped, read through `try_get()`);
- `preview_document_alert_projection: Option<(owner_id, alert_key)>` (the ticket);
- `announcements: InlineAlertAnnouncementCounts { editor_info_bar, preview_surface, status_lane }` (counted by `execution` when it calls `announce`);
- `announcement_ledger_len`.

The surface owes the convention's three driven proofs: side-effect freedom across publish, dismiss, a selection change, and a mode change; disposal honesty; and materialization neutrality (vacuous, because there is no lazy collection, and stated as such). `surfaces.preview_document_alert_visible` projects from it.

### D9. Automation and accessibility surface

- `AutomationSurfaceSnapshot` gains `preview_document_alert_visible: bool`, projected from `NotificationEvidence` (D8), so a scenario can tell "preview-only with no alert" from "the alert is shown". No title or body goes into the field. The visible alert title is already reachable through the AT-SPI anchor, and the bounded snapshot rules stay unchanged. Docs: `snapshot-field-preview-document-alert-visible` in `docs/automation-reference.md`, a paragraph in `docs/automation.md`, then `make check-automation-docs`.
- AT-SPI: the preview copy keeps the widget's `Group` (`alert_box`) and `Alert` (`alert_title`) roles. It is a sibling above the preview's `document` accessible ("Markdown preview"), not inside it, and the hidden editor stays out of the tree. Anchor row: `atspi-anchor-preview-document-alert-alert-could-not-open-file` (smoke fixture) beside the existing `atspi-anchor-editor-error-alert-could-not-open-file`.
- Tab order in preview-only: the alert's actions, then the preview content.
- Accessibility smoke: a new `scripts/run-accessibility-smoke.sh` case, `preview-mode-document-alert`, mapped to `A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, and `A11Y-ERROR-SURFACES`. It reuses the `editor-failed-load` fixture with a `.md` path, enters preview-only mode through `win.set-preview-mode`, waits on `accessibility-settled`, and asserts exactly one showing `alert` named "Could Not Open File" with its Retry and Dismiss buttons reachable. The deterministic failed-load fixture is chosen over an external write because the smoke harness already owns it. The widget tests prove the external-change path.
- Visual smoke: a `preview-mode-document-alert` scenario in `scripts/run-visual-smoke.sh`, in light and dark themes and at low Background Opacity. The change is pixel-visible and crosses adaptive layout states, so `ui.md`'s state-extremes rule asks for a rendered proof, not only allocation assertions.

### D10. Workflow-convention placement

| Module | Role | Change |
| --- | --- | --- |
| `ui/window/notifications/mod.rs` | narrative facade | stage 2 names the third surface and the forward with its resumption point (the click closure); stage 2's "Announce" sub-step; stage 3 now also announces the status lane; `wire_preview_document_alert`; `notification_evidence` |
| `ui/window/notifications/policy.rs` | pure policy | `plan_preview_document_alert`, `DocumentAlertProjectionTicket`/`DocumentAlertFacts`/`document_alert_projection_is_current`, `InlineAlertSurface`, `inline_alert_perceivable_on`, `inline_alert_announcement_due` |
| `ui/window/notifications/execution.rs` | coordination | the preview step, ledger reconciliation, single-surface announcement, status announcement on acknowledgement, focus handoff |
| `ui/window/notifications/evidence.rs` | evidence (**new file**) | `NotificationEvidence` (D8) |
| `ui/info_bar/**` | called presentation surface | silent render, unthrottled and throttled `announce`, `activate` |
| `ui/status_bar/**` | called presentation surface | silent `render_message`; the announcement entry point is called from stage 3 |
| `resources/ui/window.blp`, `ui/window/imp.rs` | window shell | the host `Box`, the `preview_document_alert` template child, ticket and ledger state grouped with `notification_bus` |
| `services/notifications.rs` | shared GTK-free bus | `InlineAlertAction` |

`evidence.rs` is a new file in a migrated role home. The row must declare it, or the role-home declaration gate in `make check-workflow-boundaries` fails. `policy.rs` stays free of `gtk4`/`glib`/`gio`/`libadwaita`/`sourceview5` imports. The facade stays far below 370 lines (153 today). No `*_for_test` is added, so the row's seam count of zero stands. Tests read `NotificationEvidence`, the bus views, and widget `imp()` fields, as existing info-bar tests already do. `WFR-MARKDOWN-PREVIEW` is unchanged.

### GTK axioms

None added. The design is checked against the ledger
(`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, A1–A37):

- `AdwMultiLayoutView` slot behaviour: the design does **not** rely on what the view does with a child whose slot the current layout omits. `editor_box` is already hidden explicitly. The preview copy is outside the view, so a layout switch never reparents it. It is shown or hidden by explicit projection after `set_layout_name`, never by slot membership.
- Announcements from a widget that is unmapped or not in the accessible tree: the design announces only from the perceivable surface (D5), so it does not depend on whether GTK forwards such announcements. That behaviour is not a ledger axiom, and this change does not need it to be one.
- Where `AdwDialog` returns focus when the widget focused before it opened is hidden by the time it closes (D4): task 6.4 measures it. If the result decides the design, it becomes a ledger axiom with a probe, per the `gtk4-libadwaita-internals` skill.
- A37 (line validation before the paint) is unaffected. Neither the preview's `text_view` nor the editor's view is touched.

## Risks / Trade-offs

- [Errors and warnings are no longer re-announced on unrelated passes] → Intended. `A11Y-EDITOR-ERROR` says "not repeated heartbeats", and `A11Y-ERROR-SURFACES` says alerts are bounded. A changed alert has a different key and is announced. Covered by policy unit tests and widget tests reading `NotificationEvidence.announcements`.
- [Background-tab alerts are spoken later, on selection] → The maintainer's rule. The status lane still announces a status error such as a failed background load when it is published.
- [The status lane no longer re-announces a message uncovered by expiry] → It was announced when it was published. D6's caller audit makes sure no warning or error status loses its only announcement.
- [The superseded-by-load table misclassifies a future alert] → Each producer chooses the emit variant explicitly, and the table is mirrored in the facade and in `ui.md`. A new producer has to pick a row.
- [A forwarded click acts on the wrong document] → The ticket/facts guard (D4). A unit test covers each field mismatch. A widget test activates the copy after a selection change made during a deferred tab-projection batch, and must see no handler run.
- [The full pass now also runs on selection and mode changes] → It is bounded by the tab count, it is idempotent, and it no longer announces as a side effect. Deferred batches still re-project once.
- [The preview copy inherits a transparent background] → It is outside the preview's `text_view` (which carries `tab-content-preview-surface`), and `.editor-inline-alert` paints its own semantic background, so it stays opaque like the editor's bar. Checked by the visual-smoke scenario.

## Migration Plan

No persisted state, setting, or format changes. Rollback is reverting the change. The snapshot field is additive, so automation clients that ignore unknown fields are unaffected.

## Open Questions

None. The proposer's three questions are settled:

1. Re-announce when the alert first becomes perceivable: yes, once (D5, the maintainer's rule).
2. The collapsed side-by-side overlay: it cannot happen. `preview_split_view` never collapses, and its pane is at most one third of the content width (Context). The rule stays "preview-only only".
3. Background-tab alerts: they are announced when their tab is first selected, not when raised (D5).
