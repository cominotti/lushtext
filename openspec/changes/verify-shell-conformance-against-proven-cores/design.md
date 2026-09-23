## Context

The Kani consolidation left two proven pure cores, each checked inside an
environment model that lives in a `cfg(kani)` harness module.

- **The draft journal.** `services/draft_service/journal_core.rs` holds small
  `const fn` decisions over `Copy` facts. `services/draft_service/kani_proofs.rs`
  composes them into a `Journal` model with these parts:
  - a fixed-array disk: entries, bodies, backing versions, preserved content;
  - window state: editors, known entries, tokens, restore holds, deletions,
    trust;
  - the actions Edit … Startup, each with a `fault: bool`.

  Kani proves S1–S4 and L1 (k = 7) over it. After
  `verify-multi-window-draft-journal` (change 3), the model separates `Window`
  actors from `Process` actors.
- **The durable write.** `services/filesystem/write_protocol.rs` holds
  `WriteProtocol`, `MoveProtocol`, and `RenameProtocol`, each
  `step(outcome) -> next action`. `write_protocol/kani_proofs.rs` defines a
  `Disk` model with these parts:
  - the destination, its previous inode, and one temp inode;
  - POSIX crash semantics (`after_crash`) and the live state (`visible`).

  Kani proves crash atomicity, classification soundness, mode non-widening,
  and move/rename safety.

The shells that drive these cores are not proved:

- `services/durable_write.rs`: `atomic_write_stream_with_metadata`,
  `run_rename_protocol`, and `move_durable`, which call through
  `services/filesystem/sys.rs`;
- `services/draft_service.rs`;
- `ui/window/drafts/`: `journal.rs`, `cleanup_journal.rs`,
  `autosave_execution.rs`, and `restore_execution.rs`.

The write shell's evidence today is a handful of fault-injection unit tests.
They use `#[cfg(test)]` thread-local hooks (`FAIL_NEXT_PARENT_SYNC`,
`FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `FAIL_NEXT_RENAME_CROSS_DEVICE`, and
`TEMP_AFTER_CONTENT_OBSERVER`), which integration tests cannot reach. The
draft service has `execute_orphan_cleanup_with_fault_for_test` and the
single-case property `tests/properties/draft_orphan_cleanup.rs`.

Two concrete gaps motivate this change beyond "the shells are untested":

1. **The fault seam between the two cores.** The journal model's `fault` means
   "the step's I/O did nothing". A journal step's durable write can also end
   `AfterRename`: the manifest or body is on disk, but the call returned an
   error. The journal model has no such outcome, so S1–S4 were never checked
   over it.
2. **Step atomicity.** The model treats each action as atomic. Some service
   calls perform more than one durable write per action. Preservation, for
   example, writes a set-aside copy and a local-history snapshot, and
   registration may preserve and then commit. A fault or crash between those
   writes is a state the model never visits.

This change depends on change 1 (`harden-kani-lane-and-draft-token`): the
`test-utils` gate on fixtures, `property-tests` implying `test-utils`, and the
`kani_proofs.rs` harness-module rule. It also depends on change 3, for the
window/process model.

## Goals / Non-Goals

**Goals:**

- Close the proof chain. Kani shows the core is safe inside its environment;
  conformance shows the real shell behaves like that environment and obeys
  the core.
- Use the proven code as the only oracle. There is no second specification,
  no expected-state table, and no parallel state machine.
- Pin the environment models against the real filesystem. This does for the
  POSIX side what the GTK axiom-ledger probes do for GTK.
- Find divergences, and fix each one failing-first. A divergence in the core
  counts as a proof invalidation, not a test failure to be patched quietly.
- Fit the existing lanes: the pull-request property job, the fuzz smoke,
  corpus replay, and the widget lane. Deep runs go to `test-prop-deep` and the
  fuzz lanes.

**Non-Goals:**

- Simulating power loss. An in-process test cannot discard the page cache,
  so the real-filesystem check covers only the "everything executed has
  persisted" branch of the crash model. The volatile branches stay Kani's,
  and real-process kills stay with K5 (`make crash-recovery-smoke`).
- The `ViewportSliceBin` shell. Its environment is GTK. The rendered-bounds
  widget tests and the axiom-ledger probes already play the conformance role
  there, and a generated GTK geometry driver is not affordable in the widget
  lane.
- Multi-process (axiom A6), which stays accepted per K8.
- New user-visible behaviour, persisted formats, actions, or automation
  fields, except for fixes that the conformance findings require.

## Decisions

### D1. One environment model, two drivers

Move the journal model out of `draft_service/kani_proofs.rs` into
`services/draft_service/journal_model.rs`, and move the write `Disk` model into
`services/filesystem/write_protocol/disk_model.rs`. Gate both with
`#[cfg(any(kani, test, feature = "test-utils"))]`. Each model takes every
nondeterministic value through a small trait:

```rust
pub(crate) trait Choices {
    fn flag(&mut self) -> bool;                 // previous-session layout, crash branch
    fn fault(&mut self) -> Fault;               // None | BeforeEffect | AfterEffect
    fn restore_ending(&mut self) -> RestoreEnding;
    // ... one method per kani::any() site in today's model
}
```

`KaniChoices` implements the trait with `kani::any()` and `kani::assume`. The
conformance driver's `ScriptChoices` implements it from the generated
operation and from what the real shell actually did. The harness files keep
only the `#[kani::proof]` functions and the invariant assertions. The
assertions also become model methods, so conformance can check S1–S4 on the
model's side as a cross-check.

**Rationale.** This is what "the proven core is the oracle" means in practice.
The composition of `journal_core` calls in the model is itself proved code.
Re-writing it for the tests would re-create exactly the drift the Kani
consolidation removed.

**Proof preservation.** The move must not change what Kani checks. Task 1
re-runs the journal, liveness, K8, and multi-window harnesses and the five
write harnesses before any other change. Each must give the same verdict, and
each must take a wall time within 20 % of its recorded one. A larger shift is
investigated, because it may mean the choice trait changed the state space.

*Alternatives:*

- (a) Keep the models `cfg(kani)`-only and write a separate expected-state
  function for the tests. Rejected: that is a second specification.
- (b) Use only the pure cores as the oracle, with no environment model. For
  the write shell, `WriteProtocol::step` alone gives the action sequence and
  the classification, but it cannot say what the disk should hold. For the
  journal, the per-id decisions do not say what the whole journal should look
  like after a step. Rejected as too weak, although (b) is still the first
  check at every step.

### D2. Runner: a hand-rolled state-machine driver on plain `proptest`

The script is `Vec<Op>`, with a bounded length and small bounded fields: an
id index, a fault class, and a fault position. The driver folds the script
over `(model, real)` and checks the abstraction after every step.

**Why no `proptest-state-machine`.** Its value is precondition-aware
generation and shrinking, through a `ReferenceStateMachine` trait. This
project needs neither:

- Every model step is **total**: an inapplicable action is a no-op. Today's
  model already returns early in that case. The driver asks the model whether
  an op is enabled before calling the service, so an op that the model treats
  as a no-op is a no-op on the real side too. Every subsequence of a valid
  script is therefore valid, and proptest's built-in `Vec` shrinking (drop
  elements, then shrink each element) is sound and yields minimal sequences.
- Implementing `ReferenceStateMachine` would mean re-expressing the model as
  that crate's trait, which is a second copy.
- The crate is a new dev-dependency. It would bring `cargo deny` review, a
  cargo-hakari regeneration, and a `cargo-sources.json` regeneration (the
  vendored sources cover the whole lock file, dev-dependencies included) for
  no gain.

*Alternative:* only extend the `operation_script` fuzz target. Rejected as the
primary runner: libFuzzer does not shrink semantically (`-minimize_crash`
shrinks bytes), it is nightly-only, and it runs outside the pull-request gate.
It is kept as a secondary explorer (D7).

**Shrinking budget.** The conformance properties use their own config, built
from `support::property_config()`, with `max_shrink_iters = 256` and
`max_shrink_time = 45_000` ms. That keeps shrinking inside nextest's
`property` profile ceiling (60 s × 2). A failure persists to the existing
regression file. Task discipline: the shrunk script is pasted into a
deterministic `#[test]`, and that test is committed failing, before the fix.

### D3. The write-shell tap

A module `services/filesystem/backend_tap.rs` gated
`cfg(any(test, feature = "test-utils"))`, with three parts:

- **A thread-local `TapState`** installed by `BackendTap::install(plan) ->
  TapGuard`, which uninstalls on `Drop`. No process-global slot is used,
  following the build rule on fault seams: the driver is single-threaded, and
  nextest isolates tests by process. The `plan` maps an action ordinal
  (counted across one protocol run, including nested copy writes, and tagged
  with the protocol kind) to an injection: `Fail`, `AlreadyExists`,
  `CrashBefore`, or `CrashAfter`.
- **One call site per action** in each shell loop:
  `tap::before(kind, &action)?`, which may inject a fault, and
  `tap::after(kind, &action, outcome)`, which records the outcome fed back.
  When `apply-verification-altitude-redesigns` group 1 (the common
  `Protocol::drive`) lands first, the tap lives once in `drive`. Otherwise
  each of the three loops calls it. Under shipping builds both calls are
  `#[inline(always)]` no-ops behind `cfg(not(...))`, which is the kill-point
  pattern. Task 7 checks with `strings` that the release binary is free of
  them.
- **A fallible-call counter** in `sys.rs`. Each fallible backend primitive
  bumps a thread-local counter under the same gate: `required_metadata`,
  `create_temp_file`, `apply_mode`, `sync_file`, `rename` /
  `rename_no_replace`, `sync_dir_descriptor`, `remove_file`, and `read`. The
  content write and flush through the temp file's `Write` is the
  `WriteContent` action's one call. `best_effort_chown` and
  `copy_xattrs_best_effort` return `()` and are not fallible calls.

  The check "exactly one fallible call per non-`Finish` action" follows the
  spec's "one backend call per action". A nested `Copy` in `MoveProtocol` is
  one action that runs one whole `WriteProtocol`. The tap records the nested
  run as a child trace, and the check applies recursively.

**Crash.** A `CrashBefore` or `CrashAfter` injection panics with a private
marker payload. The driver catches it with `std::panic::catch_unwind`. Nothing
in the shell handles it. Unwinding drops the temp `File` without running
`RemoveTemp`, which is exactly the "process stopped here" disk. This adds no
control flow to the production loop.

**The existing hooks** (`fail_next_parent_sync_for_test`,
`FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `fail_next_rename_cross_device_for_test`,
and the temp observer) are re-expressed as one-entry tap plans. Their existing
tests must pass unchanged, which proves the tap against the known paths
before it is trusted with generated ones.

**`RemoveTemp`.** The shell feeds `Done` regardless of what the removal
returned, and the core's `step` maps `RemoveTemp` under any outcome to
`Finish(BeforeRename)`. The conformance check asserts this indifference
(`step(Done) == step(Failed)` from that state) instead of flagging it. The
disk model's `removal_failed` then predicts whether a temp is left behind.

**Oracle per step.** The driver keeps three things in lockstep:

1. `WriteProtocol` (or `MoveProtocol` / `RenameProtocol`), stepped with the
   outcome recorded by the tap;
2. the `Disk` model, stepped with the same action and outcome, via
   `ScriptChoices` fed from the tap record;
3. the real tempdir.

After each action it checks four things:

- the core's next action equals the one the shell executed next;
- the real destination bytes equal the model's `visible()`, with `Old` and
  `New` mapped to the generated byte strings;
- the real temp exists if and only if the model's temp is still `linked`;
- the temp's mode is no wider than the destination's (theorem 3, checked on
  the real fs).

At `Finish`, the returned `Result` variant must equal the core's
`WriteClass`. After a crash, the real destination must be `Old` or `New` and
equal the model's `visible()`, and any leftover must parse through
`temp_name`'s single owner as a LushText temp name.

**Scripts.** The destination is absent or holds old bytes, with a generated
mode. Payloads are tiny. The primitive is one of `atomic_replace`, the stream
variant, `copy_durable`, `move_durable`, `rename_durable` (same or cross
directory), and `rename_durable_no_replace`. One optional injection is placed
at a generated action ordinal. The finite protocols make this nearly
exhaustive: at most 17 actions × 4 injections × 7 primitives. So the property
is complemented by one deterministic **exhaustive** test that enumerates every
(primitive, ordinal, injection) triple. That test is cheap and runs in
`make test`.

### D4. The draft-service driver and its abstraction function

**Driver role.** At service level there is no GTK window, so the model's
window half (editors, tokens, known entries, holds) is **model-owned**: the
model decides what the window would do, and the driver translates each
enabled model action into the service call(s) the drafts coordination makes
at that point:

| Model action | Service call(s) |
|---|---|
| Edit | none (buffer change; content id minted) |
| Register | `register_draft_entries` → keeps the returned `RegisteredDraft` |
| WriteBody | `write_draft(&registered, text_of(content))` |
| Commit | `update_manifest` with the pass's `DraftManifestCommit` |
| Save | durable write of the backing file (mtime advances) |
| Discard | none (the deletion is queued; model-owned) |
| DeletionStep | `resolve_draft_restore` / `preserve_stale_draft_body` / `delete_draft_file` / `remove_manifest_entry`, per the core's `DeletionStep` |
| Inspect / ExecCleanup | `inspect_orphan_cleanup_from` / `execute_orphan_cleanup` |
| RestoreApply | `resolve_file_draft_restore` + the disposition's preservation call |
| ExternalMtime | `fixture::set_modified` on the backing file, +1 s or more (manifest mtimes are seconds) |
| Crash | drop every driver-held value (tokens, plans, restore state, authority) |
| Startup | `load_restore_state_cancellable`, then adopt its authority and restore list |

When implementation finds that the table's mapping for an action differs from
what `ui/window/drafts/` actually calls, the coordination is the source of
truth, and the table in this design is updated. Such a difference is itself a
finding about the service-level driver's fidelity, not a code defect.

**Abstraction function** (the real state projected to the model's disk and
authority):

- `entry[id].present` comes from the loaded manifest (`load_manifest`), and
  `entry[id].backing` from that entry's `original_mtime_secs`, mapped through
  the driver's mtime→version table;
- `body[id]` is the body file's text, mapped back to a content id through the
  driver's injective `content ↔ text` table (`"c{content}\n"`, extended to
  multibyte for some ids);
- `backing[id]` is the backing file's current mtime version;
- `preserved` is the set of content ids found among set-aside bodies
  (`set_aside::list`/`read`) and local-history snapshots of the backing path;
- `trusted` is the `DraftManifestAuthority` the last service call returned,
  which the driver holds as the window does.

The check at every step is `abstract(real) == model.disk_projection()`. The
model's own S1–S4 assertions also run, as a cross-check that the lockstep
stayed inside the proved envelope.

**Faults, and the atomicity seam.** A generated fault is `(class, position)`.
The driver arms the tap for the service call, and the post-step state must
equal the model's step under the matching `Fault`:

- `BeforeEffect` fails the first fallible backend call of the step. The model
  step is `Fault::BeforeEffect`, which is today's `fault = true`.
- `AfterEffect` fails the final `SyncDir` of the step's committing write. The
  model step is the new `Fault::AfterEffect`: the step's effect persists, but
  the window treats it as failed. That means no token, no `written`, no
  acceptance, and the dirty flag kept.
- `At(k)` fails the k-th fallible call of a multi-write step. The resulting
  real state must equal the model's `BeforeEffect` or `AfterEffect` outcome.
  A third, intermediate state is an **environment finding** (step atomicity).
  It is resolved by one of two changes, recorded in the programme record: the
  model gains that intermediate state, and Kani re-checks, or the shell is
  reordered so that the intermediate state cannot be observed.

**Widening the Kani fault domain.** `Fault` replaces `fault: bool` in the
shared model. `AfterEffect` is meaningful only for the Register, WriteBody,
Commit, DeletionStep(RemoveEntry), and preservation steps. Kani then checks
S1–S4 and L1 over it. L1 is stated without faults, so it is unchanged, but it
is re-run. The state space grows. The shard budget rules of change 1 apply:
split first, and reduce bounds only as a recorded last resort.

**Two windows at service level.** A second property runs two window actors of
one process actor from change 3's model, over one data directory and one
service. The service's manifest write lock and `TargetWriteGuard` are
genuinely process-wide in the test process, so this checks that the real
shared state matches what the multi-window model assumes is shared. Window
choice is an `Op` field.

### D5. GTK drafts coordination: a scripted scenario set, not a random one

**Feasibility.** Each widget step needs a settle: `spawn_blocking_then`
completions, idle drains, and readiness waits. A generated 16-step GTK
sequence costs seconds, and shrinking it would re-boot windows hundreds of
times. That does not fit the widget lane. A scripted set still has value,
because it is the only layer that exercises `journal.rs`,
`cleanup_journal.rs`, `autosave_execution.rs`, and `restore_execution.rs`
against the model:

- **Scenarios (at most 8), each a recorded model trace:**
  1. the L1 k = 7 path (stale pending restore → preserve, delete body, remove
     entry → register, write, commit);
  2. the Unavailable-restore path, from the S1 counterexample;
  3. a failed set-aside copy that keeps the hold and retries;
  4. a Data-page open of a preserved draft;
  5. a Register `AfterEffect` fault, then a restart through the startup path
     of a fresh process state;
  6. a Commit `AfterEffect` fault;
  7. a two-window cleanup racing a registration (from change 3's
     counterexamples, or its proof);
  8. a discard during a pending write.

  The trace is replayed through real window actions: editing the buffer, and
  triggering autosave, discard, and restore through the existing actions and
  `test_policy.rs` seams.
- **Comparison.** After each settled step: `DraftEvidence` against the
  model's window state, as counts (restore holds, tombstones, pending
  preservations, authority trusted, mutation in flight), and the D4 disk
  abstraction against the model's disk. If a per-id fact the comparison needs
  is missing from `DraftEvidence`, the evidence surface gets a bounded field,
  under its invariants and declared in the `WFR-DRAFT-RECOVERY` row. No
  `*_for_test` seam is added.
- **Crash.** A widget scenario cannot crash a process. A restart is modelled
  by dropping the windows without the close flush and starting a fresh
  window over the same data directory. That is valid only if change 3's
  process-once startup offers a test-utils reset. Otherwise the restart half
  stays at service level (D4) and in K5.

*Alternative:* derive scenarios automatically from the conformance lane's
shrunk failures. That is useful later, but it is not a gate. The fixed list
is reviewable.

### D6. Placement and gating

- The driver lives in `lushtext-core` at `src/services/conformance/`
  (`mod.rs`, `write_shell.rs`, `draft_service.rs`, `ops.rs`), gated
  `#[cfg(any(test, feature = "test-utils"))]`, because it must reach the
  crate-private models and the tap. The property modules in
  `tests/properties/` (`shell_conformance_write.rs` and
  `shell_conformance_drafts.rs`) are thin `proptest!` wrappers over it.
  `property-tests` already implies `test-utils` after change 1.
- These modules, the two model modules, and `backend_tap.rs` are verification
  code. They are added to `.cargo/mutants.toml` `exclude_globs`, as change 1
  does for `kani_proofs.rs`, and to the filesystem-boundary audit's allowlist
  of test-gated backend users, if the audit flags them.
- The filesystem boundary is kept: the driver uses
  `services::filesystem::fixture` for seeding and `set_modified` mtimes, and the public
  boundary for reads. It makes no raw `std::fs` calls.

### D7. Fuzz reuse: a conformance mode in `operation_script`

`lushtext_core::fuzzing` gains `exercise_conformance_script_for_fuzzing`,
which is selected when the input's first byte has its top bit set. It decodes
bytes into the same `Op` vocabulary through `ops::decode` (with the existing
operation-count and input-length caps) and runs the D3 and D4 drivers. The
`fuzzing` feature gains `test-utils`, which is acceptable because fuzz builds
never ship. Committed seeds, including every promoted divergence, replay
through `fuzz_corpus_replay` on stable. A new target was rejected: it would
need its own corpus, Makefile list entry, and docs, for no semantic gain. The
selector bit keeps the existing helper ops' throughput for the other half of
the input space.

### D8. Budgets

| Lane | Setting | Budget |
|---|---|---|
| `make test` | exhaustive write-shell triples (D3) | ≤ 5 s |
| `make test-prop` | write-shell property: 64 cases | ≤ 10 s |
| `make test-prop` | drafts property: 64 cases × ≤ 16 ops; two-window: 32 cases × ≤ 16 ops | ≤ 30 s each, measured on CI |
| `make test-prop-deep` | 512 cases, ≤ 32 ops | informational |
| `make fuzz-operation-smoke` | existing time bound, shared by both modes | unchanged |
| `make test-widget` | ≤ 8 scenarios | ≤ 60 s added |
| `make kani` | widened fault domain | the change 1 shard margins (≤ 25 min, ≤ 12 GiB) |

The tempdir sits under the default `TMPDIR`, with real fsyncs. If CI
measurement exceeds a budget, the fixes are tried in order: the script-length
bound first, then the case count for that property. The pull-request job
timeout is never raised. The measured figures are recorded in
`docs/property-testing.md`.

## Risks / Trade-offs

- [The model move changes what Kani proves.] → Task 1 re-runs every affected
  harness first and compares verdicts and times. The choice trait is a pure
  indirection whose Kani implementation is exactly today's `kani::any()` and
  `kani::assume` calls.
- [The widened fault domain blows the Kani budget.] → The fault is
  restricted to the steps where `AfterEffect` exists. Shards are split under
  change 1's rules. A bound reduction is recorded with the property it
  weakens.
- [The service-level driver re-implements window behaviour and drifts from
  the GTK coordination.] → The window half is model-owned (proved), not
  driver-owned. The D4 mapping table is checked against the real call sites
  in `ui/window/drafts/`, and D5 covers the real coordination.
- [Real-filesystem runs are slow or flaky on CI.] → Payloads are tiny, the
  driver is single-threaded, and there are no timers or watchers. Any flake is
  a blocker under the pre-existing-blockers rule and is investigated, not
  retried.
- [Mtime granularity makes `backing` ambiguous.] → The driver sets distinct
  whole-second mtimes explicitly, and never relies on the wall clock.
- [The abstraction is not injective, so real divergences hide.] → The
  content↔text table is injective by construction. Unknown files in the
  drafts directory fail the abstraction outright rather than being ignored.
- [Tap code leaks into shipping builds.] → It is behind the gate, with a
  release `strings` check and change 1's default-feature `cargo check` of the
  binary.

## Migration Plan

Nothing is migrated, because no persisted format changes. Rollback means
reverting the change, since all the added code is test-gated. If a divergence
fix touches production, that fix ships with its own failing-first test and
can be reverted separately.

## Open Questions

- Can local-history preservation be observed cheaply enough for every step?
  If not, the abstraction reads it only at preservation steps and at startup,
  and records that restriction.
- Does change 3's process-once startup expose a `test-utils` reset, which D5
  needs for an in-process restart? If not, the D5 restart scenarios are
  replaced by service-level ones.
- If the new `AfterEffect` Kani runs find a counterexample, is it reachable
  from real hardware errors, and should it be fixed or pinned as a
  `should_panic` residual? That is decided per finding and recorded, following
  the K4 and K8 precedent.
