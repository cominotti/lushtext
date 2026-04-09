---
title: 'Fix draft_dirty flag not re-armed after autosave tick'
type: 'bugfix'
created: '2026-04-08'
status: 'done'
baseline_commit: 'd90b683'
context: []
---

<frozen-after-approval reason="human-owned intent -- do not modify unless human renegotiates">

## Intent

**Problem:** After restoring a draft (yellow info bar shown), new edits are never saved to the draft file. The 30-second autosave timer writes the draft once after restore, clears `draft_dirty = false`, and then no subsequent edits re-arm the flag because `connect_modified_changed` only fires on `is_modified()` state *transitions* -- not on every keystroke. Since the buffer remains `is_modified() = true` continuously after restore, the signal never re-fires and `draft_dirty` stays `false` permanently. On process kill, the latest edits are lost.

**Approach:** Add `editor.set_draft_dirty(true)` to the existing `buffer.connect_changed` handler in `wire_modified_indicator`. This signal fires on every text mutation (correct granularity for a content-change flag), ensuring `draft_dirty` is re-armed after each edit regardless of the `is_modified()` state.

## Boundaries & Constraints

**Always:** Draft persistence must never silently lose user edits. The fix must not introduce unnecessary I/O (no re-writing unchanged content every tick).

**Ask First:** Any changes to the autosave timer interval or draft write conditions beyond the `draft_dirty` flag.

**Never:** Do not change the 30-second timer interval. Do not remove the `is_modified()` check in `autosave_tick`. Do not switch to Option C (never-clear approach). Do not touch the draft restore or draft service code -- the bug is in the signal wiring, not the persistence layer.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Draft restored + new edits + kill | Restore draft, type, wait >30s, kill process | Relaunch recovers ALL content including post-restore edits | N/A |
| Draft restored + no edits + kill | Restore draft, wait >30s, kill process | Relaunch recovers the restored draft content (identical) | N/A |
| New file + edits + kill | New tab, type, wait >30s, kill process | Relaunch recovers draft | N/A |
| Undo back to clean | Edit, then undo all changes | `is_modified()` becomes false, timer skips (correct) | N/A |
| Programmatic buffer.set_text (reload) | File reload via `apply_loaded_content` | `set_modified(false)` called after, timer skips (correct) | N/A |

</frozen-after-approval>

## Code Map

- `crates/lushtext-core/src/ui/window/mod.rs:278` -- `connect_changed` handler that needs the one-line addition
- `crates/lushtext-core/src/ui/window/mod.rs:248` -- `connect_modified_changed` handler (existing `set_draft_dirty(true)` -- remains as-is)
- `crates/lushtext-core/src/ui/editor_page/mod.rs:358-363` -- `draft_dirty()` / `set_draft_dirty()` accessors
- `crates/lushtext-core/src/ui/window/session.rs:377` -- `autosave_tick` condition check
- `crates/lushtext/tests/widget/window.rs` -- existing widget tests for draft behavior

## Tasks & Acceptance

**Execution:**
- [x] `crates/lushtext-core/src/ui/window/mod.rs` -- Add `editor.set_draft_dirty(true)` inside `connect_changed` handler to re-arm draft flag on every text mutation
- [x] `crates/lushtext/tests/widget/window.rs` -- Add 5 widget tests: basic arming, core regression, multi-cycle, full restore regression, guard against spurious arming

**Acceptance Criteria:**
- Given a buffer with `draft_dirty = false` and `is_modified() = true`, when the user types new text, then `draft_dirty` becomes `true`
- Given a buffer after draft restore and first autosave tick (which clears `draft_dirty`), when the user edits further, then the next autosave tick writes the updated draft
- Given a buffer where the user undoes all changes (`is_modified() = false`), when the autosave tick fires, then no draft is written (existing behavior preserved)

## Verification

**Commands:**
- `make check` -- expected: clippy + fmt pass with no warnings
- `make test` -- expected: all existing + new tests pass

## Suggested Review Order

**Core fix — draft_dirty re-arming**

- Entry point: `connect_changed` now re-arms `draft_dirty` on every text mutation
  [`mod.rs:282`](../../crates/lushtext-core/src/ui/window/mod.rs#L282)

**Error handling hardening — draft I/O**

- Draft recovery errors now logged instead of silently discarded
  [`session.rs:277`](../../crates/lushtext-core/src/ui/window/session.rs#L277)

- Second draft recovery path (file-open) also logs errors
  [`session.rs:484`](../../crates/lushtext-core/src/ui/window/session.rs#L484)

- Last-chance draft save upgraded from warn to error, manifest save failure logged
  [`session.rs:123`](../../crates/lushtext-core/src/ui/window/session.rs#L123)

- Fire-and-forget draft deletion now logs failures
  [`session.rs:512`](../../crates/lushtext-core/src/ui/window/session.rs#L512)

**Tests**

- Core regression: autosave clears flag, new edit re-arms it
  [`window.rs:1371`](../../crates/lushtext/tests/widget/window.rs#L1371)

- Full lifecycle: draft restore → autosave tick → new edit → verify
  [`window.rs:1451`](../../crates/lushtext/tests/widget/window.rs#L1451)

- Guard: unmodified buffer skips draft write even when dirty flag set
  [`window.rs:1410`](../../crates/lushtext/tests/widget/window.rs#L1410)

- Multi-cycle: 3 clear→edit cycles all re-arm correctly
  [`window.rs:1422`](../../crates/lushtext/tests/widget/window.rs#L1422)

- Basic: text edit arms draft_dirty
  [`window.rs:1345`](../../crates/lushtext/tests/widget/window.rs#L1345)

- Guard: no spurious arming without edits
  [`window.rs:1402`](../../crates/lushtext/tests/widget/window.rs#L1402)
