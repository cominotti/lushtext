## 1. Measure before choosing (A85)

- [x] 1.1 A85 probe and sample: the idle order around an edit's first update, the worker removed with the language, the `--edit-cost` and `--flip-cost` modes, plain and `--paced`
- [x] 1.2 Real-file measurement of ordinary keys per language below the C6 gates (design D0), and the longest blank-line-free run distribution of the reference machine's checkouts
- [x] 1.3 Lane fixtures (default and `--flavor no-closers`) at the C6 gates and the C6b worst-case sizes: ordinary and flipping keys, plain against paced
- [x] 1.4 Decide the mechanism (design D1–D3) and the allowance (D2): the engine's 10 ms first-update slice measured, then `B_SETTLE_US`

## 2. Pure rule

- [x] 2.1 `model/reanalysis_pacing.rs`: step sizing, the allowance, the step bound; unit tests
- [x] 2.2 Kani harnesses (`reanalysis_pacing/kani_proofs.rs`) in shard `core-document-scale`, with oracle entries
- [x] 2.3 `ReductionReason::EditReanalysis` and `DocumentScale::with_edit_reanalysis_reduction`; the notice sentence; unit test and Kani harness for the join

## 3. Editor

- [x] 3.1 `ui/editor_page/reanalysis_pacing.rs`: the step idle at `G_PRIORITY_HIGH_IDLE - 1`, `ensure_highlight` on an empty range, the report handler, the downgrade through `adopt_document_scale`, evidence
- [x] 3.2 Hooks in the edit after-handlers; cancellation at a new document session and when the language is cleared; stand-down while an install or replacement owns the buffer
- [x] 3.3 Widget tests (`reanalysis_pacing.rs`), failing first against the editor without pacing

## 4. Chunked caps (design D4)

- [x] 4.1 Re-measure markdown and c at the chunked sizes with pacing on both fixture families; record the outcome

## 5. Verdict and gates

- [x] 5.1 Interleaved lane verdict (`main` against the branch) for the highlight and keystroke cases
- [x] 5.2 `make check`, `make test` (0 FLAKY), Kani `core-document-scale`, `make gtk-axioms`, the smokes the change affects
- [x] 5.3 Programme record, ledger, matrix row, AGENTS.md, README
