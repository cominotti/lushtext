## Why

Large-buffer snapshots currently carry `GtkTextIter` values across main-loop turns even though character-count-changing edits invalidate outstanding iterators. Post-capture generation checks reject stale results but cannot make an invalid iterator safe, and raw periodic-history timeouts remain registered after being superseded.

## What Changes

- Replace cross-turn iterator ownership with an explicit snapshot session that uses GTK-stable positions, owns its signal/timer lifecycle, and removes every temporary resource on success, cancellation, overflow, or disposal.
- Make edit cancellation a first-class snapshot outcome so draft autosave, encoding analysis, note preview, local-history restore, periodic history, and save can each apply an explicit retry/discard/freeze policy.
- Preserve the direct small-buffer path and bounded chunked path without allowing partial or mixed-generation text to reach persistence or analysis.
- Replace untracked periodic local-history timeouts with one tab-owned superseding timer.
- Add widget regressions for edits, deletion, reload, close, cancellation, overflow, and consumer-specific retry behavior during chunked capture.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `main-thread-responsiveness`: Require chunked buffer snapshots to remain toolkit-valid and mutation-safe across yielded main-loop turns.
- `draft-session-recovery`: Require draft capture to cancel and retry when the source buffer changes during chunked snapshotting.
- `local-history`: Require snapshot cancellation resources and periodic timers to be superseded and released deterministically.

## Impact

- Affects `ui/buffer_snapshot.rs` and its consumers in editor save/load, draft autosave, encoding, local history, and note preview workflows.
- Uses GTK `TextMark`/signal lifecycle rules within the UI adapter; it does not move GTK objects into services or create a new GTK Lush API.
- May reshape private snapshot callbacks around a typed outcome, with focused widget coverage for every consumer.
- This is the portfolio foundation and should be implemented before `harden-editor-recovery-ordering` so draft ordering builds on the final snapshot outcome contract.
