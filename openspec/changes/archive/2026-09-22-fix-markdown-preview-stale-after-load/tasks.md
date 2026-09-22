## 1. Editor-level content-republished hook

- [x] 1.1 Add `connect_content_republished<F: Fn() + 'static>` on `LushtextEditorPage` in `editor_page/load/mod.rs` beside `connect_file_loaded`, same `RefCell<Vec<Box<dyn Fn()>>>` storage, with a doc comment naming the four firing sites and the `WeakRef`-only listener contract (no list is cleared on dispose, matching `connect_file_loaded`)
- [x] 1.2 Add a private `fire_content_republished()` helper using the snapshot-then-invoke borrow pattern from `load/execution.rs:614-624`, and call it from `complete_loaded_installation` beside `refresh_minimap` (after `load_state` is `Loaded`)
- [x] 1.3 Call it from `publish_load_error` after `load_state`, `latest_load_failed`, and the inline notification are set
- [x] 1.4 Call it from `buffer_replacement/execution.rs::finish_session` after `restore_guard` and before the caller's `callback(outcome)`, on every terminal where `policy::guard_restores_on_terminal` is true (all reasons except `Disposed`)
- [x] 1.5 Call it at the end of `document_identity.rs::republish_document_identity`, after `refresh_accessibility_metadata`
- [x] 1.6 Add one narration line to each facade (`load/mod.rs` stage 7, `buffer_replacement/mod.rs` stage 7) naming the republish; confirm `buffer_replacement/mod.rs` stays within its declared 168-of-370 cell or update the cell (updated: 169 of 370)

## 2. Window wiring and installing-state placeholder

- [x] 2.1 Add `wire_preview_projection(page, editor)` in `ui/window/documents.rs` beside `wire_modified_indicator`; it registers the hook with weak refs and calls `refresh_preview()` only when `tab_view.selected_page() == page`
- [x] 2.2 Call `wire_preview_projection` from both `new_tab` and `open_document_with_intent_and_planning_terminal`
- [x] 2.3 In `ui/window/preview.rs::refresh_preview`, inside the Markdown arm and before snapshotting, when `load_state() == Loading || load_projection_suspended() || has_incomplete_load_installation()`: clear the pending source snapshot, call `show_content_placeholder("Preparing Markdown preview…")`, return; keep the non-Markdown arm unchanged
- [x] 2.4 Extract the predicate as a GTK-free pure function over three booleans with a unit test beside `markdown_snapshot_requires_chunked`
- [x] 2.5 Add a code comment at `refresh_selected_tab_model_projections` stating that `reload_if_evicted` must precede `refresh_preview` because `begin_load_request` sets `Loading` synchronously and the placeholder branch depends on it

## 3. Widget tests (window level; `tests/widget/markdown_preview.rs` builds the widget standalone, so these go in `window.rs` or a new `window_preview.rs` module)

- [x] 3.1 Preview-only active via `activate_action("toggle-preview-mode")`, open a temp `.md`: `wait_until_or_false` for `load_state() == Loaded` and for `markdown_preview.buffer_text()` containing the heading text; assert `lookup_action("toggle-preview-mode").state()` is still true
- [x] 3.2 Side-by-side active via `activate_action("toggle-preview-pane")`, open a temp `.md`: same `buffer_text()` assertion; assert `toggle-preview-pane` state is still true
- [x] 3.3 Immediately after `window.open_document(path)` returns and before pumping the main loop, assert `markdown_preview.buffer_text() == "Preparing Markdown preview…"` for a `.md`, and `evidence().placeholder_description` is "Not a Markdown file" for a `.rs`
- [x] 3.4 Buffer replacement on the selected Markdown tab through a real entry (local-history restore via `restore_execution`, or `evict()` which is public and empties the buffer): assert `buffer_text()` reflects the replaced buffer after the terminal
- [x] 3.5 Background tab terminal: open two `.md` tabs, select the first, let the second reach `Loaded`, and assert `buffer_text()` still shows the first tab's content and `evidence().projection.dispatch_count` did not advance
- [x] 3.6 Save As identity flip: preview active on an untitled tab showing "Not a Markdown file"; drive `set_file_path` to a `.md` path (or `complete_save_as` if reachable) and assert `buffer_text()` renders the Markdown
- [x] 3.7 Cancelled replacement: use `make_buffer_replacement_stale_after_slices_for_test` (existing seam) on the selected tab and assert the placeholder is replaced by a render of the buffer as it stands

## 4. Verification and documentation

- [x] 4.1 Run `make check`, `make test`, and `make check-workflow-boundaries`; fix any pre-existing blocker in the same stream (two blockers fixed: a Libadwaita NaN side-by-side spring warning, and a sidebar flake that was a real emptiness-probe collapse bug)
- [x] 4.2 Reproduce in a live isolated-XDG session: preview-only on, open a `.md` from the sidebar, confirm content appears without toggling; also confirm Save As of an untitled buffer to `.md`, and the external-change reload path (done in a fully isolated real session instead of the live desktop, because the user's own LushText instance owned `dev.cominotti.lushtext` on the session bus: private D-Bus + headless Mutter + isolated XDG via the `gtk-agentic-debugging` capture tool, the `.md` opened as a second `lushtext <path>` forwarded to the primary like a file-manager open rather than from the sidebar. Preview-only open renders; side-by-side external-change reload renders the new content; unmodified `HEAD` control shows the blank preview. Save As was not driven live because it needs the native file chooser; it is covered by the widget test through the same `set_file_path` identity hub)
- [x] 4.3 Update the Markdown preview bullet in `AGENTS.md` key design decisions: the four republish sites, the installing-interval placeholder and its three-term predicate, the eviction ordering dependency, and that preview mode is window-level and follows the tab
- [x] 4.4 Re-derive the `WFR-MARKDOWN-PREVIEW` size cell in `docs/workflow-readability-matrix.md` unconditionally (it records `ui/window/preview.rs` at 572/556; the file is already 586), and `WFR-DOCUMENT-LOAD` / `WFR-BUFFER-REPLACEMENT` cells if they move
- [x] 4.5 `README.md`: no feature statement changes; confirm and note no change (confirmed: README describes preview features, not the stale-after-load defect; no edit)
