## Why

Kani proves the pure cores (`journal_core` inside its journal model, and
`WriteProtocol`, `MoveProtocol`, `RenameProtocol` inside their disk model). It
does not prove the imperative code that drives them:

- the real `draft_service` over a real data directory;
- the durable-write shell loop in `services/durable_write.rs` running against
  the real filesystem through `services/filesystem/sys.rs`;
- the GTK drafts coordination in `ui/window/drafts/`;
- the seam between the journal and the durable write. The journal model
  treats every I/O fault as "nothing happened". The write shell can also fail
  **after** its rename: the bytes land, but an error is reported.

Each proof holds only if the shell really does what the model says it does,
and nothing checks that today. A few hand-picked fault-injection unit tests
cover the write shell, and single-scenario widget tests cover the journal.

This is candidate N11 of `docs/next/formal-verification-next.md`, row 9 of its
change table. It runs after `harden-kani-lane-and-draft-token` (the
`test-utils` gating and the harness-module rule it builds on) and after
`verify-multi-window-draft-journal` (the window-actor and process-actor model
it reuses).

## What Changes

- **Share each Kani environment model with the tests.** The journal model is
  moved out of `services/draft_service/kani_proofs.rs` into a shared model
  module, and so is the write-protocol disk model from
  `services/filesystem/write_protocol/kani_proofs.rs`. Both are compiled under
  `cfg(any(kani, test, feature = "test-utils"))`, and each reads its
  nondeterminism from one small choice source: Kani gets `kani::any()`, a
  conformance test gets its generated script. The harnesses keep proving the
  same properties at the same bounds, which is re-measured. After this, the
  oracle is exactly the code Kani proved. It is not a second specification.
- **Conformance of the durable-write shell (b).** A new test-only backend tap
  in the shell does three jobs:
  - it records every action and the outcome the shell feeds back;
  - it counts the fallible backend calls each action makes;
  - it can inject a fault (`Failed`, `AlreadyExists`) at any action, or a
    "crash", which stops the shell mid-protocol.

  The existing `FAIL_NEXT_*` test hooks become thin wrappers over the tap.
  Generated fault scripts drive the real `atomic_write_*`, `copy_durable`,
  `move_durable`, and `rename_durable` over a tempdir. The disk model and the
  pure protocol run in lockstep with them. After every action, three things
  are checked:
  - the action sequence matches the core's;
  - the shell made exactly one fallible backend call for the action and fed
    the outcome back unchanged;
  - the real disk equals the model's live state.

  After a crash, the destination must be one of the states the model's crash
  semantics allow, and any leftover temp file must be one the startup sweep
  recognises as LushText's own.
- **Conformance of the draft service (a).** Generated operation sequences
  drive the real `draft_service` over a tempdir data directory, with faults
  injected through the same tap. The operations are Edit, Register, WriteBody,
  Commit, Save, Discard, DeletionStep, Inspect, ExecCleanup, RestoreApply,
  ExternalMtime, Crash, and Startup; restart means drop all in-memory state and
  reload from disk. The journal model steps in lockstep with the service.
  After every step, an abstraction of the real disk and of the service's
  returned state must equal the model's state. The disk abstraction covers
  manifest entries, bodies, backing mtimes, set-aside copies, and local-history
  preservation. The returned state covers the registration token, manifest
  authority, restore dispositions, and cleanup plans. A second property runs
  two in-process window drivers over one service, which checks the
  process-wide lock and guard against the multi-window model.
- **Close the fault-domain seam.** The journal model's fault becomes three
  cases: none, before the effect, or after the effect (persisted but reported
  as failed). The Kani harnesses are re-run over it. If S1–S4 or L1 fail, that
  counterexample is a finding in the core, handled as below.
- **GTK coordination (c), at coarse granularity.** A small scripted set of at
  most eight multi-step widget scenarios. Each is derived from a known model
  trace: the L1 k = 7 path, the three fixed loss paths, a restore that fails
  its set-aside copy, and a multi-window cleanup. Each scenario runs the model
  alongside and compares `DraftEvidence` plus the disk abstraction after every
  settled step. It is not randomised, because the widget lane cannot afford
  generated GTK sequences.
- **Runners, shrinking, and budgets.** The runner is a state-machine driver on
  plain `proptest`, which is already a dependency: a `Vec<Op>` script, with
  shrinking by proptest's own vector and element shrinking. That is sound
  because every model step is total. **No new dependency**:
  `proptest-state-machine` was considered and rejected. The existing
  `operation_script` fuzz target gains a conformance mode that decodes bytes
  into the same `Op` vocabulary, for coverage-guided deep runs. The
  pull-request property lane keeps a measured budget. Deep runs use
  `make test-prop-deep` and the fuzz lanes.
- **Divergence discipline.**
  - A divergence caused by the shell is committed first as a failing
    deterministic regression test, then fixed.
  - A divergence caused by the model's environment is fixed in the model, and
    the Kani lane is re-run.
  - A divergence that reveals the core itself is wrong invalidates the proof.
    The programme record marks the property as invalidated, the harness fails
    first, and then the core is fixed and re-proved.
- **Documentation sync.** Update the programme record (new phase 6: shell
  conformance), the next-candidates doc (N11 row), the Testing section of
  AGENTS.md, the Property Testing, Fuzzing, and Formal Verification sections
  of `.agents/rules/build.md`, `docs/end-user-coverage.md`,
  `docs/property-testing.md`, and `docs/fuzzing.md`.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `formal-verification-kani`: ADDED "Imperative shells are conformance-tested
  against their proven cores", and "Verification environment models are
  shared between Kani and conformance tests and cover every outcome the real
  shell can produce".
- `durable-file-write-contract`: ADDED "Durable-write fault injection is
  per-action, observes the shell, and is absent from shipping builds".
- `draft-session-recovery`: ADDED "The draft service and drafts coordination
  conform to the verified journal model".
- `property-based-testing`: ADDED "Model-based conformance properties", which
  covers generated operation sequences over tiny tempdirs, a total model so
  shrinking needs no preconditions, a shrink-time budget, and regression
  promotion.
- `structured-operation-fuzzing`: ADDED "Operation scripts can drive shell
  conformance".

## Impact

- **Code (test-only, or compiled out of shipping builds):**
  - `services/draft_service/journal_model.rs` (new, the model moved out of
    `kani_proofs.rs`) and `services/filesystem/write_protocol/disk_model.rs`
    (new);
  - a backend tap in `services/durable_write.rs`, with a fallible-call counter
    in `services/filesystem/sys.rs`, gated
    `cfg(any(test, feature = "test-utils"))`;
  - a conformance driver module in `lushtext-core` behind `test-utils`;
  - new property modules under `crates/lushtext-core/tests/properties/`;
  - a conformance mode in `lushtext_core::fuzzing` and `operation_script`;
  - a new widget test module in `crates/lushtext/tests/widget/`.
- **Production code:** it changes only where a divergence is found, and each
  such fix comes behind a failing-first test. No persisted format, GSettings
  key, action, or automation field changes. A release binary must contain no
  tap code, which is checked the same way as the kill points.
- **Verification lanes:**
  - `make test-prop` gains conformance properties within a recorded budget;
  - `make kani` re-runs the journal and write harnesses over the shared models
    with the widened fault domain, and the shard budgets are re-measured under
    the rules of change 1;
  - `make fuzz-operation-smoke` and `make fuzz-corpus-replay` gain conformance
    seeds;
  - `make test-widget` gains at most eight scenarios.
- **Dependencies:** none added. `cargo deny`, cargo-hakari, and
  `cargo-sources.json` are unchanged, and this is verified.
- **Configuration:** `.cargo/mutants.toml` excludes the two shared model
  modules, which are verification code, as change 1 does for `kani_proofs.rs`.
  The Sonar scope is reviewed for the new files.
