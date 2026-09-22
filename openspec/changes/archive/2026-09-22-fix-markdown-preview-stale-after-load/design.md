## Context

The Markdown preview is window-level state (`preview_mode`, `preview_visible` in `ui/window/imp.rs`) projected from the selected tab's buffer by `LushtextWindow::refresh_preview` in `ui/window/preview.rs`. Every call site today: the preview toggles (`preview.rs`), `refresh_selected_tab_model_projections` on tab selection (`documents.rs:1017`), the workspace-scope change (`workspace_scope.rs:46`), and the 300 ms debounce from the buffer `changed` handler wired in `documents.rs::wire_modified_indicator`.

Document load (`ui/editor_page/load/`) and whole-buffer replacement (`ui/editor_page/buffer_replacement/`) both install text under a projection guard. The `changed` handler returns early while `editor.load_projection_suspended()` holds, which is deliberate: it stops draft autosave and preview re-renders from firing once per 256 KiB slice. Both workflows' terminals then republish the suspended projections explicitly (`refresh_minimap`, accessibility metadata, memory policy, monitor). The preview is the one suspended projection with no republish at either terminal.

Sequence for the reported bug:

1. `open_document_with_intent_and_planning_terminal` calls `set_file_path_for_pending_load` (`documents.rs:216`), which sets `load_state = Loading` and runs `republish_document_identity` → `reapply_language`, so `is_markdown` is already true from the extension.
2. `tab_view.set_selected_page` (`documents.rs:310`) runs 13 lines before `load_file_async`; the `selected-page` notify handler (`window/imp.rs:1023-1036`) synchronously reaches `refresh_preview`, which renders `""`. The preview shows an empty but legitimate render.
3. `begin_load_installation` sets `projection_suspended` (`load/execution.rs:521`); the `changed` handler returns early (`documents.rs:595-597`), so `refresh_preview_debounced` is unreachable.
4. `complete_loaded_installation` restores the guard (`:584`), sets `Loaded` (`:585`), refreshes the minimap (`:607`), runs the one-shot `load_completed_callback` (`:610`, which refreshes sidebar, popover, and status bar only) and the `file_loaded_callbacks` fan-out (`:618-624`). Nothing refreshes the preview.

A sibling symptom needs no load at all: `adopt_saved_destination` → `set_file_path_with_canonical` → `republish_document_identity` → `reapply_language` (`save/mod.rs:213-216`, `document_identity.rs:35-45`) changes `is_markdown` for the tab, and neither `complete_save_as` (`window/dialogs.rs`) nor the sidebar rename loop (`documents.rs:1074`) refreshes the preview. Save an untitled buffer as `notes.md` with preview on and "Not a Markdown file" stays until a toggle.

Session restore is the one path that can republish incidentally: `end_tab_projection_refresh_batch` → `refresh_tab_model_projections` → `refresh_preview` (`documents.rs:977-1017`) may close after the loads. It is not a fix, and a session-restore test is not a clean proof of the new hook.

## Goals / Non-Goals

**Goals:**
- The preview shows the selected document's installed content and current language identity after every install or identity terminal, success or failure, without a user toggle.
- The interval between tab selection and install completion is named ("Preparing Markdown preview…") rather than shown as an empty or partial render.
- One wiring site, one hook, covering the whole class rather than the reported symptom.
- Keep suspension semantics intact: no per-slice preview re-render, no autosave side effects.

**Non-Goals:**
- Per-tab preview mode. The preview stays window-level; the fix follows the tab like tab switching does.
- Changing when preview-only exits. `new_tab` still exits preview-only (owned by `new-document-flow`); opening an existing document does not.
- Any change to Markdown planning, projection, retirement, or image admission in `ui/markdown_preview/`.
- Readiness predicates or automation snapshot fields. `preview-animation` already covers the render that the republish triggers.

## Decisions

### D1. One editor-level fan-out fired at install and identity terminals, not per-caller patches

Add `LushtextEditorPage::connect_content_republished<F: Fn() + 'static>` beside `connect_file_loaded` (`load/mod.rs:211-217`), using the same `RefCell<Vec<Box<dyn Fn()>>>` storage and the snapshot-then-invoke borrow pattern at `load/execution.rs:614-624`, so a listener that re-registers cannot panic the GTK thread.

It fires from four sites:

1. Load success publish, `complete_loaded_installation`, beside `refresh_minimap` (`:607`). The guard is already restored at `:584`, so the listener observes `load_projection_suspended() == false`.
2. Load failure publish, `publish_load_error` (`:426-450`), after `load_state` and the inline notification are set. This runs only from worker errors before any installation begins, so no guard is held there.
3. Buffer replacement, `finish_session` (`buffer_replacement/execution.rs:737-812`), after `restore_guard` (`:804`) and before the caller's `callback(outcome)` (`:806`). It fires on every terminal that restores the guard, which per `policy::guard_restores_on_terminal` is every reason except `Disposed`. Firing on `Cancelled`, `Stale`, and `Superseded` too is deliberate: the guard is restored, the buffer is renderable, and a render of what is really there is more honest than a placeholder nobody will ever replace. A superseding replacement fires again at its own terminal, so a transient render of a partial buffer self-heals. The one exception: a cancelled terminal that finds a parked superseding request does not fire, because that request's own terminal will.
4. `republish_document_identity` (`document_identity.rs:74-81`), the hub whose doc comment already says a new projection must not be forgotten. This covers Save As, sidebar rename, and reopen with encoding (`window/encoding/execution.rs:86` goes through the load path too, but its identity flip happens here first). During `open_document` this fires before the page exists, so no listener is registered yet and it is a no-op there.

The window wires one listener per editor in `documents.rs`, in `wire_preview_projection(page, editor)` beside `wire_modified_indicator`, called from both `new_tab` and `open_document_with_intent_and_planning_terminal`. The listener refreshes the preview only when `tab_view.selected_page() == page`, mirroring the guard the `changed` handler already applies at `:608-612`.

Alternatives considered:
- Reuse `connect_file_loaded` plus `connect_load_failed_once`. Rejected: `load_failed_callback` and `load_completed_callback` are one-shot `.take()` (`execution.rs:448`, `:610`) and registered only by `open_document`, so eviction, external-change, and Replace All reloads would miss them, and the replacement terminal and identity hub have no equivalent.
- Patch each buffer-replacement caller's terminal callback (drafts, local history, save mirror-back, `evict()`). Rejected: four sites, and the next caller forgets.
- Let the `changed` handler through once suspension lifts. Rejected: suspension guards autosave and preview together; splitting them inside the same `return` creates a fragile exception.

Lifecycle: no editor callback list is cleared on dispose today (`editor_page/imp.rs::dispose` clears `SignalBag`s and `RefCell`s but no callback vec; `dispose_load_resources` only discards requests and bumps the generation). Disposal safety comes from the closures holding only `WeakRef`s. The new list follows exactly that contract; this change does not start clearing lists, because that would alter `connect_file_loaded` too.

`connect_file_loaded` stays for its existing consumers (bookmarks, eviction). The new hook is intentionally broader (failure, replacement, identity), so it is not a rename.

### D2. Installing state shows the preparing placeholder

In `refresh_preview`, inside the Markdown arm and before snapshotting, evaluate:

```
editor.load_state() == EditorLoadState::Loading
    || editor.load_projection_suspended()
    || editor.has_incomplete_load_installation()
```

When true, call `preview.show_content_placeholder("Preparing Markdown preview…")`, clear any pending source snapshot, and return. `load_projection_suspended()` already ORs the replacement guard, so the second term covers both workflows. The third term covers a failed load over a partially installed buffer (`installation_incomplete` is set by `load/retirement.rs:171` and is the flag the save workflow refuses on), where `load_state` is `Failed` but the buffer holds neither document.

Ordering matters: the non-Markdown arm still shows "Not a Markdown file" immediately, because the language is known from the path before content arrives. Untitled tabs have no language and take that arm unchanged. The `Failed` state with a complete buffer renders the buffer, which is what the user has.

Eviction works only because of an ordering that must be commented in code: `reload_if_evicted` (`documents.rs:1013`) runs before `refresh_preview` (`:1017`), and `begin_load_request` sets `Loading` synchronously (`load/admission.rs:216`), so a re-selected evicted tab hits the placeholder rather than an empty render. An evicted tab whose reload cannot start (no path) renders empty; `is_evicted()` is not added to the predicate because that case has no content to wait for.

`show_content_placeholder` (`markdown_preview/widgets.rs:111-116`) cancels the render session, switches to the content view, and writes the text into the preview's text buffer. It does not set the `AdwStatusPage` description, so `evidence().placeholder_description` stays `None`; tests observe it through `buffer_text()`.

Alternative considered: render the partial buffer during chunked install. Rejected: it spends the planning budget on text that will be replaced, and the debounce would still show stale slices.

### D3. Preview mode is window-level and follows the tab

Opening an existing document keeps the current mode and renders the new document in it. Rationale: preview mode is already a window property honoured by tab switching; opening a file is "create tab and select it"; a reader clicking the next `.md` in the sidebar wants to keep reading; non-Markdown files already have a named placeholder. `new_tab` keeps calling `exit_preview_only_mode_now` (`documents.rs:541`), a rule `openspec/specs/new-document-flow/spec.md` already owns; this change only references it. The existing `markdown-preview-headings` requirement that the menu action renders "the active Markdown document's current buffer content" is read together with D2: during an install the current buffer is not yet the document, and the placeholder names that.

### D4. Placement under the workflow role convention

- The fan-out registration lives with the load facade beside `connect_file_loaded` (`load/mod.rs`), and is called from two coordination terminals, the replacement facade's `finish_session`, and the cross-cutting identity group. No new role file, no new `*_for_test` seam; the ratchet stays at its recorded ceiling.
- The D2 predicate lives in `window/preview.rs`, a declared called presentation surface of `WFR-MARKDOWN-PREVIEW`. If extracted as a pure function it takes three booleans, imports no GTK types, and gets a unit test beside `markdown_snapshot_requires_chunked`.
- Facade budgets: `WFR-BUFFER-REPLACEMENT` declares 168 physical lines of 370 and the file is exactly 168; one narration line fits. `WFR-DOCUMENT-LOAD` declares no parseable `of 370` cell, so the gate is inert for that row; `load/mod.rs` is 277 physical lines today and stays far under budget. Each facade gains one line naming the republish.
- Matrix: `WFR-MARKDOWN-PREVIEW`'s size cell records `ui/window/preview.rs` at 572 physical / 556 production while the file is 586. Under the pre-existing-blockers rule this change re-derives that cell unconditionally, and the other two rows if their cells move.

## Risks / Trade-offs

- [The terminal fires for a non-selected tab and triggers a full render] → the listener checks `selected_page() == page` first; the background-tab widget test asserts `evidence().projection.dispatch_count` does not advance.
- [Double render when the load-completed callback and the new hook both run] → the load-completed callback does not refresh the preview today and must not start; the hook is the single owner.
- [A `Cancelled` or `Superseded` replacement renders a partial buffer briefly] → accepted; the superseding request fires again at its terminal. The alternative (placeholder forever) was measured worse.
- [A load terminal fires while an unrelated replacement still holds the guard] → the listener shows the placeholder; the replacement's own terminal fires the hook again. Self-healing.
- [Widget test cannot advance `AdwTimedAnimation` for the layout switch] → assert `buffer_text()` and evidence counters via end-state waits, not layout geometry, following the timed-animation note in `.agents/rules/widget-wiring.md`.
- [The hook adds one `Box<dyn Fn>` per editor] → same cost class as `connect_file_loaded`, same `WeakRef` contract.

## Migration Plan

No persisted state changes. Ship in one commit with the widget tests. Rollback is a revert.

## Open Questions

None. Save mirror-back was checked: it reaches the replacement terminal at `save/execution.rs:352` and is covered by D1 site 3.
