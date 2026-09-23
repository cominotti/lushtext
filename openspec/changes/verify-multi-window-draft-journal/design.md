## Context

The K3 draft-journal model (`crates/lushtext-core/src/services/draft_service/kani_proofs.rs`)
drives the production decision core `journal_core.rs` over a model disk with
3 file-backed ids, 3 edits, and 8 actions. It proves S1–S4 and L1 (k = 7). Its
recorded unmodelled assumption is "one process and one window per data
directory". K8 dropped the *process* half of that assumption: a second
process over the same disk (`step_as` swapping a parked `Window`) breaks S1
and "no body without an entry" within 8 and 6 steps. That result was accepted
with documentation, because a unique `GApplication` makes it need two D-Bus
sessions.

This change drops the *window* half. The ownership split inside one process
today:

| State | Scope today | Where |
| --- | --- | --- |
| manifest read-reconcile-write lock | process | `draft_service::manifest_write_lock()` (`OnceLock<Mutex<()>>`) |
| stable target write guard (body replace vs. cleanup delete) | process | `services/durable_write.rs::TargetWriteGuard` |
| leftover sweep | process-once | `startup_data.rs::sweep_app_data_leftovers_once` |
| `mutation_inflight`, `autosave_inflight`, `orphan_cleanup_inflight`, `autosave_pending` | **window** | `ui/window/drafts/*` via `imp().drafts` |
| manifest copy, `manifest_authority`, tombstones, `pending_deletes`, `restore_pending_ids`, `stale_preservations` | **window** | `imp().drafts` |
| startup restore (`load_session_and_drafts`) and `schedule_orphan_cleanup` | **once per window** | `startup_data.rs::continue_startup_data_flow`, `session_restore/admission.rs` |
| duplicate-path detection (`open_paths`) | **window** | `LushtextWindow` |
| session collected for reconciliation | **window** (`collect_session_for_draft_reconciliation`) | `session_restore/journal.rs` |

In a single window, `mutation_inflight` is what keeps orphan cleanup
(`run_orphan_cleanup_pass`) out of the registration → body write → commit
window, and out of a deletion's body-then-entry steps. A second window has its
own flag, so nothing orders its cleanup against the first window's pass. The
per-window startup flow also means a second window restores the same
`session.json` and adopts its own copy of the same drafts.

Production reaches a second window only when `app.active_window()` is `None`
while another window still exists, which does not happen today. It is
reachable in the widget suite (several tests build two windows over one
`GApplication`), and a "New Window" action would make it reachable for users.

## Goals / Non-Goals

**Goals:**

- Put the in-process multi-window case under Kani: two window actors sharing
  the disk, the manifest lock, the target guard, and any process-wide
  coordinator, each with its own per-window state. Prove S1–S4, or obtain
  concrete counterexamples.
- Fix every counterexample. Each fix comes after a multi-window widget test
  that failed first, and the harness then proves S1–S4.
- Keep the single-window harnesses and L1 (k = 7) proved, and keep single-window
  behaviour byte-for-byte the same.
- Keep K8 as the A6 pin, unchanged in meaning.

**Non-Goals:**

- An inter-process data-directory lock (N6); A6 stays accepted.
- A user-visible "New Window" action. This change makes the journal safe for
  one; it does not add one.
- Redesigning multi-window **session** persistence (each window's
  `collect_session` currently writes the whole `session.json`). It is in
  scope only as far as the journal depends on it; see D6.
- Slice-bin or any non-journal multi-window work.

## Decisions

### D1. Separate window actors from process actors in the model

Refactor `kani_proofs.rs` so that `Window` holds exactly the per-window
production state (editors, manifest copy / `known_entry`, `trusted`, restore
holds, tombstones, in-flight flags, and `startup_restored`). A new `Process`
holds one or two `Window`s plus the **coordinator state** (D3) that production
makes process-wide. `Journal` holds the disk and one or two `Process`es.

- The multi-window harness is `journal_invariants_hold_across_two_windows`:
  one process, two windows.
- K8 becomes two processes of one window each: separate coordinators, shared
  disk only.
- The existing single-window harness is one process of one window, with
  unchanged meaning.
- `Crash` is a **process** action and drops every window of that process. A
  new `CloseWindow` window action runs the modelled close flush and drops only
  that window's editors.

*Alternative considered:* reuse K8's `step_as` swap with a shared
coordinator. That was rejected: the swap makes "which state is shared" implicit,
which is how a model silently proves the wrong thing. Naming the two actor
kinds is what lets the spec state what each harness shares.

### D2. Model what production gates, before fixing anything

The single-window model never models `mutation_inflight`: `Inspect` and
`ExecCleanup` may fire at any step. That is sound only because the modelled
cleanup never removes missing-body entries and every id is file-backed. To
see the multi-window hazards, the model gains three things, and the
**single-window harness gets them too**:

1. **The mutation lane.** `Register` opens a pass, and `WriteBody` / `Commit`
   continue it. `DeletionStep` holds the lane for its step. `ExecCleanup` is
   admitted only when the lane admits it. In the baseline the lane is
   per-window, as production is today.
2. **Missing-body entry removal** in `ExecCleanup`, matching
   `plan.missing_body_entries` in `inspect_orphan_cleanup_from`. It is
   revalidated against the latest persisted manifest under the lock.
3. **One untitled id.** It needs no registration, and reconciliation
   reconstructs an entry for an untitled body without one. The model uses 2
   ids (one file-backed, one untitled) to pay for the second window; see D5.

Step 1 of the tasks runs the extended **single-window** harness first. If it
fails, that is a pre-existing single-window defect, and it is fixed in this
change before any multi-window work (pre-existing-blockers rule). Step 2 runs
the two-window harness against the **baseline** (per-window lane, per-window
startup restore) and records every counterexample by concrete playback.

### D3. The expected fix: one process-wide journal coordinator

Kani confirms or refutes the fix. The design expects it to be a process-wide
coordinator with three responsibilities:

- **One journal lane per process.** Registration, body-write/commit passes,
  deletions, and orphan-cleanup execution all acquire one process-wide lane
  instead of `imp().drafts.mutation_inflight` / `orphan_cleanup_inflight`. A
  window that finds the lane held marks itself pending (the existing
  mark-pending, never-queue admission) and is woken when the lane releases.
  The per-window flags stay as **projections** for `DraftEvidence` and the
  `draft-autosave` readiness blocker, so observers keep their meaning.
- **Draft-id ownership.** Each draft id is claimed by at most one window.
  Autosave, restore, and deletion of an id require the claim. The claim is
  released on page detach or window dispose.
- **Process-once startup.** The first window to reach
  `continue_startup_data_flow` performs session/draft restore and owns
  orphan-cleanup scheduling for the process. Later windows start empty of
  restored tabs, as a "new window" would. Orphan cleanup is scheduled once,
  gated on the process lane, and inspects against the **latest persisted
  manifest under the lock** rather than one window's copy.

**Pure half, Kani-checked:** the admission and ownership rules (`lane_admits`,
`claim_draft_id`, `startup_restore_claim`, or whatever the counterexamples show
is needed) are added to `journal_core.rs` as `const fn`s over small `Copy`
facts. The model calls them, as it calls every other decision.

**GTK half:** the coordinator's state is GTK-thread-only (window weak refs
for wakeups). It lives in the draft workflow's role home as coordination. Its
file is chosen with the `lushtext-workflow` skill: the default is to extend
`ui/window/drafts/admission.rs`, because admitting a window to the journal lane
*is* admission, with a stage-order-qualified sibling if size requires it. The
new file is declared in the `WFR-DRAFT-RECOVERY` row.

*Alternatives considered:*

- (a) Make only orphan cleanup process-once, leaving per-window lanes. This
  closes the cleanup counterexamples but leaves two windows' deletions and
  registrations of the same id unordered.
- (b) Share one `DraftState` between windows. That breaks the per-window
  evidence surface and the window-scoped notification routing.
- (c) Forbid a second window entirely. That encodes today's accident as a rule,
  and the widget suite already relies on two windows.

### D4. Same path in two windows: redirect, with the claim as backstop

A draft id is the stable hash of its path, so the same file open in two
windows shares one id. **Decision:** `open_document` checks duplicates
process-wide. Opening a path already open in another window of the process
presents that window and selects its tab, instead of creating a second editor.
The D3 claim is the enforced backstop. If any path (session restore, Save As
onto an open path, sidebar rename) would still produce a second editor for a
claimed id, that editor's autosave holds, and it publishes a status message
rather than writing.

*Alternative:* allow both editors and let the claim decide who writes. That was
rejected: the second editor's unsaved work would then have no durable home,
which is itself a data-safety gap.

### D5. Bounds and CI budget

K8 at 6 actions after two startups took 500.8 s and about 9 GB with 3 ids. The
two-window harness starts at **2 ids × 2 windows, 3 edits, 8 actions after the
first startup**, with a fault possible on every step. If that exceeds one
30-minute shard or a runner's memory, it drops to 6 actions. The bound and the
reason go in the harness rustdoc and the programme record (small-scope
hypothesis). It gets its own shard, `core-multi-window`, in
`scripts/kani-shards.py`.

### D6. Session persistence is audited, not redesigned

The session file is written whole by whichever window saves last, and draft
reconciliation receives that window's session. With D3's process-once restore
and D4's single owner per id, the journal's S1 does not depend on the session
(S1 counts a body on disk or a set-aside copy as recoverable). Whether the
draft is **offered** does depend on it, and that is the existing deferral "an
untitled draft absent from `session.json` is registered but never offered".
The data-safety audit task checks this path explicitly with two windows. If a
second window's session save can make the first window's untitled drafts
unoffered, that is a reachable defect in this change's scope (for widget tests
today, for "New Window" later), and it is fixed in the same work stream. The
expected fix is that session collection for save and reconciliation unions
every window of the process. Otherwise the audit's finding is recorded in the
deferral inventory, with the reasoning.

## Risks / Trade-offs

- [Model state explosion makes the harness exceed CI memory or time] →
  Mitigation: fixed arrays, 2 ids, and a dedicated shard. Step bounds are
  reduced with the reduction recorded, never silently.
- [The extended single-window model (lane, missing-body removal, untitled id)
  finds a single-window defect] → Mitigation: this is a finding, not a risk to
  hide. It is fixed first, failing-first, per the pre-existing-blockers rule.
- [A process-wide lane serializes two windows' autosaves, so one window's
  large body write delays the other's] → Mitigation: the lane is already one
  body at a time per window. The added latency is bounded by one pass, and
  first-dirty timing is measured in the widget test.
- [Process-once restore changes what a second window shows] → Mitigation: no
  production path creates a second window today. The widget tests that build
  two windows are audited, and any that relied on a second restore are updated
  with that reasoning.
- [The coordinator adds a cross-window wakeup (an inversion)] → Mitigation: it
  is narrated in the `drafts/mod.rs` facade as a named resumption point and
  covered by the multi-window tests.

## Migration Plan

No persisted format changes. The change can be reverted by reverting the
commit: per-window flags are restored, and the new harness and tests are
removed with it.

## Open Questions

- Which counterexamples Kani actually finds. The two in the proposal are
  hypotheses from reading the code, and the tasks do not presume them.
- Whether `CloseWindow` within the bound surfaces a close-flush / other-window
  interaction worth a dedicated scenario. That is decided by the harness
  result.
