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
- Keep preview work single-flight per panel: cancel the active generation when search/query/options/panel state changes, retain at most one latest superseding request, and move accepted rows into confirmation instead of cloning their preview payload on the GTK thread.
- Keep search/replace controls and generated/checked/omitted/skipped feedback outside the item-only scroller. Dense Unicode or awkward paths must not introduce horizontal scrolling or make confirmation unreachable.
- Keep undo backup lifetime bounded to the active panel/search safety window. Do not let stale replace backups outlive panel close, search reset, or app exit semantics.
- Preserve match-navigation and progress-callback contracts with the window shell.

## Editing Rules

- If a change is really search service logic rather than GTK panel behavior, move it to `services/content_search/` instead of expanding this adapter.
- Update the root `AGENTS.md` and `README.md` module map when this folder's structure materially changes.
