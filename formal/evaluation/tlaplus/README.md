# TLA+ evaluation models

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
| `WriteProtocol.tla` (PlusCal) | T1 | `WriteProtocol::step` and the model disk of `write_protocol/kani_proofs.rs` | the three write harnesses |
| `MC_WriteProtocolApalache.tla` | T1 | an Apalache wrapper: typed declarations plus `INSTANCE WriteProtocol` | — |
| `MoveRename.tla` | T1 | `MoveProtocol::step`, `RenameProtocol::step` | the move and rename harnesses |
| `Journal.tla` | T2 | every `journal_core.rs` decision the harness calls, plus the environment of `draft_service/kani_proofs.rs` | `journal_invariants_hold_under_crashes`, `a_second_writer_breaks_the_journal_invariants` |
| `MC_JournalApalache.tla` | T2 | an Apalache wrapper for `Journal.tla` (E5); the type aliases and operator annotations live in `Journal.tla` | — |
| `DataDirLock.tla` (plain TLA+; PlusCal's per-label fairness did not fit) | T3 | a design sketch of the N6 inter-process data-directory lock | none (new ground) |
| `LockProof.tla` | E6 | a TLAPS proof of the lock layer's at-most-one-writer invariant for every behaviour (`tlapm --cleanfp LockProof.tla`) | none |
| `JournalLiveness.tla` | E9 | L1 (a dirty editor becomes clean) as an unbounded liveness property under weak fairness | `a_dirty_editor_becomes_clean_without_faults` (bounded, k = 7) |
| `*.cfg` | — | one TLC model per check; `DataDirLock_*.cfg` carry an `\* expect:` line read by the script | — |

The PlusCal translation is committed inside each `.tla` file (between
`BEGIN TRANSLATION` and `END TRANSLATION`). After editing an algorithm, run
`java -cp build/formal-evaluation/tools/tlc/tla2tools.jar pcal.trans -nocfg <file>.tla`.

## Tool version

`tla2tools.jar` 1.7.4 (TLC 2.19, the latest stable release; 1.8.0 was a
pre-release on the day), Apalache 0.62.2, TLAPS 1.5.0 (for E6, installed by
hand from the release page into `build/formal-evaluation/tools/tlaps/`). Installed by
`scripts/formal-evaluation.sh install` into the gitignored
`build/formal-evaluation/tools/`.

## Commands

```sh
make formal-evaluation FORMAL_EVAL_TARGET=install
make formal-evaluation FORMAL_EVAL_TARGET=t1     # or t2, t3, all
java -cp build/formal-evaluation/tools/tlc/tla2tools.jar tlc2.TLC -workers 1 -config WriteProtocol.cfg WriteProtocol.tla
java -cp build/formal-evaluation/tools/tlc/tla2tools.jar tlc2.TLC -workers 1 -config Journal_k8_6.cfg Journal.tla
apalache-mc check --cinit=CInitMutant --init=Init --next=Next --inv=RunWriteAssertions --length=17 MC_WriteProtocolApalache.tla
```

## Bounds

The same as the Kani harnesses. `Journal.tla` bounds the loop with
`MaxSteps` and fingerprints states by `VIEW JournalView == <<j, n>>`, which
drops only the trace decoration `last`. The step counter must stay in the
view: an earlier journal-only view was unsound (review E7).
