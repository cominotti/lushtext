# Formal Verification — Quint vs TLA+, Measured

Status: **decision record** of the OpenSpec change
`evaluate-quint-and-tlaplus-empirically`, written on 2026-09-24. It tests the
tool judgements recorded on 2026-09-23 in
[`formal-verification-next.md`](./formal-verification-next.md) ("Tool
criteria") against measurements on this project's own protocols. The
programme record [`formal-verification.md`](./formal-verification.md) keeps
phases, results, axioms, and deferrals. This file keeps the comparison and its
decision.

The models live in `formal/evaluation/`: Quint, TLA+, a `stateright` package,
and trace validation. They are **disposable evaluation models, not maintained
properties** (maintainer decision D7, option A, 2026-09-23):

- no build, test, lint, policy, audit, or CI gate reads that directory;
- no claim elsewhere rests on these models alone;
- `make formal-evaluation` reruns them locally (§9).

**Decision (§8):**

- **Kani stays the only maintained formal tool.** No external tool gains a CI
  lane, a dependency, or a maintained model.
- **TLA+ with TLC** is the recorded choice for the disposable N6 design
  sketch, if the N6 trigger ever fires.
- **Quint is not adopted**, and **it cannot replace Kani**.
- The most valuable results are Rust-native:
  - A Rust abstract-disk model, driven against the real `draft_service`,
    found a real set-aside defect that Kani's model cannot see (§7, E1). That
    approach is carried into N11.
  - `stateright`, on the same model and the real `journal_core`, beat Kani
    7–10× on T2. A follow-up change weighs it for the journal's interleaving
    checks.

## 1. Tool versions and machine

Versions were resolved on 2026-09-24 from the npm registry, the GitHub release
pages, and crates.io. Each checksum is the SHA-256 of the downloaded artefact.

| Tool | Version | Source | Notes |
|---|---|---|---|
| Quint | 0.32.0 (2026-03-31) | npm `@informalsystems/quint` | latest stable; Rust simulator `rust-evaluator-v0.6.0` |
| Quint language server | 0.19.0 | npm `@informalsystems/quint-language-server` | latest |
| Apalache, standalone on TLA+ | 0.62.2 (2026-08-26) | GitHub, `765f6105…c10e2` | latest stable |
| Apalache, driven by Quint | 0.58.3 (2026-07-09) | GitHub, `ba622db9…d0a5` | newest release Quint 0.32.0 can drive (§6.1) |
| TLC, SANY, PlusCal | `tla2tools.jar` 1.7.4, TLC 2.19 (2024-08-08) | GitHub, `936a2620…0e88` | latest stable; 1.8.0 "Clarke" was a pre-release published 2026-09-23 |
| TLC under `quint verify --backend tlc` | the TLC bundled in `apalache.jar` 0.58.3 | — | an unversioned build that prints the current date as its version; Quint runs it with `-deadlock` and `-Xmx8G` unless `--tlc-config` overrides |
| TLAPS | 1.5.0 (tag `202210041448`) | GitHub, `ebb7a3f2…e08a` | latest stable; 1.6.0-pre is a rolling pre-release |
| Quint Connect | 0.1.2 (2026-05-25) | crates.io `quint-connect` | latest |
| stateright | 0.31.0 | crates.io | latest |
| Kani (the baseline) | 0.68.0, CBMC 6.11.0 | the programme record | not re-run here |

**The machine.**

- CPU and memory: Intel Core Ultra 9 285K (24 cores), 93 GiB RAM, about
  46 GiB of it free during the runs.
- Environment: a Fedora 44 toolbox with OpenJDK 25.0.2 and Node 24.15.0.
- Kani's figures come from the same toolbox (programme record, phases 2, 4,
  and 5).

**Install and CI cost.**

- A cold `scripts/formal-evaluation.sh install` took **21.7 s and 426 MB**:
  Quint 41 MB, two Apalache releases 380 MB, TLC 2.2 MB, Quint's Rust
  evaluator 2.6 MB.
- Prerequisites: Node 20+ for Quint; Java 17+ for TLC, Apalache, and
  `quint verify`.
- TLAPS 1.5.0 is a 152 MB self-extracting installer. It built its Isabelle
  theories in 27 s.
- The `stateright` package's cold release build took 73 s with 6 jobs,
  because it compiles the whole `lushtext-core` GTK tree to reach
  `journal_core`.
- Every tool installs well inside a 30-minute CI job. The checks are another
  matter; see T2.

## 2. The fixed protocol

The design (D3) fixes three targets. Every model re-expresses its Rust source
of truth arm for arm and cites it in its header. A model counts only after it
has reproduced Kani's known answers.

- **T1, calibration: the durable write.**
  - Source: `WriteProtocol`, `MoveProtocol`, and `RenameProtocol` in
    `crates/lushtext-core/src/services/filesystem/write_protocol.rs`, plus the
    model disk of `write_protocol/kani_proofs.rs` (POSIX axioms A1–A5).
  - Known answers: crash atomicity and classification soundness hold, and a
    shell that answers `SyncTemp` without syncing tears the destination.
- **T2, scale: the draft journal.**
  - Source: every `journal_core.rs` decision the harness calls, plus the
    harness environment of `draft_service/kani_proofs.rs`: 3 ids, 3 edits,
    13 actions, and a possible fault on every step.
  - Known answers with one process: S1–S4 hold. This is the control.
  - Known answers with two processes (K8): at 6 actions only "a body written
    without an entry" fails, and within 8 actions S1 fails too.
- **T3, the real use case: a design sketch of the N6 data-directory lock.** It
  runs over the T2 journal and needs safety and liveness under fairness.
  There is no known answer.

Every run went through `scripts/formal-evaluation.sh measure`, which records:

- wall time;
- the **peak RSS of the whole process tree** (Quint spawns a Java server);
- the distinct-state count;
- the command and the full output, under `build/formal-evaluation/runs/<run>/`.

It kills a run at 30 minutes (the CI job cap) or at a memory ceiling, and
records the ceiling as the result. Most runs used `FORMAL_EVAL_MEM_MB=36000`
(a 27 GiB JVM heap; Apalache through `JVM_ARGS=-Xmx27g`). The exceptions are
`t2-tlc-k8-6-w1` (an 18 GiB heap), `t2-tlc-k8-6-w24` (a 40 GiB heap), and
`t2-tlc-simulate-k8-8` (a 10-minute timeout). The script's default ceiling is
24 GiB. One checker ran at a time.

The recorded wall times come from a sampler that polled once a second after a
0.2 s start, so each is an upper bound quantized to about 1.05 s: a figure of
1.2 s means "at most 1.2 s". The runner now polls every 0.1 s; the recorded
figures predate that change.

- TLC used one worker in T1 and in `t2-tlc-k8-6-w1`. Every T2 row marked
  "w24", every T3 check (both tools), and E9 used all 24 cores.
- `stateright` uses 8 threads.
- Kani's solver is single-threaded.

## 3. T1 — the durable write (calibration)

**Both tools reproduced both known answers.** Every check of this finite
protocol was exhaustive, as Kani's is.

| Tool | Check | Result | Wall | Peak tree RSS | States |
|---|---|---|---|---|---|
| Kani | the three write harnesses | proved; the mutant fails | 2.1–3.5 s each | — | — |
| TLC (PlusCal, hand-written) | `Safety`: crash atomicity, mode non-widening, metadata-before-sync, classification | pass | 1.2 s | 108 MiB | 911 |
| TLC | mutant, `RunWriteAssertions` | torn destination, 8 states | 1.2 s | 101 MiB | 146 |
| TLC | `Terminates` under `Spec` (no fairness) | fails by stuttering | 1.2 s | 101 MiB | 911 |
| TLC | move and rename protocols | pass | 1.2 s | 95 MiB | 84 |
| Apalache (TLA+), `--length=17` | `Safety` | pass | 7.4 s | 539 MiB | symbolic |
| Apalache (TLA+) | mutant | torn destination, shortest trace | 6.3 s | 345 MiB | symbolic |
| Quint simulator, 200 k runs | `safety` | no violation (see the coverage note) | 2.3 s | 285 MiB | — |
| Quint simulator | mutant | torn destination | 2.3 s | 242 MiB | — |
| Quint → Apalache, 17 steps | `safety` | pass | 22.7 s | 1.2 GiB | symbolic |
| Quint → Apalache | mutant | torn destination, shortest trace | 9.4 s | 1.1 GiB | symbolic |
| Quint → TLC | `safety` | pass | 8.4 s | 670 MiB | 512 |
| Quint → TLC | mutant | torn destination | 5.3 s | 578 MiB | 146 |
| Quint → TLC | `eventuallyTerminal`, no fairness | fails by stuttering | 5.3 s | 607 MiB | 512 |
| Quint → TLC | `weakFair(step) implies eventuallyTerminal` | holds | 5.3 s | 613 MiB | 512 |
| Quint → TLC | move and rename protocols | pass | 5.3 s | 669 MiB | 84 |

The mutant trace from Apalache (through both front ends) and from TLC is the
shortest one:

1. probe the destination;
2. create the temp file;
3. write the content;
4. apply the metadata;
5. the sync that the mutant skips;
6. rename over the destination;
7. crash, keeping the rename.

After the crash the destination shows `Torn`. This is exactly the ordering
Kani's `should_panic` harness pins. Kani does not decode its trace by default.

**What T1 found beyond the known answers:**

- **The mutant also breaks classification soundness.** Kani's harness set does
  not check this combination.
  - A shell that skips the temp sync reports `Success` while the durable bytes
    are torn.
  - TLC found it before any crash, on the mutant's first check against the
    full `Safety`.
  - The like-for-like comparison uses `RunWriteAssertions`, the assertions of
    `run_write` that the `should_panic` harness checks.
  - The extra counterexample is correct, and cheap in either tool.
- **The Quint simulator rarely reaches deep states.** It picks uniformly at
  random.
  - A successful write needs 7 consecutive `Done` outcomes with no crash
    first. It was witnessed 0 times in 100 k runs of 20 steps, and 9 times in
    2 M runs (15 s).
  - `--witnesses` makes this gap visible, which is good. But a simulator
    "pass" is weak evidence; the exhaustive backends are what count.
- **Quint's simulator hid one silent modelling bug.**
  - The cause: in Quint, as in TLA+, `x' = a or b` parses as
    `(x' = a) or b`. When `b` holds, `x` is left unassigned, and both Quint
    simulators (Rust and TypeScript) silently keep its old value.
  - The effect: a false classification violation. It was found only by reading
    the trace, and reproduced in a 5-line spec.
  - The other backends reject the same spec. Quint's TLC backend reports
    "Successor state is not completely specified", and Apalache reports
    "Missing assignments to: x".
  - Quint's effect checker catches a double assignment, a `pure def` that
    reads state, and `any` branches that update different variables. It does
    not catch a missing assignment.
- **PlusCal fitted T1.** T1 is one sequential process with `either` and
  `with`. The translation is committed inside the module. One PlusCal rule has
  to be kept in mind: a statement later in the same step sees the earlier
  assignments. So `secondTemp` must be computed before `temp` is replaced.
- **Apalache on TLA+ needed a typed wrapper.** Apalache needs a type
  annotation on every `VARIABLE` and `CONSTANT`, but the PlusCal translator
  regenerates the `VARIABLES` line without annotations. The typed
  declarations therefore live in `MC_WriteProtocolApalache.tla`, which
  instantiates the module. Quint needs no wrapper, because its types are its
  annotations.

## 4. T2 — the draft journal at scale

Both tools transliterate the same 13 actions, faults, and invariants.
`quint test --main k8_traces` replays both decoded K8 traces step for step,
and `stateright`'s `tests/k8_traces.rs` replays them against the real core.

**Known answers reproduced:**

| Checker | K8 "body without an entry" at 6 | K8 S1 loss | One-process control at 8 |
|---|---|---|---|
| Kani | found (500.8 s, about 9 GB) | found within 8 (1017.5 s, about 18 GB) | S1–S4 hold (487 s, about 8 GB) |
| TLC, hand-written TLA+, w24 | found: 251.6 s, 12.3 GiB, 26.8 M states | S1 holds at 6 (full exploration: 1695.2 s, 20.3 GiB); found at 7: 1490.1 s, 20.1 GiB | S1–S4 hold: 1121.3 s, 16.8 GiB, 113.4 M states |
| TLC, hand-written, 1 worker | not within 30 min (14.1 M states, first `VIEW`) | not attempted | not attempted |
| Quint → TLC, w24 | not within 30 min (131.6 M states, 12.1 GiB) | not attempted | not attempted |
| Quint → Apalache 0.58.3 | out of heap (27 GiB) in the inlining pass, 13.6 min | not attempted | not attempted |
| Apalache 0.62.2 on TLA+ | out of heap (27 GiB) in the inlining pass, 7.4 min, after 26 type annotations | not attempted | not attempted |
| Quint simulator | missed in 20 M traces (114.8 s) | not attempted | not attempted |
| TLC `-simulate` | not attempted | missed within 8 actions in 1.6 G states (10 min cap) | not attempted |
| stateright, real `journal_core`, 8 threads, DFS | found; S1 holds at 6: 73.1 s, 2.3 GiB, 141.8 M states | found at 7: 408.0 s, 9.0 GiB, 692.4 M states; within 8, stopping at the first K8 pair: 0.3 s (its own timer) | S1–S4 hold: 49.5 s, 1.1 GiB, 111.8 M states |
| `quint test` / `stateright` tests (replay of the decoded traces) | reproduced | reproduced | — |

Two facts every tool agreed on:

- **The S1 trace is 7 actions long, not 8.** After both startups it is: A
  Edit, Register, WriteBody, Commit, Crash; then B Discard (or Save),
  DeletionStep.
  Kani's 8-action run contained it, and its 6-action CI pin is one action
  short of it.
- **Neither trace needs the other two ids.** The TLC trace at 6 differs from
  Kani's: A registers and writes, while B *saves* and deletes. It is the same
  body-without-entry class. E3 (§7) replays it against the real `journal_core`.

**Scale (every run):**

| Run | Expected | Outcome | Wall (s) | Peak tree RSS (MiB) | Distinct states | Ended by |
|---|---|---|---|---|---|---|
| `t2-tlc-k8-6-w1` | violation | timeout | 1802.8 | 11300 | 14068603 | timeout |
| `t2-tlc-k8-6-w24` | violation | violation | 215.4 | 16867 | 22270746 | finished |
| `t2-quint-tlc-k8-6-w24` | violation | timeout | 1802.6 | 12404 | 131552255 | timeout |
| `t2-quint-apalache-k8-6` | violation | error(1) | 835.2 | 29101 | - | finished |
| `t2-apalache-tla-k8-6` | violation | error(120) | 5.3 | 268 | - | finished |
| `t2-tlc-k3-8-w24` | pass | pass | 1121.3 | 17170 | 113383014 | finished |
| `t2-tlc-k8-6-body-w24` | violation | violation | 251.6 | 12572 | 26788325 | finished |
| `t2-tlc-k8-6-s1-w24` | pass | pass | 1695.2 | 20783 | 145382969 | finished |
| `t2-tlc-k8-7-s1-w24` | violation | violation | 1490.1 | 20577 | 146372126 | finished |
| `t2-apalache-tla-k8-6` | violation | error(255) | 452.1 | 28556 | - | finished |
| `t2-stateright-k3-8` | pass | pass | 49.5 | 1163 | 111810703 | finished |
| `t2-stateright-k8-6` | violation | violation | 73.1 | 2314 | 141812524 | finished |
| `t2-stateright-k8-7` | violation | violation | 408.0 | 9226 | 692371330 | finished |
| `t2-stateright-k8-8-finish` | violation | violation | 1.2 | 14 | 591931 | finished |
| `t2-quint-sim-k8-6-body` | violation | pass | 114.8 | 598 | - | finished |
| `t2-tlc-simulate-k8-8` | violation | timeout | 603.1 | 9326 | - | timeout |

How to read the rows in this table:

- `t2-tlc-k8-6-w1` and `t2-tlc-k8-6-w24` used the first `VIEW`, the journal
  alone. That view is sound for a violation, but it is not sound for a pass
  with more than one worker, and its state counts are not comparable. The
  `-body` and `-s1` rows use the corrected `<<j, n>>` view.
- The first `t2-apalache-tla-k8-6` (`error(120)`) is the type error before
  the annotations. Its second run and `t2-quint-apalache-k8-6` are the
  out-of-heap failures.
- A simulator "pass" on an expected violation means the simulator missed it.
- Peak RSS is sampled once a second, so it may understate runs shorter than a
  few seconds.

How to read the scale runs:

- **Hand-written TLA+ with TLC is the only external route that beat Kani on
  T2.**
  - It needs all 24 cores and 1.5–2.4× Kani's memory (12.3–20.3 GiB).
  - With one worker, TLC did not finish 6 actions in 30 minutes.
  - The fingerprint `VIEW` (drop the trace decoration `last`, keep the step
    counter `n`) is what makes it feasible. Without it, the first run held
    1.68 M states after two actions.
  - Quint has no `VIEW`. Its TLC translation fingerprints `n` and `last`, so
    Quint → TLC timed out at 6 actions after 131.6 M distinct states.
- **Apalache did not reach the solver.**
  - Through Quint, its inlining pass ran out of a 27 GiB heap after 13.6
    minutes. The journal model is written as nested pure functions, and
    Apalache inlines every call.
  - On the hand-written TLA+ it first rejected the record-parameter operators.
    26 type annotations fixed that (E5).
  - With the annotations in place, its inlining pass also ran out of the 27 GiB heap, after 7.4 minutes. Apalache never reached the solver on T2 through either front end.
- **stateright calls the real `journal_core`** and needs no second language.
  It is **the scale result of this evaluation.** It runs with 8 threads and depth-first search, against the real `journal_core`:

  | Target | stateright | Kani |
  |---|---|---|
  | one-process control at 8 actions | 49.5 s, 1.1 GiB | 487 s, about 8 GB |
  | K8 at 6 actions | 73 s, 2.3 GiB (full exploration) | 500.8 s, about 9 GB |
  | K8 at 7 actions, where S1 first fails | 408 s, 9.0 GiB | not run |

  - A full K8 exploration at 8 actions is projected at about 6 G states and
    70 GB, beyond this machine. Stopping at the first K8 pair takes 0.3 s
    (the harness's own timer).
  - At 6 actions it also reports S4 as failing, and at 7 and 8 both S3 and
    S4. Every such trace passes through a
    body written without an entry. From there the port's sticky flag lets the
    path continue, where Kani's panicking `assert!` ends it. They add no new
    class.
  - Caveats: it enumerates a bounded model explicitly (Kani's small-scope
    hypothesis again). Its environment is a 752-line hand port of
    `kani_proofs.rs`, a second copy of the environment, though not of the
    decisions. Depth-first counterexamples are not minimal.
- **Random simulation is a poor falsifier here.** The Quint simulator missed the K8 violation in 20 M traces of 6 steps (115 s). TLC `-simulate` checked 1.6 G states at 8 actions in 10 minutes without finding it.

**Counterexample readability, measured as the steps needed to map a trace
onto K8's decoded form:**

- **Quint (simulator and Apalache).** It prints each state as a readable
  record, `last: { actorB: true, act: Register, … }`, with no
  post-processing.
- **Quint → TLC.** It prints the raw TLA+ encoding of the sum types, such as
  `[tag |-> "Register", value |-> …]` and
  `[CreateTemp |-> [tag |-> "UNIT"]]`, which needed a script to read.
- **TLC on hand-written TLA+.** It prints the `last` record directly. A
  94-line script (`stateright/seed_from_tlc.py`) turned it into a Rust test.
- **stateright.** It prints the Rust action tuples directly.
- **Kani.** It needs concrete playback plus hand decoding (the programme
  record, K8).

## 5. T3 — the N6 data-directory lock (design sketch)

`DataDirLock.tla` and `data_dir_lock.qnt` model the same protocol:

- two processes over one data directory;
- `flock(LOCK_EX | LOCK_NB)` at startup;
- the kernel releasing the lock on death;
- a crash while the lock is held;
- two variants for the second instance: read-only with a lock retry, or
  refuse-to-start.

They also model the rejected alternatives:

- a pidfile that a crash leaves behind;
- no lock at all (today's K8 world);
- a read-only instance that upgrades **in place** instead of re-reconciling.

The safety checks run the T2 journal under the lock with one id and 6 journal
actions. The liveness checks drop the journal, so the state space is finite
and unbounded.

**Every expected verdict held, in both tools:**

| Check | Property | Fairness | Expected and found | TLC wall / states | Quint → TLC wall / states |
|---|---|---|---|---|---|
| `safety_ro_reconcile` | `Safety` | — | pass | 13.6 s / 3363860 | 39.3 s / 3363860 |
| `safety_refuse` | `Safety` | — | pass | 6.4 s / 1305792 | 27.9 s / 1305792 |
| `safety_pidfile` | `Safety` | — | pass | 16.7 s / 4912780 | 43.5 s / 4912780 |
| `safety_ro_in_place` | `Safety` (in-place upgrade) | — | violation: the K8 S1 loss | 3.3 s / 540723 | 22.8 s / 310408 |
| `safety_ro_edits_dropped` | `NoReadOnlyEditsDropped` | — | violation (open question) | 1.2 s / 4571 | 20.7 s / 7687 |
| `safety_no_lock` | `JournalSafe` (no lock) | — | violation: K8 | 2.3 s / 106278 | 20.7 s / 102343 |
| `live_ro_sf` | `NoReadOnlyWedge` ∧ `ReadOnlyEventuallyLeaves` | WF startup, SF retry | pass | 1.2 s / 126 | 19.7 s / 126 |
| `live_ro_wf_wedge` | `NoReadOnlyWedge` | WF startup, WF retry | violation: relaunch churn | 1.2 s / 126 | 19.7 s / 126 |
| `live_ro_wf_eventual` | `ReadOnlyEventuallyLeaves` | WF startup, WF retry | pass | 1.2 s / 126 | 19.7 s / 126 |
| `live_ro_nofair` | `ReadOnlyEventuallyLeaves` | none | violation: stuttering | 1.2 s / 126 | 18.6 s / 126 |
| `live_pidfile_sf` | `ReadOnlyEventuallyLeaves` (pidfile) | WF startup, SF retry | violation: wedge | 1.2 s / 234 | 19.7 s / 234 |
| `live_refuse` | `RefusedEventuallyWrites` | WF startup, WF relaunch | pass | 1.2 s / 72 | 19.7 s / 72 |
| `live_refuse_nofair` | `RefusedEventuallyWrites` | none | violation | 1.2 s / 72 | 19.7 s / 72 |

**What the sketch decides for a future N6 change.** Each point is backed by a
counterexample or a passing check.

1. **A read-only instance that wins the lock must re-run startup
   reconciliation.** Upgrading in place reproduces **the K8 S1 loss**,
   through a *clean* exit, under the lock:
   - A acquires the lock, B opens read-only;
   - A edits, registers, writes, and commits, then exits;
   - B retries, wins the lock, keeps its stale view, discards, and deletes
     A's committed body.
2. **Use `flock`, not a pidfile.** A crash leaves a pidfile behind, and
   `ReadOnlyEventuallyLeaves` then fails even under strong fairness: a
   permanent read-only wedge. Real pidfile schemes check whether the PID is
   alive, and that adds PID-reuse races the sketch does not model.
3. **A polling retry needs strong fairness.**
   - Weak fairness is enough once the other process has gone for good
     (`ReadOnlyEventuallyLeaves`).
   - It is not enough when the other process keeps relaunching. The lock is
     then free only intermittently, and `NoReadOnlyWedge` fails under WF but
     holds under SF.
   - A real implementation should wait on the lock (a blocking `flock` in a
     worker, or an inotify on the lock file) rather than poll, so it cannot
     be starved.
4. **Refuse-to-start is live only if the user relaunches**
   (`RelaunchWF`). Without that, `RefusedEventuallyWrites` fails.
5. **Open question: a read-only user's typing.** S1 protects committed drafts
   only. An exit or a reconciling upgrade throws away a read-only instance's
   unsaved edits. `NoReadOnlyEditsDropped` fails in every variant. N6 must
   decide whether read-only edits get their own drafts, a warning, or a
   refusal.
6. **No lock reproduces K8** (`safety_no_lock`), and at-most-one-writer holds
   in every locked variant. E6 also proved it unbounded with TLAPS.

**Constructs actually checked:**

- in TLC: `WF_vars` and `SF_vars` on individual actions, and `[]<>`, `<>[]`,
  and `~>`;
- in Quint: `weakFair` and `strongFair` on parameterized actions, and
  `always` and `eventually`, translated to TLC.

Apalache checked no liveness: it warns that its temporal support is
experimental, and points to TLC.

**PlusCal did not fit T3.** Its fairness attaches to a label or a whole
process, but T3 needs fairness on one sub-action (the lock retry). Getting it
would have meant a label per branch and extra `pc` states. The TLA+ model is
therefore plain TLA+.

**The E7 review (§7) caught a soundness bug in the first TLC runs of this
target.** The first T3 configs used a `VIEW` that left out the journal step
bound. TLC could therefore keep a copy of a state that had used more steps,
and discard a later copy with fewer, silently skipping in-bound behaviours.
All T3 figures above come from the corrected model.

**A cross-check falls out of the corrected runs.** On every passing safety
check, hand-written TLC and Quint → TLC report **identical** distinct-state
counts: 3,363,860, 1,305,792, and 4,912,780. So do all seven liveness checks.
Two independent encodings of the same sketch agree state for state.

## 6. Quint feature survey

Each feature was exercised on this project's models. The verdicts are for
this project.

| Feature | Exercised on | Good at here | Bad at here |
|---|---|---|---|
| Types, sum types, records, `match` | every model | typed sum types (`RestoreApply(RestoreEnding)`, `Finish(WriteClass)`) mirror the Rust enums one to one; the E7 reviewer found this easier to check than TLA+ strings | many ordinary names are builtins or keywords and are refused: `next`, `cross`, `contains`, `action` (4 of Quint's 9 first-attempt errors) |
| Effect system, modes (`pure def`, `def`, `action`, `temporal`) | eff1–eff4 probes | catches a `pure def` reading state (QNT200), a double assignment (QNT202), and `any` branches with different frames | does not catch a missing assignment (eff1) or an assignment hidden inside a boolean `or` (T1's precedence bug) |
| Modules, constants, instances | T2, T3 | `import journal(MaxSteps = 6, …)` fixes bounds without a config file | `import m(C = C).*` conflicts on constant names (QNT101); an instance's names are not re-exported to its importers; cross-file imports need a relative `from` path, and absolute paths fail (QNT013) |
| Simulator: `quint run` with `--invariant`, `--witnesses`, `--seed` | T1, T2 | fast (about 177 k traces/s on T2); `--witnesses` quantifies coverage; seeds reproduce | uniform random choice; it missed every K8 trace (§4); it silently keeps unassigned variables |
| `quint test` and `run` | T2 `k8_traces`, E2 | step-by-step regression tests of decoded counterexamples, with an error that points at the failing `.then` | has no `--mbt`, so a `run` cannot feed Quint Connect without a hand-kept `choice` variable (E1) |
| `quint verify` with Apalache | T1, T2 | a bounded symbolic proof of T1 in about 9–23 s | Quint 0.32.0 cannot drive Apalache ≥ 0.59 (§6.1); on T2 the inlining pass ran out of a 27 GiB heap |
| `quint verify --backend tlc` and `--temporal` | T1, T3 | real liveness under `weakFair` and `strongFair`; the same verdicts and state counts as hand-written TLA+ on T3 liveness | no `VIEW` and no symmetry: on T2 the fingerprint includes every variable, so it timed out at 6 actions; several seconds of fixed start-up per run (it compiles through an Apalache server) |
| `quint compile --target tlaplus` | T1 | readable, typed TLA+ (874 lines from a 311-line spec) that TLC and Apalache accept | one action shared between two `--init` candidates cannot be translated (QNT409) |
| ITF traces and `run --mbt` | T2, E1 | machine-readable traces with `mbt::actionTaken` and `mbt::nondetPicks` | variable names are module-qualified for imported instances and bare for the main module; each nondet pick is wrapped in an `Option` |
| Quint Connect (Rust) | E1 | drove a Rust driver over the real `draft_service` and found a real divergence (§7) | the catch came from the abstract disk model, which is Rust (`stateright/src/env.rs`), not from Quint; about 700 lines of driver and mapping; random traces (a trivial injected mutant took 1246 traces to hit) |
| Language server | eff2 probe | LSP diagnostics identical to `typecheck`; completion, hover, definition, rename | no evident gain over running `quint typecheck` in an agent loop |
| REPL | T1 debugging | `--backend typescript` evaluates expressions against a spec | the default Rust backend fails when stdin is piped ("readline was closed"), so an agent can script it only with the TypeScript backend |
| `quint docs` | — | not needed | — |

### 6.1 Quint toolchain reliability (a blocking finding for adoption)

- **A false green.** `quint verify --apalache-version 0.62.2` (the latest
  Apalache) printed the server banner and **exited 0 without a verdict**.
  Pointed at the same server by hand, Quint shows the cause: Apalache 0.59
  changed its configuration schema (`$.input: Unknown configuration key`).
  Quint 0.32.0 pins Apalache 0.56.1 by default, and 0.58.3 is the newest that
  works. A CI lane that trusted Quint's exit status would have passed without
  checking anything. Every Quint→Apalache figure here uses 0.58.3.
- **The TLC backend is the Apalache jar's TLC.** Its version is not reported,
  and Quint fixes `-deadlock` and `-Xmx8G`. A JSON `--tlc-config` sets only
  the workers and the heap.
- **Other issues with a cost:**
  - `quint run --n-traces 20000` ran Node out of heap. Quint Connect reported
    only "Quint returned non-zero code".
  - A divergence diff is printed only at `QUINT_VERBOSE=1`, which also prints
    every trace.

### 6.2 TLA+ feature survey, for balance

| Feature | Exercised on | Good at here | Bad at here |
|---|---|---|---|
| TLC, breadth-first, `-workers`, `VIEW` | T1, T2, T3 | the only external checker that beat Kani on T2 (§4); exact state counts; `VIEW` removes trace decoration | memory-heavy (12.3–20.3 GiB at w24); `VIEW` is a soundness footgun (§5, E7); a bounded model needs `CHECK_DEADLOCK FALSE` and an explicit step counter |
| TLC liveness: `WF`, `SF`, `~>`, `[]<>` | T1, T3, E9 | mature and fast; the WF-vs-SF distinction in T3 took about a second | the state space must be finite, so liveness runs need a journal-free or budgeted model (E9) |
| TLC simulation (`-simulate`) | T2 | fast (about 2.7 M states/s); it missed the K8 violation that breadth-first search found | — |
| PlusCal | T1 | natural for one sequential process; fewer lines than plain TLA+ | per-step sequential assignment semantics; fairness per label only (did not fit T3); regenerates the `VARIABLES` line, which fights Apalache annotations |
| Apalache on TLA+ | T1, T2 | bounded symbolic check of T1 in about 6–7 s | needs typed wrappers and 26 operator annotations on T2 (E5), then ran out of heap while inlining |
| TLAPS | E6 | proved at-most-one-writer for **every** behaviour in 0.4 s; a guard-removal mutant fails 3 of 35 obligations | the stable 1.5.0 is from 2022; proofs over the journal would need far more work; every lemma is maintenance |
| Trace validation | E2 | validated all 61 recorded smoke event logs (190 events) against a 40-line spec in about 1 s | the logs witness only two workflows today (`minimap-refresh` and `search`) |

## 7. Exploration log

Each idea's budget was declared before it started (default 2 h, maximum 4 h,
24 h in total). About 3 agent-hours were used, plus the measured checker runs.

| # | Idea | Budget / used | Outcome | Evidence and what it would take to go further |
|---|---|---|---|---|
| E1 | Quint Connect replay: `journal_core` (A), then the real `draft_service` (B) | 4 h / 0.6 h | **Worked (A); worked and found a real defect (B)** | See the E1 notes below. |
| E2 | Trace validation of `workflow-events.json` | 3 h / 0.4 h | **Worked, with limits** | See the E2 notes below. |
| E3 | Counterexamples as seeds | 2 h / 0.4 h | **Partially worked** | TLC's own K8 trace (not Kani's) became a Rust test through `stateright/seed_from_tlc.py`; it reproduces on the real `journal_core` (`tests/seeded_from_tlc.rs`, 0.8 s). A Kani assumption was not tried, because Kani harness changes were out of scope (task 8.2). |
| E4 | Conformance sequences generated from a spec | 2 h / within E1 | **Partially worked** | Quint Connect *is* such a generator. Uniform generation barely reaches the protocol: 2000 uniform traces made 6 body writes and no commit. It needed a hand-weighted step. The spec adds no guidance that a Rust generator over `env.rs` would lack. |
| E5 | Apalache against TLC on T2 | 2 h / 0.5 h | **Failed on this model** | Quint → Apalache ran out of heap while inlining. Apalache on TLA+ needed 26 annotations, then it also ran out of heap while inlining (7.4 min). A crossover would need an Apalache-shaped rewrite (flat actions instead of nested pure functions): a third encoding. |
| E6 | One TLAPS lemma | 3 h / 0.4 h | **Worked** | See the E6 notes below. |
| E7 | Specs as review documents | 1 h / 0.1 h | **Worked, and found a bug in this evaluation** | See the E7 notes below. |
| E8 | `stateright` T2 comparator on the real `journal_core` | 4 h / 0.3 h (subagent) | **Worked: it beat Kani on T2 scale without a second language** (§4) | `formal/evaluation/stateright/`: a standalone package excluded from the workspace, with `cargo deny`, `cargo hakari verify`, and `cargo metadata` unaffected. It needed 1 compile error and a hand-written `Hash`. Figures in §4. |
| E9 (added) | L1 as an unbounded liveness property in TLC | 2 h / 0.3 h | **Worked at 1 id; exceeded the cap at 2** | `JournalLiveness.tla`: every dirty open editor eventually becomes clean under weak fairness of the fault-free autosave pass. The environment may fault, crash, save, discard, and move the backing file up to `EnvBudget` times, and edit at any time. Kani checks only the bounded form (k = 7). With 1 id, L1 holds under `WF` (13.5 s, 212 k states) and fails without fairness. With 2 ids the fair check exceeded 30 minutes with 24 workers (17.1 M distinct states, 25.1 GiB, 4.2 M states still queued): TLC was still exploring, slowed by its periodic temporal-property checks; the unfair check found its counterexample in 4 s. |

**E1 notes.**

- **Part A.** 40,000 Quint simulation traces (320,000 steps) of `k3_8` and
  `k8_6` were replayed through the Rust port (`stateright/src/env.rs`, which
  calls the real `journal_core`). The full journal record was compared after
  every step, with **0 divergences** in 178 s.
- **Part B.** A restricted Quint instance (`k3_service`: one process,
  fault-free) drove the **real `draft_service` over a tempdir**. It found, on
  every seed tried, **a real set-aside defect**:
  1. `preserve_stale_draft_body` keeps a copy under
     `set_aside::keep_copy(id, entry.saved_at_secs)`.
  2. After a crash between a body write and its manifest commit, the body on
     disk is newer than its entry's stamp.
  3. A second unapplied restore then finds `{id}.{stamp}.draft` already there
     and returns `SetAside` **without copying the newer body**.
  4. The restore hold is released, and autosave may replace the only copy.
- **Why the proofs miss it.** S1 does not flag it, because that body was never
  committed. Kani's model preserves exact contents and never models set-aside
  naming.
- **Reproduction.** `stateright/tests/e1_findings.rs` reduces it to 5 public
  calls. **It is not fixed here (task 8.2 forbids production changes); see
  "Follow-up" in §8.**
- **The lesson.** The catch came from comparing the real service with an
  abstract disk model. That model is the Rust `env.rs`; Quint only generated
  the traces. Costs: the driver and mapping took about 700 lines; a trivial
  injected mutant needed 1246 random traces to be caught; and `quint test`
  cannot feed Connect directly.

**E2 notes.**

- The recorded logs: 61 non-empty logs, 190 events.
- TLC checked all 61 in about 1 s (`WorkflowEventsTrace.tla`), and generated
  Quint `run` tests checked them in 4.3 s. A mutated log fails in both.
- The data corrected the spec: a `search` workflow's `started` event names the
  blocker `editor-search`, not the workflow id.
- The limits of the witness:
  - the log is content-free, and records readiness changes observed on reads;
  - only `minimap-refresh` and `search` appear in local artefacts;
  - no save, session-restore, or draft event exists to validate the journal
    protocol against.
- Going further needs a smoke scenario that exercises save and restore, and
  the log to carry per-draft events.

**E6 notes.**

- `LockProof.tla` proves at-most-one-writer for every behaviour of the T3 lock
  layer, with unbounded crashes and steps: 35 obligations in 0.4 s, first
  attempt.
- Removing the lock guard fails 3 of them.
- Compare `extend-closed-loop-geometry-verification`, which still plans in-Kani
  attempts at unbounded claims. TLAPS gives them cheaply for a small
  inductive invariant. The journal's S1 would need a large inductive
  strengthening.

**E7 notes.**

- A fresh agent read the N6 prose first (18 open questions), then the two
  sketches.
- The sketches answered 7 of the 18: startup ordering, the variant choice,
  what a read-only instance may do, the upgrade, crash vs exit, which state is
  protected, and liveness. They raised 6 new questions.
- The reviewer found **the `VIEW` soundness bug** (§5) and three modelling
  gaps, all fixed or recorded:
  - `ExternalMtime` was gated on the writer;
  - dropped read-only typing was invisible to S1;
  - a liveness property's name promised more than it checked.
- It rated Quint "modestly" easier to review: typed sum types, one-place
  fairness pairing, `{ ...r, f: v }` updates.
- It noted that TLA+'s `VIEW` pitfall lives in cfg files, where a reader does
  not look.
- It raised these as unmodelled: NFS, fd inheritance across exec, the lock
  file being unlinked, mixed versions, and D-Bus forwarding.
- A maintainer review of the sketch is **not attempted** in this change: it
  needs the maintainer, and is offered with the report.

## 8. Decision

**Chosen: A, keep Kani only, for every maintained property in this change.
And, if the N6 trigger fires, B: TLA+ with TLC as the disposable design-sketch
tool.**

The two are combined only because each part has its own evidence.

- Quint is not adopted.
- `stateright` met D's condition on T2. Its adoption as a maintained lane is
  left to a follow-up change (follow-up 3), because it amends the Kani-only
  requirement.
- The Rust abstract environment moves into N11 as a conformance oracle.

### Why A for maintained properties

- **Kani checks the code that ships.** T1 and T2 were each re-expressed by
  hand in two languages, and the re-expressions drifted in small ways:
  - the precedence bug the Quint simulator hid;
  - the model-level choices the E7 review had to catch.

  Kani's harnesses call `journal_core` and `WriteProtocol` themselves, so this
  class of drift does not exist.
- **Kani found real production defects this programme has already fixed.**
  The `Unavailable` restore, found this way, had to be modelled explicitly in
  every external model before those models could "find" it.
- **No external tool beat Kani on T2 without a large cost.**
  - Hand-written TLC needed 24 cores and 1.5–2.4× the memory at 6 actions.
  - Quint could not finish at 6 actions (TLC) or preprocess the model at all
    (Apalache).
  - `stateright` *did* beat Kani on T2: 7–10× faster with about 4–7× less memory, on the same `journal_core`. That is the evidence for follow-up 3 below, not for dropping Kani. Kani still proves the finite write protocol completely, and it proves the geometry harnesses over symbolic `i32` pixel ranges, which an explicit-state checker cannot enumerate.
- **What Kani lacks that the project needs now:** nothing on T1 or T2.
  Liveness under fairness (T3, E9) is needed only by the dormant N6.

### Why B, and why not Quint, for the N6 sketch

- **Both tools checked T3 completely, with the same verdicts and the same
  liveness state counts.** TLC was fastest (about 1 s per liveness check).
  Quint → TLC took about 19–20 s per liveness check (21–44 s for safety),
  most of it fixed start-up.
- **The decisive difference is reliability, not expressiveness.**
  - Quint's pipeline had a silent false green (§6.1).
  - Its simulator silently keeps unassigned variables.
  - It has no `VIEW` or symmetry reduction, so it cannot scale a T2-sized
    model.
  - Its TLC is an unversioned jar inside Apalache.
- **TLA+ offers what a design sketch needs.** It adds TLAPS for the one
  unbounded lemma a lock deserves (E6, 0.4 s), and trace validation against
  real logs (E2).
- **Quint's advantages were real, but not decisive.**
  - Readability (E7).
  - Typed sum types.
  - Quint Connect. Its E1 success came from the Rust abstract disk model,
    which N11 can drive without Quint.
- **Outcome C needs Quint's agent-friendliness or bridge to measurably beat
  TLA+.** Neither did:
  - **Quint's first-attempt errors:** 5 of the 6 Quint files (one of them
    generated) failed a parse or a type check on the first try, with 9 errors
    in all:
    - 4 name clashes with builtins or keywords;
    - 1 doc comment placed inside a record type;
    - 1 constant-import conflict;
    - 3 import-path forms.
  - **TLA+'s first-attempt errors:**
    - 0 of 9 TLA+ files failed SANY.
    - The 2 Apalache wrappers each failed type checking once: annotation
      placement, and the 26 missing operator annotations.
    - 1 model failed at TLC run time, on an untyped `Init`.
    - Most TLA+ iterations were configuration: deadlock, `VIEW`, and fairness
      placement.
  - **Silent errors: 1 on each side.** Quint's was the precedence bug that its
    simulator hid. TLA+'s was the unsound `VIEW` that E7 caught. Both
    produced plausible results.
  - The bridge's value was not Quint-specific.

### Could Quint replace Kani? No.

Evidence **for** replacing Kani:

- real liveness under weak and strong fairness, which Kani cannot check (T3,
  and T1's termination);
- readable specs and readable counterexamples (E7, §4);
- seconds-scale checks on T1 and T3;
- a test bridge, Quint Connect, that drove the real service and found a real
  defect (E1).

Evidence **against**:

- **Quint checks a model, not the Rust.** Conformance through Connect is
  sampled, not exhaustive. It is 40,000 random traces, weighted by hand, and a
  trivial mutant took 1246 of them to catch. Kani's harnesses *are* the
  production functions, over every interleaving within their bounds.
- **Scale.** Quint could not reproduce T2's known answers within the CI cap:
  - TLC timed out at 6 actions with 131.6 M states;
  - Apalache ran out of heap while preprocessing;
  - the simulator missed the traces.

  Kani does 6 actions in 500 s.
- **Reliability.** The false-green exit status, the hidden unassigned
  variables, and the version coupling between Quint and Apalache are all in
  §6.1.
- **Every Quint model is a second copy to keep in sync.** That is the bridge
  the 2026-09-23 consolidation deliberately removed. E1 needed about 700 lines
  of mapping code for one model.

A replacement would trade a proof about the shipped code for a check of a
drifting model. The one capability Quint adds, liveness, is equally available
in TLA+ with TLC, which also scaled further.

### What the losing options showed

- **C (Quint):** see above.
- **D (`stateright`):** the evidence for D is strong on T2 scale (§4), and D's condition, "E8 beat Kani on T2 scale without a second language", is met. It is **not adopted in this change** for two reasons:

  - Adopting a maintained `stateright` lane amends the "Kani is the single formal verification tool" requirement, and the ADDED requirement says such an adoption is carried by its own follow-up change.
  - Its advantage covers the finite-choice journal interleavings only. Kani's symbolic integers remain necessary for the geometry and budget policies.

  Follow-up 3 opens that change.
- **B for maintained properties:** rejected. The same drift argument applies,
  and T2 needed 12.3–20.3 GiB and 24 cores.

### What happens to `formal/evaluation/`

It is **kept as dated evidence**, pinned to the versions in §1 and reproducible
with `make formal-evaluation`. It is not maintained. A future change may delete
it, and this report stays as the record either way.

### Follow-up changes

1. **Fix the E1 set-aside defect** (data safety, fix first). A body that is
   newer than its entry's `saved_at_secs` must not be treated as already kept.
   Options: key set-aside names by content or body identity, or compare
   contents before skipping. Start from the reproduction in
   `formal/evaluation/stateright/tests/e1_findings.rs`, promoted to a failing
   `draft_service` test. Recorded in the programme record's deferral
   inventory.
2. **N11 (`verify-shell-conformance-against-proven-cores`).** Adopt E1's
   structure without Quint: the Rust abstract environment (`env.rs`, which
   calls `journal_core`) as the oracle, driving the real `draft_service` over
   a tempdir. That means a deterministic generator with weights, or
   `stateright`'s explored paths, plus fault seams and kill points.
3. **Weigh `stateright` for the journal's exhaustive interleaving checks.**
   Take K3, K8, and bounded L1 from `formal/evaluation/stateright/` at the
   same bounds, measure them on CI runners, and decide between a
   complementary lane and moving the journal shard. This amends the Kani-only
   requirement, so it needs its own OpenSpec change. Its environment is the
   same Rust `env.rs` that follow-up 2 needs.
4. **N6, only if its trigger fires.** Start from `DataDirLock.tla` and §5's
   six points. Check the sketch in TLC, prove the lock invariant in TLAPS, and
   keep the final core Kani-checked Rust.

## 9. Reproduction

```sh
make formal-evaluation FORMAL_EVAL_TARGET=install   # 21.7 s cold, 426 MB, into build/formal-evaluation/tools/
make formal-evaluation FORMAL_EVAL_TARGET=t1        # the T1 known answers, about 1.5 minutes
make formal-evaluation FORMAL_EVAL_TARGET=t3        # every T3 check in both tools
FORMAL_EVAL_WORKERS=auto FORMAL_EVAL_T2_DEPTHS="6" make formal-evaluation FORMAL_EVAL_TARGET=t2
make formal-evaluation FORMAL_EVAL_TARGET=report-data
```

`t2` at 8 actions was not run exhaustively. TLC needed 1490 s with 24 cores
at 7, and Quint → TLC and Apalache already failed at 6, so 8 is expected to
exceed the cap; the `FORMAL_EVAL_TIMEOUT` and `FORMAL_EVAL_MEM_MB` ceilings
then record it as the result.

The `stateright` package and the E1 and E3 tests build separately:

```sh
cd formal/evaluation/stateright
# .cargo/config.toml targets build/formal-evaluation/target-rust; the E1
# tests also need the pinned quint on PATH (formal/evaluation/stateright/README.md).
cargo test --release
```

E2 regenerates its traces from local smoke artefacts:

```sh
formal/evaluation/trace-validation/gen_traces.py OUT $(find build/smoke -name workflow-events.json)
```

TLAPS:

```sh
tlapm --cleanfp formal/evaluation/tlaplus/LockProof.tla
```

## 10. Gate audit (task 1.1)

Before anything was written, every gate's scan roots were read. A gate that
reads a path below is marked as seeing it.

| Gate | Sees `formal/evaluation/`? | Sees this report? | Sees `scripts/formal-evaluation.sh`? |
|---|---|---|---|
| `scripts/check-filesystem-boundary.sh` | no (named `crates/` roots only) | no | no |
| terminology guard (`crates/lushtext-core/tests/workspace_terminology.rs`) | no | **yes** (`docs`); the report follows the folder-set terminology, and the guard passes | no |
| `sonar-project.properties` | no (`formal` is not in `sonar.sources`) | no | **yes** (`scripts` is in scope), like every other repository script |
| `scripts/check-workflow-boundaries.py` | no (`crates/lushtext-core/src`, the matrix, and `crates/**/kani_proofs.rs`) | no | no |
| `scripts/check-agent-docs.sh` | no (rules index, skills, and the filesystem audit) | no | no |
| `scripts/check-workflow-timeouts.py` | no (`.github/workflows` only) | no | no |
| `cargo deny`, hakari, Clippy, nextest | no: they cover workspace members only, and `formal/evaluation/stateright` is in the root `exclude`; `cargo deny check` and `cargo hakari verify` pass unchanged | no | no |

The design needed no adjustment. The script's Sonar coverage is the ordinary
coverage for a repository script, so no Sonar scope change was made.
