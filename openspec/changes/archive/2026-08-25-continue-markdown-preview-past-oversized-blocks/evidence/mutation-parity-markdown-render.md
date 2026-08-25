# Mutation parity: `services/markdown_render.rs`

Task 10.7 comparison against `evidence/mutation-baseline-markdown-render.md`.
Captured after the full change (sections 1-9) plus the survivor-killing tests
described below.

## Command

Identical to the baseline command, so the mutant set is comparable:

```
MUTANTS_RE='markdown_render' \
MUTANTS_EXCLUDE_RE='delete field (active|pending) from struct' \
MUTANTS_JOBS=3 MUTANTS_TEST_THREADS=4 MUTANTS_BUILD_JOBS=4 \
scripts/run-mutants.sh full
```

Scope was verified with `scripts/run-mutants.sh list` before running.

**The `MUTANTS_EXCLUDE_RE` does not actually exclude anything**, and this run
proved it: the excluded names still appear in `--list`, and both the baseline and
this run tested exactly the same **31** foreign mutants from
`services/single_flight.rs`, `services/palette/index.rs`, and
`services/palette/notes.rs`. It is kept in the command only so the two runs stay
byte-identical and therefore comparable. Foreign mutants are counted out **by
file attribution** instead:

```
grep -c markdown_render.rs mutants.out/{caught,missed,unviable}.txt
```

The baseline file has been corrected to say the same.

## Result (cargo-mutants 27.x, `cargo nextest`, 17m wall)

Both columns are **generated-based**: `generated (tested)` is the total
cargo-mutants attempted, and unviable mutants are part of it. Viable is
`generated - unviable`, which is the denominator for the kill rate.

| Outcome | Whole focused run | `markdown_render.rs` only | Baseline (`markdown_render.rs`) |
| --- | --- | --- | --- |
| Generated (tested) | 234 | 203 | 105 |
| Caught | 176 | 161 | 57 |
| Missed | 35 | 19 | 41 |
| Unviable | 23 | 23 | 7 |
| Timeout | 0 | 0 | 0 |
| **Viable** | 211 | **180** | **98** |
| **Killed** | 83.4% | **89.4%** | **58.2%** |

The whole-run column reconciles: 176 + 35 + 23 = 234 tested, of which 31 are the
foreign mutants above, leaving 203 attributed to `markdown_render.rs`. The
baseline's whole-run column reconciles the same way: 72 + 57 + 7 = 136.

Parity is met on both required axes:

- **Mutants still generate.** 203 tested against the baseline's 105. Nothing
  disappeared behind the rewrite; the planner grew and so did its mutable
  surface.
- **Killed count did not regress.** 161 caught against 57, and the absolute
  survivor count fell from 41 to 19 even though the mutable surface nearly
  doubled.

### Why unviable grew 7 -> 23

Unviable means cargo-mutants generated a mutation that does not compile, so it
was never a coverage opportunity. The growth is a direct consequence of two
design decisions, not of weaker tests:

- **Wildcard-free exhaustive matches.** Task 2.2 classifies every one of the 23
  `pulldown_cmark::Tag` variants with no `_` arm, precisely so a parser version
  bump becomes a compile error. `delete match arm` mutants against such a match
  cannot compile, because the match would no longer be exhaustive. That is the
  compile-time guarantee working as intended.
- **New struct-literal seams.** `MarkdownBlockOmission`, `MarkdownOmissionMarker`,
  `MarkdownCarrySignature`, `MarkdownOpenContainer`, `BlockCheckpoint`, and
  `UnretainedEmbedCounts` add `delete field` and `replace with Default::default()`
  mutants whose types have no `Default` or whose constructors require the deleted
  field.

## Survivors killed during 10.7

Nine tests were touched in `markdown_render.rs`'s co-located test module —
eight added and one (`image_flood_stops_at_descriptor_budget`) extended.
**Nothing was excluded.** They killed **10** mutants:

| Test | Mutants killed |
| --- | --- |
| `plan_metrics_report_the_source_byte_count` (new) | 2 — `delete field source_bytes` in `plan_markdown_inner` and in `source_limited_markdown_plan` |
| `a_sub_sliced_block_retains_a_segment_exactly_at_the_event_budget` (new) | 1 — `sub_slice_block` segment **event** comparison `>` -> `>=` |
| `two_blocks_exactly_filling_the_slice_byte_budget_share_one_batch` (new) | 2 — `append_unit` **byte** comparison `>` -> `>=` and `>` -> `==` |
| `a_structure_exactly_at_the_depth_ceiling_still_completes` (new) | 2 — structural-depth guard `>` -> `>=` and `>` -> `==` |
| `a_segment_on_the_event_budget_but_over_the_byte_budget_reports_bytes` (new) | 1 — omission-reason selection `>` -> `>=` |
| `a_sub_sliced_block_retains_a_segment_exactly_at_the_byte_budget` (new) | 1 — `sub_slice_block` segment **byte** comparison `>` -> `>=` |
| `two_blocks_exactly_filling_one_slice_share_one_batch` (new) | 0 — the `append_unit` **event** comparison was already caught |
| `image_flood_stops_at_descriptor_budget` (extended) | 0 — the embed-descriptor guard was already caught |
| `documented_planning_ceilings_keep_their_published_values` (new, review follow-up) | 1 — the `MAX_MARKDOWN_SOURCE_BYTES` literal arithmetic `*` -> `+` |

Two of the nine killed nothing and are kept deliberately: they pin the same
"a ceiling is a maximum, not the first rejected value" boundary on their own
axis, so a future refactor that loses the existing coverage fails loudly instead
of quietly. Nine of the ten kills are that same defect class — the existing
tests only ever proved the *rejecting* side of a ceiling; the tenth is the
published-ceiling literal that README and AGENTS.md quote to users.

## Remaining survivors and their disposition

19 survivors remain. **None is excluded, and none is a genuine open coverage
gap.** Every one is classified below, and only two (`845` and `956`) sit in code
this change introduced.

**Pre-existing, in code this change did not touch (14).** These were already
surviving at baseline: the `MarkdownRenderSession` accessors and `is_current`
(137 x2, 147, 152 x3), `MarkdownImageAdmission::try_admit` and
`reset_high_water` (176, 194), `MarkdownEventBatch::into_events` and `is_empty`
(452, 485 x2), `MarkdownRenderPlan::is_complete` (501), and the
`event_retained_bytes` link/image and footnote-definition arms (1210, 1227).
Retiring these belongs
to the deferred `WFR-MARKDOWN-PREVIEW` migration, which will move the
session/admission projections behind an evidence surface where they become
assertable.

**Unreachable terminal (2).** `next_retained > MAX_MARKDOWN_RETAINED_BYTES`
(1075, `>=` and `==`). Design decision 4 records that the 8 MiB retained-byte
terminal is unreachable end to end under the 4 MiB source cap, so no fixture can
sit exactly on it. What is pinned instead is the charge *arithmetic*, by
`retention_charge_arithmetic_is_pinned_for_a_retention_heavy_document` — note
that this test asserts `limit == None`; it deliberately does **not** claim the
terminal fires, and it is named accordingly. Killing the boundary comparison
would require a test-only budget override, which is the kind of seam
`.agents/rules/rust.md` asks us not to add casually.

**Unreachable guard (1).** `withdraw_partial_unit`'s
`checkpoint.events >= floor_events` match guard -> `true` (845). The mutant can
only diverge when `checkpoint.events < floor_events`, and that state is
unreachable by construction:

- `floor_events` is the embed charge's `block_start`, set to
  `planner.block.len() + 1` while the container's own `Start` event is being
  processed — that is, the block index immediately *after* that `Start`.
- Immediately after that same event, `plan_markdown_inner` calls
  `record_checkpoint` whenever `frames.is_admissible()`, pushing a checkpoint at
  `events == planner.block.len()`, which equals `block_start` exactly. A `Table`
  or `CodeBlock` frame is itself a block container, so pushing it onto an
  already-admissible stack leaves the stack admissible.
- The only way that checkpoint is not recorded is a **cut-forbidding ancestor**
  (a `Paragraph`, `FootnoteDefinition`, and so on). But such an ancestor
  suppresses *every* checkpoint in that whole region, so `block_checkpoints` is
  empty, `last()` is `None`, and the guarded arm is never entered — the `_` arm
  runs instead.

So either a checkpoint exists at `>= floor_events`, or none exists at all. The
guard is defensive, and `guard -> true` is behaviourally equivalent.

**Behaviourally equivalent (2).** `segment_len > 0` -> `>= 0` (956) admits
zero-length segments, which produce an empty slice, no omission, and only a
placement value that no marker can reference. `event_index % 64 == 0` -> `!=`
(1062) still cancels, one event later at worst; the cadence is a cost bound, not
an observable contract.

## Scope note

`ui/markdown_preview/**` is outside the mutation scope by design: a `deferred`
workflow gets no `policy.rs`, and GTK adapters are not policy modules. The
projector side is proved by the widget lane instead, so its absence here is not
a coverage gap for task 10.11 to flag.
