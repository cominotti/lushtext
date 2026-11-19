# Tasks

Tests: none (no behaviour change); the probe's `--check` and its catalogue entry are the evidence.
Kani: none (no proven module touched).
Timing: the spike's own verdict (design D1-D2), through `scripts/measure.sh`.
Start: only after every edit, spec and queued change of the programme coordinator's session has finished (maintainer, 2026-11-19), and after the maintainer places this change in `PARALLEL-LEDGER.md`. Task 0.1 re-measures the baseline first.

## 0. Re-measure the baseline

- [ ] 0.1 Re-count the factories, the `next_sibling` calls and the `set_data` sites on the then-current `origin/main`, and update `proposal.md`. Verify with `openspec validate spike-list-row-construction --strict`.
- [ ] 0.2 Have a Fable subagent cross-review the re-measured artifacts (read-only, against `openspec/config.yaml`'s rules and the then-current `origin/main`), and apply its findings.

## 1. Probe and sample

- [ ] 1.1 Add the probe with the three arms and the observation fields (design D1) to `gtk-lush-axioms`, its sample with `SampleFlags` and `support::guard`, and the catalogue entry with the axiom id the coordinator assigns. Register it in the adoption lab's `axiom_probes` test. Verify with `make check-gtk-axioms` and `make gtk-axiom-sample AXIOM=<id> CHECK=1`.
- [ ] 1.2 Commit through `heavy.sh`, signed, and push.

## 2. Measurement and decision

- [ ] 2.1 Run the interleaved rounds through `scripts/measure.sh` on the shipped toolkit, then the `--check` sample on the GNOME 50 floor (`scripts/run-gtk-axioms.sh runtimes --only <id>`). Queue in `MEASURE-QUEUE.md` if the load gate refuses. Record the artifacts and figures in the change's handoff.
- [ ] 2.2 Apply the pass bar (design D2). Write the ledger entry and the "List rows" decision section of `docs/next/refactor-1-disposition.md`. Verify with `make check-gtk-axioms` and `make check-agent-docs`. Commit through `heavy.sh`, signed, and push.

## Workflow follow-up

- `adopt-typed-row-state-and-dialog-futures` starts from the recorded decision.
