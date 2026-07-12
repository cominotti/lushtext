## Why

Replace Preview is generated off the GTK thread, but its model can still duplicate original, replaced, and replacement strings for every match, and GTK row binding performs linear lookups and repeated path formatting. Large result sets or large replacement text can therefore create avoidable RAM growth and O(N squared) presentation work before the user applies anything. Preview itself needs explicit resource and identity contracts, not only asynchronous dispatch.

## What Changes

- Add explicit Replace Preview row-count and total-byte budgets with a typed truncated outcome and user-visible omitted-result summary.
- Preserve the preview/apply split: only generated, visible, checked preview rows are eligible for confirmation, and truncation never expands the apply set implicitly.
- Share immutable literal replacement data where safe, retain owned regex expansions where required, and remove duplicate row content that has no consumer.
- Give preview rows stable identities and direct indexes so list binding, toggling, and activation do not scan the full result set or rebuild display paths repeatedly.
- Keep the query, replacement, search-result, and panel-generation stale-result guards.
- Add service, property, widget, geometry, and scale coverage across empty, representative, 10,000-match, large-replacement, awkward-path, and constrained-window states.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `search-replace-safety`: Extends Replace Preview with bounded memory, explicit truncation semantics, stable row identity, and confirmation rules that cannot apply omitted matches.

## Impact

- Affects `services/content_search`, `model/content_search.rs`, and `ui/search_panel/` preview/list-factory workflows.
- Does not weaken atomic Replace All writes, stale-file validation, cancellation rollback, or the undo journal.
- Can be implemented independently after draft changes and should precede GTK adapter decomposition.
