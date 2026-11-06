## MODIFIED Requirements

### Requirement: The protected install slice plan is proved over a stated domain
The sliced protected install SHALL take its slices from one GTK-free, I/O-free planner, and Kani harnesses SHALL check that production planner, C1's paragraph-boundary function and C2's fold placement step directly, never a copy. Each hidden-tail slice SHALL end at the previous slice's end plus `tail_slice_bytes`, or at the tail end if that is nearer, rounded down to the nearest boundary that is a char boundary and not between `\r` and `\n`; `tail_slice_bytes` SHALL be at least 4, so every slice advances. The tail and clear slice budgets SHALL be derived from B_settle and the measured per-KiB costs by one pure, integer-only function. The harnesses SHALL apply only when the adoption gate adopted this change, and only for the adopted half.

The proofs hold on these domains, which the harnesses and the planner's rustdoc SHALL state:
- the **document domain**: every string of at most 8 bytes made of whole scalars from `a`, `é`, `€`, `𝄞`, `\r`, `\n` and U+2029; every fold plan of at most two ranges in strictly increasing paragraph order with a head of 0 to 3 characters (a superset of the plans C2's fold placement step makes); a tail slice budget of 4 to 7 bytes; a visible byte budget of 1 to 8 bytes with no character budget or one of 1 to 3; and, for the planner's step harnesses, every well-formed cursor over such a document and plan. The bound is 8 bytes because it is what was measured to fit. If the implementation measures 10 bytes within the shard budget, the bound becomes 10 and this text, the harnesses and the rustdoc change together;
- the **giant-paragraph domain**: one paragraph of the document domain whose content holds no delimiter, folded with a head of 1 to 3 characters; for C2's fold of one paragraph, every `u64` length with density parameters under which the density rule does not shorten a first paragraph's head; for the `should_panic` rounding-guard harness, one undelimited 12-byte paragraph of one 2-, 3- or 4-byte scalar;
- the **clear-plan domain**: at most four recorded ranges of 0 to 15 bytes each and a clear slice budget of 4 to 7 bytes;
- every `u64` input of the budget derivation.

Larger sizes SHALL be covered by property tests of the same assertions, and no rustdoc or spec text SHALL claim the Kani result beyond these domains.

**As implemented:** Kani proves the clear plan; C2's fold of one paragraph of any `u64` length; the budget derivation's freedom from panics and its admissibility; the `should_panic` rounding-guard harness; and, by one-step induction, every whole-plan scenario below over the document domain. The induction has two parts. A harness proves the initial cursor well-formed. The step harnesses prove that one call of the production planner from any well-formed cursor, in each of its hidden-tail, head and visible steps, returns a slice that satisfies every per-slice property (a safe end, homogeneity, the hidden and visible bounds, the head ending at the fold point, `last_of_tail` exactly at a tail end) and leaves a well-formed cursor at that slice's end, with progress measured by the offset and then the phase. Hence every plan of the domain terminates, its slices concatenate to the input, and every slice of it has those properties; the slice-count bound follows from the `B − 3` lower bound on every tail slice but the last. The derivation's fit and optimality stay property-tested (128-bit arithmetic over a symbolic divisor did not finish in CBMC), and the whole-plan properties at production sizes are property-tested beyond the domain. The whole-plan harnesses that run a plan to completion did not fit CBMC at any domain tried (2026-11-02, `docs/next/formal-verification.md`); the step harnesses replace them.

#### Scenario: The slices reproduce the exact bytes
- **WHEN** Kani plans any input of the document domain
- **THEN** every step advances, and the slices concatenated in order equal the input byte for byte

#### Scenario: No boundary splits a scalar or a CRLF pair
- **WHEN** Kani plans any input of the document domain
- **THEN** every slice end is a char boundary of the input and never falls between the `\r` and `\n` of a pair

#### Scenario: Every slice fits the budget derived from B_settle
- **WHEN** Kani derives the slice budget from any `u64` settle budget, per-KiB cost and margin
- **THEN** the derivation never overflows or panics, and an accepted budget is at least 4 bytes, at most the stated maximum slice, the largest size whose predicted cost, margin included, is within the settle budget, while an unattainable budget is reported as an error
- **AND** when Kani plans any input of the document domain with such a budget, every hidden-tail slice is at most that budget and every slice is wholly visible or wholly hidden

#### Scenario: Boundaries and the clear plan are placed exactly
- **WHEN** Kani plans any input of the document domain
- **THEN** a boundary follows a slice exactly when the next slice is a hidden-tail slice of the same paragraph
- **AND** the clear plan deletes each paragraph's recorded ranges from its first, each turn within the clear budget, and with the ordinary clear slices covers the buffer exactly once

#### Scenario: The tag boundary falls exactly at N_visible
- **WHEN** Kani plans one giant paragraph of the giant-paragraph domain
- **THEN** if the paragraph is longer than `N_visible` characters, the switch from visible to hidden slices falls at the end of the paragraph's `N_visible`-th character
- **AND** every byte before it is in the paragraph's head slice and every byte from it to the tail end of C2's fold range is in a hidden-tail slice, and the executor inserts the head untagged and hidden-tail slices with the protected tag

#### Scenario: The slice count of one giant paragraph is bounded
- **WHEN** Kani plans one giant paragraph whose content is `len` bytes, delimiter excluded, with tail slice budget `B`
- **THEN** the paragraph's content takes at most ⌈len / (B − 3)⌉ + 1 slices: one head slice and at most ⌈tail bytes / (B − 3)⌉ tail slices, each tail slice but the last being at least `B − 3` bytes

#### Scenario: A boundary choice that splits UTF-8 is caught
- **WHEN** the `should_panic` harness runs the production stepper with the rounding guard removed, so a tail slice ends at the raw previous end plus `B`
- **THEN** Kani finds a slice end inside a multibyte character and the harness passes as `should_panic`

