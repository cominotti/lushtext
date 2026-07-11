## Why

The broad main-thread responsiveness contract is sound, but several narrower paths still scale with live document size or repeated allocation: periodic local-history capture copies the whole buffer based on stale load-time size classification, Ctrl+F/Ctrl+H can copy an arbitrarily large selection into the query, byte scans use avoidable scalar loops, and Markdown code-block width updates revisit every embedded block even when the effective width is unchanged. These are modest individually but worth closing before declaring the codebase finished.

## What Changes

- Choose direct, chunked, or explicitly skipped periodic local-history capture from the current buffer size, with document/lifetime generation checks before accepting a snapshot.
- Bound in-document search/replace selection prefill to a query-sized maximum while leaving the existing query stable for oversized selections.
- Use established byte-search primitives for newline and compatible byte scans where they improve hot-path clarity and throughput without changing Unicode behavior.
- Cache the effective Markdown code-block width and avoid traversing embedded blocks when allocation inputs produce no width change.
- Add focused responsiveness, stale-completion, large-selection, Unicode, Markdown allocation, and performance-regression tests.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: Adds concrete live-buffer, selection-prefill, byte-scan, and unchanged-allocation bounds to the existing GTK responsiveness policy.
- `local-history`: Makes periodic capture use live buffer classification and bounded snapshot acceptance rather than stale file-size assumptions.
- `markdown-preview-code-blocks`: Prevents unchanged preview allocations from reprocessing every embedded code block while preserving all nested-width behavior.

## Impact

- Affects editor search and local-history adapters, byte-oriented helper code, and Markdown preview layout bookkeeping.
- Reuses existing chunked snapshot and performance-test infrastructure; no new task framework or abstraction layer is introduced.
- Should land before `decompose-large-gtk-adapters`, which may move some of the touched window/editor helpers.
