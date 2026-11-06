## Context

`model/protected_install.rs` (C3) is the GTK-free planner behind every sliced protected install. `plan_protected_install(text, ranges, &mut cursor, budget, limits)` returns the next `ProtectedSlice { end, kind, last_of_tail }` and advances a `ProtectedInstallCursor { offset, paragraph, next_range, phase }`. `phase` is `ParagraphStart`, `InTail` or `AtDelimiter`. One call is one of three steps:

- **hidden-tail** (`InTail`): the slice ends at the next delimiter, or at `offset + tail_slice_bytes` rounded down to a safe boundary, or at the text end. If a delimiter sits exactly at `offset`, the step continues into a visible step.
- **head** (`ParagraphStart` on the next folded paragraph): it ends at the `head_chars`-th scalar, or earlier at the content end.
- **visible** (`ParagraphStart` otherwise, or `AtDelimiter`): `visible_slice` takes whole paragraphs under the `PacedSlice` byte/character budget and stops before the next folded paragraph.

Each property C3's design D5 listed is either a property of one slice and the step that made it (K3, K4, K5, K6, the head end of K8, the per-slice half of K9), or it follows from contiguity and progress (K1 and K2: slices are contiguous because `cursor.offset == slice.end` after every step).

The whole-plan harnesses ran the planner to completion over symbolic documents. CBMC then has to unroll every step for every document at once, and it exhausted 16 GiB at 6 bytes and failed the unwinding bound at 4 (`docs/next/formal-verification.md`, "Sliced protected install", table of runs `kani-3` … `kani-7`). A one-step harness unrolls one call.

Proof strength (task 3.11) and the eight local-only runner budgets are the other two deferred items this change closes.

## Goals / Non-Goals

**Goals:**
- Kani proves K1–K6, K8 and K9 for every plan of a stated document domain, by induction over one step of the production planner. No copy of the planner is used, and the rounding seam is called only as production calls it.
- Proof strength is measured for `model/protected_install.rs` with a Kani arm that includes the step harnesses, and every survivor is triaged.
- Every Kani shard budget in the table comes from dispatched runner runs, including the shard this change adds.

**Non-Goals:**
- Changing the planner's behaviour, budgets or constants. Production code changes only for a defect that triage finds, and then failing-first.
- Proving the K10 derivation's fit and optimality. That needs 128-bit multiplications with a symbolic divisor, which did not finish in 20 minutes and stays property-tested. The same holds for anything outside the stated domain.
- Changing `kani.yml`, the shard margins or the proof-strength tool.
- Any GTK, widget or responsiveness-lane work. No toolkit behaviour is newly relied on, so there is no axiom, probe or sample to add and no lane case to change.

## Decisions

### D1. One-step harnesses over a well-formed cursor, not a bounded whole plan

The bounded whole-plan form was re-attempted on paper and rejected. The table of five domains shows it does not fit below 6 bytes even without C2's fold in front, and 4 bytes is too small to hold a fold, a CRLF and a multibyte tail together. One step does fit. The spec-stage prototype (`.claude/handoffs/close-protected-install-proof-strength/kani_step_spike.rs`, log `spike-1.log`) measured three harnesses, each on any document of at most 8 bytes built from the C3 alphabet (`a`, `é`, `€`, `𝄞`, `\r`, `\n`, U+2029, whole scalars only), any cursor offset, up to two fold ranges in increasing paragraph order with `head_chars` 1..=3, a tail budget of 4..=7, and a byte budget of 1..=8 with an optional character budget of 1..=3:

| Harness | Verification time | Peak RSS | Wall (with build) |
|---|---|---|---|
| hidden-tail step | 93.6 s | 2.30 GiB | 155 s (cold build) |
| head step | 87.5 s | 2.25 GiB | 101 s |
| visible step | 169.4 s | 2.44 GiB | 183 s |

Conditions: `main` `fbf0f875`, Kani 0.68.0, CBMC 6.11.0, `taskset -c 8-23`, `scripts/with-memory-cap.sh --address-space 16G`, one CBMC at a time. The 1-minute load was 0.59 before the first harness, 1.48 between them and 1.11 after. Every cover was satisfied and every assertion held. The prototype's assertions are a subset of D3's, so the final figures will be higher. D5 sizes the shard for that.

Alternatives considered:
- **Function or loop contracts** for `visible_slice`/`first_delimiter_in`: experimental, and the slice-loop attempt failed on resources (programme record, phase 3).
- **A smaller whole plan with a fixed shape** (one giant paragraph, concrete bytes): this is what K11 already does. It proves one counterexample's absence, not the domain.
- **Keep property tests only:** the deferral explicitly asked for this follow-up, and it now fits.

### D2. The well-formed cursor predicate and the induction

`well_formed(text, ranges, cursor)` is a harness-side predicate. It reads the cursor's private fields from the child module, as `kani_proofs.rs` already does for `plan_with`, and holds when:

1. `cursor.offset ≤ len` and is a safe point: a char boundary, not between `\r` and `\n`.
2. `cursor.paragraph` is the number of paragraph delimiters wholly before `offset`. The harness counts them with its own small delimiter scanner, written independently from `next_paragraph_break` so that it does not copy it.
3. `ranges[..next_range]` all have `paragraph < cursor.paragraph`, or `== cursor.paragraph` when `phase` is `InTail` or `AtDelimiter` (the folded paragraph being finished). `ranges[next_range..]` all have `paragraph ≥ cursor.paragraph`, or `> cursor.paragraph` in those two phases.
4. Per phase:
   - `ParagraphStart`: `offset` is a paragraph start (0 or just after a delimiter).
   - `InTail`: `next_range ≥ 1`, `ranges[next_range − 1].paragraph == cursor.paragraph`, and `offset` lies within the content of that paragraph (at or before its content end), at or after the head end of `ranges[next_range − 1]`.
   - `AtDelimiter`: `next_range ≥ 1`, `ranges[next_range − 1].paragraph == cursor.paragraph`, and `offset` is the content end of that paragraph: the start of its delimiter, or `len`.

   The `next_range − 1` clauses of the last two phases are what make a hidden-tail slice provably part of a *folded* paragraph's tail. Without them the predicate would admit an `InTail` cursor in an unfolded paragraph, and the step harness would prove that hidden-tail slices are well-cut without proving that they hide only folded text (the homogeneity half of "Every slice fits the budget derived from B_settle").

The fold plans are any ranges in strictly increasing paragraph order with `head_chars` 0..=3, as C3's whole-plan draft used. That is a superset of what C2's `fold_step` produces, so a property over these plans holds over C2's. K8b separately proves C2's fold of a lone paragraph. `tail_chars` is not read by the planner.

Harnesses:
- **`step_initial_cursor_is_well_formed`**: `ProtectedInstallCursor::new()` satisfies `well_formed` for every document and plan of the domain.
- **`step_hidden_tail_…`, `step_head_…`, `step_visible_…`**: from any well-formed cursor of the matching phase, `plan_protected_install` returns `None` exactly when `offset == len`. Otherwise it returns a slice satisfying D3, and the cursor afterwards is well-formed with `offset == slice.end`. The hidden-tail harness does **not** assume that the offset is off a delimiter (the prototype did): a delimiter at `offset` must take the `continue` path into a visible slice, and the harness asserts that slice instead.
- Each harness carries the `kani::cover!` calls D3 lists, so a vacuous proof shows.

The induction is written once, in the harness module's doc comment and in the programme record. The initial cursor is well-formed. Every step from a well-formed cursor yields a valid slice and a well-formed cursor whose offset is no smaller. A step that does not advance the offset (an empty head) moves the phase forward, and the next step advances it. So every plan terminates (K1), its slices are contiguous from 0 to `len` (K2), and every per-slice property holds for every slice of every plan (K3–K6, K8, K9). This is a proof over every plan of the 8-byte domain, not a sample.

### D3. The per-step assertions (what K1–K6, K8 and K9 become)

| Property | Step assertion |
|---|---|
| K1 progress | `slice.end > offset`, except a head slice of an empty head, where the phase moves from `ParagraphStart` to `InTail` (a well-founded measure: `(offset, phase rank)`) |
| K2 exact bytes | `cursor.offset == slice.end` after the step; `None` only when `offset == len` |
| K3 safe ends | `slice.end` is a char boundary and not between `\r` and `\n` |
| K4 homogeneity, `last_of_tail` | a head or hidden-tail slice contains no delimiter byte, and lies in paragraph `ranges[next_range − 1].paragraph` after the step; a visible slice ends at a paragraph start or `len`, and every paragraph whose content it holds has an index below `ranges[next_range].paragraph` (if any), so it holds no folded content (from `AtDelimiter` it may also hold the finished folded paragraph's delimiter); `last_of_tail` exactly when a hidden-tail slice ends at a delimiter or `len` |
| K5 bounds | hidden-tail: `len ≤ B` and, unless `last_of_tail`, `len ≥ B − 3`; visible: within the byte and character budget unless it holds exactly one unit, because `PacedSlice::admit` admits its first unit unconditionally (one paragraph with its delimiter, or, from `AtDelimiter`, the finished folded paragraph's delimiter alone) |
| K6 consecutive tail slices | a hidden-tail step that is not `last_of_tail` leaves the cursor `InTail` at a non-delimiter |
| K8 head end | a head slice ends at the `head_chars`-th scalar of the paragraph, or at its content end if that comes first, and is followed by `InTail` |
| K9 slice count | follows from K5's `B − 3` lower bound on every tail slice but the last, so it needs no assertion of its own |

Covers: a tail end rounded by 1, 2 and 3 bytes; a tail of exactly one budget; `last_of_tail` at a delimiter and at `len`; the `continue` path; an empty head; a visible slice cut by the byte budget, by the character budget, and before a folded paragraph; a `\r\n` straddling a budget.

### D4. Domain: 8 bytes, widened only if it fits

The spec states the document domain at **8 bytes**, which is what was measured to fit. Implementation tries 10 bytes, the domain C3's spec named, once per harness under the same cap. 10 is adopted only if every harness stays within 6 GiB and the new shard within D5's budget. Otherwise 8 stays, and the 10-byte figures are recorded. Either way the spec text, the harness docs and the planner rustdoc state the adopted bound, and none claims more.

### D5. A shard of its own: `core-protected-install-step`

The step harnesses go into a new `scheduled` shard with filter `model::protected_install::kani_proofs::step_`. Shard filters are substrings of the qualified harness name (`Shard.owns`), and `core-protected-install`'s filter today is the whole module, `model::protected_install::kani_proofs::`, which would also own every `step_` harness: `make check-kani-shards` would then fail ("matches several shards") and `make kani KANI_SHARD=core-protected-install` would run the step harnesses too. Its filter is therefore narrowed to its four harnesses by name (`…::protected_clear_plan_`, `…::settle_budget_derivation_`, `…::unrounded_tail_end_`, `…::one_paragraph_fold_`), none of which contains `step_`. The step harnesses are not added to `core-protected-install`, whose local figure is already 4.54 min / 7.94 GiB (K11's 249 s and 7.9 GiB). The prototype's verification times sum to 5.9 min before D3's extra assertions, and runner times have run 1.2–2× local in earlier measurements. Under the fit order (split first), keeping them separate also keeps each shard's runner figure readable. If the full harnesses exceed 15 local minutes together, they are split further by harness before any bound is reduced. Every step harness is added to `ORACLES` for `protected_install.rs` with its measured local seconds and tier (`ci` if under 60 s, `local` otherwise).

### D6. Proof strength: when and how

The run happens **after** the step harnesses verify on the unmutated tree, so the Kani arm measures them (the deferral's reason). Command: `make proof-strength PROOF_STRENGTH_MODULE=crates/lushtext-core/src/model/protected_install.rs`, with `PROOF_STRENGTH_TIER=all` (the default). The Kani arm is sharded 2 ways (`PROOF_STRENGTH_ARM=kani PROOF_STRENGTH_SHARD=0/2` and `1/2`, each with its own `PROOF_STRENGTH_WORKTREE`, each a 16 GiB admitted scope) and the tests arm runs once, unsharded. Two scopes fit the 48 GiB host slice with its 16 GiB reserve, so a third is not started.

Population at `fbf0f875`: 143 mutants, floor 41, none outside the oracle scope (`make proof-strength-list`). Cheapest-first ordering puts K10 (0.3 s), K8b (1.5 s) and K7 (7 s) before the step harnesses, so mutants of `for_settle_budget` and `clear_ranges_for_turn` die cheaply. The estimate is 6–10 hours serial for the Kani arm.

Triage classes are the spec's three. This module has known structural cases:
- Mutants inside `#[cfg(not(kani))]` blocks (the `memchr` fast path in `first_delimiter_in`) are invisible to Kani by construction. They are triaged **accepted gap** (constrained by the tests arm, which compiles that path) unless the tests arm also lets them survive, in which case a unit test is added.
- K11 runs `plan_with` with `unrounded`, so mutants of `round_down_to_safe_boundary` are the step harnesses' job (K3).
- A survivor in a `const` value (`PROTECTED_*_COST`, `MARGIN`) is checked against the unit test that pins the derived constants. Kani does not see those values except through K10's `u64` domain.

A timeout is re-run once on an idle host, at load below 4, before it is triaged.

### D7. Runner budgets: two full dispatches of the final branch

`kani.yml` has `workflow_dispatch` with no inputs, and a dispatch runs every shard. That is how every earlier budget was measured, and it re-measures all of them for free, since public runners cost nothing. **No `shards` input is added.** Limiting a dispatch would save no money, would need a workflow change that itself has to be validated, and would skip the re-measurement that `core-paragraph-profile` and the older shards need anyway.

Procedure:
1. Rebase the branch onto `origin/main` once the step harnesses and any triage fixes are final. If PR #84 has merged by then, its constants are part of what is measured. The change does not depend on it either way.
2. `gh workflow run kani.yml --ref close-protected-install-proof-strength`, twice, sequentially. Each run's figures come from its `kani-measure-<shard>` artifacts (`gh run download`): wall time and peak RSS per shard.
3. Records:
   - every `provisional`/`PENDING DISPATCH` shard and the new shard: the larger of the two runs, rounded up to 0.1, with both run ids in `measured_in`. The local figures and their notes are removed.
   - every other shard: it keeps its figure unless a run exceeds it, in which case the larger value is recorded with a `raised by runs …` suffix (the precedent of `PROOF_STRENGTH_RAISED_IN`).
   - `core-paragraph-profile` explicitly: if a run exceeds 8.3 GiB, the figure is raised. If it exceeds 12 GiB, the shard is split by harness (the fit order), and no bound is reduced.
4. A shard that breaks a margin on the runner is split, and the two dispatches are repeated for the split shards.
5. `make check-kani-shards` passes on the final table.

Gate decision: **an owner-keyed gate, not a gate that refuses local figures.** The table entry has to exist (with some figure) before the branch can be pushed and dispatched, because `check-kani-shards` runs in the pre-commit hook and in the workflow's own `shards` job, so a gate that refused local figures would block the dispatch that replaces them. But a written obligation alone is what failed: C2's record said the change "must not merge on it", and it merged. So `budget_problems` gains one rule. A `measured_in` that marks a local figure (it contains `PENDING DISPATCH` or `provisional`) SHALL name its owner as `PENDING DISPATCH (owed by <change>)`, and the gate fails when it names no owner, or when `openspec/changes/<change>/` is not an active change directory (archived to `openspec/changes/archive/…` or never created). While the owning change is open, its branch can push and dispatch. The commit that archives it fails the pre-commit hook while it still owes a figure. Implementation first re-labels the eight existing pending records as owed by this change, because the changes that added them are archived (the new gate fails on `main`'s table as it stands, which is the failing-first evidence), and then the dispatches replace them. The gate's self-test gains three cases: a pending record with no owner, one owed by an archived change, and one owed by an active change (passes). The programme record still lists any record that remains pending.

### D8. Documentation

- `docs/next/formal-verification.md`, "Sliced protected install": the "Not proved by Kani" paragraph and table move to history. A section describes the step harnesses, D2's induction, the domain and the figures. The proof-strength table is added, the "Shard measured locally" note is replaced by the runner figures, the C3 deferral-inventory entry is closed, and the provisional-figure note of C6 (line ~1746) and the "Runner figures pending dispatch" notes of C2 are replaced.
- `docs/next/long-line-documents.md`: only the two queue entries (C3 task 3.11 proof strength, provisional Kani CI figures). Neither exists on `main` today, so one row is added for this change, placed away from the PT row that PR #84 edits.
- `openspec/specs/main-thread-responsiveness/spec.md` through the delta. `.agents/rules/build.md`, "Shard gates and budgets": D7's owner-keyed pending rule is a gate change, so the bullet states it beside "developer-machine timing does not count". Its pull-request shard list (`widgets-geometry`, `core-geometry-policies`) is unchanged.
- `docs/mutation-testing.md`: no edit unless the tool's usage changes.

## Risks / Trade-offs

- [The full D3 assertions make a step harness exceed the cap or 15 minutes] → Split by phase sub-case (for example, visible step from `ParagraphStart` and from `AtDelimiter` as two harnesses). Try the CaDiCaL solver only with the verdict unchanged. Reduce the domain to 6 bytes only as a recorded last resort, stating the weakened property.
- [`well_formed` is too strong, so the induction proves a smaller set of plans than production reaches] → `step_initial_cursor_is_well_formed` anchors it. Each clause is also checked for necessity by a cover that a reachable cursor satisfies it at its boundary. The programme record states the predicate in full.
- [`well_formed` is too weak, so a step harness fails on an unreachable cursor] → This is triaged like any counterexample: decide whether the predicate (domain) or the planner (code) is wrong. A reachable failure is a defect fixed failing-first.
- [Proof-strength Kani arm takes most of a day] → It is sharded 2 ways and resumable (`proof-strength.py` keeps per-mutant results). Runs only start at load below 4, and the waits are recorded.
- [A dispatched run is slower than the margin on a busy runner] → The larger of two runs is recorded. A shard over 25 min is split and re-dispatched, and the 30-minute `timeout-minutes` is never raised.
- [PR #84 lands mid-change] → Rebase before the two dispatches, then measure. The overlap is one doc-comment line in `protected_install.rs` and one `ORACLES` line in `kani-shards.py`, both disjoint hunks.

## Migration Plan

Not applicable: verification code, records and the shard table only. The rollback is to revert the change. The shipped binary is identical unless triage fixes a defect, and that fix then carries its own test.

## Open Questions

None for the maintainer. The domain choice (D4) and any shard split (D5, D7) are decided by measurement during implementation and recorded.
