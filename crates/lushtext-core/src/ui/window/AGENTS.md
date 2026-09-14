# Window Shell

This folder owns the top-level application shell adapter.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextWindow`.
- Keep workflow-specific logic in sibling modules and per-workflow role homes: `actions`, `dialogs`, `documents`, `drafts/`, `editor_focus`, `editor_memory_eviction/`, `encoding/`, `focus_mode/`, `geometry/`, `local_history/`, `notes/`, `notifications/`, `palette_shell`, `preview`, `print/`, `recent_documents_journal`, `recent_open`, `search`, `search_progress_execution`, `session_restore/`, `startup_data`, `tab_strip/`, `transient_dismissal/`, `workspace_scope`, and `zoom`. The list is the tree as of slot 7b; `focus_indexing`, `tabs`, and `transient_surfaces` are the names it retired.
- Keep `imp.rs` focused on template children, state, and setup glue rather than long workflow implementations.

## Local Contracts

- Treat draft lifecycle and session snapshot persistence as separate workflows. Coordinate them explicitly; do not collapse them back into one catch-all module.
- Preserve failed file-backed page identity across load errors and session snapshots; retry must keep the original path and resolve a released eager draft from durable storage before applying recovery content.
- Keep progressive session page creation and file-plan admission in `session_restore.rs`. Pending work must remain compact, page creation must stay within the per-turn cap, every typed plan permit must terminate exactly once, and tab-derived projections must publish once only after the current generation becomes terminal. Cancellation and window/editor teardown must clear the scheduled source, permits, compact descriptors, and projection deferral without publishing stale aggregate state. Before startup publishes descriptors, close must merge bounded persisted descriptors with current pages through a linear stable-identity index so neither the unrestored session nor newly edited tabs are lost; retain the full recovery result, and abort close if its preservation diagnostics forbid replacement.
- Keep per-draft intent allocation and wrap-safe freshness in `draft_ordering.rs`. `drafts.rs` remains the GTK driving adapter and owns the single-flight body/manifest/delete choreography; commands waiting behind document-sized work must stay compact and GTK-free. New untitled identities must remain collision-resistant even before startup descriptors arrive. Requested deletion must remove the body before retiring its persisted manifest entry, keep an explicit current tombstone until both stages terminate, and pass explicit removal intent through retries after a body is already absent.
- Keep confirmed window-close coordination in `dialogs.rs`: reject user input across the selected-save pipeline and later draft/session yields, fingerprint discarded editor identity/content generation/modified/path state at confirmation, recheck active saves and freshness before cleanup/destruction, and restore retryable drafts plus sensitivity on every aborted close.
- Keep bookmark and note workflows in the private `notes/` module. `mod.rs`
  owns shared callbacks, migration coordination, and menu availability;
  `bookmarks.rs`, `editors.rs`, and `browser.rs` own their named workflows.
  Keep these GTK workflows out of `documents.rs`, `imp.rs`, and services. Browse
  Notes and Browse Bookmarks are modes of one live dialog/source/query owner;
  mode participates in every generation check. Bound live-editor snapshot bytes
  before a deferred request can wait for progress-lane admission, and never
  reintroduce an unrestricted production bookmark aggregate collector.
- Keep status-bar refresh and properties-panel refresh behavior aligned when window-level document state changes.
- Keep search-panel shell integration here, but keep search-panel internal list/history/replace/runtime mechanics in `ui/search_panel/`.
- Keep window-level transient dismissal in the `transient_dismissal/` role home (`WFR-TRANSIENT-DISMISSAL`, migrated in slot 7b; the file was `transient_surfaces.rs`): Escape closes one topmost dismissible shell surface before Focus Mode exit, and command-palette click-away routes through `close_command_palette()` so focus restoration stays centralized.
- Keep automation-friendly target-state actions in `actions.rs` thin and routed
  through the same production workflows as visible toggles. After adding or
  changing any externally observable action, update `services::action_catalog`
  plus `docs/automation-reference.md` and run `make check-automation-docs`.
- When split-view geometry changes, preserve the total-window width contracts and the mirrored status-bar toggle behavior described in the root `AGENTS.md` and `.agents/rules/ui.md`.
- Keep split-view allocation paths cheap and runtime-only. `size_allocate()` may clamp live sidebar fractions and update cached breakpoint thresholds when the actual allocated width changes, but it must not persist GSettings, rebuild/reparse `AdwBreakpoint` conditions, or rehost secondary surfaces on every animation frame.
- Keep plain split-view width, fraction, breakpoint, and compact-surface
  decisions in `geometry/policy.rs`, the pure policy role of
  `WFR-SHELL-GEOMETRY` (migrated in slot 7b; the module was `adaptive_shell.rs`,
  then flat `ui/window/policy.rs`). `geometry/execution.rs` owns restore,
  breakpoint installation, and allocation-time reconciliation;
  `geometry/mod.rs` is the narrative facade. `imp.rs` keeps the `size_allocate`
  vfunc — a subclass override GTK calls, which cannot move — and its
  `constructed()` wiring, and `actions.rs` keeps the two toggle action bodies
  that persist user intent; **both remain literal path keys in three
  visual-proof predicates in each of two implementations**, and the role home
  was **added** to all three as a narrow prefix rather than replacing them.
  Moving geometry code out without adding that key disarmed two named pixel
  invariants and the sidebar animation matrix while every gate still exited 0 —
  observed before it was fixed.
- Keep tab pin, reorder, and bulk close in the `tab_strip/` role home
  (`WFR-TAB-STRIP`). The teardown for a closed tab exists **once**, in
  `handle_tab_detached`: `close_page` is cancellable, and running teardown
  before that terminal strands a cancelled tab's draft.
- Keep the recent-documents journal in `recent_documents_journal.rs` (the
  `journal` coordination role of `WFR-RECENT-DOCUMENTS`, whose canonical role
  home is `ui/open_popover/`). `recent_open.rs` is that row's window-side
  **called presentation surface** and owns no stage.

## Editing Rules

- Prefer a new sibling workflow module over growing `mod.rs` or `imp.rs` into another large mixed-responsibility file.
- Update the root `AGENTS.md` and `README.md` module map when this folder's workflow files materially change.
