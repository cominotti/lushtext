# Proposal

## Why

Two small defects in the GTK Lush family, measured on `origin/main` `761866ff` (task 0.1 re-measures):

- **`SettleBurst::schedule` leaves every superseded GLib source installed** (`crates/gtk-lush/settle/src/lib.rs:281-299`).
  - Each call adds a `timeout_add_local_once` holding a weak reference to its target, and the source stays until its deadline even though a newer schedule made it stale.
  - `Debounce` had the same shape until 131,072 schedules in one dispatch passed GLib's per-object weak-reference limit (`Too many GWeakRef registered`, `CHANGELOG.md`). It now removes the superseded source at once and is pinned by `debounce_keeps_one_pending_source_across_a_burst` (`lib.rs:475`).
  - `SettleBurst` is used by the minimap reflow settle (`ui/editor_page/minimap/reflow_execution.rs:71`) and the workspace-sidebar transition settle (`ui/window/geometry/mod.rs:215`), both scheduled from allocation-driven notifications.
- **Two crates take a refcount just to take a weak reference.** `gtk-lush-signals/src/lib.rs:73` and `gtk-lush-viewport/src/lib.rs:271` call `source.clone().upcast()` before `downgrade()`, where `upcast_ref::<Object>()` borrows, as `gtk-lush-widgets/src/render_hold.rs:73` already does.

## What Changes

- **One pending source.** `SettleBurst::schedule` removes the superseded source before installing the new one, with the same reentrancy order as `Debounce` (advance first, then remove). `clear` removes the pending source too. The pending flag and the handle semantics are unchanged.
- **`upcast_ref`** replaces `clone().upcast()` at the two sites.

## Non-goals

- **`SupersedingTimer::is_armed()`.** It ships with the change that migrates the draft evidence flags in `ui/window/imp.rs` (its first consumer), per the family's friction-driven API review.
- **Unifying `Debounce` and `SupersedingTimer` internals.** There is no observable gain.

## User-visible impact

None. Allocation-driven settles stop accumulating stale GLib sources. Nothing is lost compared with `main`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-lush-settle`: a settle burst keeps at most one pending GLib source however often it is scheduled.

## Impact

- **Code:** `crates/gtk-lush/settle/src/lib.rs` and its `CHANGELOG.md`; `crates/gtk-lush/signals/src/lib.rs`; `crates/gtk-lush/viewport/src/lib.rs`.
- **Governance:** a behaviour fix in governed crates, with no public API change. It goes through the `gtk-lush-stewardship` skill and `crates/gtk-lush/GOVERNANCE.md`.
- **Order:** the Start line in `tasks.md`.
