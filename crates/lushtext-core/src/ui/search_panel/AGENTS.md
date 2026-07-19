# Search Panel

This folder owns the workspace-wide content search panel adapter.

## Responsibilities

- Keep the panel as the driving adapter and keep its major workflows split into `history`, `list_factory`, `replace`, `results`, and `runtime`.
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
