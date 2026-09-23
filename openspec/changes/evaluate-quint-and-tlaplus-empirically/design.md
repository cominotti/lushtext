## Context

The formal-verification programme (`docs/next/formal-verification.md`) runs
Kani only. That decision was made on 2026-09-23 and replaced an earlier plan
to mix tools. The Kani lane has 27 harnesses in five shards, about 40 minutes
in total. Three results matter for this evaluation:

- **Phase 5, durable writes.** The I/O-free `WriteProtocol`, `MoveProtocol`,
  and `RenameProtocol` cores
  (`crates/lushtext-core/src/services/filesystem/write_protocol.rs`) are
  proved crash-atomic in seconds. Classification soundness, mode
  non-widening, and metadata-before-final-sync are also proved. The
  `should_panic` harness `skipping_the_temp_sync_tears_the_destination` pins
  the known-bad ordering.
- **Phase 4 / K3, the draft journal.** The journal decision core
  (`services/draft_service/journal_core.rs`, 768 lines) is driven by
  `kani_proofs.rs` over a model disk:
  - 3 ids, up to 3 generations, 8 nondeterministic actions (`unwind(9)`);
  - 13 action kinds, including Crash and Startup, with an I/O fault possible
    on every step;
  - S1–S4 PROVED in 487 s (about 8 GB), and L1 bounded liveness tight at
    k = 7.
- **K8, the second process.** Dropping axiom A6 by adding a second process
  gave two decoded loss traces at 8 actions after both startups. That run
  took 1017.5 s, about 18 GB, and 17 minutes of wall time. The CI pin was
  cut to 6 actions (500.8 s, about 9 GB), where only the body-without-entry
  trace is still found:
  - **S1 loss.** A commits, A crashes, B discards.
  - **Body without an entry.** A deletion races B's registration.

Kani has three structural limits:

- no unbounded liveness and no fairness;
- state explosion when actors multiply;
- fixed-array models.

The next-candidates list records untested opinions about TLA+, Quint, and
`stateright`. N6, the inter-process lock, is dormant until axiom A6 is
breached. N11 is the planned shell-conformance change, which uses the proven
cores as oracles.

The maintainer has authorised local installs (Java 25 and Node 24 are present
in the toolbox). The maintainer also asked that the design leave agents room
to explore benefits not yet discussed.

## Goals / Non-Goals

**Goals:**
- Measure Quint and TLA+ head to head on identical targets drawn from this
  codebase, under a fixed protocol and fixed metrics.
- Calibrate each tool against answers Kani already established, before
  trusting any new answer it gives.
- Answer the scale question (T2) and the liveness question (T3) with numbers,
  not opinion.
- Give the creative exploration explicit, bounded room, and keep an honest
  record of it, failures included.
- End with one explicit decision, backed by evidence, in
  `docs/next/formal-verification-quint-vs-tlaplus.md`.

**Non-Goals:**
- Adopting any tool, adding a CI lane, or adding a dependency to any
  production crate. A recommendation of that kind becomes a follow-up change.
- Implementing the N6 lock or changing any production code. T3 is a design
  sketch.
- Replacing or weakening any Kani harness, bound, or shard.
- Changing the automation contract. Trace validation reads existing artifacts
  only.

## Decisions

### D1. Location: `formal/evaluation/`, outside every gate

Models go in `formal/evaluation/quint/`, `formal/evaluation/tlaplus/`, and
optionally `formal/evaluation/stateright/`. Each has a README that states:

- the tool version;
- the exact commands;
- the bounds;
- the Kani result the model is compared against.

This location is outside every gate:

- **Sonar.** `sonar.sources` does not list `formal`.
- **Filesystem-boundary audit.** It scans named `crates/` roots only.
- **Terminology guard.** It scans `docs`, among other roots, so the report
  must follow the folder-set terminology. Task 1 confirms that the guard's
  roots exclude `formal/`.
- **Cargo.** `stateright/` is a standalone package with its own `[workspace]`
  table, and it is also added to the root `exclude` (as `fuzz` is), so
  `cargo deny`, hakari, Clippy, and nextest never see it. Its `Cargo.lock`
  is committed inside that directory.

Alternatives considered:

- `docs/formal/`: it mixes executable models with prose, and would fall
  under the terminology guard.
- `crates/lushtext-formal-eval`: it enters the workspace and every gate.
- `build/`: it is gitignored, so the models would not be reviewable.

### D2. Tools installed into a gitignored prefix, versions pinned in the report

`scripts/formal-evaluation.sh install` installs each tool into
`build/formal-evaluation/tools/`:

- Quint, through `npm install --prefix`;
- the Apalache release archive;
- `tla2tools.jar`;
- optionally TLAPS.

It resolves "latest stable" once, at the start of the evaluation. It checks
current release pages or Context7, and records the resolved versions and
checksums in the report so reruns are reproducible. Nothing is installed
globally except what the maintainer already has.

Current Quint documentation confirms `quint verify` (Apalache by default),
`quint verify --backend tlc` with `--temporal` properties, `quint run` (the
simulator), and `quint compile --target tlaplus`. Quint can therefore reach
TLC both natively and through transpilation, and both paths are measured.

### D3. The fixed comparative protocol

The same target semantics are modelled in each tool, taken from the Rust
cores. Where a tool forces a representational change (for example
bit-precise state versus sets and records), the change is recorded as
**expressiveness friction**, not silently absorbed.

| Target | Source of truth | Known answer (from Kani) | Must show |
|---|---|---|---|
| **T1 calibration** | `write_protocol.rs` (`WriteProtocol` S0–S6, copy fallback, POSIX axioms A1–A5 as in `write_protocol/kani_proofs.rs`) | crash atomicity and classification soundness hold; with `SyncTemp` answered without a sync, the destination tears | both known answers reproduced; the torn trace found |
| **T2 scale** | `journal_core.rs` plus the `kani_proofs.rs` model disk and 13 actions, with two process actors (`step_as`) | at 8 actions: two decoded K8 traces (S1 loss; body without an entry); at 6: body without an entry only | both traces at 8; wall time, memory, and distinct states at 6, 8, 10, 12, and deeper until a 30-minute or memory ceiling is reached; single-process S1–S4 still holds as a control |
| **T3 N6 lock** | `formal-verification-next.md` N6 and the K8 decision: a `flock` lease on a data-directory lock file, the kernel releasing it on process death, crash while holding, and a second instance that goes read-only or refuses | none yet; this is new ground | safety (at most one writer per data directory, S1 across processes, no body without an entry); liveness under weak or strong fairness (a waiting or read-only instance eventually becomes the writer once the holder exits or crashes; no permanent read-only wedge) |

In each tool:

- **Quint.** `quint run` (the simulator) for fast falsification, then
  `quint verify` with Apalache for bounded symbolic checking, and
  `quint verify --backend tlc` for explicit-state checking and temporal
  properties.
- **TLA+.** A hand-written module (PlusCal where the protocol is naturally
  sequential per process, such as T1 and T3), checked with TLC. Apalache is
  run on the same module where the type annotations cost less than a time box.

**Fair-comparison rules:**

- the same bounds (ids, generations, action depth) as the Kani harness, then
  deeper;
- the same machine, one checker at a time;
- `/usr/bin/time -v` for wall time and peak RSS;
- worker counts recorded;
- every run's command line and output kept under
  `build/formal-evaluation/runs/`, with a summary committed in the report.

**Metrics** (one row per tool × target):

| Metric | How it is measured |
|---|---|
| modelling effort | agent wall time, spec lines, fix iterations until the spec parses and type-checks |
| expressiveness friction | a list of every construct that had to be re-expressed away from the Rust core |
| check cost | wall time, peak RSS, distinct states or solver steps, depth reached |
| properties exercised | safety, liveness, and fairness properties actually checked, not merely expressible |
| known answers | T1 and T2 known answers reproduced (yes or no, with the trace) |
| counterexample readability | steps to map the trace onto K8's decoded trace; whether the tool's output needed post-processing |
| install and CI cost | download size, runtime prerequisites, cold-install time, whether a check fits a 30-minute job |
| agent-friendliness | first-attempt parse, type-check, and check success rate; typical error-message quality; reliability of LLM repair (logged per iteration) |
| bridge to Rust | what exists (Quint Connect, trace validation, ITF traces, transpilation) and what was tried |

### D4. The open exploration track

Exploration follows the controlled protocol and never replaces it. Every idea
has a **time box** (default 2 agent-hours, maximum 4, extendable only by
recording why), and gets a log entry in the report:

- the hypothesis;
- the budget and the time spent;
- what was tried;
- the outcome (worked, partially worked, failed, or abandoned);
- the evidence;
- what would be needed to go further.

Failures are recorded as carefully as successes. Seed ideas:

| # | Idea | Budget |
|---|---|---|
| E1 | Quint Connect: replay T2 traces against `journal_core` (pure) and against the real `draft_service` over a tempdir; compare with N11's "proven core as oracle" design (cost, what each catches) | 4 h |
| E2 | TLA+ trace validation: map `workflow-events.json` and smoke artifacts from `docs/automation.md`'s `GetWorkflowEvents` stream (`started`/`finished` phases of `save`, `session-restore`, …) onto a small spec, and check a recorded smoke run conforms; note what the bounded, content-free event log can and cannot witness | 3 h |
| E3 | Counterexamples as seeds: turn a TLC or Apalache trace (T2 or T3) into a Kani harness assumption, a proptest regression, or an N11 operation sequence | 2 h |
| E4 | Generating N11 conformance sequences from a spec (Quint `run` traces, or TLC simulation mode), versus N11's random generator | 2 h |
| E5 | Apalache (symbolic) against TLC (explicit) on the same T2 model: crossover depth and memory profile | 2 h |
| E6 | TLAPS: one unbounded lemma (for example T1 crash atomicity for any number of retries, or the T3 at-most-one-writer invariant), compared with `extend-closed-loop-geometry-verification`'s in-Kani unbounded attempts | 3 h |
| E7 | Specs as review documents: have a fresh agent and the maintainer read the T3 spec against a prose design, and record the questions each raises | 1 h |
| E8 | `stateright` as a Rust-native explicit-state comparator on T2, calling `journal_core` directly (no second language, so no bridge) | 4 h |

Agents may add ideas (E9 and on) that are not listed. A new idea gets the
same log entry and a budget declared before it starts. The total exploration
budget is 24 agent-hours. When that is reached, the remaining ideas are
recorded as "not attempted" with a reason.

### D5. Reproduction: a local-only make target and script

`scripts/formal-evaluation.sh {install|t1|t2|t3|all|report-data}` is fronted
by `make formal-evaluation` (with `FORMAL_EVAL_TARGET=` to select a target).

- It is not a prerequisite of `check`, `check-policy`, `test`, `kani`, or
  `end-user-smoke`, and no workflow calls it.
- When a tool is missing, it exits with a clear "run install" message,
  following the `make kani` version-check precedent.
- Long runs honour `FORMAL_EVAL_TIMEOUT` (default 30 minutes per check, the
  CI job cap) and a memory ceiling, so "does not fit" is itself a recorded
  measurement, not a hang.

### D6. The decision section

The report ends with exactly one of the four outcomes below, chosen against
evidence, with the losing options' evidence stated:

| Outcome | Evidence required |
|---|---|
| **A. Keep Kani only** | T1–T3 show no capability Kani lacks that the project needs now, or the cost outweighs it |
| **B. TLA+ as the N6 design-sketch tool** | T3 liveness under fairness was checked and was useful; TLC was fast enough; the sketch stays disposable, and the final core is still Kani-checked Rust |
| **C. Quint, with or without Connect** | as B, plus Quint's agent-friendliness or bridge (E1/E4) measurably beat TLA+ |
| **D. `stateright`** | E8 beat Kani on T2 scale without a second language |

Outcomes may be combined only where the evidence supports each part (for
example "A for maintained proofs, B for the N6 trigger"). The section also
states what happens to `formal/evaluation/`: kept as dated evidence, or
removed with the report as the only record. It names any follow-up change.

### D7. Spec delta

"Kani is the single formal verification tool" is MODIFIED, not left alone.
Its current text forbids separate models in another language, and the
evaluation's models would violate that without a recorded exception. The
modification keeps the prohibition for maintained properties, and defines the
narrow conditions under which a disposable evaluation model is allowed. The
ADDED requirement makes empirical evidence the gate for any future adoption.
No new capability is warranted: an evaluation is not product behaviour.

**Maintainer decision, 2026-09-23: keep the models in the repository (option
A).** The exception is approved as scoped here. Evaluation models live in the
repository, outside every gate, and no claim rests on them alone. The
alternative was rejected: running the evaluation outside the repository and
committing only the report would leave the rule untouched, but it would also
lose the models, so the comparison could not be reproduced or checked.

## Risks / Trade-offs

- [The models drift from the Rust cores, so the comparison measures a
  different protocol] → Each model's README cites the core's line ranges and
  Kani harness. T1 and T2 must reproduce the known answers before any metric
  counts.
- [An unfair comparison: Kani checks real code, and the external models check
  re-expressions] → The re-expression gap is itself reported as a cost
  (expressiveness friction, and bridge options). Speed is compared only after
  the known answers are reproduced.
- [Deep T2 runs exhaust memory or time on the toolbox] → Enforce a timeout
  and a memory ceiling; record the ceiling as the result; never run two
  checkers at once.
- [Exploration expands without limit] → Budgets are declared before an idea
  starts, the total is capped at 24 h, and remaining ideas are recorded as
  not attempted.
- [Agent-friendliness is anecdotal] → Log every iteration (error, fix,
  outcome) in the run directory, and report counts, not impressions.
- [Repository gates pick up the new files] → Task 1 audits every gate's scan
  roots before anything is written, and the final tasks rerun `make check`,
  `make check-policy`, and `make test` to prove nothing changed.
- [Tool versions go stale] → Versions and checksums are pinned in the report.
  The decision states its date and versions.

## Migration Plan

This change is additive, and nothing ships. Rollback is deleting
`formal/evaluation/`, the script, and the make target. The report stays as a
record.

## Open Questions

- Should T3's "second instance" be read-only or refuse-to-start? The sketch
  models both variants, because choosing between them is part of what N6
  would decide.
- Is E2 feasible, given the event log is bounded and records readiness-state
  changes observed by snapshot reads, not a live source? If the log cannot
  witness enough, the E2 entry records that result.
