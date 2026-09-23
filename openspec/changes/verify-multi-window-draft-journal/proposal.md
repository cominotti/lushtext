## Why

The K3 Kani journal machine proves S1–S4 and L1 for **one window of one
process**. Inside one LushText process, though, the manifest write lock
(`draft_service::manifest_write_lock`) and the `TargetWriteGuard` table are
process-wide, while everything that serializes the journal from the GTK side is
**per window**: `mutation_inflight`, `autosave_inflight`,
`orphan_cleanup_inflight`, the tombstones, the restore holds, the manifest copy
and its authority, and the startup data flow (`continue_startup_data_flow`
restores the session and schedules orphan cleanup once per window). Production
creates a second `LushtextWindow` only when none is active (`app.rs`), but the
widget suite already builds several windows in one process
(`test_transient_load_budget_is_shared_across_windows_and_honors_protected_residency`),
and a future "New Window" action would make the case reachable for users.
Reading the code suggests at least two in-process losses the single-window
model cannot see:

- window B's orphan cleanup, which is not gated by window A's
  `mutation_inflight`, could remove the write-ahead entry A registered before
  A's body write (a missing-body entry), or delete A's untitled body before A's
  manifest commit;
- a second window runs its own startup restore of the same `session.json`, so
  the same draft id can be restored and edited in two windows, which is the K8
  two-process trace reproduced inside one process.

These are hypotheses. This change (step 5a of the formal-verification
programme, candidate N4 "multi-window" in `docs/next/formal-verification-next.md`)
lets Kani decide them, and then fixes each counterexample it finds,
failing-first.

## What Changes

- Extend the K3 harness in `services/draft_service/kani_proofs.rs` with a
  **second in-process window actor**. The two windows share the model disk, the
  process-wide manifest lock, and the process-local target guard. Each window
  has its own editors, manifest copy, authority, restore holds, tombstones, and
  in-flight flags. A new harness,
  `journal_invariants_hold_across_two_windows`, checks S1–S4 after every step.
  The bounds are stated in the harness and in the programme record.
- Record every counterexample Kani finds, decoded by concrete playback, in
  `docs/next/formal-verification.md`.
- For each counterexample, write a **failing-first multi-window widget test**
  that drives two `LushtextWindow`s of one `GApplication` over one isolated data
  directory. Then fix the cause. The expected shape of the fix is a
  process-wide **draft journal coordinator** that owns what must be
  process-wide: the mutation and cleanup lane, draft-id ownership across
  windows, and process-once startup restore and orphan-cleanup scheduling. The
  final shape follows from the counterexamples, not from this list.
- Once fixed, the two-window harness proves S1–S4, and the single-window
  harnesses and L1 (k = 7) stay proved. If a counterexample is decided as not
  fixed, it is kept as a `should_panic` harness with reachability evidence,
  following the K4 residual precedent. That needs a recorded decision in the
  design.
- **Unchanged:** the K8 two-process harness
  `a_second_writer_breaks_the_journal_invariants` stays a `should_panic` pin
  for axiom A6. The model is refactored so that a *window* actor and a
  *process* actor are distinct. K8 still runs two processes with separate
  locks, guards, and coordinators, and still fails.
- Update `scripts/kani-shards.py` so the new harness lands in exactly one
  shard, plus the programme record, the next-candidates list, the deferral
  inventory, the `WFR-DRAFT-RECOVERY` matrix row, and the AGENTS.md /
  `ui/window/AGENTS.md` draft-persistence guidance.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `draft-session-recovery`: ADDED requirement "Draft journal invariants hold
  across windows of one process". Journal serialization, cleanup gating,
  draft-id ownership, and startup restore/cleanup scheduling are process-wide,
  S1–S4 are Kani-checked with two in-process windows, and multi-window widget
  tests cover every counterexample found.
- `formal-verification-kani`: ADDED requirement "Journal harnesses distinguish
  window actors from process actors". The multi-window harness shares
  process-wide state, and the K8 two-process pin keeps it separate.

## Impact

- **Code:** `crates/lushtext-core/src/services/draft_service/kani_proofs.rs`
  (actor refactor and new harness), possibly
  `services/draft_service/journal_core.rs` (a new pure decision, only if a fix
  needs one), `crates/lushtext-core/src/ui/window/drafts/` (`journal.rs`,
  `cleanup_journal.rs`, `autosave_execution.rs`, `evidence.rs`, and a new
  process-wide coordinator home), `ui/window/startup_data.rs`, and
  `ui/window/session_restore/` (process-once restore). Tests go in
  `crates/lushtext/tests/widget/` (a new multi-window drafts test module).
- **Behaviour:** no single-window behaviour change. In a second window of the
  same process, startup restore, orphan cleanup, and journal mutations
  serialize with the first window instead of racing it.
- **Verification:** `make kani` gains one harness (with shard table and
  `make check-kani-shards` updates). `make test-widget` gains multi-window
  tests. `make check-workflow-boundaries` must stay green after the new module
  is declared in the matrix row.
- **Docs:** `docs/next/formal-verification.md` (phase 4, deferral inventory),
  `docs/next/formal-verification-next.md` (N4), `docs/workflow-readability-matrix.md`
  (`WFR-DRAFT-RECOVERY`), AGENTS.md and `crates/lushtext-core/src/ui/window/AGENTS.md`,
  and the `ui/window/drafts/mod.rs` narrative facade.
- **No** new dependency, persisted format, GSettings key, action, or automation
  snapshot field.
