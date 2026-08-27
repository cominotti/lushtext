# Search Panel

This folder owns the workspace-wide content search panel adapter.

## Responsibilities

- Keep the panel as the driving adapter, and keep the workflow readability roles this folder now carries (it is the exemplar for `docs/workflow-readability-matrix.md` row `WFR-SEARCH-REPLACE`):
  - `mod.rs` is the **narrative facade**: it narrates the search and Replace All stage orders, names each control-flow inversion and where control resumes, and delegates every stage. It must not gain timers, generation counters, admission bookkeeping, or stage machinery. Beyond the trivial entry-point reads and writes of the visible query controls, widget mutation belongs in the coordination and adapter roles: a stage that needs to take state and repaint controls is delegated to a named intent-first operation such as `replace_execution::begin_confirmed_replacement` or `journal::hand_back_undo_backup`.
  - `policy.rs` is the **pure policy** module: search single-flight ownership, the per-turn retirement budget, and the `ReplacePreviewTicket`/`ReplacePreviewFacts` freshness seam. It must stay free of `gtk4`, `glib`, `gio`, `libadwaita`, and `sourceview5` imports so the default mutation scope keeps reaching it.
  - `execution.rs` (streaming search), `retirement.rs` (bounded result disposal), `replace_execution.rs` (Replace All preview and checked apply), and `journal.rs` (the durable, generation-guarded undo journal) are the **coordination** modules. Do not reintroduce a `runtime.rs`, and do not reintroduce a workflow-descriptive `replace.rs`: a coordination file is named for the job it performs. `replace_execution.rs` carries the stage-order qualifier because `execution.rs` is already the search stage order's execution module; `journal.rs` needs none. The three `imp::SearchPreviewState` fields both Replace All modules touch (`replace_transaction_pending`, `replace_transaction_generation`, `undo_backup_generation`) are owned by `journal.rs`, and `replace_execution` reads the first two only through `replace_transaction_claimed` and `replace_transaction_generation_reserved` — do not add a direct field read back.
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
- Detach cleared, replaced, or closed result generations immediately. Retire their GTK rows and auxiliary navigation/preview caches through the generation-owned bounded disposer; one slice may release at most 250 references and must never touch the current model. Non-empty query churn applies latest-query backpressure at two detached generations, with one third slot reserved only for the immediate close/clear escape path.
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
- Preserve match-navigation and progress-callback contracts with the window shell.

## Editing Rules

- If a change is really search service logic rather than GTK panel behavior, move it to `services/content_search/` instead of expanding this adapter.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
