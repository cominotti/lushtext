## Why

Two Kani evidence gaps from the long-line programme are still open, and both make the formal-verification record claim less than it should, or more than it can show.

1. **The protected install planner is proved only at its edges.** `sliced-protected-install` (C3, archived 2026-11-02) wrote the whole-plan harnesses K1–K6, K8 and K9 (progress, exact bytes, no slice end inside a scalar or a CRLF, homogeneity, the per-slice bounds, the visible/hidden switch, the slice count). None of them fitted CBMC: every domain from 10 down to 4 bytes ran out of the 16 GiB cap or past 20 minutes. The planner's core properties are therefore property-tested, which samples the domain and covers none of it. Task 3.11 (proof strength for `model/protected_install.rs`) was deferred with them, because with K7, K8b, K10 and K11 alone a mutation run would measure the property tests, not Kani. The deferral named the follow-up: **one-step harnesses over a symbolic, well-formed `ProtectedInstallCursor`**, which the per-slice properties' locality (C3 design D5) makes sufficient.
2. **Eight Kani shard budgets come from a developer machine.** `scripts/kani-shards.py` marks `core-post-open-settle` `provisional`, and `core-protected-fold`, `core-protected-fold-shape`, `core-protected-fold-mapping`, `core-protected-view`, `core-protected-view-navigation`, `core-protected-install` and the settle-evidence addition to `widgets-slice-loop-requests` `PENDING DISPATCH`. The `formal-verification-kani` spec says developer timing does not count as a measurement, and the record of C2 says the change "must not merge on it". It merged anyway, and the two dispatched `kani.yml` runs that would replace those figures were never made. `power-of-two-thresholds` (PR #84, open, not merged) also measured `core-paragraph-profile` at 8.8 GiB locally, above its 8.3 GiB runner figure.

The first gap is now closable. The spec stage measured a prototype of the three one-step harnesses on `main` `fbf0f875`, at Kani 0.68.0 / CBMC 6.11.0, one at a time under `scripts/with-memory-cap.sh --address-space 16G`, with the host's 1-minute load between 0.59 and 1.49:

| Prototype harness (8-byte document, symbolic cursor) | Verification time | Peak RSS | Covers |
|---|---|---|---|
| hidden-tail step | 93.6 s | 2.30 GiB | satisfied |
| head step | 87.5 s | 2.25 GiB | satisfied |
| visible step | 169.4 s | 2.44 GiB | satisfied |

The whole-plan harness never finished at 4 bytes. The one-step harnesses finish in 1.5–3 minutes at 8 bytes, using about a seventh of the cap. The mutation run can therefore measure Kani, as the deferral wanted.

## What Changes

- **One-step harnesses over a well-formed cursor** (`model/protected_install/kani_proofs.rs`, new shard `core-protected-install-step`). They cover the hidden-tail, head and visible steps (the visible step also from a delimiter) of the production `plan_protected_install`, on any document of the document domain (at most 8 bytes over `a`, `é`, `€`, `𝄞`, `\r`, `\n`, U+2029), any fold plan of up to two ranges in paragraph order, any tail budget of 4–7 bytes and any small byte and character budget. For each step they prove the per-slice properties: progress, a safe end (a char boundary not between `\r` and `\n`), homogeneity, the hidden-slice bounds `B − 3 ≤ len ≤ B` (all but a tail's last), the visible budget, the head ending exactly at the fold point, `last_of_tail` exactly at a tail end, and that the cursor is well-formed after the step. A `well_formed` predicate, plus a harness that proves it holds for the initial cursor, makes the steps an induction. K1–K6, K8 and K9 then hold for every plan of the domain, and the reasoning is written down beside the harnesses and in the programme record.
- **Proof strength for `model/protected_install.rs`** (C3 task 3.11): `make proof-strength PROOF_STRENGTH_MODULE=crates/lushtext-core/src/model/protected_install.rs`, both arms, recorded in `docs/next/formal-verification.md` with mutant and kill counts per oracle, the revision and the tool versions. Every Kani-oracle survivor, timeouts included, gets exactly one triage class. A survivor that exposes a defect is fixed failing-first.
- **Measured runner budgets for every pending shard.** Two `workflow_dispatch` runs of `kani.yml` on the change branch's final form. Their figures replace every `provisional` and `PENDING DISPATCH` record, including the new shard and the re-measured `core-protected-install`. `core-paragraph-profile` is re-measured against its 8.3 GiB figure, and any older shard that the runs measure above its recorded figure is raised, as earlier changes did. `make check-kani-shards` passes on the result.
- **Spec and record text match what is proved.** In `main-thread-responsiveness`, the "As implemented" paragraph of "The protected install slice plan is proved over a stated domain" changes from "property-tested, not proved" to "proved by the one-step induction at 8 bytes". The document-domain bullet states that bound. The rustdoc of `plan_protected_install` and the harness module's docs say the same. The deferral-inventory entry for C3 closes, and so do the 3.11 note and the provisional-figure note in the programme record.

No production code changes unless a survivor exposes a defect. No GTK behaviour is newly relied on (no axiom), and no user-visible behaviour changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: "The protected install slice plan is proved over a stated domain". The whole-plan properties are proved by one-step induction over the document domain at 8 bytes, instead of being property-tested only. The scenarios are kept, not renamed.
- `formal-verification-kani`: "Kani shard budgets are measured on the CI runner and enforced with margin". A shard recorded from local figures is explicitly pending, names its replacement runs, and is replaced by two dispatched runs before the change that added it is archived. Measured shards keep the larger of their old and new figures.

## Impact

- **Code:** `crates/lushtext-core/src/model/protected_install/kani_proofs.rs` (step harnesses, the `well_formed` predicate, generators), `crates/lushtext-core/src/model/protected_install.rs` (rustdoc domain only, unless triage finds a defect). `scripts/kani-shards.py`: the new shard, the `ORACLES` entries with measured seconds, and every pending record replaced.
- **Docs:** `docs/next/formal-verification.md` (C3 section, deferral inventory, proof strength, runner figures), `docs/next/long-line-documents.md` (the two queue entries only; PR #84 and #85 lines untouched), `.agents/rules/build.md` only if the shard list it names changes.
- **CI:** two dispatched `kani.yml` runs, about 30 shard jobs each. Workflow unchanged.
- **Local cost:** the proof-strength Kani arm, on the order of 6–10 hours serial for 143 mutants, sharded 2 ways in 16 GiB scopes. One CBMC per scope, never more than two scopes.
- **Not touched:** PR #84 (`power-of-two-thresholds`) and PR #85 (`avx2-health-scan-kernel`). Neither is a dependency. #84 touches one doc line of `protected_install.rs` and one `ORACLES` line, both disjoint from this change.
