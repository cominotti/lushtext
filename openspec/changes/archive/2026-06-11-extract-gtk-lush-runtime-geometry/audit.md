# Phase 3 Preflight Audit

Change: `extract-gtk-lush-runtime-geometry`

## Baseline

- `git status --short` before implementation:
  - `?? .claude/worktrees/`
  - `?? openspec/changes/extract-gtk-lush-runtime-geometry/`
- `rg -n "spawn_blocking_then\(" crates/lushtext-core/src crates/lushtext/tests | wc -l`: `62`
- The only tracked implementation changes at audit start are the new OpenSpec proposal artifacts.

## Task Caller Audit

All `spawn_blocking_then` call sites must move to `gtk-lush-tasks` while retaining app-owned domain freshness, persistence ordering, and data-safety policy at the call site.

| File and lines | Workflow owner | Classification | Migration note |
| --- | --- | --- | --- |
| `crates/lushtext-core/src/ui/sidebar/workspaces.rs:32,281,687` | Workspace load and debounced workspace persistence | Persistence ordering retained by sidebar generation/in-flight save state | Replace dispatch import only; sidebar continues owning dirty snapshots and stale-save rejection. |
| `crates/lushtext-core/src/ui/window/notes.rs:364,413,487,763,928,1041,1137,1230,1271,1358,1400,1456,2678` | Document notes, folder notes, bookmark-note sidecars, moves, and previews | Sidecar identity and UI lifetime remain window-owned | Replace dispatch import only; keep path identity, selected editor/window weak refs, and stale UI checks in window code. |
| `crates/lushtext-core/src/ui/editor_page/local_history.rs:219,287` | Editor-local history capture and restore | Editor lifecycle and modified-state ownership retained by editor page | Replace dispatch import only; keep editor-owned path and cancellation state outside the reusable crate. |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs:1604` | Markdown preview render worker | Preview generation and mode state remain preview-owned | Replace dispatch import only; keep stale render rejection in the preview module. |
| `crates/lushtext-core/src/ui/window/session_persistence.rs:84,122,143` | Session save/load safety net | Session generation and close-request safety remain window-owned | Replace dispatch import only; keep synchronous close safety separate from the worker abstraction. |
| `crates/lushtext-core/src/ui/editor_page/load_save.rs:67,424` | File load/save | Load cancellation, save-in-progress, durable-write result handling remain editor-owned | Replace dispatch import only; preserve all data-loss prevention and modified-dot behavior. |
| `crates/lushtext-core/src/ui/window/search.rs:175,239,292,304,587` | Saved searches and search panel async helpers | Window/search state owns history, scope, and selected editor freshness | Replace dispatch import only; do not move search-domain policy into `gtk-lush-tasks`. |
| `crates/lushtext-core/src/ui/editor_page/imp.rs:947` | Editor page background visual/computation helper | Editor state and visual generation retained by editor page | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/folders.rs:459` | Folder-picker/tree refresh helper | Section owns selected folder and model reconciliation | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/window/local_history.rs:147,208,263,572,671` | Window-level local-history list/load/move/delete flows | Window owns active editor identity, selected path, and stale callbacks | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/editor_page/monitor.rs:61` | External file monitor metadata probe | Editor owns file identity and monitor state | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/window/focus_indexing.rs:320` | Focus Mode indexing | Index generation and mode state remain window-owned | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/tree_loading.rs:146` | File-tree async child loading | Section owns expanded path and store reconciliation | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/window/encoding.rs:376` | Encoding inspection/conversion support | Active editor and requested encoding remain window-owned | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/actions.rs:42,242,365` | New file/folder, rename, delete | Section owns path identity, inline rename, and destructive-action confirmation | Replace dispatch import only; keep filesystem mutation policy in app services. |
| `crates/lushtext-core/src/ui/search_panel/runtime.rs:345` | Workspace content-search runtime | Runtime owns search generation, cancellation, and event stream | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/window/drafts.rs:205,325,515,625,761,844` | Draft load/save/delete/flush/recovery flows | Draft manifest ordering and close safety remain window-owned | Replace dispatch import only; keep draft data-safety checks in LushText. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs:335` | Sidebar file peek | Section owns selected row/path lifetime | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/command_palette/imp.rs:459` | Command-palette file indexing | Palette owns query/index freshness | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/search_panel/replace.rs:62,109,132,185` | Replace All, undo backup, and cleanup | Search panel owns replace/undo safety windows | Replace dispatch import only; preserve backup and undo policy in search code. |
| `crates/lushtext-core/src/ui/window/documents.rs:639,746` | Document path/metadata helper work | Window/editor own open path set and tab state | Replace dispatch import only. |
| `crates/lushtext-core/src/ui/search_panel/history.rs:159,179` | Search history persistence | Search panel owns history cap and dedupe policy | Replace dispatch import only. |
| `crates/lushtext-core/src/services/async_task.rs:60,138` | Existing helper implementation and test | Replaced by `gtk-lush-tasks` | Remove or reduce old helper after migration so policy forbids app-owned clones. |

Post-migration:

- `services::async_task` was deleted after app call sites moved to `gtk_lush_tasks::spawn_blocking_then`.
- `rg -n "spawn_blocking_then\(" crates/lushtext-core/src | wc -l`: `60`
- `rg -n "async_task" crates/lushtext-core/src crates/lushtext/tests`: no matches
- The two removed baseline references are the old helper's internal saturated-backpressure code and its unit test, not LushText workflow callers.
- Retained explicit sites are the workflow modules listed above: stale tab/path/search/draft/session/workspace/replace/minimap policy stays in LushText, while the reusable crate owns only bounded dispatch and main-thread completion mechanics.

## Viewport And Overscroll Audit

- Current overscroll ownership lives in `crates/lushtext-core/src/ui/editor_page/overscroll.rs`.
- `OverscrollState` stores rest state, animation frames, and reflow repair logic for the editor scroller.
- The reusable crate can own adjustment observation, rest-state pause/resume, and change notification wiring.
- LushText must keep caller-owned decisions: when to repair scroll, how to classify minimap/sidebar geometry, and how editor lifecycle cancels pending animation frames.
- Migration target: retain `editor_page/overscroll.rs` as the app adapter and delegate adjustment observation/rest-state bookkeeping to `gtk-lush-viewport`.

Post-migration:

- `gtk_lush_viewport::ViewportObserver` owns adjustment `changed` and `value-changed` signal registrations and disconnects them through `OverscrollState::observer`.
- `gtk_lush_viewport::RestState` owns horizontal and vertical lower-edge bookkeeping; `OverscrollState::reflow_pause` holds a `RestPause` during minimap width-reflow settle so transient GTK-preserved values cannot rewrite user intent.
- Retained explicit app sites:
  - `editor_page/overscroll.rs` still schedules dynamic EOF overscroll, left-edge clamp, top-edge clamp, Focus Mode refresh, and minimap refresh because those are LushText editor policies.
  - `editor_page/minimap.rs` still owns reflow settle timing, reveal delay, and source-map repair because the viewport crate must not depend on minimap behavior or other GTK Lush crates.
  - Widget tests remain the proof surface for actual GTK adjustment signal behavior; crate unit tests keep pure rest-state logic always-on and mark display-required observer tests ignored for ordinary no-display cargo runs.

## Clip Bin Audit

- Current widget lives in `crates/lushtext-core/src/ui/shrinkable_bin/`.
- Resource references:
  - `crates/lushtext-core/resources/ui/window.blp`
  - `crates/lushtext-core/resources/ui/window.ui`
  - `crates/lushtext-core/resources/ui/template-contract.json`
- Tests cover status-bar visibility and shrink behavior in `crates/lushtext/tests/widget/window.rs`.
- Migration target: move the single-child zero-minimum clipping widget to `gtk-lush-widgets::ClipBin`, register the type before template loading, and update Blueprint/template contracts.

Post-migration:

- `gtk_lush_widgets::ClipBin` owns the single-child zero-minimum measurement, child allocation, and snapshot clipping behavior under the GLib type name `GtkLushClipBin`.
- `LushtextShrinkableBin` and the app-local `ui/shrinkable_bin` module were deleted.
- `resources/ui/window.blp`, generated `window.ui`, and `template-contract.json` now reference `GtkLushClipBin#window_content_clipper`.
- Retained explicit app sites:
  - `window/imp.rs` still registers the type before binding the window template, because template construction order is an application shell concern.
  - Short-window/status-bar/minimap/search-panel proof remains in LushText widget tests, because `ClipBin` intentionally does not know the surrounding shell chrome.

## Render Hold And Minimap Audit

- Minimap reflow freeze state currently lives in `crates/lushtext-core/src/ui/editor_page/imp.rs` with `reflow_freeze_picture`, `reflow_settle`, and source-map opacity restoration.
- Runtime diagnostics reference the freeze picture in `crates/lushtext-core/src/ui/automation.rs`.
- Widget tests assert minimap native `GtkSourceMap` behavior, reflow freeze behavior, and geometry in `crates/lushtext/tests/widget/editor_page.rs` and `crates/lushtext/tests/widget/window.rs`.
- Migration target: introduce `gtk-lush-widgets::RenderHoldOverlay` for capture/reveal/clear mechanics while LushText retains minimap timing, warm-up, automation fields, and visual proof tests.

Post-migration:

- `gtk_lush_widgets::RenderHoldOverlay` owns cover picture creation, capture, non-targetable overlay behavior, live-child opacity pairing, warm-under-cover, reveal/clear, supersede preservation, and `Drop` cleanup.
- `MinimapState::render_hold` replaced the app-local `reflow_freeze_picture` field.
- The automation visual surface name `minimap-reflow-freeze` is unchanged; it now points at `RenderHoldOverlay::cover()` so the external snapshot contract does not need a D-Bus/schema change.
- Retained explicit app sites:
  - `editor_page/minimap.rs` still owns when to capture before shell transitions, when to schedule settle, when to warm, when to reveal early on direct user scroll, and how to repair native `GtkSourceMap` geometry.
  - `editor_page/minimap.rs` still keeps marker layering above the hold cover by creating the render hold before adding the marker strip overlay.
  - Widget and visual-geometry tests remain the proof surface for native source-map highlight rendering and final settled geometry, because the reusable crate deliberately has no minimap knowledge.

## Docs And Rule Impact

- Update root and crate `AGENTS.md` files for Phase 3 crate ownership.
- `.agents/rules/rust.md`, `.agents/rules/ui.md`, and `.agents/rules/widget-wiring.md` changed where task dispatch, viewport observation, ClipBin, or RenderHoldOverlay policy became stable guidance. `.agents/rules/build.md` did not need a Phase 3 edit because build commands are still routed through the same GTK Lush make targets.
- `docs/next/gtk-lush.md` and the GTK Lush policy script/docs changed for the new crates and example policy. There is no live `docs/gtk-lush.md` file in this checkout, so no additional documentation path was updated.
- Add or update rustdoc/module docs in each new crate so consumers can identify what belongs in reusable GTK Lush APIs versus app-owned workflows.

## Review Outcomes

- Data-safety review: no findings. Drafts, close-time draft flushing, session/workspace persistence, Replace All undo backup, stale completion guards, and task call-site migration retained their caller-owned freshness and persistence ordering. Residual note: workspace persistence still logs failed saves and retries on later mutation, which remains workspace metadata durability rather than document/draft data loss.
- GTK responsiveness/performance review: fixed all actionable findings. `gtk-lush-tasks` now keeps worker slots alive until the GLib idle completion consumes the result, eliminating completed-result buildup outside the cap; saturated work waits in a main-thread FIFO woken by slot release instead of per-task retry timers; `ViewportObserver` now compares against the last emitted page size so cumulative sub-epsilon changes are not lost; modified-line minimap marks are capped and evenly sampled for large ranges/restored drafts.
- GTK/Libadwaita internals review: fixed all actionable findings. `ClipBin` now clamps child measurement requests before querying the child and suppresses baselines so the zero-minimum contract does not pair `-1` with a child natural baseline. `RenderHoldOverlay` now restores the original live-child opacity instead of hard-coding `1.0`.
- Architecture review: fixed all actionable findings. `crates/lushtext-core/AGENTS.md`, root `AGENTS.md`, and `README.md` no longer advertise deleted app-local task/shrinkable-bin modules; `RenderHoldOverlay` gained narrow cover helpers for styling/visibility while retaining the diagnostic cover accessor for automation geometry.
- Comments/documentation review: fixed all actionable findings. Public examples now model a GLib main loop or `gtk4::Application`, widget implementation modules gained lifecycle and measurement comments, low-level task/viewport constants and signal lifetimes gained rationale, GTK Lush dual-license policy is documented, and stale `docs/next/gtk-lush.md` / audit wording was corrected.
- Focused fix verification after reviews:
  - `cargo fmt --all -- --check`
  - `cargo check -p gtk-lush-tasks -p gtk-lush-viewport -p gtk-lush-widgets --examples --tests`
  - `cargo test -p gtk-lush-tasks`
  - `cargo test -p gtk-lush-viewport`
  - `cargo test -p gtk-lush-widgets`
  - `cargo check -p lushtext-core --tests`
  - `cargo test -p lushtext-core test_modified_line_mark_samples_cover_large_ranges_with_a_cap`

## Final Proof Summary

- Formatting and lint:
  - `cargo fmt --all -- --check`
  - `make check-clippy`
- GTK Lush family proof:
  - `make check-gtk-lush-policy`
  - `make gtk-lush-doctests`
  - `make gtk-lush-examples`
  - `make gtk-lush-msrv`
  - `make gtk-lush-api-advisory` (expected advisory-only missing crates.io baselines for in-tree `0.0.0` crates; public API snapshots generated)
- Repository proof:
  - `cargo nextest run --workspace` (`760 passed, 10 skipped`)
  - `cargo deny check advisories bans sources licenses` (exit 0 with existing workspace-duplicate warnings)
  - `make check-agent-docs`
  - `make check-automation-docs`
  - `make check-policy`
  - `make check`
- Widget proof:
  - `make test-widget-headless` initially exposed a flaky retry in `window::test_bookmark_gutter_edit_dialog_validates_moves_and_persists`.
  - The test was stabilized by waiting for Adw `EntryRow` text changes before clicking `Save`; the focused test then passed 10 consecutive no-retry runs.
  - Final `make test-widget-headless` passed all 756 widget tests with no flaky summary.
- Visual proof:
  - `make visual-geometry-smoke SMOKE_ARTIFACT_DIR=build/smoke`
  - `make check-visual-proof-policy`
  - Current proof summary: `build/smoke/visual-geometry/summary.json`
  - Required visual invariant ids pixel-verified: `native-minimap-highlight-anchors`
  - Required visual invariant ids animation-verified: `native-minimap-animation-highlight-anchors`
