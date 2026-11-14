# Window Shell

This folder owns the top-level application shell adapter.

## Responsibilities

- Keep `mod.rs` as the small public facade for `LushtextWindow`.
- Keep workflow-specific logic in sibling modules and per-workflow role homes: `actions`, `attention_refresh`, `composition/`, `construction`, `dialogs`, `documents`, `drafts/`, `editor_focus/`, `editor_memory_eviction/`, `encoding/`, `focus_mode/`, `geometry/`, `local_history/`, `notes/`, `notifications/`, `palette_shell`, `preview`, `print/`, `recent_documents_journal`, `recent_open`, `search`, `search_progress_execution`, `session_restore/`, `set_aside_review`, `startup_data`, `tab_strip/`, `transient_dismissal/`, `workspace_scope`, and `zoom`.
- Keep `imp.rs` focused on template children, state, and setup glue rather than long workflow implementations.

## Local Contracts

- Treat draft lifecycle and session snapshot persistence as separate workflows. Coordinate them explicitly; do not collapse them back into one catch-all module.
- Preserve failed file-backed page identity across load errors and session snapshots; retry must keep the original path and resolve a released eager draft from durable storage before applying recovery content.
- Keep progressive session page creation and file-plan admission in `session_restore/` (`admission.rs`, `execution.rs`). Pending work must remain compact, page creation must stay within the per-turn cap (at most four pages per GTK turn, at most two file-load planning operations at once), every typed plan permit must terminate exactly once, and tab-derived projections must publish once only after the current generation becomes terminal. Cancellation and window/editor teardown must clear the scheduled source, permits, compact descriptors, and projection deferral without publishing stale aggregate state. Before startup publishes descriptors, close must merge bounded persisted descriptors with current pages through a linear stable-identity index so neither the unrestored session nor newly edited tabs are lost; retain the full recovery result, and abort close if its preservation diagnostics forbid replacement.
- Keep per-draft intent allocation and wrap-safe freshness (`DraftMutationOrder`) in `drafts/policy.rs`. The rest of `drafts/` is the GTK driving adapter and owns the single-flight body/manifest/delete choreography; commands waiting behind document-sized work must stay compact and GTK-free. New untitled identities must remain collision-resistant even before startup descriptors arrive. Requested deletion must remove the body before retiring its persisted manifest entry, keep an explicit current tombstone until both stages terminate, and pass explicit removal intent through retries after a body is already absent.
- The draft journal is **one per application**, not one per window (`drafts/admission.rs`, `ProcessDraftJournal`): every registration, body-write pass, deletion, and orphan-cleanup pass of every window takes its one journal lane and marks itself pending when another window holds it; a draft id is claimed by one window (an open of a file another window has open presents that window's tab, and any second editor that still appears holds its autosave, restore, and deletion); only the first window to reach its startup data flow restores the session and schedules orphan cleanup; every commit, authority change, and cleanup removal is mirrored into every window's manifest copy; cleanup inspects the persisted manifest under its lock; and session saves carry every window's tabs in creation order with an application-wide generation. Window dispose leaves the journal (lane, claims, cleanup ownership) exactly once. The two-window Kani journal harness (`journal_invariants_hold_across_two_windows`) and `crates/lushtext/tests/widget/multi_window_drafts.rs` pin each of these; do not scope any of them back to one window.
- Keep confirmed window-close coordination in `dialogs.rs`: reject user input across the selected-save pipeline and later draft/session yields, fingerprint discarded editor identity/content generation/modified/path state at confirmation, recheck active saves and freshness before cleanup/destruction, and restore retryable drafts plus sensitivity on every aborted close.
- Keep bookmark and note workflows in the private `notes/` module. `mod.rs`
  is the narrative facade; `bookmark_execution.rs`, `editor_execution.rs`,
  `source_execution.rs`, `query_execution.rs`, and `install_execution.rs` own
  their coordination, and `browser.rs` is a called presentation surface.
  Keep these GTK workflows out of `documents.rs`, `imp.rs`, and services. Browse
  Notes and Browse Bookmarks are modes of one live dialog/source/query owner;
  mode participates in every generation check. Bound live-editor snapshot bytes
  before a deferred request can wait for progress-lane admission, and never
  reintroduce an unrestricted production bookmark aggregate collector. The
  browser lists every admitted row in a virtualized `GtkListView` over
  `row_model.rs`; do not reintroduce a row cap or an `AdwSidebar` there
  (ledger A92), keep publishes a removal then an addition (A93), keep the
  open-editor capture sliced (`turn_continues`, `RetainedBudget`, no count),
  and keep the model free of any reference to the `DisposalOwned` source.
- Keep status-bar refresh and properties-panel refresh behavior aligned when window-level document state changes.
- Keep search-panel shell integration here, but keep search-panel internal list/history/replace/retirement mechanics in `ui/search_panel/`.
- Keep window-level transient dismissal in the `transient_dismissal/` role home (`WFR-TRANSIENT-DISMISSAL`): Escape closes one topmost dismissible shell surface before Focus Mode exit, and command-palette click-away routes through `close_command_palette()` so focus restoration stays centralized.
- Keep automation-friendly target-state actions in `actions.rs` thin and routed
  through the same production workflows as visible toggles. After adding or
  changing any externally observable action, update `services::action_catalog`
  plus `docs/automation-reference.md` and run `make check-automation-docs`.
- When split-view geometry changes, preserve the total-window width contracts and the mirrored status-bar toggle behavior described in the root `AGENTS.md` and `.agents/rules/ui.md`.
- Keep split-view allocation paths cheap and runtime-only. `size_allocate()` may clamp live sidebar fractions and update cached breakpoint thresholds when the actual allocated width changes, but it must not persist GSettings, rebuild/reparse `AdwBreakpoint` conditions, or rehost secondary surfaces on every animation frame.
- Keep plain split-view width, fraction, breakpoint, and compact-surface
  decisions in `geometry/policy.rs`, the pure policy role of
  `WFR-SHELL-GEOMETRY`. `geometry/execution.rs` owns restore,
  breakpoint installation, and allocation-time reconciliation;
  `geometry/mod.rs` is the narrative facade. `imp.rs` keeps the `size_allocate`
  vfunc — a subclass override GTK calls, which cannot move — and its
  `constructed()` wiring, and `actions.rs` keeps the two toggle action bodies
  that persist user intent; **both remain literal path keys in three
  visual-proof predicates in each of two implementations**
  (`scripts/check-visual-proof-policy.py` and
  `crates/cargo-gtk-proof/src/policy.rs`), and the role home is a narrow prefix
  **added** to all three. Moving geometry code without adding its key disarms
  the named pixel invariants and the sidebar animation matrix while every gate
  still exits 0.
- Keep tab pin, reorder, and bulk close in the `tab_strip/` role home
  (`WFR-TAB-STRIP`). The teardown for a closed tab exists **once**, in
  `handle_tab_detached` (`tab_strip/close_execution.rs`), which also cancels the
  editor's load: `close_page` is cancellable, and running teardown before that
  terminal strands a cancelled tab's draft.
- Keep the recent-documents journal in `recent_documents_journal.rs` (the
  `journal` coordination role of `WFR-RECENT-DOCUMENTS`, whose canonical role
  home is `ui/open_popover/`). `recent_open.rs` is that row's window-side
  **called presentation surface** and owns no stage.

## Editing Rules

- Prefer a new sibling workflow module over growing `mod.rs` or `imp.rs` into another large mixed-responsibility file.
- Update the root `AGENTS.md` and `README.md` module map when this folder's workflow files materially change.
