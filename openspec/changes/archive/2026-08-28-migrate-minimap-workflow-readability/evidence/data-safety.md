# Data-safety pass — slot 6 (`WFR-MINIMAP`)

Explicit-mode pass run **before** any code moved, per task 0.9. Five consecutive
slots each found at least one confirmed finding, so the pass budgeted for
findings rather than hoping for none. It found **two**, and both are fixed in
this change per `.agents/rules/preexisting-blockers.md`, which has no exceptions.

The three candidates the proposal named are each given a verdict below, including
the two that came back clean, because a negative finding with its evidence is a
result and an unexamined candidate is not.

## Candidate 1 — the sliced live-GTK-buffer cursor — NO FINDING

**Whole-buffer copy: none.** The only text-consuming helpers taking a `&str` are
both `#[cfg(test)]`. Production analysis walks a `gtk4::TextIter` one scalar at a
time, bounded by `MINIMAP_ANALYSIS_CHARS_PER_SLICE` (32 KiB), handing each `char`
to the GTK-free accumulator. There is no `buffer.text()` and no document-sized
`String`, so no `ui::buffer_snapshot` obligation arises.

**The O(1) live-byte rule holds.** `wrapped_layout_analysis_required` reads the
wrap mode and `estimated_live_buffer_bytes()`, which is `buffer().char_count()` —
O(1) scalar metadata. Nothing scans or copies text to classify eligibility, which
is what the exact-2-MiB contract requires.

**No cursor `TextMark` leak on any exit path.** The mark is created once and
deleted at every terminal: buffer-identity mismatch, the post-slice staleness
recheck, completion, and cancellation. The one exit that does not delete it — the
entry guard — is reachable only when a generation or lifetime already changed,
and both counters advance **only** through cancellation, which takes the session
and deletes its mark first. The mark is anchored on `session.buffer`, so even the
buffer-swap branch deletes it from the correct buffer.

**Generation and lifetime are checked at every resumption and before every
publication**: on entry, on session-identity revalidation including buffer
identity, after the slice (which covers the reentrancy window `move_mark` opens
by emitting `mark-set`), and immediately before the cache is published.

**Cancellation leaks neither the source id nor the mark.** The `SourceId` is taken
before `remove()`, so `remove()` is never called on an already-dispatched source.

**Main-thread blocking: no finding.** 32 K `forward_char()` calls per idle turn is
a bounded turn that yields between slices. One scale note, not a defect: every
buffer `changed` discards the cache, so on a wrapping >2 MiB document a typing
user restarts the scan on each keystroke. It never converges while typing, but it
is idle-priority, cancellable, and cannot lose content.

## Candidate 2 — the modified-line mark operations — NO FINDING

Callers traced complete: the accepted-save publish (after the durable write
succeeded), the load publish stage, `finish_local_history_buffer_replacement`
(after `set_modified(true)`), the eviction terminal, and draft restore's
`mark_entire_buffer_modified` (after `set_modified(true)`).

**Cannot perturb or lose user content.** `clear_modified_line_marks` removes
source marks scoped to the `"lushtext-minimap-modified"` category, disjoint from
the only other category in the tree (`"lushtext-bookmark"`). Minimap marks are
created unnamed, so they cannot collide with bookmark marks, which are named by
bookmark id. Source marks carry no text and no minimap path inserts, deletes, or
replaces buffer content.

**Cannot mis-mark clean as dirty or dirty as clean.** The minimap never calls
`set_modified`; it only reads `is_modified()`. Ordering at every call site is
correct — the owning workflow sets the flag before the marker call.

**No `RefCell` hazard.** `record_modified_lines` holds two borrows across
`create_source_mark`, which emits `mark-set`; the only `mark-set` handlers in the
tree are Focus Mode's (filtered to the `"insert"` mark) and the notes menu
refresh, neither of which touches minimap state.

One scale note, not a data-loss finding: draft restore of a file-backed document
can create up to 2,000 source marks in one main-loop turn, each synchronously
re-entering the notes menu-state refresh. Bounded and one-shot, but worth a
`gtk-perf-review` follow-up rather than a data-safety fix. Handed on.

## Candidate 3 — `set_minimap_tracking_suspended` pairing — CONFIRMED FINDING

Two suspenders exist and both capture with `replace(true)` and restore with
`set_minimap_tracking_suspended(saved)`.

**Buffer replacement is balanced.** Every terminal funnels through
`finish_session`, which takes the guard exactly once and restores unless the
terminal is `Disposed` — an intentional, documented exclusion for a widget that is
going away. The supersede path is correct too: the old session is finished before
the new one captures, so `begin_guard` never captures an already-suspended value.

**Load has three non-restoring exits, and one of them leaves a live editor.**
`load/execution.rs`'s `finish_chunked_install` returns without calling
`restore_load_installation_state` on the disposed-during-finalization path
(acceptable — the editor is going away), on the editor-gone path (acceptable), and
on the **superseded-generation** path, which does not:

```rust
if editor.imp().load.dispose_during_finalization.get()
    || editor.imp().load_tracking.generation.get() != generation
{
    buffer.end_irreversible_action();
    conclude_installation(&editor, session, permit);   // no restore
    drop(loaded);
    return;
}
```

`dispose_during_finalization` was set `false` eleven lines earlier, so in practice
this is the superseded-generation branch. The `loaded == None` arm skips the
restore **and** `buffer.end_irreversible_action()`, leaving undo disabled.

**Why this is worse than a cosmetic minimap bug.** `LoadInstallationState` bundles
four flags. Skipping the restore leaves the editor `set_editable(false)`,
`cursor_visible(false)`, `projection_suspended = true`,
`history_capture_suppressed = true`, and `tracking_suspended = true`. A
superseding load B then captures those already-suspended values as its own
"previous" state and **faithfully restores them to `true` when it finishes** — so
the tab stays permanently read-only, with automatic local-history capture (a
recovery mechanism) silently disabled, for the rest of the session. This is the
capture-of-an-already-suspended-value shape the seam rules exist to prevent.

**Reachability.** The branch is defensive. `finish_chunked_install` runs in the
same main-loop turn as the freshness check in `run_install_slice`, and no handler
in this tree was found that bumps `load_tracking.generation` from there. Reported
as a **latent defect with no demonstrated trigger**, not as a routinely-hit bug.
The contrast with the cancellation path, which *does* restore, is what makes it
read as an oversight rather than a decision.

**Fix landed, and it is untested.** The restore now runs for a **live** editor on
both arms, and the payload-less arm gained the missing `end_irreversible_action()`.
The disposed-during-finalization disjunct is deliberately excluded from the
restore: restoration reaches `source_view()` and `refresh_minimap()`, both of
which read panicking `TemplateChild` accessors that GTK4 has already cleared, so
restoring state to a widget that is going away could only turn a teardown into a
crash. HEAD restored nothing on either disjunct, so this change narrows the arm it
fixes rather than widening behavior on the disposal path.

**No regression test accompanies it**, and that is a recorded gap rather than an
oversight. Both disjuncts are defensive: no handler in this tree was found that
advances `load_tracking.generation` from inside the same main-loop turn as
`finish_chunked_install`, and reaching either arm from a test would require a new
test-only actuation seam to force the counter mid-finalization. This change's
actuation-seam budget is deliberately **zero** and slot 5b's spare remains
unspent, so the seam was not spent on a latent defect with no demonstrated
trigger. The correctness argument here is a reading of the code, not a passing
assertion, and a later change that finds a real trigger should say so and add the
seam then.

**Note for slot 7.** The structurally stronger fix — giving
`LoadInstallationState` a scope-owned restore, as `with_construction_charge`
does in `services/palette/notes.rs`, so no future exit can drop it — is **not**
done here. This change is a minimap migration and reshaping the load workflow's
ownership model is out of its scope. Handed on with the owning row named.

## Additional finding — the minimap's timers were not cancelled in `dispose()`

**CONFIRMED. This row's own, and it is fixed here.** `dispose()` invalidated the
local-history periodic timer, cleared five `SignalBag`s, and took the source map,
marker strip, and render hold — but never invalidated
`minimap.refresh_debounce` or cleared `minimap.reflow_settle`. Both callbacks
reach panicking `TemplateChild` accessors: `refresh_minimap` reads
`minimap_overlay`, and the settled repair reads `source_view()`.

Both primitives capture the page weakly, so in production the upgrade should fail
and the timer no-op; no production path leaving a strong reference to a disposed
page was found. The reachable stage today is a `run_dispose`-style teardown
observation with a timer armed — which the repo already treats as a legitimate
stage, and which this change's own evidence surface must answer honestly at.

Fixed in `ui/editor_page/imp.rs::dispose`, which now invalidates both and clears
the two pending flags. **The regression test is
`editor_page::test_minimap_evidence_reads_stay_honest_after_dispose`**, which
disposes the page, reads the evidence surface, and then flushes past both the
80 ms debounce and the 150 ms settle window asserting the surface is unchanged.

## Unresolved candidate, handed on

`minimap_work_pending` takes the analysis session *out* of its `RefCell` for the
duration of a slice, so during that window queued analysis is invisible to
readiness. Because GTK is single-threaded the window is only observable from a
signal emitted inside the slice (`move_mark` → `mark-set`), and no readiness query
was found on that path. Not classifiable as safe or as a finding from the code
alone; the evidence needed is whether any Automation1 readiness or
visual-geometry snapshot can be taken from a `mark-set` handler.

## Clean domains

- **draft-integrity** — no draft I/O, manifest, or dirty flag in scope.
- **atomic-write** — no filesystem writes, no `thread::spawn`, no
  `spawn_blocking_then` anywhere in scope.
- **replace-safety** — no undo-backup or replacement state in scope.
- **restore-lifecycle** — clean for the minimap's participation;
  `mark_entire_buffer_modified` runs after `set_modified(true)` and gates nothing.

## Counting rule

Two defects are counted for this slot. The load-workflow finding is counted here
once, and is recorded against `WFR-DOCUMENT-LOAD` as the owning row in the
programme record's slot 6 baseline — not counted twice.
