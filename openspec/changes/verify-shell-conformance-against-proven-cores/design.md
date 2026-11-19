## Revision against 761866ff (2026-11-18)

The change was proposed in 1546d7e8, before its prerequisites landed. This
revision re-derives the plan against `main` 761866ff, after
`harden-kani-lane-and-draft-token`, `verify-multi-window-draft-journal` and
`apply-verification-altitude-redesigns` (PR #117, merged 8d4f5d7b, archived
761866ff) merged.

**What the shared driver already gives.** Altitude group 1 put all six
proven-core shells on one loop, `protocol_drive::drive`:
- the atomic write, `run_rename_protocol` and `move_durable` in
  `durable_write.rs`;
- the Replace All apply and Undo shells;
- the journal shrink in `search_backup.rs`.

A source check (`no_shell_steps_its_core_outside_protocol_advance`) pins that
no shell steps its core anywhere else. So **the sequence of actions a shell
executes is the core's by construction**. The old D3 check "the core's next
action equals the one the shell executed" is now redundant. What conformance
still has to show is narrower and sharper:
- (i) the outcome a shell feeds back reflects what the backend really did;
- (ii) each `execute` arm has exactly its action's effect on the disk;
- (iii) the service built on the shells keeps the journal model's state.

`drive` is one site that can observe every action and inject a crash. It
cannot make a typed fault faithful: a shell builds `last_error`, and decides
`AlreadyExists`, inside its own `execute` arms (archived design, "What the
site does not give"). This revision designs that half (D3).

**Groups, re-decided.**

| Group (1546d7e8) | Decision | Why |
|---|---|---|
| D1 shared models | **Kept, split in two** | Write model: the choices become explicit arguments, with no trait. Journal model: a `Choices` trait. The move is its own commit, before any widening (D1). |
| D2 plain-proptest runner | **Kept, generation changed** | E4 measured that uniform scripts barely reach the protocol. Generation is now enabled-indexed, with a recorded reach figure and a mutant calibration (D2). |
| D3 backend tap | **Kept, redesigned** | Observation, crash and before-action hooks live once in `drive`. Typed faults are injected at the `sys.rs` backend call. "Exactly one fallible call per action" is replaced by a pinned per-action call table (D3). |
| D3 write-shell conformance | **Kept** | Exhaustive and cheap. It is the only check that the real disk matches the proven disk model (D4). |
| D4 draft service, one window | **Kept** | E1 found a real defect exactly this way (D5). |
| D4 draft service, two windows | **Dropped** | The service holds no window state, and a sequential driver cannot exercise the process-wide lock. K8 and the multi-window harness already prove the coordinator (D5). |
| D4 `AfterEffect` widening | **Kept, semantics corrected** | "Persists" alone is unsound: an after-rename failure is visible but not durable (D6). |
| D4 step atomicity, `At(k)` | **Kept** | Preservation is compared as a superset (D7). |
| D5 GTK scenarios | **Dropped, deferred** | They need the widget lane and the three smokes (UI prefix `crates/lushtext/tests/widget/`), and the evidence-surface fields would touch `ui/`. Single-scenario widget tests already cover the paths (D10). |
| D7 fuzz conformance mode | **Dropped** | The write shell is enumerated exhaustively, and the draft property shrinks semantically. A byte-decoded mode adds a corpus, seeds and a feature edge for no new coverage (D10). |
| (new) Replace shells | **Tap only** | The three `process_file` mutation survivors owed to this change are killed. Lockstep against the Replace model is deferred (D8). |

**Owed by others to this change**, from the programme record's deferral
inventory: the three `replace/apply.rs::process_file` mutation survivors (the
pass-A facts check, and the two cancelled tests), each needing code between
two actions of one file's protocol (D8).

## Spec review (stage 2, 2026-11-18)

An independent review checked this revision against the code at 761866ff and
corrected it in place. Each item is folded into its decision:

- **Preservation is the set-aside copy only (D5).** Production says so in
  `preserve_stale_draft_body`. Reading local history into `preserved` would
  also have hidden calibration mutant 1.
- **The fault domain is complete (D6).** The first version widened four
  steps, but treated as durable several effects whose directory sync can
  fail or never runs: the set-aside placements counted as placed after a
  failed sync, ExecCleanup's and Startup's manifest writes, and the orphan
  unlink. The manifest also reverts as one file, not entry by entry. L1's
  bound has a pre-registered outcome if the widened prefix lengthens it.
- **Environment-decided choices come from the real service (D1, D5).** The
  script picks only what it controls. The restore endings the service
  decides, and the crash's keep-or-revert flags, are read off the real run.
  `any_below` is harness-only, so `Choices::below` is dropped.
- **`RemoveTemp` steps the model with the removal call's own result (D4).**
  The fed-back `Done` would contradict a real temp left by an injected
  failure.
- **The hooks' setters reach five other files, one under `ui/` (D3).** They
  keep their signatures and semantics, so no UI-prefix file is edited.
- **`proptest` and `tempfile` are dev-dependencies (D2, D11).** The in-crate
  drivers use neither, and the strategies and the reach test move to the
  property target, so the reach test runs in `make test-prop`.
- **The property config is overridden, not inherited (D2).** The shared
  config has 1,024 shrink iterations and no shrink-time cap.
- **Tap plans are armed only around the calls under test (D3).** The
  fixtures and the read boundary share `sys.rs`.
- **Proof strength is also owed for `write_protocol` (D9).** The transfer
  harness bodies are rewritten, and a verdict cannot show that a moved model
  was not weakened.
- **The 649 test keeps the target's bytes (D8).** One test kills 689 and 691.
- Factual fixes: `WRITE_STEPS` is 15 actions with the finish included;
  `core-second-writer` has one journal harness of seven; three
  `durable_write` tests, not four, read a hook cell.

## Context

Kani proves two pure cores, each inside an environment model that lives in a
`cfg(kani)` harness module:

- **The draft journal.** `services/draft_service/journal_core.rs` holds the
  decisions. `services/draft_service/kani_proofs.rs` (1,721 lines) composes
  them into the `Journal` model:
  - `Window` and `Process` actors;
  - a `Scope` trait of associated constants;
  - the actions `Edit` … `Startup`, each with a `fault: bool`.

  The model has seven `kani::any()` / `kani::assume` choice sites before the
  harnesses. It proves S1–S4, L1 (k = 7), K3 and the multi-window properties.
- **The durable write.** `services/filesystem/write_protocol.rs` holds
  `WriteProtocol`, `MoveProtocol` and `RenameProtocol`. Their harness module
  (481 lines) defines a `Disk` model of one destination, its previous inode
  and one temp inode, with `after_crash` and `visible`. Its four choice sites
  are the outcome inside `Disk::execute`, the crash branch inside
  `after_crash`, the crash step, and the harness-level outcomes.

The shells are not proved:

- `durable_write.rs`. Its evidence is 40 unit tests, a few of them using
  `#[cfg(test)]` hooks (three of whose setters other modules' tests also
  call, D3): five thread-locals
  (`FAIL_FINAL_TEMP_SYNC_AFTER_METADATA`, `FAIL_NEXT_PARENT_SYNC`,
  `FAIL_NEXT_SOURCE_REMOVAL`, `FAIL_NEXT_RENAME_CROSS_DEVICE`,
  `NEXT_NO_REPLACE_UNSUPPORTED`) and one process-global observer
  (`TEMP_AFTER_CONTENT_OBSERVER`). Some of these return early, before the
  protocol runs (`rename_durable`'s cross-device hook). That is not the path a
  real `EXDEV` takes.
- `draft_service.rs` (5,055 lines), whose evidence is single-scenario tests and
  the property `draft_orphan_cleanup.rs`.
- The Replace All shells, with their own target-keyed hook registry in
  `content_search/replace/mod.rs`.

Two concrete gaps:

1. **The fault seam between the cores.** The journal model's `fault` means
   "the step's I/O did nothing". A draft body write (`write_draft` →
   `atomic_replace`) and a manifest commit can end `AfterRename`: the bytes are
   visible, the directory sync failed, and the call returns an error, so the
   window treats the step as failed. The service already treats this outcome
   specially in one place: `set_aside::keep_copy` counts an `AfterRename`
   placement as placed (`set_aside.rs`, "`AfterRename` means it is in the
   set-aside area"). The model has no such outcome.
2. **Step atomicity.** The model's actions are atomic. Some service calls make
   several durable writes: a preservation makes a set-aside copy plus a
   local-history snapshot, and a registration may preserve and then commit.

## Goals / Non-Goals

**Goals:**

- Close the proof chain for the durable write and the draft service. Kani
  shows each core is safe inside its environment; conformance shows the real
  shell behaves like that environment.
- The proven code is the only oracle. There is no second specification, no
  expected-state table, and no parallel state machine.
- Pin the write environment model against the real filesystem, for every
  outcome the shell can feed back.
- Find each divergence and fix it failing-first. A divergence in the core
  counts as a proof invalidation.
- Kill the three Replace mutation survivors owed to this change.

**Non-Goals:**

- Simulating power loss. An in-process test cannot discard the page cache, so
  the real-filesystem check covers only the branch of the crash model in which
  everything executed has persisted. The volatile branches stay Kani's, and
  real-process kills stay with K5 (`make crash-recovery-smoke`).
- **Crashes in the middle of a draft-service call** (D5). They stay with K5's
  kill points.
- The GTK drafts coordination, the fuzz conformance mode, the two-window
  service property, and lockstep checks of the Replace shells. These are
  deferred with reasons (D8, D10).
- The `ViewportSliceBin` shell: its environment is GTK, and the axiom-ledger
  probes play the conformance role there.
- Multi-process (axiom A6), which stays accepted per K8.
- Any user-visible behaviour, persisted format, action or automation field,
  except a fix a conformance finding requires.

## Decisions

### D1. One environment model per core, two drivers, moved before anything else

**Write model.** `Content`, `Entry`, `TempInode`, `Disk` and the `Shell`
mutant move into `services/filesystem/write_protocol/disk_model.rs`, gated
`#[cfg(any(kani, test, feature = "test-utils"))]`. They move verbatim, except
that each choice becomes an argument:
- `Disk::execute(action, shell, outcome)`;
- `Disk::after_crash(durable_branch: bool)`.

The harnesses pass `kani::any()` at the call site. A choice made at the call
site is the same nondeterminism, so the state space is unchanged. No `Choices`
trait is needed for a model whose choices are all per call. The stateright
port `env.rs` used the same explicit-argument transformation.

**Transfer models.** The move and rename harnesses keep their environment
inline today: `source_present` and `destination_durable` for a move;
`renamed`, `source_synced` and `destination_synced` for a rename. That
environment moves into the same module, as a `MoveDisk` and a
`RenameNamespace`, each with `execute(action, succeeded)`. The harnesses keep
every assertion and `kani::cover!` over these fields. Without them, a move or
rename would have no disk oracle, and D4 could check only its classification.
Unlike the write harnesses, the transfer harnesses' bodies change beyond their
choice sites: their inline field updates become `execute` calls. The two
rename harnesses update their fields slightly differently today
(`source_synced = succeeded && renamed` in one; an `assert!(renamed)`, then
`source_synced = succeeded`, in the other). `RenameNamespace` keeps the
assertion, which the protocol already satisfies, so the first harness gains a
property the second already proves and loses none. Because of this rewrite,
D9 owes proof strength for `write_protocol` too.

**Journal model.** The model moves into
`services/draft_service/journal_model.rs`, under the same gate. Some of its
choices are taken inside the model, not at a harness call site: the
previous-session layout bits of `previous_session()`, and the `RestoreEnding`
inside `RestoreApply`, under a state-dependent `kani::assume`. After D6, a
crash's keep-or-revert choice for each unconfirmed effect joins them. So the
model takes them through a small trait:

```rust
pub(crate) trait Choices {
    fn flag(&mut self) -> bool;  // previous-session layout bits; D6 crash keep/revert
    fn restore_ending(&mut self, admits: impl Fn(RestoreEnding) -> bool) -> RestoreEnding;
}
```

`any_below` is used only at harness call sites (the id, window and process
of `Journal::step`), so it stays in `kani_proofs.rs` and the trait does not
need it.

- `KaniChoices` implements it with `kani::any()` and `kani::assume`. It lives
  in `kani_proofs.rs`. The only other `cfg(kani)` code outside the proof
  functions is `cfg_attr(kani, derive(kani::Arbitrary))` on the model's
  `Action` (and, after D6, `Fault`) enums in `journal_model.rs`, as
  `journal_core.rs` already does for its own enums.
- The conformance driver's `ScriptChoices` answers **from the script only
  where the script controls the choice**, and from the real environment
  otherwise:
  - the previous-session layout comes from the script, and the driver then
    seeds the real data directory to that layout before the first
    `Startup`. The manifest is written with `save_manifest`, bodies with
    `draft_service::fixture::write_body`, which needs no token, and backing
    files and their mtimes with `filesystem::fixture`;
  - a restore ending is read off the real resolution. The window-side
    endings (`Applied`, `EditedOver`, `InstallCancelled`) are the script's
    to pick only when the real `resolve_file_draft_restore` returned a body;
    `Stale`, `Unavailable` and `MissingBody` come from what it returned;
    `ReadFailed` comes from an injected read fault; `Oversized` is unreachable
    with tiny bodies and is never chosen, a restriction recorded with the
    property. The driver asserts that the model `admits` the ending it hands
    over, and a real ending the model does not admit is a divergence;
  - a D6 crash keeps every unconfirmed effect, because an in-process crash
    cannot revert one (D6).

The action, id and fault stay explicit arguments of `Journal::step`, as they
are today. The S1, S3 and S4 assertions become model methods that return
their verdict; S2 stays the inline assertion inside `exec_cleanup_one`, which
panics in a conformance run as it fails a harness. The harnesses assert those
methods, and conformance calls them as a cross-check.

**Placement and the tables.**
- The model modules are verification code. They are added to:
  - `.cargo/mutants.toml` `exclude_globs`, with a reason;
  - `NOT_ORACLE_MODULES` in `scripts/kani-shards.py`, because a harness file
    imports from them and `make check-kani-shards` requires that every module
    a harness imports is an oracle or an exception.
- `kani_proofs.rs` keeps the `#[kani::proof]` functions, their assertions
  and `KaniChoices`, so the harness-module rule (`testing.md`, "Kani
  Harnesses") still holds.
- The write and journal harness bodies change only at their choice call
  sites; the transfer harnesses as above. No harness compile runs before
  task 9.1: the four shards run once, after the final
  code, and a compile-only run would be a second run. `make check` compiles
  the models under `cfg(test)` from the first commit.

**Order.** The move is one commit, before the tap, the drivers or the
widening. The affected scheduled shards run once, after the final code (D9).
The move gets no run of its own. A passing verdict alone cannot show that a
moved model was not weakened, so its preservation is shown three ways on the
final code: each harness keeps its recorded verdict, each `kani::cover!`
reports SATISFIED (an UNSATISFIABLE one is investigated as a possibly
weakened model; Kani does not fail on it by itself), and the D9 proof
strength runs kill at least the recorded number of mutants of `write_protocol`
and `journal_core`. The widening changes only the journal shards, and only by
the domain D6 states.

*Alternatives:*
- A separate expected-state function for the tests: a second specification.
- The core decisions alone as the oracle: too weak. E1's defect was found by
  the environment model, while the core answered correctly.
- Reusing `formal/evaluation/stateright/src/env.rs`: it is a port, a second
  copy that already drifts from the Kani model (two inline asserts turned into
  flags, and a hand-written `Hash`).

### D2. Runner: plain `proptest`, enabled-indexed generation

A script is a `Vec<Op>` with a bounded length.
- Each `Op` carries a **choice index**, not an action name. Before every
  step, the driver asks the model for its enabled actions and takes
  `index % enabled.len()`, so every op does something.
- The fault field and the id are bounded small integers, interpreted the same
  way.

So every generated script is valid. Removing an op shifts the later ops'
choices, but they stay valid, so proptest's own `Vec` and element shrinking is
sound. No `proptest-state-machine` dependency is added: it would bring
`cargo deny`, hakari and `cargo-sources.json` churn, and a second expression of
the model as its trait.

**Where the pieces live.** `proptest` and `tempfile` are dev-dependencies of
`lushtext-core`, so code under `src/` gated only on `feature = "test-utils"`
cannot name them (the `properties` target compiles the library without
`cfg(test)`). So the `Op` type, its interpretation against the model, and the
drivers live in `src/services/conformance/` with no proptest or tempfile use:
each driver takes an op slice and a caller-owned root directory. The proptest
strategies, the `TempDir` that owns each case's root, and the reach test live
in `tests/properties/`. Deterministic unit tests under `cfg(test)` (D4's
enumeration) use `tempfile` there as usual.

**Reach and calibration.** E4 measured 2,000 uniform traces that made 6 body
writes and no commit, and a trivial mutant that needed 1,246 traces to be
caught. Enabled-indexed generation is meant to fix that, and it must be shown
to:
- **Reach figure.** A deterministic test in the `properties` target (so it
  runs in `make test-prop`, where the strategy is) generates the property's
  default case count with a fixed seed and interprets each script over the
  journal model alone, with no disk. Reach is a property of the generator
  and the model's enabled sets, and a passing lockstep run takes exactly the
  model's steps, so the model-only count is the property's reach. It counts
  the steps that reached each of `WriteBody`, `Commit`, `DeletionStep`,
  `RestoreApply`, `ExecCleanup` and `Startup`, and asserts a recorded floor
  for each. A generator change that drops reach then fails a test instead of
  passing vacuously.
- **Mutant calibration.** Two scratch, never-merged mutants of the service
  are each caught within the default case count, and the counts are recorded:
  - `preserve_stale_draft_body` skipping its set-aside copy;
  - `remove_manifest_entry` returning `Ok` while leaving the entry on disk,
    an effect mutant. The deletion order is the driver's, taken from
    `journal_core`, so an order mutant would not be a service mutant.

**Shrink budget.** The conformance properties start from
`support::property_config()` (64 cases, `max_shrink_iters = 1024`, a 10 s
per-case `timeout`, failures persisted to `proptest-regressions/properties.txt`)
and override two fields: `max_shrink_iters = 256` and `max_shrink_time =
45_000` ms, which the shared config does not set. A property's run plus its
shrink then stays inside nextest's `property` profile (`slow-timeout` 60 s,
terminated after 2 periods), and the drafts property's 30 s budget (D12) plus
45 s of shrinking is below 120 s. The shrunk script is committed as a
deterministic `#[test]`, failing, before the fix.

### D3. The conformance tap: one site in `drive`, faults at the backend call

The tap is `services/filesystem/backend_tap.rs`, gated
`#[cfg(any(test, feature = "test-utils"))]`. It is a per-invocation fault
seam (`testing.md`): a thread-local `TapState`, installed by
`BackendTap::install(plan) -> TapGuard`, which uninstalls on drop. It has no
process-global slot. The drivers are single-threaded, and so are the
shells' own calls: Replace runs `process_file` on its caller's thread.

**Action site (in `drive`).**
- `Protocol` gains `const KIND: ProtocolKind`:
  - `Write`, `Move` and `Rename`;
  - `ReplaceFile`, `ReplaceUndo` and `Shrink`.
- `drive` calls `tap::before::<P>(&action)` and `tap::after::<P>(&action,
  &outcome)` around its single `execute(action)`, which now needs
  `P::Action: Copy + Debug + 'static` and `P::Outcome: Copy + Debug + 'static`.
  The bounds go on the trait's associated types, unconditionally: all six
  action and outcome types are already `Copy + Debug` (`StepOutcome`, `bool`,
  and the Replace enums in `replace/protocol.rs`, so that oracle module is
  not edited), and a bound that changed with the gate would give the shipping
  and test builds different signatures.
- The tap records the trace as `(kind, run, action ordinal, action,
  outcome)`, with `Box<dyn Any>` payloads that the driver downcasts. A nested
  `drive` (a move's `Copy`) records a child run, between its parent's
  `before` and `after`.
- At an address, `before` can do one of three things:
  - run a closure (a before-action hook, which D8 needs);
  - unwind with a private `TapCrash` payload (a crash before the action),
    through `std::panic::resume_unwind`, which skips the panic hook, so a
    few thousand enumerated crashes print no panic messages;
  - leave a crash armed for `after` (a crash after the action).
- Outside the gate, both calls are `#[inline(always)]` empty functions, as
  `kill_point::reach` is.

**Call site (in `sys.rs`).** Every fallible backend primitive the shells
reach calls `tap::fallible(call, path)` first, and returns its `Some(error)`
when an injection is armed:
- `required_metadata` (`metadata` on non-Unix);
- `create_temp_file`, `apply_mode` and `sync_file`;
- `rename` and `rename_no_replace`;
- `sync_dir_descriptor`, `remove_file`, `read`, `create_dir_all` and
  `create_dir`.

This list is the durable-write shells' set. The draft service also reaches
reads and traversal (`canonicalize`, `symlink_metadata`, `metadata`,
`read_prefix` and the bounded readers, `visit_directory_entries`), which a
scope-wide address counts. Task 2.3's enumeration from the code, pinned in a
test, is the authoritative list, not this paragraph.

The injected error then takes the real error path:
- `outcome_of` stores it as `last_error`;
- `create_temp_file` returning `ErrorKind::AlreadyExists` becomes
  `StepOutcome::AlreadyExists`;
- an `EXDEV` (the error `sys::cross_device_error_for_test` builds, whose gate
  widens from `cfg(all(test, unix))` to the tap's) takes the real
  cross-device branch.

Each fault is therefore as faithful as a real one, with no generic outcome
forging in `drive`. An injection carries the error to inject (`Other`,
`AlreadyExists`, `CrossDevice`, or `Unsupported`) and one of three addresses:
- **protocol-scoped**: the protocol kind, the run occurrence in the armed
  scope, the action ordinal in that run, and the call index in that action.
  D4 enumerates these;
- **destination-scoped**: the protocol kind, a destination path and an
  action. The tap learns a run's destination from that run's rename call.
  The drafts driver's `AfterEffect` is "the `SyncDir` of the `Write` run whose
  destination is the manifest" (or the body);
- **scope-wide**: the k-th fallible call since the plan was installed, any
  primitive, inside or outside a protocol run. Service steps make calls
  outside any run: `create_dir_all_durable`, the target resolution in
  `write_body_file` (`canonicalize`, `symlink_metadata`),
  `remove_file_if_exists`, and manifest and body reads. D5's `BeforeEffect`
  is scope-wide call 0: every failure before the step's first effect looks
  the same to the window. D7's `At(k)` is scope-wide call k.

The covered primitives are every fallible `sys.rs` function the durable-write
shells and the draft service reach. Task 2.3 enumerates them from the code
and pins the list in a test, so a primitive added later without a tap call
fails it.

**A plan is armed only around the calls under test.** `filesystem::fixture`
and the public read boundary go through the same `sys.rs` primitives, so a
scope-wide count would also count the driver's own seeding, mtime setting and
abstraction reads, and an injection there would panic a fixture's `expect`.
So a drafts driver installs a step's plan just before that step's service
calls and drops its guard before it seeds, sets mtimes or abstracts; the
write-shell driver installs it around the one primitive call.

**Content.** `WriteContent` writes through the temp file's `Write`, not a
`sys.rs` primitive. A content fault comes from one of two places:
- for the stream variants, the test's own producer, which may fail;
- for the byte variants, the post-content tap point. The old
  `observe_temp_after_content_for_test` becomes `tap::content_written(path)`,
  whose error flows through `outcome_of`.

**Per-action call table (it replaces "exactly one fallible call").** Reading
the shell shows that the inherited claim is false for some actions:
- `ProbeMetadata` succeeds on a `NotFound` when the destination is absent and
  no metadata source is required;
- `ApplyMetadata` makes no call when there is no metadata source (a new
  file);
- `WriteContent` makes no `sys.rs` call;
- a move's `Copy` is a whole nested write;
- `RemoveTemp` makes one call and ignores its result.

So the tap counts the fallible calls per action, and the conformance test
pins the table: `(kind, action, source present) → count`. A shell change that
adds a call to an action fails that test, and the new call becomes a new
injection point, enumerated automatically. The checked contract is that **the
outcome fed back equals the result of the action's calls**:
- `Done` exactly when every call succeeded, with two named exceptions: the
  probe's `NotFound` on an absent, not-required destination is `Done`, and
  a `WriteContent` failure comes from the content stream (the producer or
  `tap::content_written`), not a backend call;
- otherwise the failing call's class;
- except `RemoveTemp`, whose outcome the core provably ignores.

**The existing hooks** are re-expressed as one-entry tap plans. The hooks
are:
- the five thread-locals;
- `TEMP_AFTER_CONTENT_OBSERVER`;
- the early-return cross-device and no-replace paths.

They are not all file-private. Two are re-exported through
`filesystem::write`: `fail_next_parent_sync_for_test` (`pub(crate)`) and
`fail_next_rename_cross_device_for_test` (`pub(in crate::services)`), and
`next_no_replace_unsupported_for_test` is called from `write.rs`. Their
callers outside `durable_write.rs` are the tests in `filesystem/write.rs`,
`draft_service/set_aside.rs`, `local_history_service.rs`,
`content_search/replace/tests.rs` and
`ui/sidebar/workspace_section/file_execution.rs`. The last is under the UI
prefix `crates/lushtext-core/src/ui/`, so editing it would make the commit
owe the three smokes. Therefore:
- every `*_for_test` setter keeps its name, visibility and signature, and
  keeps its semantics: armed on this thread until consumed, with no guard
  for the caller to hold. It installs a guardless one-entry plan that the
  tap retires when the injection fires;
- each plan reproduces its hook's counting exactly. The parent-sync hook
  counts `sync_parent_dir` calls, while `create_dir_all_durable` also syncs
  each created directory through `sync_dir`, so a plan that counted every
  `sync_dir_descriptor` call would fail a different call;
- every caller, the 40 `durable_write` tests and the tests in the five other
  files, keeps its assertions unchanged, and no file under a UI prefix is
  edited.

Three `durable_write` tests read a hook cell directly:
`the_parent_sync_fault_lets_the_given_syncs_succeed_first` (lines 1251 and
1255), `a_cross_directory_rename_whose_destination_sync_fails_is_after_rename`
(line 1284), and the observer in
`atomic_write_temp_with_new_bytes_is_not_wider_than_private_destination`
(lines 1592 and 1604). Those reads move to the tap's plan-state query
("armed", "consumed"), with no other change.

The cross-device hook now fails `sys::rename` inside the protocol instead of
returning before it. That is the path a real `EXDEV` takes. The Replace
hook registry (`content_search/replace/mod.rs`) is keyed by target across
threads, is used by its own tests, and stays.

**Crash by unwinding: where it is faithful.** A `TapCrash` panic is caught by
the driver with `catch_unwind`. Unwinding drops the temp `File` without
running `RemoveTemp`, which is the "process stopped here" disk. That holds
only while no `Drop` on the unwind path does I/O and no lock it poisons is
read afterwards. The audit for this revision:
- `TargetWriteGuard::drop` is in memory and poison-recovering: faithful;
- `draft_service::save_manifest` holds a process-global `Mutex` with
  `.expect("draft manifest write lock poisoned")`;
- `search_backup::acquire_journal_guard` maps poison to an error.

A crash inside either would poison state that a restarted *process* would not
have. Changing that poison handling is out of scope: poison is the right
production signal there. So **panic crashes are used only where the test
calls a durable-write primitive directly** (D4), and the draft driver
crashes only between model steps (D5). Task 2.6 re-runs this audit over the
final code.

A between-steps crash drops the driver's values, but process-wide service
state survives it, where a restarted process would start fresh: the launch
nonce and pid that name this process's temp files (so the startup sweep keeps
them as its own), `set_aside::PLACEMENTS`, and local history's
`obsolete_lineage_cleanup_failure`. Task 2.6 lists every such static the
draft service reaches and shows that none changes the abstracted state, or
resets it in the driver's `Crash`.

### D4. Write-shell conformance: exhaustive, against the shared disk model

The driver lives in `services/conformance/write_shell.rs` (test-utils). For
each case it does three things:
1. It seeds a tempdir destination, absent or holding `Old` bytes, with a
   generated mode.
2. It installs one injection, or none.
3. It calls the real primitive. The primitives are `atomic_replace`, the
   stream variant, `copy_durable`, `move_durable`, `rename_durable` (same or
   cross directory), and `rename_durable_no_replace`.

From the tap's `after` record, the driver steps the shared `Disk` model with
the same action and the recorded outcome, with one exception. The shell feeds
`RemoveTemp` back as `Done` whatever its `remove_file` returned, which is
sound only because the core ignores that outcome. The model, though, uses it:
a failed removal leaves its temp `linked` and sets `removal_failed`. So the
driver steps the model's `RemoveTemp` with the removal call's own result,
read from the tap, not with the fed-back `Done`. Otherwise an injected
removal failure would leave a real temp the model calls unlinked.

After each action it checks that:
- the outcome equals the result of the action's calls, as in D3;
- the real destination's bytes equal the model's `visible()`, with `Old` and
  `New` mapped to the case's byte strings;
- the real temp exists exactly while the model's temp is `linked`;
- the temp's mode is no wider than the destination's, when the destination
  existed (theorem 3, on the real filesystem);
- the call counts match the pinned table.

At the terminal action, the returned `Result` variant must equal the core's
`WriteClass` or `RenameClass`, and the move's `bool`. A move's `Copy` is a
nested write, checked against its own `Disk`, so the destination's bytes are
that model's. `MoveDisk` checks the source's presence and the copy's
durability. `RenameNamespace` checks which name the entry has. After an injected crash,
three more checks run:
- the destination is `Old` or `New`, equal to the model's
  `after_crash(false)`, the live branch;
- every leftover parses through `temp_name` as LushText's own;
- `filesystem::leftovers::is_sweepable_leftover` accepts it. The predicate is
  pure, so the test passes the real file's facts with a `LeftoverClock` that
  names another launch and a `now` past `STALE_LEFTOVER_AGE`. A leftover
  minted by the test carries the test's own launch, which the sweep
  deliberately keeps.

**Exhaustive, deterministic.** The protocols are finite: at most
`WRITE_STEPS` = 15 actions in a write, the finish included (a probe, eight
temp-name attempts, content, metadata, sync, rename and directory sync), and 3
plus the finish in a transfer. The enumeration is
(primitive × destination state × action ordinal × call index × injection).
The injection is one of:
- `Fail`;
- `AlreadyExists`, only at `CreateTemp`;
- `CrossDevice`, only at a rename;
- `CrashBefore` or `CrashAfter`.

That is a few thousand tiny cases, run in `make test` within 5 s. The temp-name
retry path is reached by failing up to `MAX_TEMP_NAME_ATTEMPTS` consecutive
`CreateTemp` calls with `AlreadyExists`, which is one case per count. A
proptest property over the same driver adds what enumeration does not vary:
modes, payload sizes, and two injections in one run
(`tests/properties/shell_conformance_write.rs`, 64 cases).

### D5. Draft-service conformance: one window, crashes between model steps

**Driver role.** At service level there is no GTK window, so the model's
window half (editors, tokens, known entries, holds) is **model-owned**. The
model decides what the window would do. The driver translates each enabled
model action into the service calls the drafts coordination makes at that
point:

| Model action | Service call(s) |
|---|---|
| Edit | none (a buffer change; mints a content id) |
| Register | `register_draft_entries`, keeping the returned `RegisteredDraft` |
| WriteBody | `write_draft(&registered, text_of(content))` |
| Commit | `update_manifest`, whose closure upserts the pass's accepted entries |
| Save | a durable write of the backing file (the mtime advances) |
| Discard | none (the deletion is queued; model-owned) |
| DeletionStep | `preserve_stale_draft_body` / `delete_draft_file` / `remove_manifest_entry`, per the core's `DeletionStep` |
| Inspect / ExecCleanup | `inspect_orphan_cleanup_against_persisted` / `execute_orphan_cleanup` (one worker in production; the model's Inspect is read-only, so the driver holds the plan between the two) |
| RestoreApply | `resolve_draft_restore` (which delegates to `resolve_file_draft_restore` for a file-backed entry), plus the disposition's preservation call |
| ExternalMtime | `fixture::set_modified` on the backing file, at least +1 s (manifest mtimes are whole seconds) |
| Crash | drop every driver-held value (tokens, plans, restore state, authority) |
| Startup | `load_restore_state_cancellable`, then adopt its authority and restore list; when trusted it already preserves and retires stale entries, which the model queues as a deletion at `Preserve` |

Task 4.1 checks the table against the real call sites in
`ui/window/drafts/` and `ui/window/session_restore/`. Where they differ, the
coordination is the source of truth, and the table here is corrected.

**Crashes happen between model steps.** The model's actions are atomic, so
its `Crash` falls between two of them, never between two service calls of one
step. That is where the driver crashes, by
dropping its in-memory values: the D3 audit forbids a panic inside a call
that holds the manifest lock. Faults inside a multi-write call are D7's
`At(k)`. A process killed inside a call is K5's, through the
`DraftBodyBeforeCommit` kill window and its siblings.

**Abstraction function** (the real state projected onto the model's disk and
authority):
- `entry[id].present` comes from `load_manifest`, and `entry[id].backing`
  from the entry's `original_mtime_secs`, mapped through the driver's
  mtime→version table;
- `body[id]` is the body file's text, mapped back to a content id through an
  injective content↔text table (`"c{content}\n"`, multibyte for some ids);
- `backing[id]` is the backing file's mtime version;
- `preserved` is the set of content ids among the set-aside bodies
  (`set_aside::list`/`read`) **only**. Production counts a stale body as
  preserved only once its set-aside copy exists: "a local-history snapshot
  alone can be pruned, so without the copy nothing counts as preserved"
  (`preserve_stale_draft_body`). A local-history snapshot is therefore not
  read into `preserved`. Counting it would also hide calibration mutant 1
  (D2), whose skipped set-aside copy the snapshot taken right after it would
  cover;
- `trusted` is the `DraftManifestAuthority` that the last service call
  returned and the driver holds.

Unknown files in the drafts directory fail the abstraction outright. Known
means the manifest, `{id}.draft` bodies, the `set-aside/` area, and a file
whose name parses through `filesystem::leftovers::parse_durable_temp_name` as
a LushText durable-write leftover. An injected `RemoveTemp` failure leaves
one of those, which the journal cannot see and the next foreign launch's
sweep removes. A leftover is counted and reported, not compared.

**Comparison.**
- `abstract(real).preserved ⊇ model.preserved`. An extra preserved copy is
  safe for every S property, so it is counted and reported, not a
  divergence.
- Every other field must be equal.
- The model's own S1–S4 methods must hold, as a cross-check that the lockstep
  stayed inside the proved envelope.

**Two windows: dropped.** The 1546d7e8 design added a second property, with
two window actors over one service. The service holds no per-window state:
the windows' state is the model's. Its process-wide manifest lock and
`TargetWriteGuard` serialize concurrent calls, but a sequential single-thread
driver never makes calls concurrent, so the property would re-run the
single-window check with a different model-side interleaving. The
coordinator's multi-window behaviour is proved by K8 and
`journal_invariants_hold_across_two_windows`, and its GTK half is in D10's
deferral.

### D6. Widening the journal fault domain: before, after-unconfirmed

`Fault { None, BeforeEffect, AfterEffect }` replaces `fault: bool` in
`journal_model.rs`.

- `BeforeEffect` is today's `fault = true`.
- `AfterEffect` exists only for the steps whose effect is one rename that
  the shell reports as a failure when its directory sync fails. It makes the
  effect **live but unconfirmed**, and the window treats the step as failed:
  no token, no `written`, no acceptance, and the dirty flag is kept. These
  steps are:
  - the manifest commits of Register, Commit and `DeletionStep(RemoveEntry)`;
  - WriteBody;
  - ExecCleanup's one manifest write for missing-body entries;
  - Startup's reconciliation commit, whose failure today is
    `commit_authority(.., false)`.

  On any other action the harnesses `kani::assume` it away.

**Every effect whose directory sync can fail or is absent is unconfirmed.**
Review (2026-11-18) found the first version of this decision incomplete. It
widened four steps and modelled the set-aside placement and the cleanup and
startup writes as durable, which is the "persists" assumption this decision
exists to remove. The complete table:

| Effect | Shell outcome when the sync fails or is absent | Model |
|---|---|---|
| Manifest commit (Register, Commit, RemoveEntry, ExecCleanup, Startup) | error (`AfterRename`) | `AfterEffect`: live, unconfirmed, step failed |
| Body write (WriteBody) | error (`AfterRename`) | `AfterEffect` |
| Body unlink (`DeletionStep(DeleteBody)`, `delete_draft_file`) and orphan delete (ExecCleanup, `execute_orphan_cleanup`) | success: `remove_file_if_exists` syncs no directory | always unconfirmed, with any fault value |
| Set-aside placement (`keep_copy` in Register's preservation, `DeletionStep(Preserve)` and RestoreApply's `PreserveCopy`; `move_in` in Startup's set-aside move), through `set_aside::place` | success: an `AfterRename` placement "counts as placed" | `None` fault, effect unconfirmed |

- **Unconfirmed is not persisted.** An after-rename failure means that the
  directory sync failed, so a crash may revert the rename. The model keeps:
  - one bit for the manifest file, with the entry array it would revert to
    (the last confirmed one). The manifest is one file, so its entries revert
    together, not one by one;
  - one bit per body file, covering a write and an unlink;
  - one bit per set-aside copy.
- `Crash` chooses, through `Choices::flag`, whether each unconfirmed effect
  survives or reverts. A reverted set-aside **move** returns the body to
  `drafts/`, because a rename takes effect at most once. It is never lost from
  both directories, which every Linux filesystem guarantees and
  `RenameNamespace` also assumes. A reverted set-aside **copy** is simply
  gone, while its source may already be durably unlinked. That is the path
  this widening most needs Kani to explore.
- Several unconfirmed manifest commits revert together to the last confirmed
  array. POSIX also allows an intermediate version, so this
  under-approximation is a stated domain restriction, recorded in the harness
  rustdoc, the spec delta and the programme record (`testing.md`, "Harness
  style").
- **Confirmation is a later successful sync of the same directory.** That is
  the `SyncDir` of a durable write or rename into it, or the source-directory
  sync of a rename out of it. The bodies and the manifest share `drafts/`
  (`app_data_layout`: `drafts/manifest.json`), so a successful manifest
  commit confirms the earlier body writes and unlinks there. Set-aside copies
  live in `drafts/set-aside/` and are confirmed only by a later successful
  placement there.
- A resurrected body whose entry is gone is the startup sweep's unregistered
  body, which is set aside and never deleted. Kani now checks that path
  instead of assuming that the unlink is durable.

Kani checks S1–S4 over the widened domain.

L1 is stated without faults, so it is not checked over `AfterEffect` itself.
Its three-action prefix admits faults, so L1 is re-checked with that prefix
widened, and the liveness shard re-runs. A prefix ending in an unconfirmed
effect may lengthen the fault-free path to clean. So, registered in advance:
- if L1 fails at `LIVENESS_STEPS` = 7 and the counterexample is a longer
  fault-free path that does reach clean, it is an environment finding. The
  bound is re-derived as the smallest `k` that holds, with its `k - 1`
  `should_panic` sibling, and recorded;
- an editor that never becomes clean is a core finding.

*Fallback, registered in advance.* If a shard breaks its margin (≤ 25 min,
≤ 12 GiB), the runbook order applies (`docs/runbooks/kani-shards.md`): split
the shard, then try a solver, then make a recorded bound reduction that names
the property it weakens. If a harness, most likely the two-window one, still
cannot carry the widened domain after that, it runs with the part it cannot
carry excluded: `AfterEffect`, or the unconfirmed set-aside bit. That
restriction is stated in the harness, in the spec and in the programme
record as a named residual. The single-window S1–S4 harness carries the
whole table, without exclusion.

*Findings.* A counterexample over the widened domain is a core or an
environment finding, handled as the formal-verification spec requires. Its
reachability from real errors is recorded, as K4 and K8 did. The set-aside
copy row is expected to be exercised. A Preserve whose set-aside sync fails
is counted as placed. The body is then unlinked, and the entry removed by a
manifest commit that makes the unlink durable. A crash then may lose the
only copy. If Kani confirms this, it is a core or shell finding with a real
error path (a failed directory `fsync` of `drafts/set-aside/`), fixed
failing-first under the divergence discipline. It is not a budget exclusion.

### D7. Step atomicity: `At(k)`

A generated fault may also be `At(k)`: the k-th fallible backend call of a
multi-write service step fails (D3's call addressing, counted across the
step). The real post-step state must abstract to the model's `BeforeEffect`
or `AfterEffect` outcome for that step, under D5's comparison. Any other
state is an **environment finding (step atomicity)**, resolved in one of two
ways, with the choice recorded in the programme record:
- the model gains the intermediate state, and the journal shards are re-run;
- or the shell is reordered behind a failing-first test, so that the
  intermediate state cannot be observed.

The driver first enables `BeforeEffect`, `Crash` and `Startup`, and it must
pass on the real service before `AfterEffect` and `At(k)` are enabled. Each
divergence is triaged before the domain grows.

### D8. Replace shells: tap only, and the three survivors

Lockstep against the Replace model is **deferred**:
- That model lives in `replace/protocol/kani_proofs.rs` (339 lines), the
  pull-request-gated shard `core-replace-protocol`. Moving it changes that
  shard's inputs and owes a CI budget figure from the change's own pull
  request.
- Replace's crash windows are already covered by K5's four replace kill
  windows, and by the property `replace_journal_recovery.rs`.
- Its durable writes are the write shell that D4 checks.

The tap still serves Replace in two ways:
- the typed backend faults reach its nested writes;
- the before-action hook kills the three `process_file` survivors that
  altitude's review recorded.

Two tests kill the three mutants. Each is shown failing first against its
mutants, which survive today:
- **Pass-A facts check, 649 (`||` to `&&` in `done.source_len !=
  facts.byte_size || !facts_unchanged(facts, path)`).** Before `WriteBody`,
  rewrite the target with **the same bytes** and a new mtime. The bytes must
  stay the same. A content change at a matched line makes pass A's dry run
  stop at `StaleLine` (`ChangedSinceSearch`) before the facts check, and then
  the mutant and the original agree. With equal sizes, only the original's
  `!facts_unchanged` fires, so the file must be skipped under pass A's
  `ChangedDuringReplace`, not pass B's reason.
- **Cancelled test, 689 (`||` to `&&`) and 691 (delete `!` before
  `verdict.defer`).** Before `WriteBody`, raise the cancel flag and make the
  body's write fail before its producer runs. Either replace the bodies
  directory with a file, or inject a tap `Fail` at the body write's
  `ProbeMetadata`. The stream then never observes the cancel
  (`verdict.cancelled` stays false), so only the raised flag can make the
  outcome `Cancelled`. The original reports `Cancelled`, and both mutants
  report `Failed`, so one test asserting the `Cancelled` terminal kills both.
  A cancel raised before `AcquireTarget` would be seen there, and one raised
  while the producer runs sets `verdict.cancelled`. Neither reaches the
  mutated clause.

The line anchors (649, 689, 691) are read off the current `apply.rs`. The
mutation-testing doc records the three as killed.

### D9. Kani, proof strength, axioms, timing

- **Shards touched** (all `gate="scheduled"`):
  - `core-journal-and-write` (12.0 min, 2.6 GiB): the write-model move and
    the journal move plus widening;
  - `core-journal-liveness` (15.2 min, 4.8 GiB);
  - `core-second-writer` (7.4 min, 2.7 GiB; seven harnesses, one of them
    the journal's `a_second_writer_breaks_the_journal_invariants`, a
    `should_panic` that the widened domain keeps panicking);
  - `core-multi-window` (8.9 min, 2.7 GiB).

  Each runs **once, after the final code, detached**
  (`setsid nohup scripts/heavy.sh --envelope 16G -- make kani
  KANI_SHARD=<shard>`). The baseline is the `kani-shards.py list` figures
  above, not a fresh run before the change. The pull-request shard
  `core-replace-protocol` is not touched (D8) and is left to CI.
- **Budgets.** The four records become
  `PENDING DISPATCH (owed by verify-shell-conformance-against-proven-cores)`
  with their local figures. At the **end of stage 3**, **two** `kani.yml`
  dispatches run on the pushed final code, as #103 did. A scheduled shard
  runs only weekly or on dispatch, and the archive commit's hook fails on a
  pending record, so the next scheduled run cannot be relied on. Stage 4 does
  not watch them. The archive records the larger figure of the two runs in
  each record.
- **Proof strength is owed for `journal_core` and `write_protocol`.**
  `verification-runs.md` asks for it when a change changes harnesses. The
  journal harnesses assert over a widened domain. The transfer harnesses'
  environments are rewritten into `MoveDisk` and `RenameNamespace` (D1). A
  passing verdict cannot show that a moved model was not weakened, and a kill
  count at least the recorded one can.
  - `write_protocol` (34 mutants, every harness ≤ 6 s) runs whole.
  - `journal_core` (51 mutants; four harnesses near 300 s each before the
    widening) runs with its Kani arm sharded (`PROOF_STRENGTH_SHARD=k/n`).

  Both run once on the final code, detached
  (`PROOF_STRENGTH_MODULE=<module path>`). Their kill counts are compared
  with the programme record's last figures, and their survivors are triaged
  there.
- **GTK axioms: none.** The change relies on no toolkit behaviour (no
  GTK-facing code; D10 drops the widget scenarios).
- **Timing: no timing verdict.** No hot path changes. Shipping builds compile
  the tap to empty inline functions. `test-utils` builds, which include every
  benchmark command, carry the *uninstalled* tap: one thread-local check per
  protocol action and per backend call. Next to an `fsync`, that cost is noise,
  and the benches measure no durable-write throughput verdict.

### D10. Dropped groups, with their deferrals

- **GTK drafts coordination scenarios.** These were up to eight widget
  scenarios replaying model traces. Their cost:
  - widget-lane time;
  - the three smokes, because `crates/lushtext/tests/widget/` is a UI prefix;
  - probably new `DraftEvidence` fields under `ui/`, with their matrix-row and
    smoke obligations.

  The paths they would replay already have single-scenario widget tests: the
  L1 path, the Unavailable restore, the failed set-aside copy, the Data-page
  open, and the discard during a write. With D5 in place, the remaining risk
  is that the coordination's calls differ from the D5 table, and task 4.1
  checks exactly that by reading. Deferral entry: "GTK drafts coordination
  conformance: reopen when a drafts-coordination defect escapes D5's table
  check, or when the evidence surface gains per-id facts for another reason."
- **Fuzz conformance mode.** It would mean a top-bit selector in
  `operation_script`, `fuzzing` implying `test-utils`, and new seeds. For the
  write shell, enumeration is exhaustive. For drafts, proptest shrinks
  semantically while libFuzzer shrinks bytes, and the mode would run outside
  the pull-request gate. Dropped, with a deferral entry, and the
  `structured-operation-fuzzing` delta is removed.
- **Two-window service property.** See D5.
- **Replace lockstep.** See D8.

### D11. Placement, gating and the release check

- The drivers live in `lushtext-core` at `src/services/conformance/`:
  - `mod.rs` and `ops.rs`;
  - `write_shell.rs` and `draft_service.rs`.

  They are gated `#[cfg(any(test, feature = "test-utils"))]`, because they
  reach crate-private models and the tap. They name neither `proptest` nor
  `tempfile`, which are dev-dependencies (D2): each driver takes its op slice
  and a root directory its caller owns and removes. The property modules
  under `tests/properties/` hold the strategies and the `TempDir`s, and are
  otherwise thin `proptest!` wrappers over the drivers.
- These modules, the two model modules and `backend_tap.rs` are added to the
  `.cargo/mutants.toml` exclusions, and the Sonar scope is reviewed for
  them.
- The drivers seed the tempdir and set mtimes through
  `services::filesystem::fixture`, and read through the public boundary. They
  make no raw `std::fs` calls.
- **Release check.** The crash-recovery smoke's marker guard
  (`scripts/crash-recovery-smoke-driver.py`, the one list G6 left for this
  change) gains the tap's crash-payload text and a unique tap string. It then
  fails if the ordinary binary, or `target/release/lushtext`, contains any of
  them. `cargo check -p lushtext --bins --locked` with default features
  already fails if production names a gated item.

### D12. Budgets

| Lane | Setting | Budget |
|---|---|---|
| `make test` | D4 exhaustive enumeration, D3 call table | ≤ 5 s together |
| `make test-prop` | D2 reach test (model only, fixed seed) | ≤ 2 s |
| `make test-prop` | write-shell property, 64 cases | ≤ 10 s |
| `make test-prop` | drafts property, 64 cases × ≤ 16 ops | ≤ 30 s |
| `make test-prop-deep` | 512 cases, ≤ 32 ops | informational |
| `make kani` | the four journal and write shards | ≤ 25 min, ≤ 12 GiB each |

The budgets are measured locally through `heavy.sh`, with the load recorded.
The CI figure of the property job is read once, at the programme's CI check,
and never waited for. If a budget breaks, the script bound is cut first, then
the case count. The job timeout is never raised. The figures are recorded in
`docs/property-testing.md`.

## Measured baseline at 761866ff

| Fact | Value | Source |
|---|---|---|
| Shells through `drive` | 6, in 4 files | `protocol_drive.rs` `SHELL_SOURCES` |
| Longest write / transfer | `WRITE_STEPS` = 15 actions, `Finish` included (`MAX_TEMP_NAME_ATTEMPTS` = 8) / 3 + `Finish` | `write_protocol.rs`, `kani_proofs.rs` `WRITE_STEPS`, `TRANSFER_STEPS` |
| Durable-write unit tests | 40 | `grep -c '#\[test\]' durable_write.rs` |
| Ad hoc durable-write hooks | 5 thread-locals + 1 process-global observer; 3 setters reached from 5 other files, one under `ui/` | `durable_write.rs`, `filesystem/write.rs` |
| Property config | 64 cases, 1,024 shrink iterations, 10 s per case, no shrink-time cap | `tests/properties/support.rs` |
| `proptest`, `tempfile` | dev-dependencies of `lushtext-core` | `crates/lushtext-core/Cargo.toml` |
| Journal model | 1,721 lines, 7 model-level choice sites | `draft_service/kani_proofs.rs` |
| Write model | 481 lines, 4 choice sites | `write_protocol/kani_proofs.rs` |
| Stateright port | 752 lines (`env.rs`) | `formal/evaluation/stateright` |
| E4 uniform reach | 2,000 traces → 6 body writes, 0 commits; a mutant caught after 1,246 traces | `formal-verification-quint-vs-tlaplus.md` |
| Shards touched | `core-journal-and-write` 12.0 min / 2.6 GiB; `core-journal-liveness` 15.2 / 4.8; `core-second-writer` 7.4 / 2.7; `core-multi-window` 8.9 / 2.7 | `scripts/kani-shards.py list` |
| Locks a panic would poison | the draft manifest write lock (`expect`), the replace journal guard (an error) | `draft_service.rs:416`, `search_backup.rs:265` |

## Risks / Trade-offs

- [The model move changes what Kani proves.] → The move is mechanical: each
  choice becomes a call-site argument or a `KaniChoices` call, which is the
  same `kani::any()` and `kani::assume`. The final shard runs must give
  today's verdicts on the unchanged properties. A wall time more than 20 %
  off its record is investigated before it is accepted.
- [The widened domain blows a budget.] → The D6 fallback order is
  pre-registered, and any bound reduction or exclusion is named in the spec.
- [The service driver drifts from the GTK coordination.] → The window half is
  model-owned, which is proved. Task 4.1 checks the table against the real
  call sites. The GTK half is a recorded deferral.
- [Generated scripts are vacuous.] → The D2 reach floors and the two-mutant
  calibration.
- [Unwinding is mistaken for a kill.] → The D3 audit, and panic crashes only
  where no lock or I/O `Drop` is on the path.
- [A flake on real filesystems.] → Payloads are tiny, the driver is
  single-threaded, and there are no timers. Any flake is a blocker under the
  pre-existing-blockers rule.
- [Mtime granularity.] → Distinct whole-second mtimes are set explicitly.
- [Tap code ships.] → The gate, the default-feature binary check, and the
  extended marker guard.

## Migration Plan

Nothing is migrated: no persisted format changes. Everything added is
test-gated, so a rollback is a revert. A production fix that a finding forces
ships with its own failing-first test, and can be reverted separately.

## Open Questions

- ~~Can local-history preservation be read cheaply enough at every step?~~
  Resolved in review: the abstraction does not read local history at all,
  because production does not count a snapshot as preservation (D5).
- Does an `AfterEffect` counterexample, if one is found, have a real-error
  path? That is decided per finding and recorded, following the K4 and K8
  precedent.
