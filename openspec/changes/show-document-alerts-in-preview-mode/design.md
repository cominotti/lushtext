## Context

### Where the alert lives today

- `LushtextInfoBar` (`ui/info_bar/`) is the first child of every `LushtextEditorPage` (`resources/ui/editor-page.blp`). It is a `GtkRevealer` wrapping an `AdwWrapBox` (`content_wrap`). The message box and the action group (`retry_button`, `discard_button`, `save_button`, `dismiss_button`) are the wrap box's two children.
- The page sits in `tab_view` inside `editor_box`, which is the `[editor]` child of `preview_layout_view` (`resources/ui/window.blp`). The `"editor"` `Adw.Layout` places `editor` in the `AdwOverlaySplitView` content slot and `preview` in its sidebar slot. The `"preview"` layout places only `preview`. `LushtextWindow::apply_preview_shell_state` (`ui/window/preview.rs`) also sets `editor_box` invisible in preview-only mode. In preview-only mode nothing in the editor is on screen or in the accessibility tree, and that includes the banner.

### Who owns the state: one source of truth already exists

- **Owner:** the window's `NotificationBus` (`services/notifications.rs`), a GTK-free reducer. Editor alerts are records on `NotificationSurface::EditorInfoBar(owner_id)` owned by `NotificationOwner::Editor(owner_id)`. `owner_id` is `LushtextEditorPage::notification_owner_id()`. `ui/window/notifications/policy.rs::EditorNotificationTarget` derives both halves once. The answer for a surface is `NotificationBus::editor_info_bar_view(owner_id) -> Option<InlineActionNotification>`.
- **Publish path:** producers call `editor.emit_inline_notification[_with_warning_action]`. The window installed `connect_inline_notification` in `documents.rs::wire_info_bar`, so the call becomes `publish_editor_inline_notification`. That is stage 1, followed by `render_notifications()`.
- **Projection:** `notifications/execution.rs::project_notification_surfaces` does a full pass. It renders the status lane, then every tab's `editor_info_bar_view` onto that tab's `info_bar()`. The pass is bounded by the tab count.
- **Per-editor side state that is not on the bus:** `PendingWarningAction` (which flow the warning-primary button runs) and `is_draft_restored()` live on the editor. The handlers read them when a button is clicked, not when it is rendered. Because the preview surface forwards to the same handlers, they keep reading the same state. **Nothing is duplicated.**
- **Two leaks around the bus, found while tracing the state:**
  1. `ui/editor_page/load/execution.rs` calls `editor.clear_inline_notification()` on install success, and that renders `None` directly into the widget. The bus record survives. The next full pass (any publish on any tab, any expiry sweep) brings the alert back. Most interactive flows dismiss through the bus before reloading (Retry, Discard), so this is latent today. It becomes visible as soon as a second surface reads the bus, so it is a pre-existing blocker for this change.
  2. `render_notification` announces inside the widget, through a per-widget `AnnouncementThrottler`. `AnnouncementLane::Alert` bypasses throttling (`cooldown() == ZERO`), so every full pass re-announces every error alert. A second surface would double that.

### Action wiring

`documents.rs::wire_info_bar` installs four closures per editor on that editor's bar: `connect_retry`, `connect_discard` (dispatching on `take_pending_warning_action()` and then `is_draft_restored()`), `connect_save` (`window.save_current()`), and `connect_dismissed`. The dismiss button first renders `None` on itself, then runs `dismissed_callback`.

### The preview widget

`LushtextMarkdownPreview` (`resources/ui/markdown-preview.blp`) is a vertical `Box` holding `scrolled_window` (the `text_view`) and an `Adw.StatusPage placeholder`. `widgets.rs` toggles the two for placeholder states. One instance per window is the `[preview]` child of `preview_layout_view`, so it moves between the split view's sidebar slot (side-by-side) and the content slot (preview-only). `apply_preview_shell_state` sets it hidden for AT when no preview is active and calls `clear()`.

## Goals / Non-Goals

**Goals:**

- A document-state inline alert is visible in preview-only mode, for the selected tab, with the same text, style, and actions as the editor's bar, running the same handlers.
- Exactly one banner is visible in every layout: editor-only, side-by-side, and preview-only.
- Either surface's action updates both. Switching tabs re-projects or clears the preview copy.
- One screen-reader announcement per alert appearance, independent of how many surfaces render it.
- The projection stays inside the notification workflow (`WFR-STATUS-NOTIFICATIONS`). `ui/info_bar/` stays a called presentation surface. The preview widget hosts the instance and decides nothing.

**Non-Goals:**

- Moving or reparenting the per-editor info bar, or merging the bars into one widget.
- Editor-state messages: the find bar's counter, minimap warnings, and read-only overlays. They are not inline alerts and are not projected.
- Auto-reload on external change.
- Projecting alerts of non-selected tabs. Their banners appear on the tab once selected, as they do today.
- Changing alert copy, buttons, styles, or `InlineActionNotification`'s shape.

## Decisions

### D1. Every `EditorInfoBar` record is document-state, and the bus surface is the classifier

Each current inline-alert producer describes the document or its recovery: "File Has Changed on Disk" (`editor_page/monitor.rs`), "Could Not Open File" (`load/execution.rs`), "Loading Cancelled" (`load/retirement.rs`), "Mixed Line Endings Detected" (`load/execution.rs`), "Draft Changes Restored" / "Document Restored", "Draft Not Restored" (both variants), "Draft Restore Failed", "Automatic Recovery Paused" (`window/drafts/restore_execution.rs`), and "Restored from Local History" (`window/local_history/restore_execution.rs`). So no per-notification flag is added. The rule recorded in `ui.md` and the facade is: **`NotificationSurface::EditorInfoBar` carries document-state alerts only.** A future editor-state message must use a different surface.

*Alternative rejected:* a `scope: Document | Editor` field on `InlineActionNotification`. Nothing would set it to `Editor` today, it widens a struct every producer builds, and a default would be a silent policy.

### D2. Host a second `LushtextInfoBar` instance in the preview template

`markdown-preview.blp` gains `$LushtextInfoBar document_alert {}` as the **first** child of the template `Box`, above `scrolled_window` and `placeholder`. So it shows above rendered content and above every placeholder state ("Preparing Markdown preview…", "Not a Markdown file", "Open a Markdown file…"). `imp.rs` adds the `TemplateChild` and calls `LushtextInfoBar::ensure_type()` in `class_init`, as `editor_page/imp.rs` already does. `widgets.rs` adds one accessor, `pub(crate) fn document_alert(&self) -> &LushtextInfoBar`. It adds no rendering or visibility logic of its own. `clear()`, `clear_source_snapshot()`, `show_content_placeholder()`, and the render/retirement lanes must not touch the instance. A widget test pins this.

Reusing the widget gives the agreed narrow-width behaviour without a copy: the `AdwWrapBox` (`justify: spread`, the action group as one atomic child), wrapped button labels (`wrap_button_label`), the `SizeGroup` balancing Discard and Save, the `Group`/`Alert` roles, the `.editor-inline-alert` CSS scoping, and the `GtkRevealer` slide. In preview-only mode the instance spans the full content width, which is the same column the editor bar would have had.

*Alternatives rejected:*
- **Wrap `[preview]` in a new window-level `Box` in `window.blp`.** The slot child would then no longer be `markdown_preview`. About 37 window-side sites address that widget as the slot occupant (`ui/window/preview.rs`, `geometry/execution.rs`, `focus_mode/mod.rs`, the notes workflow, and `ui/automation.rs`), including `accessibility::set_hidden(&*imp.markdown_preview, …)` and its visibility toggling. Each would have to decide whether it means the wrapper or the preview. More churn, no gain.
- **A new, lighter "preview banner" widget.** It would fork the wrap layout, button wrapping, roles, and CSS: two presentations that drift.
- **Reparent the editor's bar in preview-only mode.** Excluded by the maintainer, and reparenting a template child of another composite widget breaks `TemplateChild` ownership.

### D3. Projection: one more step in the existing pass, plus two narrow re-projection points

`execution::project_notification_surfaces` gains a final step, `project_preview_document_alert(window)`:

1. Read `preview_only = imp.preview_mode.get()` and the selected editor, through the same lookup `refresh_preview` uses (`tab_view.selected_page()` downcast to `LushtextEditorPage`).
2. `let view = selected.and_then(|e| bus.editor_info_bar_view(e.notification_owner_id()))`.
3. Pure policy `policy::plan_preview_document_alert(preview_only, selected_owner, view.as_ref()) -> Option<DocumentAlertProjectionTicket>` returns `Some` only when preview-only is active and the selected tab has an alert.
4. Render `view` (or `None`) silently into `markdown_preview.document_alert()`, and store the ticket in `imp.preview_document_alert_ticket` (window-owned coordination state).

The facade exposes `pub(super) fn render_preview_document_alert(&self)`, which runs this step alone. It is called from:

- `apply_preview_shell_state` (`ui/window/preview.rs`), after the layout name and visibility are applied. That covers `Alt+P`, the target-state automation actions, Focus Mode's preview toggling, `exit_preview_only_mode_now`, and the empty-tab-stack exit.
- `refresh_selected_tab_model_projections` (`ui/window/documents.rs`). That covers selection change, tab close/detach (which changes selection), and the deferred session-restore batch's terminal rebuild, which already calls it once.

Publish, dismiss, resolve, and expiry sweep already run the full pass, so they need no new call site. The step is O(1) (one bus lookup), so it is cheap on every pass. The full pass stays bounded by the tab count.

*Alternative rejected:* render from `refresh_preview`. That is the Markdown-preview workflow's content path, with a 300 ms debounce, an early return when no preview is visible, and a Markdown-only branch. Putting a notification decision there would make the preview workflow own notification policy.

### D4. Actions forward to the selected editor's own bar, guarded by a projection ticket

`services/notifications.rs` gains the GTK-free vocabulary `InlineAlertAction { Retry, Primary, Secondary, Dismiss }`, beside `InlineActionNotification`, so both surfaces import it and neither defines a private copy (the honesty test in the facade). `LushtextInfoBar` gains `pub(crate) fn activate(&self, action: InlineAlertAction)`. It runs exactly what the matching click runs: the stored callback, and for `Dismiss` the `render_notification(None)` first. The `connect_clicked` closures call `activate`, so there is one code path.

The window wires the preview instance once, at construction (`notifications/mod.rs::wire_preview_document_alert`, called from the window's `constructed`). Each of its four callbacks resolves the forward at click time:

```text
ticket  = imp.preview_document_alert_ticket            (captured at projection)
facts   = DocumentAlertFacts { preview_only, selected_owner, current_alert_key }
if policy::document_alert_projection_is_current(&ticket, &facts)
    selected_editor.info_bar().activate(action)        (the closures wire_info_bar installed)
else
    window.render_preview_document_alert()             (stale: re-project, act on nothing)
```

`DocumentAlertProjectionTicket { owner_id, alert_key }` and `DocumentAlertFacts` follow the repo's `*Ticket` + `*Facts` + `*_is_current` seam shape. The ticket is built once at projection and crosses into the window's state and then the click closure (two boundaries), and it is validated as a unit. `alert_key` is `inline_alert_announcement_key(notification)`, the existing semantic key, not renamed while crossing. The guard covers a click delivered after the selection changed, or after the alert was replaced, in the same main-loop turn. Without it, a stale button could run "Discard Changes and Reload" on a different document.

So the agreed rule holds by construction. Retry dismisses and reloads, Discard runs the pending warning action or draft discard and reload, Save runs `save_current()`, and Dismiss runs `dismiss_editor_notifications`. Each changes the bus, and the full pass then re-renders **both** surfaces. Mnemonics (`_Retry`, `_Discard…`) do not collide: in preview-only mode the editor's bar is unmapped, and in the other layouts the preview copy is hidden.

**Focus:** when a focused button on the preview copy hides because its action resolved the alert, focus moves to the preview's `text_view` (or the placeholder when that is showing), not to GTK's first-focusable fallback, which is a sidebar button. `.agents/rules/ui.md`'s focus-restoration rule requires this. The decision is the projection's: it knows the alert went away while preview-only is active.

*Alternative rejected:* refactor the four closures into window methods and wire both bars to them. It works, but it touches every handler, and "the same handler" would then rest on two registrations staying identical. Forwarding through `activate` makes it a single registration.

### D5. Announce once per alert, from the projection, on the surface the user perceives

- `LushtextInfoBar::render_notification` becomes **silent**. It sets labels, buttons, style, reveal, and the labelled description. The new `LushtextInfoBar::announce(&self, notification)` holds today's announcement code: `inline_alert_announcement_text`, `_lane`, and `_key`, the widget's throttler, and `announce_with_lane` on `alert_title`. It also bumps a production read-only counter `announcements_emitted() -> u64`, in the spirit of `ViewportSliceBin::allocation_count()`. The windowless fallback in `LushtextEditorPage::emit_inline_notification*` (no callback connected) calls both, so the windowless behaviour and its existing test (`test_inline_alert_announcements_use_shared_throttling_policy`) are unchanged.
- The window owns `InlineAlertAnnouncementLedger`, a `HashMap<owner_id, alert_key>` in `imp`, as coordination state for `execution`. Each full pass reconciles it against the tabs it visits. An editor whose alert key differs from its ledger entry is due. An editor with no alert, or no longer open, loses its entry, so a later reappearance is announced again. The ledger never outgrows the tab count.
- Pure policy: `inline_alert_announcement_due(previous: Option<&str>, current: &str) -> bool` and `inline_alert_announcer(preview_only, is_selected) -> InlineAlertAnnouncer { EditorInfoBar, PreviewSurface }`. When an alert is due, the pass calls `announce` on exactly one surface: the preview copy when preview-only is active and the editor is selected, otherwise the editor's own bar (today's behaviour for every other case). The lane still comes from `inline_alert_announcement_lane`, so errors keep `Alert` urgency, but they are announced once per appearance rather than on every pass.
- The narrow `render_preview_document_alert` path never announces. Entering preview-only mode, or switching to a tab whose alert was already announced, shows the alert without speaking it again (see Open Questions).

*Alternatives rejected:* share one throttler between the two instances. That still re-announces `Alert`-lane errors on every pass, and it couples two widgets' private state. *Let both surfaces announce and rely on AT dropping announcements from unmapped widgets:* that relies on an unpinned toolkit behaviour (see "GTK axioms"), and side-by-side would still announce from both if the copy were shown.

### D6. Load success resolves through the bus

`LushtextEditorPage::clear_inline_notification` routes through the window when one is connected. It uses a second callback installed by `wire_info_bar` (`connect_inline_notification_cleared`) that calls `resolve_editor_inline_notification(editor)`. That is the existing stage-1 resolve, which removes only the editor's `EditorInfoBar` record, not its other records. With no window connected it keeps rendering `None` directly. Ordering in `load/execution.rs` is already *clear, then publish the mixed-line-endings warning* inside one terminal, and it stays that way. Before switching, the implementer audits every `clear_inline_notification` caller for an alert that is published before load success and expected to survive it. The audit result goes in the task log. A failing-first regression test proves the resurrection on the current tree: publish an alert, complete a load through a path that does not dismiss first, run a full pass, and see the editor bar visible again.

### D7. Automation and accessibility surface

- `AutomationSurfaceSnapshot` gains `preview_document_alert_visible: bool`, the rendered visibility of the preview copy (its revealer showing, its widget visible, and its ancestors mapped). It is derived from the widget, not re-derived from `preview_mode`, so a scenario can tell "preview-only with no alert" from "the alert is shown". No title or body goes into the snapshot field. The visible alert title is already reachable through the AT-SPI anchor, and the bounded snapshot rules stay unchanged. Docs: `snapshot-field-preview-document-alert-visible` in `docs/automation-reference.md`, a paragraph in `docs/automation.md`, then `make check-automation-docs`.
- AT-SPI: the preview copy keeps the widget's `Group` (`alert_box`) and `Alert` (`alert_title`) roles. It sits inside the preview's `document` accessible ("Markdown preview"), and the hidden editor stays out of the tree. Anchor rows: `atspi-anchor-preview-document-alert-alert-could-not-open-file` (smoke fixture) beside the existing `atspi-anchor-editor-error-alert-could-not-open-file`.
- Smoke: a new `scripts/run-accessibility-smoke.sh` case, `preview-mode-document-alert`, mapped to `A11Y-EDITOR-FOCUS-PREVIEW`, `A11Y-EDITOR-ERROR`, and `A11Y-ERROR-SURFACES`. It reuses the `editor-failed-load` fixture with a `.md` path, enters preview-only mode through `win.set-preview-mode`, waits on `accessibility-settled`, and asserts exactly one showing `alert` named "Could Not Open File" with its Retry and Dismiss buttons reachable. The deterministic failed-load fixture is chosen over an external write because the smoke harness already owns it. The external-change path is proved by the widget tests.

### D8. Workflow-convention placement

| Module | Role | Change |
| --- | --- | --- |
| `ui/window/notifications/mod.rs` | narrative facade | stage 2 names the third surface and the forward; a new "Announce" sub-step in stage 2; `render_preview_document_alert`, `wire_preview_document_alert` |
| `ui/window/notifications/policy.rs` | pure policy | `plan_preview_document_alert`, `DocumentAlertProjectionTicket`/`DocumentAlertFacts`/`document_alert_projection_is_current`, `inline_alert_announcement_due`, `inline_alert_announcer` |
| `ui/window/notifications/execution.rs` | coordination | the preview step, ledger reconciliation, single-surface announcement, focus handoff |
| `ui/info_bar/**` | called presentation surface | silent render, `announce`, `activate`, `announcements_emitted` |
| `ui/markdown_preview/{imp,widgets}.rs` + `markdown-preview.blp` | called presentation surface of `WFR-MARKDOWN-PREVIEW`, now **hosting** one notification-workflow surface instance | template child plus accessor only |
| `services/notifications.rs` | shared GTK-free bus | `InlineAlertAction` |

No new file, so the role-home declaration gate is unaffected. `policy.rs` stays free of `gtk4`/`glib`/`gio`/`libadwaita`/`sourceview5` imports. The facade stays far below 370 lines (153 today). No `*_for_test` is added. Tests read production API (`announcements_emitted`, the bus views, widget `imp()` fields as existing info-bar tests already do), so the row's recorded seam count of zero stands.

### GTK axioms

None added. The design is checked against the ledger
(`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, A1–A37):

- `AdwMultiLayoutView` slot behaviour: the design does **not** rely on what the view does with a child whose slot the current layout omits. `editor_box` is already hidden explicitly. The preview copy lives inside the `[preview]` child, which both layouts place. It is shown or hidden by explicit projection after `set_layout_name`, never by slot membership. The preview widget is reparented between the sidebar slot and the content slot on each mode switch. The design re-projects after every switch rather than relying on the `GtkRevealer`'s `reveal-child` state surviving the reparent.
- Announcements from a widget that is unmapped or not in the accessible tree: the design announces only from the perceivable surface (D5), so it does not depend on whether GTK forwards such announcements. That behaviour is not a ledger axiom, and this change does not need it to be one.
- A37 (line validation before the paint) is unaffected. The preview's `text_view` is not editable, and the copy is a sibling outside the text view.

## Risks / Trade-offs

- [Errors are no longer re-announced on every full pass] → Intended. `A11Y-EDITOR-ERROR` says "not repeated heartbeats". An error that is replaced by a different error has a different key and is announced. Covered by unit tests on `inline_alert_announcement_due` and a widget test counting `announcements_emitted`.
- [Clear-as-resolve could remove an alert a flow expected to survive load success] → D6's caller audit, the failing-first resurrection test, and the existing inline-alert widget tests (draft restore, mixed line endings, local history) must stay green.
- [A forwarded click acts on the wrong document] → The ticket/facts guard (D4). A unit test covers each field mismatch. A widget test switches tabs and then activates the preview copy's button, and must see no handler run.
- [Side-by-side on a narrow window collapses `preview_split_view` into an overlay that can cover the editor's bar] → Out of the agreed rule (preview-only only). Recorded as an open question, not silently changed.
- [The preview copy inherits the preview's background opacity] → It is outside `text_view` (which carries `tab-content-preview-surface`), and `.editor-inline-alert` paints its own semantic background, so it stays opaque like the editor's bar. A visual check is in the tasks.
- [Two instances of the same template add widget-tree weight] → One extra hidden widget per window, not per tab.

## Migration Plan

No persisted state, setting, or format changes. Rollback is reverting the change. The snapshot field is additive, so automation clients that ignore unknown fields are unaffected.

## Open Questions

1. **Re-announce when the alert first becomes perceivable?** D5 announces once per appearance. A screen-reader user who enters preview-only mode with an alert that was already announced (or announced on a background tab and never perceived) hears nothing new. They can still reach the alert by its `Alert` role. Should becoming perceivable on a *different* surface count as a new announcement, once? The design leaves this off so that "once per alert" holds literally. Maintainer to confirm.
2. **Collapsed side-by-side overlay.** When `preview_split_view` collapses (narrow window), its sidebar overlays the content and can hide the editor's bar. Should the preview copy also show while the collapsed overlay sidebar is open? Not in scope as agreed, and the geometry policy (`ui/window/geometry/`) would own the "is the editor visible" fact.
3. **Background-tab announcements.** Today, and still after this change, an alert published on a non-selected tab is announced from that tab's unmapped bar. Whether GTK delivers it is unpinned. Should background-tab alerts instead be announced when their tab is first selected? Not changed here.
