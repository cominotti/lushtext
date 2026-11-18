## Why

Kani proves two pure cores, each inside an environment model:
- `journal_core`, inside the journal model;
- `WriteProtocol`, `MoveProtocol` and `RenameProtocol`, inside their disk
  model.

It does not prove the imperative code that drives them: the durable-write
shells running against the real filesystem, and the real `draft_service` over
a real data directory. Each proof holds only if the shell really does what the
model says it does, and nothing checks that today. There are 40 durable-write
unit tests with six ad hoc fault hooks, and single-scenario draft tests.

`apply-verification-altitude-redesigns` (PR #117, merged 8d4f5d7b, archived
761866ff) put all six proven-core shells on one loop, `protocol_drive::drive`,
and pinned that no shell steps its core elsewhere. So the **action sequence**
a shell runs is now the core's by construction. What is still unchecked:
- whether the **outcome** a shell feeds back reflects what the backend did;
- whether each **action's effect** on the disk is the one the model assumes;
- whether the **service** built on the shells keeps the journal model's state.

`drive` gives one place to observe every action and inject a crash. Its
archived design says that it cannot make a typed fault (`Fail`,
`AlreadyExists`) faithful, and leaves that to this change.

Two concrete gaps make this more than "the shells are untested":

1. **The fault seam.** A draft body write or manifest commit can end
   `AfterRename`: the bytes are visible, the directory sync failed, and the
   call reports an error. The journal model's `fault` means "nothing
   happened", so S1–S4 were never checked over that outcome.
2. **Step atomicity.** Some service calls make more than one durable write. A
   fault between those writes reaches a state the model never visits.

This is candidate N11 of `docs/next/formal-verification-next.md`, row 9 of its
change table. The 2026-09-24 evaluation (E1) found a real set-aside defect
exactly this way: the real service driven against the Rust environment model.
The design was re-derived against 761866ff on 2026-11-18 (design, "Revision
against 761866ff"). Three groups were dropped, with recorded deferrals: the
GTK coordination scenarios, the fuzz conformance mode, and the two-window
service property. The Replace shells join through the tap, and the change
kills the three mutation survivors it owes them.

## What Changes

- **Share each Kani environment model with the tests.**
  - The write disk model moves to `write_protocol/disk_model.rs`, with its
    choices as call-site arguments.
  - The journal model moves to `draft_service/journal_model.rs`, behind a
    small `Choices` trait.
  - Both are gated `cfg(any(kani, test, feature = "test-utils"))`.
  - The harnesses keep proving the same properties at the same bounds. The
    move is its own commit, before anything else.
- **One conformance tap.**
  - In `drive`, it records every action and outcome by protocol kind (a new
    `Protocol::KIND`). It can run a closure or crash, by unwinding, before or
    after any action.
  - In `sys.rs`, every fallible backend primitive can return an injected
    error. The shell then builds its outcome and `last_error` from the real
    error path, so `Fail`, `AlreadyExists`, `EXDEV` and `Unsupported` are as
    faithful as real ones.
  - The six existing durable-write hooks become one-entry tap plans.
  - Shipping builds compile the tap to nothing, and the crash smoke's marker
    guard checks that.
- **Write-shell conformance.** A deterministic, exhaustive enumeration
  (primitive × destination state × action × call × injection) runs the real
  primitives over a tempdir, with the shared disk model stepped from the tap
  record. After every action it checks four things:
  - the outcome fed back equals the result of the action's backend calls,
    with the per-action call counts pinned;
  - the destination bytes equal the model's live state;
  - the temp file exists exactly while the model says it does;
  - the temp mode is never wider.

  After a crash, the destination must be the old or the new bytes, and any
  leftover must be one the startup sweep recognises. A property adds generated
  modes, payloads and double faults.
- **Draft-service conformance**, for one window.
  - Generated, enabled-indexed operation scripts drive the real
    `draft_service` over a tempdir, in lockstep with the journal model, and
    compare an abstraction of disk and returned state after every step.
  - Crashes happen between service calls, because a panic inside one would
    poison the manifest lock.
  - Reach floors and a two-mutant calibration keep the property from passing
    vacuously.
- **Widen the journal fault domain.** `Fault { None, BeforeEffect,
  AfterEffect }`. `AfterEffect` makes the effect live but unconfirmed: a later
  crash may revert it, and a later directory sync confirms it. The unsynced
  body unlink of a deletion is unconfirmed in the same way. S1–S4 and L1 are
  re-checked over it.
- **Step atomicity.** An `At(k)` fault fails one call inside a multi-write
  step. The result must match a model outcome. Preservation is compared as a
  superset, because an extra preserved copy is safe.
- **Replace shells: tap only.** The before-action hook kills the three
  `process_file` mutation survivors recorded for this change. Lockstep against
  the Replace model is deferred.
- **Divergence discipline.** Every divergence is classified in the programme
  record and handled by its class:
  - **shell:** a failing-first regression test, then the fix;
  - **environment:** a model fix and the Kani re-run;
  - **core:** the property is marked invalidated, a harness fails first, and
    the property is re-proved after the fix.
- **Documentation sync.**
  - The programme record gains phase 6 and the deferral entries.
  - The N11 row is marked done.
  - Also updated: AGENTS.md Testing, `build.md` (Property Testing, Formal
    Verification, fault seams), `testing.md` (fault seams),
    `docs/property-testing.md`, `docs/end-user-coverage.md`, and
    `docs/mutation-testing.md`.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `formal-verification-kani`: ADDED requirements:
  - "Imperative shells are conformance-tested against their proven cores";
  - "Conformance divergences are classified and recorded";
  - "Verification environment models are shared between Kani and
    conformance tests";
  - "The journal model distinguishes a fault before an effect from one after
    it".
- `durable-file-write-contract`: ADDED requirements:
  - "The shared protocol driver hosts one test-only conformance tap";
  - "Durable-write faults are injected at the backend call";
  - "The durable-write shell conforms to its disk model".
- `draft-session-recovery`: ADDED "The draft service conforms to the verified
  journal model".
- `property-based-testing`: ADDED "Model-based conformance properties".
- `crash-restart-recovery-coverage`: MODIFIED "The ordinary binary is checked
  free of kill points", which also checks for the conformance tap's markers.

## Impact

- **Code (test-only, or compiled out of shipping builds):**
  - `services/filesystem/write_protocol/disk_model.rs` and
    `services/draft_service/journal_model.rs`, both new and moved out of their
    `kani_proofs.rs`;
  - `services/filesystem/backend_tap.rs`, with its calls in
    `protocol_drive::drive` and in the fallible `sys.rs` primitives;
  - `Protocol::KIND`, and `Debug` derives where needed;
  - `services/conformance/{mod,ops,write_shell,draft_service}.rs`;
  - `tests/properties/shell_conformance_{write,drafts}.rs`;
  - the three Replace survivor tests in `content_search/replace/tests.rs`.
- **Production code:** changes only where a finding requires, and each such
  fix comes behind a failing-first test. No persisted format, GSettings key,
  action, automation field or UI file changes. No UI prefix is touched, so
  the three smokes are not owed.
- **Verification lanes:**
  - `make test` gains the exhaustive write-shell enumeration and the reach
    test;
  - `make test-prop` gains two properties, within recorded budgets;
  - the scheduled Kani shards `core-journal-and-write`,
    `core-journal-liveness`, `core-second-writer` and `core-multi-window` each
    run once, detached, on the final code;
  - their budgets become `PENDING DISPATCH (owed by this change)` until a
    `kani.yml` dispatch at the end of implementation;
  - proof strength is owed, with its Kani arm sharded;
  - no GTK axiom, and no timing verdict: no hot path changes, and the
    uninstalled tap costs one thread-local check per action in `test-utils`
    builds only.
- **Dependencies:** none added. `cargo deny`, cargo-hakari and
  `cargo-sources.json` are unchanged, and that is verified.
- **Configuration:**
  - `.cargo/mutants.toml` excludes the model, tap and conformance modules;
  - `scripts/kani-shards.py` lists the two model modules in
    `NOT_ORACLE_MODULES`;
  - the Sonar scope is reviewed.
