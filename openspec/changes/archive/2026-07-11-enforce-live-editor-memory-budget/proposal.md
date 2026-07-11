## Why

LushText advertises a 256 MiB live-editor memory budget, but the current accounting is based mainly on the last known on-disk file size. Untitled buffers and documents that grow substantially after opening can therefore consume memory without affecting the budget, while eviction is triggered only at a few lifecycle points. The policy should reflect live editor state and react when the aggregate actually crosses the limit without risking modified work.

## What Changes

- Introduce live, constant-time editor memory estimates that account for current buffer growth as well as known file bytes.
- Re-evaluate the aggregate budget after relevant load, edit, save, activation, and close transitions through a coalesced policy path.
- Keep active, modified, saving, untitled, and otherwise non-recoverable editors in memory; treat the budget as soft when safe eviction cannot satisfy it.
- Prevent eviction churn with explicit eligibility, recency, generation, and hysteresis rules.
- Add deterministic unit, integration, and scale coverage for untitled documents, large unsaved growth, delayed session restore, stale callbacks, and non-evictable over-budget states.

## Capabilities

### New Capabilities

- `live-editor-memory-budget`: Defines truthful live editor memory accounting, safe eviction eligibility, reactive enforcement, and soft-budget behavior.

### Modified Capabilities

None.

## Impact

- Affects editor buffer accounting and window-level loaded-editor eviction policy in `crates/lushtext-core/src/ui/editor_page/` and `crates/lushtext-core/src/ui/window/`.
- Adds policy-focused tests and benchmark/scale fixtures; it does not change the 256 MiB shipped budget.
- Depends on no other proposed change and should land before the final GTK adapter decomposition so moved modules inherit the settled policy.
