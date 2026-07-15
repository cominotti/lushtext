# Search Panel

This folder owns the workspace-wide content search panel adapter.

## Responsibilities

- Keep the panel as the driving adapter and keep its major workflows split into `history`, `list_factory`, `replace`, `results`, and `runtime`.
- Keep shell-level toggling, shortcuts, and window integration in `ui/window/search.rs`; keep panel-internal widget behavior here.
- Keep GTK list/tree model construction in this folder; services should return plain Rust search events and replacement data.

## Local Contracts

- Keep query state flowing through named value objects such as `SearchQuerySpec` instead of rebuilding parallel booleans and strings ad hoc.
- Preserve the preview/apply split for multi-file replace. If the user needs to inspect consequences before mutating files, that remains a two-step workflow.
- Keep Replace Preview generation bounded by its plain-Rust row/byte policy and resolve GTK rows through generation-scoped `SearchMatchId` lookup. Checked apply state may include only generated rows from the current accepted outcome; omitted, skipped, unchecked, cancelled, and stale rows never cross the confirmation boundary.
- Keep both workspace search and Replace Preview single-flight per panel. Workspace search retains at most one active controller/walker group plus one latest compact request and waits for terminal stream disconnection before restart; preview retains at most one active generation plus one latest request. Seal accepted matches once into a shared immutable snapshot and hand that snapshot into confirmation without cloning the whole result vector on GTK.
- Detach cleared, replaced, or closed result generations immediately. Retire their GTK rows and auxiliary navigation/preview caches through the generation-owned bounded disposer; one slice may release at most 250 references and must never touch the current model. Non-empty query churn applies latest-query backpressure at two detached generations, with one third slot reserved only for the immediate close/clear escape path.
- Keep search/replace controls and generated/checked/omitted/skipped feedback outside the item-only scroller. Dense Unicode or awkward paths must not introduce horizontal scrolling or make confirmation unreachable.
- Keep undo backup lifetime bounded to the active panel/search safety window. Ordinary panel close and successful undo clear it; after a crash, startup must expose a valid active journal as retryable Undo while cleaning only inactive or malformed state.
- Keep the active undo payload shared by `Arc` across callbacks and move final payload destruction off GTK when the panel replaces, clears, or consumes it; GTK may update only the small visible undo state synchronously.
- Preserve match-navigation and progress-callback contracts with the window shell.

## Editing Rules

- If a change is really search service logic rather than GTK panel behavior, move it to `services/content_search/` instead of expanding this adapter.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
