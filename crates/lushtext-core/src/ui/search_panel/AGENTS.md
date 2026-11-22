# Search Panel

This folder owns the workspace-wide content search panel adapter.

## Responsibilities

- Keep the panel as the driving adapter, and keep the workflow readability roles this folder carries (it is the exemplar for `docs/workflow-readability-matrix.md` row `WFR-SEARCH-REPLACE`):
  - `mod.rs` is the **narrative facade**: it narrates the search and Replace All stage orders, names each control-flow inversion and where control resumes, and delegates every stage. It must not gain timers, generation counters, admission bookkeeping, or stage machinery. Beyond the trivial entry-point reads and writes of the visible query controls, widget mutation belongs in the coordination and adapter roles: a stage that needs to take state and repaint controls is delegated to a named intent-first operation such as `replace_execution::begin_confirmed_replacement` or `journal::hand_back_undo_backup`.
  - `policy.rs` is the **pure policy** module: search single-flight ownership, the per-turn retirement budget, and the `ReplacePreviewTicket`/`ReplacePreviewFacts` freshness seam. It must stay free of `gtk4`, `glib`, `gio`, `libadwaita`, and `sourceview5` imports so the default mutation scope keeps reaching it.
  - `execution.rs` (streaming search), `retirement.rs` (bounded result disposal), `replace_execution.rs` (Replace All preview and checked apply), `replace_admission.rs` (Replace stage 4's admission: a confirmed run parked until the disposal lane can admit its undo reservation, and its one capacity wakeup), and `journal.rs` (the durable, generation-guarded undo journal) are the **coordination** modules. A coordination file is named for the job it performs (no `runtime.rs`, no workflow-descriptive `replace.rs`); `replace_execution.rs` carries the stage-order qualifier because `execution.rs` is the search stage order's execution module. The three `imp::SearchPreviewState` fields both Replace All modules touch (`replace_transaction_pending`, `replace_transaction_generation`, `undo_backup_generation`) are owned by `journal.rs`, and `replace_execution` reads the first two only through `replace_transaction_claimed` and `replace_transaction_generation_reserved` — do not add a direct field read back.
  - `evidence.rs` is the **evidence surface**: `SearchPanelEvidence` plus the scalar observation accessors. New inspection needs extend that surface; do not add per-field `*_for_test` getters.
  - `history`, `list_factory`, `item`, `results`, and `accessibility` are **called
    presentation surfaces** of `WFR-SEARCH-REPLACE`: they carry no role, own no
    `policy.rs` and no `evidence.rs`, and are named in that workflow's matrix row.
  - `test_policy.rs` holds the workflow's single test-only timing/limit value and is entirely behind `#[cfg(feature = "test-utils")]`. Do not add module-level override statics elsewhere.
- Keep shell-level toggling, shortcuts, and window integration in `ui/window/search.rs`; keep panel-internal widget behavior here.
- Keep GTK list/tree model construction in this folder; services should return plain Rust search events and replacement data.

## Local Contracts

- Keep query state flowing through named value objects such as `SearchQuerySpec` instead of rebuilding parallel booleans and strings ad hoc.
- Preserve the preview/apply split for multi-file replace. If the user needs to inspect consequences before mutating files, that remains a two-step workflow.
- Keep Replace Preview generation bounded by its plain-Rust row/byte policy and resolve GTK rows through generation-scoped `SearchMatchId` lookup. Confirmation consumes the current outcome on a worker using the incrementally maintained checked-identity set, then revalidates panel/search/preview generation immediately before invoking Replace All. Checked apply state may include only generated rows from the current accepted outcome; omitted, skipped, unchecked, cancelled, and stale rows never cross the confirmation boundary.
- Keep both workspace search and Replace Preview single-flight per panel. Workspace search retains at most one active controller/walker group plus one latest compact request and waits for terminal stream disconnection before restart; preview retains at most one active generation plus one latest request. Seal accepted matches once into a shared immutable snapshot and hand that snapshot into confirmation without cloning the whole result vector on GTK.
- Detach cleared, replaced, or closed result generations immediately. Retire their GTK rows and auxiliary navigation/preview caches through the generation-owned bounded disposer; one slice may release at most 256 references (`SEARCH_RETIREMENT_ROWS_PER_SLICE`) and must never touch the current model. Non-empty query churn applies latest-query backpressure at two detached generations, with one third slot reserved only for the immediate close/clear escape path.
- Detach prior Replace Preview outcomes and checked identities on every enter, exit, invalidate, replace, stale completion, and close path. Document-sized plain preview payloads must reach final destruction through the bounded worker path, while GTK projection objects stay on the main thread; readiness remains pending through that handoff.
- Keep search/replace controls and generated/checked/omitted/skipped feedback outside the item-only scroller. Dense Unicode or awkward paths must not introduce horizontal scrolling or make confirmation unreachable.
- Keep undo backup lifetime bounded to one active recovery lineage. Panel hide, Escape, ordinary app close, and new searches preserve it; successful undo, explicit recovery discard, or a newly accepted Replace All journal may clear or supersede it. After a crash, startup must expose a valid active journal as retryable Undo while cleaning only inactive or malformed state.
- Serialize Replace All apply and undo behind one panel transaction. Reserve the apply journal generation before worker launch, reject stale reservations while holding the journal coordinator before any journal preparation or file mutation, disable prior Undo during apply, and never allow two service commits to publish out of order.
- Keep the active undo payload shared by `Arc` across callbacks and move final payload destruction off GTK when the panel replaces, clears, or consumes it; GTK may update only the small visible undo state synchronously.
- Replace All accounting separates reversible live undo bytes from monotonic
  high-water evidence. Every pre-rename removal reclaims its exact charge and
  journal entry; ambiguous post-rename durability keeps both. Replace/Undo may
  return only exact totals, deterministic 32-entry/32-KiB diagnostic samples,
  and the affected/restored intersection of paths that were open at worker
  submission. Undo ingestion stays bounded inside the read so growth races
  preserve retryable backup state without an unsafe write.
- The GTK side of Replace All owns **descriptors only**: the undo journal's
  original bytes live in raw body files on disk, so the undo owner that crosses
  to GTK is paths and digests, never file content. A run in flight is a
  `ReplaceRunControl` (cancel flag plus `ReplaceProgress` atomics) the window's
  progress poll reads; Cancel only sets the flag, and the worker rolls back
  before its completion runs, so the transaction stays claimed (and
  `search-complete` pending) through the rollback.
- The skip report is plain data (`ReplaceSkipReport`); `skipped_files.rs` is a
  called presentation surface over a lazy `GListModel` + `GtkSectionModel` that
  builds a row object only when GTK asks for it. Its first publish is staged
  (`list_budget::staged_first_rows`, then `ui::list_growth::RowGrowth` once the
  dialog's open animation is over or the list scrolls; a key finishes it):
  handed every row at once, GTK realized its whole band (about 205 rows, A1)
  inside the opening dispatch. Its pure decisions stay in `policy.rs` and its
  observable state in `evidence.rs`.
- **Open and Replace in Editor** (`win.replace-in-editor`) prefills the in-tab
  bar from the run's **request** (`policy::ReplaceRunIntent`: the accepted
  preview ticket's query spec plus its replacement text, captured on preview
  acceptance and copied at `begin_replace_run`), never from the panel's live
  fields, which the user may have edited since. A regex template reaches
  GtkSourceView only through `policy::rust_replacement_to_gtksourceview`
  (ledger A127); a reference to a missing group leaves the field empty with
  its reason. The action writes nothing. Its availability is
  `policy::replace_in_editor_availability`; a row whose file cannot open has
  no action.
- **A confirmed run that cannot reserve its undo descriptors waits; it never
  gives up** (`replace-all-confirm-disposal-lane`). The wait is the run's
  first phase: `begin_replace_run` has run, so the button is Cancel Replace
  All and `replace_in_flight` is true, and the panel holds the transaction, so
  another window's apply or Undo is refused as during a run. The parked
  `ParkedReplaceRun` (the `DisposalOwned` selection plus its
  `ReplaceJournalFreshness`) lives in `replace_admission.rs` with one
  `DisposalCapacityWakeup`; each wakeup asks `policy::replace_admission_resume`
  and hands the window a `ReplaceAdmissionResumption`, and the window
  recomputes the unsaved-tab skip set before every attempt. Every window's
  earlier Undo stays installed until admission, and the earlier skip report
  is cleared only at admission (`clear_previous_skip_report`). Only Cancel,
  Stop and Close, or dispose ends the wait: hiding the panel, Escape and a new
  search do not. A cancelled or superseded wait ends through
  `end_deferred_replace_run`, never `finish_replace_run(None)`, which would
  erase the earlier skip report. Dispose cancels the wakeup, drops the parked
  selection to the lane, and calls `journal::release_replace_transaction_on_dispose`,
  which resets only the two transaction fields and touches no widget; it
  calls no window callback.
- **The undo journal is one per application** (design D3a): `journal.rs`
  keeps a thread-local registry of every panel. A transaction (a run, its
  rollback, an Undo) in one window refuses an apply or Undo in another with a
  status message, never queues it; a run's start retires every window's Undo
  and each window that lost one says so once. Never scope either back to one
  panel.
- Preserve match-navigation and progress-callback contracts with the window shell.

## Editing Rules

- If a change is really search service logic rather than GTK panel behavior, move it to `services/content_search/` instead of expanding this adapter.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
