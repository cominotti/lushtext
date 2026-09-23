## 1. Gate audit and home

- [ ] 1.1 Read the scan roots of `scripts/check-filesystem-boundary.sh`, `crates/lushtext-core/tests/workspace_terminology.rs`, `sonar-project.properties`, `scripts/check-workflow-boundaries*`, `scripts/check-agent-docs*`, `scripts/check-workflow-timeouts.py`, `cargo deny`, hakari, and `.config/nextest.toml`. Record which of them would see `formal/evaluation/` or `docs/next/formal-verification-quint-vs-tlaplus.md`, and adjust the design if any would.
- [ ] 1.2 Create `formal/evaluation/{quint,tlaplus}/README.md`. Each README gives the purpose, the "disposable evaluation model, not a maintained property" status, and a pointer to the report.
- [ ] 1.3 Add `build/formal-evaluation/` to `.gitignore`.
- [ ] 1.4 Run `make check` and `make check-policy` to confirm the empty home breaks no gate.

## 2. Tooling and reproduction

- [ ] 2.1 Resolve the latest stable versions of Quint (npm), Apalache, `tla2tools.jar`, and optionally TLAPS from current release pages or Context7. Confirm the current `quint verify --backend tlc`, `--temporal`, and `quint compile --target tlaplus` behaviour and the state of Quint Connect.
- [ ] 2.2 Write `scripts/formal-evaluation.sh`:
  - `install` into `build/formal-evaluation/tools/`, recording versions and checksums;
  - one subcommand per target (`t1`, `t2`, `t3`), plus `all` and `report-data`;
  - per-run `/usr/bin/time -v` capture under `build/formal-evaluation/runs/`;
  - `FORMAL_EVAL_TIMEOUT` (default 30 min) and a memory ceiling;
  - a clear missing-tool message.
- [ ] 2.3 Add a local-only `make formal-evaluation` target, with `FORMAL_EVAL_TARGET`. Add it to `.PHONY` and `help`. Confirm it is not a prerequisite of `check`, `check-policy`, `test`, `kani`, or `end-user-smoke`, and that no workflow references it.
- [ ] 2.4 Install the tools and record the install time, download size, and prerequisites for the install and CI cost metric.

## 3. T1 calibration: the durable-write protocol

- [ ] 3.1 Model `WriteProtocol` (S0–S6, copy fallback, POSIX axioms A1–A5 as in `write_protocol/kani_proofs.rs`) in Quint. Check crash atomicity and classification soundness with the simulator, Apalache, and `--backend tlc`. Log every fix iteration.
- [ ] 3.2 Model the same protocol in TLA+ (PlusCal if it fits), and check it with TLC and optionally Apalache.
- [ ] 3.3 In both tools, add the "`SyncTemp` answered without a sync" variant and confirm the torn-destination counterexample is found. Record the trace next to Kani's.
- [ ] 3.4 Record the T1 metrics rows. A tool that fails to reproduce either known answer is marked as such before any later target is compared.

## 4. T2 scale: the two-process draft journal

- [ ] 4.1 Model the `journal_core` decisions, the `kani_proofs.rs` model disk, and all 13 actions (with an I/O fault on every step) with one process in both tools. Confirm S1–S4 hold as the control.
- [ ] 4.2 Add the second process actor (the K8 `step_as` semantics). Confirm both K8 traces are found at 8 actions after both startups (S1 loss; body without an entry), and only the body-without-entry trace at 6.
- [ ] 4.3 Measure wall time, peak RSS, and distinct states or solver depth at 6, 8, 10, 12, and deeper for each checker (Quint simulator, Apalache, TLC through Quint, and TLC on hand-written TLA+). Stop each at the timeout or memory ceiling and record the ceiling. Run one checker at a time.
- [ ] 4.4 Compare the results against Kani's 500.8 s / about 9 GB at 6 actions and 1017.5 s / about 18 GB at 8, and record counterexample readability against K8's decoded traces.

## 5. T3 design sketch: the N6 inter-process lock

- [ ] 5.1 Model the data-directory `flock` lease with kernel release on process death, a crash while holding it, and both second-instance variants (read-only, refuse-to-start), over the T2 journal abstraction, in Quint and in TLA+.
- [ ] 5.2 Check the safety properties: at most one writer per data directory, S1 across processes, and no body without an entry. Confirm the K8 traces are now excluded.
- [ ] 5.3 Check liveness under explicitly stated weak or strong fairness: a waiting or read-only instance eventually becomes the writer after the holder exits or crashes, and there is no permanent read-only wedge. Show that each liveness property fails without its fairness assumption.
- [ ] 5.4 Record the T3 metrics, including which liveness and fairness constructs each tool actually checked.

## 6. Open exploration (time-boxed)

- [ ] 6.1 Before starting any idea, declare its budget in the report's exploration log (default 2 h, maximum 4 h, 24 h in total).
- [ ] 6.2 E1: Quint Connect replay against `journal_core`, then against the real `draft_service` over a tempdir, in a scratch crate outside the workspace. Compare it with N11.
- [ ] 6.3 E2: trace validation of `GetWorkflowEvents` / `workflow-events.json` smoke artifacts against a small spec, reading existing artifacts only.
- [ ] 6.4 E3 and E4: turn a TLC or Apalache counterexample into a Kani assumption, a proptest regression, or an N11 sequence; and generate conformance sequences from a spec. Keep any produced test code in scratch space, not in production crates.
- [ ] 6.5 E5: Apalache against TLC crossover on the T2 model.
- [ ] 6.6 E6: one TLAPS lemma. Compare it with the in-Kani unbounded attempts in `extend-closed-loop-geometry-verification`.
- [ ] 6.7 E7: review the T3 spec as a design document with a fresh agent and with the maintainer, and record the questions each raises.
- [ ] 6.8 E8: a `stateright` T2 comparator calling `journal_core`, as a standalone package under `formal/evaluation/stateright/` with its own `[workspace]` table, added to the root `Cargo.toml` `exclude`. Confirm `cargo deny` and hakari are unaffected.
- [ ] 6.9 Record any agent-added idea (E9 and on) with the same log format. Mark every unattempted idea "not attempted", with a reason.

## 7. Report and decision

- [ ] 7.1 Write `docs/next/formal-verification-quint-vs-tlaplus.md` with the following, following the folder-set terminology guard:
  - the tool versions and checksums;
  - the machine;
  - the metrics tables per target;
  - the exploration log, failures included;
  - the decision section.
- [ ] 7.2 In the decision section, choose among: keep Kani only; TLA+ as the N6 design-sketch tool; Quint, with or without Connect; or `stateright`. Combine options only where each part has evidence. State the losing options' evidence, the fate of `formal/evaluation/`, and any follow-up change.
- [ ] 7.3 Update `docs/next/formal-verification.md` (§2 and the deferral inventory) and the "Tool criteria" section and change table of `docs/next/formal-verification-next.md` to cite the measured results.
- [ ] 7.4 Update the AGENTS.md build-command list and `.agents/rules/build.md` for `make formal-evaluation`, marked local-only.

## 8. Verification

- [ ] 8.1 On a clean checkout without the evaluation tools installed, run `make check`, `make check-policy`, and `make test`. They pass unchanged, and `make formal-evaluation` fails with the install message.
- [ ] 8.2 Confirm that `git diff` touches no production crate, no Kani harness or shard table, and no CI workflow.
- [ ] 8.3 Run `make formal-evaluation FORMAL_EVAL_TARGET=t1` from the recorded tool versions. It reproduces the T1 known answers.
- [ ] 8.4 Run `openspec validate evaluate-quint-and-tlaplus-empirically --strict`.
