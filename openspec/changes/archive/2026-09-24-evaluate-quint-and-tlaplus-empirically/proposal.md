## Why

On 2026-09-23 the programme settled on Kani only
(`docs/next/formal-verification.md` §2). The tool judgements in
`docs/next/formal-verification-next.md` ("Tool criteria recorded on
2026-09-23") are still reasoned opinions, not measurements:

- TLA+ is suitable only as a disposable design sketch for the N6
  inter-process lock;
- Quint is preferable only if the model should drive tests through Quint
  Connect;
- `stateright` should be tried before any external language when a model
  outgrows Kani.

Kani has already hit its scale ceiling once. The K8 two-process journal took
1017.5 s and about 18 GB at 8 actions, and had to be cut to 6 actions to fit
CI. Kani also cannot check liveness under fairness, which is what an N6 lock
needs. This change measures those judgements before any of them becomes
policy. It runs a controlled, head-to-head evaluation of Quint and TLA+ on
this project's own protocols, plus a time-boxed open exploration. The result
is a **decision record, not an adoption**.

## What Changes

- **A fixed comparative protocol.** The same three targets are modelled in
  both tools, and every tool is installed locally at its latest stable
  version:
  - **Quint**: the npm package, with its simulator, Apalache, and the
    `--backend tlc` path.
  - **TLA+**: the latest `tla2tools.jar` with TLC, optionally Apalache, and
    PlusCal where it fits naturally.

  The targets:
  - **T1, calibration.** The durable-write protocol (`WriteProtocol`), whose
    answers Kani already knows. Crash atomicity holds, and skipping the temp
    sync tears the destination.
  - **T2, scale.** The two-process draft journal (the K8 scenario) at 6, 8,
    and deeper action counts. It is measured against Kani's 1017.5 s / about
    18 GB at 8 actions, and must find both known K8 counterexample traces.
  - **T3, the real use case.** A design sketch of the N6 inter-process
    data-directory lock: a `flock` lease, a crash while holding it, and a
    second instance that goes read-only or refuses to start. Its properties
    are safety **and** liveness under fairness.

  The same metrics are recorded for every tool on every target: modelling
  effort, expressiveness friction, check time, memory, and state counts,
  liveness and fairness actually exercised, counterexample readability,
  install and CI cost against the 30-minute job cap, agent-friendliness, and
  bridge options back to Rust.
- **An open exploration track.** Every idea has an explicit time budget, and
  every attempt is recorded, failures included. The seed ideas are listed in
  `design.md`; agents may add others. They include:
  - Quint Connect replay against `journal_core` and the real `draft_service`,
    compared with N11;
  - trace validation of the D-Bus automation event stream;
  - counterexamples as seeds for Kani, proptest, or N11 tests;
  - Apalache against TLC on the same model;
  - one TLAPS lemma;
  - specs used as review documents;
  - `stateright` as a Rust-native comparator on T2.
- **Evaluation models in a non-production home.** The models live under
  `formal/evaluation/` (`quint/`, `tlaplus/`, and `stateright/` if the
  comparator is attempted). They are reproduced by a local-only
  `make formal-evaluation` target and `scripts/formal-evaluation.sh`, and
  tools are installed into gitignored `build/formal-evaluation/`. The home
  must not affect the Cargo workspace, `cargo deny`, hakari, the
  filesystem-boundary audit, the terminology guard, the Sonar scope, or any
  CI workflow.
- **A comparison report with a decision.**
  `docs/next/formal-verification-quint-vs-tlaplus.md` holds the measurements,
  the exploration log, and an explicit decision section. Exactly one outcome
  is chosen, with evidence:
  - keep Kani only;
  - adopt TLA+ as the design-sketch tool for N6;
  - adopt Quint, with or without Connect;
  - adopt `stateright`.

  The programme record and the next-candidates list are updated to point at
  it.
- **No production dependency and no CI lane.** If the decision recommends a
  lane or a maintained model, that recommendation becomes a follow-up change.

## Capabilities

### New Capabilities

<!-- none: an evaluation is not a new product capability -->

### Modified Capabilities

- `formal-verification-kani`:
  - MODIFIED "Kani is the single formal verification tool". The requirement
    now distinguishes **maintained formal properties**, which stay Kani-only,
    from **disposable evaluation models**. An evaluation model is allowed
    only under a recorded maintainer decision, in a non-production location,
    outside every gate and CI lane.
  - ADDED "External modelling tools are adopted only on recorded empirical
    evidence". Adopting Quint, TLA+, `stateright`, or any other external
    tool needs a written comparison on this project's own targets and an
    explicit decision section. Until such a decision says otherwise, Kani
    stays the single maintained tool.

## Impact

- **New files:**
  - `formal/evaluation/{quint,tlaplus}/`, the models with a README per tool;
  - optionally `formal/evaluation/stateright/`, a standalone Cargo package
    with its own `[workspace]` table, excluded from the root workspace;
  - `scripts/formal-evaluation.sh`;
  - `docs/next/formal-verification-quint-vs-tlaplus.md`.
- **Edited:**
  - `Makefile`: a local-only `formal-evaluation` target, not wired into
    `check`, `check-policy`, `test`, or CI;
  - `.gitignore`: `build/formal-evaluation/`;
  - root `Cargo.toml` `exclude`, only if `stateright` is attempted;
  - `docs/next/formal-verification.md` and
    `docs/next/formal-verification-next.md`;
  - the AGENTS.md build-command list and `.agents/rules/build.md`, for the
    new target.
- **Tools installed on the maintainer's toolbox**, with their versions
  recorded in the report: Quint (npm), Apalache, `tla2tools.jar`, and
  optionally TLAPS. Java 25 and Node 24 are already present.
- **Not touched:** production Rust code, the Kani harnesses and shard table,
  every CI workflow, persisted formats, and the automation contract. The
  trace-validation exploration only *reads* existing smoke artifacts.
