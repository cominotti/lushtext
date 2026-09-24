# Quint evaluation models

**Status: disposable evaluation model, not a maintained property.** These
files exist only as evidence for the tool decision recorded in
[`docs/next/formal-verification-quint-vs-tlaplus.md`](../../../docs/next/formal-verification-quint-vs-tlaplus.md).
Kani stays the single maintained formal tool
(`openspec/specs/formal-verification-kani/spec.md`). No build, test, lint,
policy, audit, or CI gate reads this directory, no production crate depends
on it, and no claim in rustdoc, specs, or the programme record rests on it
alone.

## Files

| File | Target | Re-expresses | Kani counterpart |
|---|---|---|---|
| `write_protocol.qnt` | T1 | `WriteProtocol::step` (`crates/lushtext-core/src/services/filesystem/write_protocol.rs`) and the model disk of `write_protocol/kani_proofs.rs` | `a_crash_never_tears_the_destination`, `every_classification_describes_the_destination`, `skipping_the_temp_sync_tears_the_destination` |
| `move_rename.qnt` | T1 | `MoveProtocol::step`, `RenameProtocol::step` | `a_move_removes_its_source_only_after_the_copy_is_durable`, `a_completed_rename_synced_every_directory_it_mutated` |
| `journal.qnt` | T2 | every `journal_core.rs` decision the harness calls, plus the environment of `draft_service/kani_proofs.rs` (`Journal`, `step`, `step_as`, `startup`, S1–S4) | `journal_invariants_hold_under_crashes`, `a_second_writer_breaks_the_journal_invariants` |
| `journal.qnt`, modules `k8_traces_connect` and `k3_service` | E1 | Quint Connect replays against the Rust port and the real `draft_service` (`formal/evaluation/stateright/tests/`) | — |
| `data_dir_lock.qnt` | T3 | a design sketch of the N6 inter-process data-directory lock over the T2 journal abstraction | none (new ground) |
| `data_dir_lock.checks` | T3 | the checks `scripts/formal-evaluation.sh t3` runs, one per line | — |

## Tool version

Quint 0.32.0 (npm `@informalsystems/quint`), Rust evaluator 0.6.0, driving
Apalache 0.58.3 (the newest release Quint 0.32.0 can drive) and the TLC
bundled in that Apalache jar. Installed by `scripts/formal-evaluation.sh install`
into the gitignored `build/formal-evaluation/tools/`.

## Commands

```sh
make formal-evaluation FORMAL_EVAL_TARGET=install
make formal-evaluation FORMAL_EVAL_TARGET=t1     # or t2, t3, all
```

Individual checks, with `QUINT_HOME=build/formal-evaluation/tools/quint-home`:

```sh
quint run write_protocol.qnt --invariant safety --max-steps 20 --max-samples 200000
quint verify write_protocol.qnt --apalache-version 0.58.3 --max-steps 17 --invariant safety
quint verify write_protocol.qnt --backend tlc --apalache-version 0.58.3 --invariant safety
quint verify write_protocol.qnt --backend tlc --apalache-version 0.58.3 --init initSkipsTempSync --invariant runWriteAssertions
quint test journal.qnt --main k8_traces
quint verify journal.qnt --main k8_6 --backend tlc --apalache-version 0.58.3 --invariant invariants
```

## Bounds

The same as the Kani harnesses: T1 is finite (at most 15 protocol steps plus
a crash), so every checker explores it completely. T2 uses 3 ids, at most 3
edits, and `MaxSteps` actions after the startup(s), fixed by the instance
modules at the end of `journal.qnt` (`k3_8`, `k8_6`, `k8_8`, …).
