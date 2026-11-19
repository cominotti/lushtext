# Design

## Context

See `proposal.md`. `Debounce` and `SupersedingTimer` share `SupersedingTimerState` (`take_scheduled`, `remove_source`, `forget_if_current`). `SettleBurst` has its own `GenerationGate` and a `pending: Rc<Cell<bool>>` that `SettleHandle`s clear.

## Goals / Non-Goals

**Goals:** one pending source per burst; no refcount churn for weak sources. **Non-Goals:** `proposal.md`.

## Decisions

### D1. `SettleBurst` keeps its scheduled source

- **The field.** `SettleBurst` gains `scheduled: Rc<RefCell<Option<(TimerToken, SourceId)>>>`. Its `schedule` follows `Debounce::schedule`:
  1. take the old entry;
  2. `begin()` (advances the gate, sets pending);
  3. remove the old source;
  4. return early if a destructor re-entered and advanced again;
  5. install the new timeout, whose callback forgets its own entry when current before running.
- **`clear`** takes and removes the entry.
- **`begin()` without a source** is unchanged.
- **Why not `SupersedingTimerState` directly.** `SettleBurst`'s gate is shared with its handles, whose `complete` must clear `pending` only for the current generation. A separate `scheduled` field keeps that contract untouched.

### D2. `upcast_ref`

`SignalRegistration::new(source.upcast_ref::<Object>(), id)` at both sites. No behaviour change.

## Risks / Trade-offs

- **Removing a source drops its closure captures synchronously,** which can re-enter the burst from a destructor. → The `Debounce` order (advance, then remove), and a reentrancy test copied from `Debounce`'s.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched; fewer pending sources only).
- **Visual proof:** the minimap reflow and sidebar transition settles are visual paths, so `make visual-geometry-smoke` runs once on the final tree.
