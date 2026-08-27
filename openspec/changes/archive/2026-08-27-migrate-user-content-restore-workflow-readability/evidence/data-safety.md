# Data-safety passes and the two routed candidates (tasks 0.6, 7.1, 7.2, 7.3)

## Task 7.1 — Candidate 1: `installation_incomplete` versus the draft-autosave lane

**Verdict: CONFIRMED DEFECT.** Recovery-data loss. Fixed in this change per
`.agents/rules/preexisting-blockers.md`.

### The missing evidence 3b named: the `draft_dirty` transition trace

`draft_dirty` is written from exactly four places
(`crates/lushtext-core/src/ui/editor_page/mod.rs:502` is the only setter):

| Writer | Condition |
| --- | --- |
| `ui/window/documents.rs:554` (`connect_modified_changed`) | `set_draft_dirty(true)` when the buffer becomes modified — **returns early if `editor.load_projection_suspended()`** |
| `ui/window/documents.rs:585` (`connect_changed`) | `set_draft_dirty(true)` on any buffer change — **same early return** |
| `ui/window/dialogs.rs:594`, `:852` | `set_draft_dirty(true)` on discard/restore flows |
| `ui/window/drafts.rs:1764` | `set_draft_dirty(false)` only after a matching-generation manifest commit |

`set_draft_dirty(true)` also advances `draft.dirty_generation`.

The autosave admission guard is `ui/window/drafts.rs:1267`:

```rust
if !editor.is_modified() || !editor.draft_dirty() || editor.is_evicted() { continue; }
```

`installation_incomplete` appears nowhere in `drafts.rs`.

### The trace, end to end

1. The editor holds unsaved edits: `is_modified() == true`,
   `draft_dirty() == true`, and a draft body exists on disk holding that work.
2. A load starts on the same tab (reopen-with-encoding, external-change reload,
   or a session-restore reopen). `begin_load_request` suspends projection, so the
   installation's own buffer mutations do **not** touch `draft_dirty`.
3. The load is cancelled mid-installation.
   `load/retirement.rs:171` sets `installation_incomplete = true`, then the
   bounded cancelled-clear runs and, at `retirement.rs:200`, calls
   `buffer.set_modified(false)` — and `restore_load_installation_state` restores
   `projection_suspended` to its pre-install value and makes the view editable
   again. **`draft_dirty` is never cleared on this path.** The buffer is now
   empty; the draft on disk still holds the step-1 work.
4. Autosave now skips, but **only because `is_modified()` is false**.
5. **One keystroke.** `connect_changed` fires, projection is no longer suspended,
   so `set_draft_dirty(true)` runs and the buffer is modified.
6. The next autosave tick — or the first-dirty 750 ms debounce that step 5
   schedules directly — collects this editor as a candidate. `is_modified()`,
   `draft_dirty()`, `!is_evicted()` all hold; `installation_incomplete` is still
   `true` and is not consulted. The pipeline snapshots the near-empty buffer and
   `draft_service::write_draft` **replaces the draft body**, destroying the
   step-1 recovery record.

The document file on disk is untouched, so this is not lost *file* content — it
is the destruction of the recovery record that exists precisely to protect
unsaved work across a crash. For this workflow family that is the same class of
failure.

### Why the save lane is not exposed

Slot 3a's save path refuses on this flag at two points —
`ui/editor_page/save/admission.rs:117` and `save/execution.rs:162` — returning
`EditorSaveError::IncompleteLoadInstallation`, with the comment "the buffer at
this \[point\] ... installs the buffer in slices and sets
`installation_incomplete`". The autosave lane needs the same guard for the same
reason.

### The fix

Add `installation_incomplete` to the draft-candidate admission guards and to the
post-snapshot freshness rechecks, in all **five** places the draft workflow decides
a buffer may be published (three admission guards and two post-snapshot rechecks;
an earlier draft of this document said "four" while listing five):

1. `collect_dirty_draft_candidates` — the autosave admission guard.
2. `collect_close_draft_candidates` — the async close-flush admission guard.
3. `flush_dirty_drafts` — the synchronous close-time guard.
4. the two post-snapshot rechecks in `drive_dirty_draft_pipeline` and
   `drive_close_draft_pipeline`, because chunked capture spans main-loop turns and
   a load can be cancelled *during* the capture.

The candidate is **skipped**, not deferred with `autosave_pending`: a pending
re-arm would spin while the installation stays incomplete, and the existing draft
on disk is already the best available recovery record. A successful retry install
clears the flag (`load/execution.rs:545`), and any later edit re-schedules the
first-dirty autosave through the ordinary path.

Skipping is also correct for close: preserving a near-empty buffer over a valid
draft is strictly worse than leaving the valid draft in place, and the close path
must not report it as a write error.

### Regression coverage, and proof it pins the defect

`window::test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft`
(widget, `crates/lushtext/tests/widget/window.rs`). It drives the real workflows:
open a file, seed a draft holding `"important unsaved work\n"` through the
ordinary first-dirty autosave, start a chunked load on the same tab, `cancel_load()`,
wait for the bounded cancelled-clear terminal (projection restored, installation
no longer active), type one character, then `autosave_tick_for_test()`.

It asserts the guard's **precondition** — `is_modified()`, `draft_dirty()`,
`!is_evicted()`, and `installation_incomplete` all hold, so the tab looks like an
ordinary dirty candidate in every respect except the new flag — and the
**safety property**, that the persisted draft body is byte-identical afterwards.

Proven to fail without the fix. With the `installation_incomplete` guard removed
from `collect_dirty_draft_candidates` and the post-snapshot recheck, the run
reports:

```
assertion `left == right` failed: autosave must not write a partially installed buffer over a good draft
  left: Some("x")
 right: Some("important unsaved work\n")
```

The draft body is replaced by the single keystroke: the entire preserved unsaved
work is destroyed. Restoring the guard makes the test pass. That is the defect,
demonstrated rather than argued.

One detail the first attempt got wrong, recorded because it is the same shape a
future test will hit: the keystroke must come **after** the cancelled-clear
terminal. Until then `load_projection_suspended()` is true, so
`connect_changed` returns early and the edit cannot mark the tab draft-dirty at
all — an earlier keystroke fails the precondition rather than exercising the
guard.

Deliberately un-automated: the *real* interleaving of a compositor-timed
cancelled chunked install with a 750 ms first-dirty debounce. The test drives
`installation_incomplete` through the load workflow's own cancellation path and
then `autosave_tick_for_test`, which exercises the same guard on the same state
without depending on frame-clock progress — the widget harness cannot advance
`AdwTimedAnimation` frame clocks, and a real-time race would be a flaky test
rather than a stronger proof.

## Task 7.2 — Candidate 2: the planning completion's dead-editor early return

**Verdict: UNREACHABLE, with a proof rather than an assumption.** No behavior
change; the invariant is written down where it lives.

### The site

`ui/editor_page/load/admission.rs:231`, inside the planning completion:

```rust
move |editor_weak, result| {
    let Some(editor) = editor_weak.upgrade() else {
        return;                                  // <- 3b's candidate
    };
    if planning_ticket.is_current(&editor) { ... } else { refuse_stale_completion(&editor); }
    finish_load_planning(&editor);
}
```

The stored terminal is `editor.imp().load.planning_terminal_callback`, written by
`begin_load_request` at `admission.rs:211`.

### The proof, two independent reasons

1. **`finish_load_planning` is unconditional on the live-editor branch.** It sits
   *after* the `if/else`, so a stale ticket releases the terminal too. The only
   path that skips it is the dead-editor arm.
2. **The dead-editor arm cannot be reached while a terminal is still stored.**
   `WeakRef::upgrade()` fails only once the object is finalized. GObject
   guarantees `dispose()` runs strictly before `finalize()`, and
   `ui/editor_page/imp.rs:624` calls `self.obj().dispose_load_resources()`
   unconditionally in `dispose()`. That reaches
   `retirement::dispose_load_resources` →
   `cancel_noninstall_load_resources` → `admission::finish_load_planning`, which
   `take()`s and **calls** the stored callback, and
   `admission::discard_pending_request`, which calls `finish_planning()` on any
   parked request. So by the time an upgrade can fail, the callback slot is
   already empty and its owner already released.

An editor that is never finalized (held by a leaked reference) never reaches the
arm either, because `upgrade()` succeeds and reason 1 applies.

### Worst case, stated in full because 3b's framing understated it

A dropped terminal would stall the **session-restore sequencer**: the descriptor's
`SessionRestorePlanPermit` would never be released, so `release_permit` would
never count it, so the next document would never open. It could **never** cause
over-admission and never lose content — over-admission is exactly the property
`release_permit` exists to protect, and a *missing* release can only
under-admit. That asymmetry is why the unreachable path is a stall risk rather
than a safety risk.

### Action taken

The invariant is recorded at the site rather than defended with a redundant
release: the dead-editor arm has no editor from which to read the callback, so
there is nothing to release there. A comment at `admission.rs:231` names both
reasons so a future edit that moves `finish_load_planning` inside the `if` is
recognisably a regression.

## Task 0.6 — pre-implementation `data-safety` pass over the intended diff

Domains checked against the intended diff, before implementing:

| Domain | Finding |
| --- | --- |
| draft persistence | **1 confirmed defect** (task 7.1 above) |
| save/close flow gaps | none new; the close-flush `autosave_pending = false` ordering and the "manifest is the durable retry marker until the body is gone" ordering are recorded in `durability-contracts.md` as must-preserve |
| replace-all backup safety | out of scope (`WFR-SEARCH-REPLACE`, migrated in 2b); `LocalHistoryUndo` is local history's own undo and is not Replace All undo |
| session restore | the `release_permit` exactly-once contract 3b handed over is intact; task 7.2's candidate is unreachable |
| async concurrency | the four `Ticket`/`Facts` seams (`DraftRestoreTicket`, `BaselineCaptureTicket`, `PeriodicCaptureTicket`, `LocalHistoryReplacementTicket`) each validate identity **and** generation before publishing; `DraftMutationOrder` uses epoch **equality**, not ordering, so wraparound stays correct |

## Task 7.3 — post-diff pass

Run against the final diff, after every code edit. The substance lives in
`durability-contracts.md`; this is the verdict and where to read it.

**One deliberate behavior change, and it is the fix.** The
`installation_incomplete` guard at five decision points, whose exact extent is listed
in `durability-contracts.md` under "The one deliberate behavior change, and its
exact extent". Its effect is strictly *more* refusal, so it cannot weaken any
durable ordering; its regression test fails without it.

**No other behavior change.** The four orderings the draft and session workflows
must preserve were each re-checked against the final diff and are intact —
`durability-contracts.md:123-200` records them with their call sites:

| Ordering | Verdict on the final diff |
| --- | --- |
| The persisted manifest stays the durable retry marker until the body is gone | preserved; delete still writes the body removal before the manifest entry |
| Tombstone re-application in `accept_draft_manifest_commit` | preserved verbatim |
| The deliberate `autosave_pending = false` in the close commit | preserved verbatim, with its reason now stated in the facade's stage 5 |
| The orphan-cleanup inode / manifest-reload / `TargetWriteGuard` / recheck contract | **compliant by non-edit** — `services/draft_service/` was not modified, so the contract is the same code that shipped |

**Two behavior-preservation slips were made during the migration and both were
caught before sign-off**, which is the honest reason to trust this row rather than
the claim that no slips occurred: a metrics-ordering change in `run_install_turn`
(restored to the exact pre-migration order) and a latent `BorrowMutError` from a
`match` scrutinee holding a `Ref` across the arm that takes `borrow_mut()`. Both
are written up in `durability-contracts.md` under "Two behavior-preservation slips
made during the migration, both caught", including the generalisable rule the
second one produced.

**Post-review additions.** The independent review found five deleted unit tests
with no surviving equivalent, two of which were the only coverage of
`services/draft_service/cleanup_types.rs::merge_committed_orphan_removals` — the
function that decides which manifest entries a destructive cleanup pass removes,
in a file that had dropped to **zero** tests inside the mutation scope. All five
were restored to homes matching the new structure and strengthened; see
`test-counts.md`. That was a real coverage regression on a destructive path, and
it is the one finding in this row that the change's own diff-scoped mutation runs
could not have surfaced, because the file was never in their diff.
