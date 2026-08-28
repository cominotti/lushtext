# Mutation evidence — slot 6 (`WFR-MINIMAP`)

Three figures are reported here and they are **not** interchangeable. The
mutation-testing capability requires the relocation's parity and the extraction's
gain to be stated separately, because merging them would let a parity loss hide
behind a gain. A third figure — what the retired hand-listed scope entry used to
generate — is required by this change's own amendment.

Every figure names its invocation and its file-level anchor. Counts come from the
tool, never from reading the configuration or the source.

## The unfilterable floor

`cargo-mutants`' `--re` narrows *reporting*, not the run: a focused invocation
still tests a fixed set of mutants that match nothing about the filter. For this
repository that floor is **34 mutants** in `services/draft_service.rs`,
`services/file_tree.rs`, and neighbours, all of them pre-existing survivors
unrelated to this change. Slot 5b recorded the same phenomenon. Every "tested"
count below is stated with the floor removed, and the floor's own outcomes are
identical before and after.

## 1. Relocation parity — `minimap_analysis.rs` → `minimap/policy.rs`

The relocation is the whole-module move of the GTK-free content accumulator. It
**can** fail: a module that stops generating mutants after a move is a coverage
regression that blocks the move.

**Before**, measured in a clean `git worktree` at `HEAD` (`f2c33206`), before any
file moved:

```
git worktree add -f build/parity-base HEAD
cd build/parity-base
MUTANTS_RE='model/minimap_analysis' ./scripts/run-mutants.sh full
```

Anchor: `crates/lushtext-core/src/model/minimap_analysis.rs`.

| | generated | caught | missed | unviable |
| --- | --- | --- | --- | --- |
| **Before** | **21** | **19** | **2** | 0 |

The two survivors are both `MinimapAnalysisAccumulator::characters_examined`
(`:106:9`, replaced with `0` and with `1`) — a `#[must_use]` getter with no unit
assertion of its own.

**After**, in the migrated tree. Anchor:
`crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`, the relocated region
only, identified by function name rather than by line so the constant offset does
not have to be asserted:

```
MUTANTS_RE='minimap/policy' ./scripts/run-mutants.sh full
```

| | generated | caught | missed | unviable |
| --- | --- | --- | --- | --- |
| **Before** (`model/minimap_analysis.rs`) | **21** | **19** | **2** | 0 |
| **After** (`minimap/policy.rs`, relocated region) | **21** | **19** | **2** | 0 |

**Parity is exact**, mutant for mutant, and the two survivors are the same two —
`MinimapAnalysisAccumulator::characters_examined` replaced with `0` and with `1`,
now at `policy.rs:2180`. Neither the count nor the identity of a single generated
mutant changed, so the relocation lost no coverage.

The two inherited survivors were then **triaged to zero** rather than carried:
`characters_examined()` had no assertion of its own because every test read the
count off the finished `MinimapAnalysisResult` instead of the accumulator's
running total — which is precisely the value the sliced GTK scan reports
mid-scan. `running_character_count_is_observable_before_the_scan_finishes`
asserts it at three points and cross-checks it against the terminal figure.

## 2. Extraction gain — pure math lifted out of the GTK adapter

The extraction has **no before-count and cannot fail**: every function listed here
was inside `ui/editor_page/minimap.rs`, which was in scope only through a
hand-listed `examine_globs` entry, and the four `exclude_re` entries covering the
adapter and its GTK wrappers removed most of it. Reporting this as a "gain from
zero" is the honest framing; merging it with §1 would not be.

Invocation:

```
MUTANTS_RE='minimap/policy' ./scripts/run-mutants.sh full
```

Anchor: `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`, whole file.

| | count |
| --- | --- |
| generated and tested | **425** |
| caught | **407** |
| missed | **12** |
| unviable | **6** |

Subtracting the relocation's 21 (§1) leaves **404 mutants of newly-extracted pure
policy that were not in the deterministic lane before**, of which **394 were
caught on the first run**. The 10 remaining survivors were all in extracted code
and are triaged in §5.

The extraction's own before-count is **zero by construction**: every function
listed in design §D2 was inside the GTK adapter, and the four retired `exclude_re`
entries covering the adapter and its live-GTK gatherers removed most of what the
hand-listed scope entry generated for it. Reporting this figure as a gain rather
than folding it into §1 is what keeps a parity loss from hiding behind it.

## 3. What the retired scope entry used to generate

This change **retired** the hand-listed
`crates/lushtext-core/src/ui/editor_page/minimap.rs` `examine_globs` entry rather
than re-pointing it, because the convention now reaches the pure logic by name and
the GTK adapter beside it is legitimately out of scope for not being a policy
module. The amended `mutation-testing` capability requires the mutants that entry
used to generate to be accounted for rather than silently dropped.

**Measured, not asserted.** After the rename and **before** the configuration was
edited, `./scripts/run-mutants.sh list` exited **0** while generating **0** mutants
for the retired entry, and **444** for `minimap/policy.rs` through the
`ui/**/policy.rs` convention alone. That is the silent disarm the amendment
exists to name.

| Population | Count | Where it is now |
| --- | --- | --- |
| `minimap.rs` in scope before, post-exclusion | **457** | — |
| `minimap.rs` unexcluded before | **689** | — |
| removed by the 14 minimap `exclude_re` entries | **232** | — |
| `model/minimap_analysis.rs` before | **21** | relocated; §1 |
| **row total in scope before** | **478** | — |
| `minimap/policy.rs` unexcluded after | **444** | — |
| removed by the 6 surviving `exclude_re` entries | **19** | documented equivalence claims, §4 |
| **row total in scope after** | **425** | — |

**The difference is the GTK adapter, and it is deliberate.** The mutants no longer
generated are the ones the four retired `exclude_re` entries were already
suppressing — `LushtextEditorPage::` adapter methods (42 + 61 matches) and the
live-GTK gatherers `sync_source_map_geometry`, `minimap_native_slider_diagnostics`,
`collect_markers`, `draw_marker_strip`, `minimap_projection_space`, and their
neighbours (64 + 54 matches). Their behavior is covered by the widget harness under
`scripts/run-widget-tests.sh`, which is the documented non-mutation lane for GTK
adapters, and by the visual geometry lane for the rendered result. Nothing that was
*being mutated and killed* left the lane: the previously-excluded population and the
now-out-of-scope population are the same set.

## 4. Exclusion re-verification

Every surviving exclusion was checked against a mutant the tool actually
generates, using `cargo mutants --list --no-config -p lushtext-core --file <path>`
to obtain the unexcluded population and matching each regex against it.

**Before — the row carried 14 entries naming 66 methods:**

| Entry | Matched | Verdict |
| --- | --- | --- |
| `LushtextEditorPage::(minimap_availability\|…)` — 15 names | 42 | **retired**: the adapter is out of scope for not being a policy module |
| `LushtextEditorPage::(minimap_refresh_blocks_readiness\|…)` — 24 names | 61 | **retired**, same reason |
| `(current_availability\|…)` GTK wrappers — 19 names | 64 | **retired**, same reason |
| `(sync_source_map_geometry\|…)` GTK wrappers — 8 names | 54 | **retired**, same reason |
| `replace < with <= in fit_native_slider_to_source_map_bounds` | 1 | **kept**, re-keyed |
| `replace > with >= in fit_native_slider_to_source_map_bounds` | 2 | **kept**, re-keyed |
| `minimap.rs:2046:55: replace - with + in fit_projected_bounds` | **0** | **deleted** |
| `minimap.rs:2047:21: replace < with <= in fit_projected_bounds` | **0** | **deleted** |
| `minimap.rs:2054:16: replace < with <= in fit_projected_bounds` | **0** | **deleted** |
| `minimap.rs:2058:19: replace > with >= in fit_projected_bounds` | **0** | **deleted** |
| `replace < with == in fit_marker_bounds` | 4 | **kept**, re-keyed |
| `replace < with <= in fit_marker_bounds` | 4 | **kept**, re-keyed |
| `replace - with + in fit_marker_bounds` | 5 | **kept**, re-keyed |
| `replace > with >= in fit_marker_bounds` | 3 | **kept**, re-keyed |

**Seven of the 66 named methods have zero definitions anywhere in the tree** —
`apply_minimap_width_from_settings`, `wrapped_minimap_layout_exceeds_budget`,
`buffer_has_line_exceeding_char_budget`, `collect_long_line_warnings`,
`line_top_in_strip`, `line_bottom_in_strip`, `buffer_y_to_strip_y`. They excluded
something that was renamed or deleted, and the exclusion outlived it.

**Four entries were anchored to a literal `line:column` that had moved.** All four
named `fit_projected_bounds` at `minimap.rs:2046`–`:2058`; the function begins at
line 2435, and line 2046 is a `markers.push` inside `normalize_line_runs`. They
matched **nothing**, and had not for some time.

**The four mutants they were written for still exist**, so per the amended
requirement they are triaged rather than re-anchored: §5 records the outcome.

**After — 4 entries naming 0 methods**, each verified against a real generated
mutant (1, 2, 1, 1 matches respectively). Two are the re-keyed
`fit_native_slider_to_source_map_bounds` pair; two are new and belong to the
extracted `expanded_to_min_height` helper. The four `fit_marker_bounds` entries
were **deleted**, not re-keyed — see §5.

## 5. Survivor triage

The first full run left **12 survivors**, all in extracted code. Each was worked
through the documented order — *is it a real missed behavior; then tighten tests;
then consider a small refactor; and only then an equivalence exclusion narrow
enough that nearby behavior still mutates* — rather than being excluded on sight.

| Survivors | Verdict | Step reached |
| --- | --- | --- |
| `MinimapAnalysisRequest::required` ×3 (`-> true`, `-> false`, `\|\| -> &&`) | **KILLED.** The predicate came out of the GTK adapter with no unit assertion at all — 5b's `workspace_scope_kind_name` lesson, arriving exactly where task 3.7 predicted. Its full truth table now covers all three | 2 (tighten tests) |
| `fitting_source_map_page_size` ×3 (two `\|\| -> &&`, `> -> >=`) | **KILLED.** The existing test proved one non-finite input and two fitting cases, leaving the later guards and the strict-epsilon comparison unexercised. Each input is now made non-finite on its own so a collapsed `\|\|` cannot short-circuit past it, and an exact-one-epsilon difference is asserted to remain *fitting* | 2 (tighten tests) |
| `characters_examined` ×2 (`-> 0`, `-> 1`) | **KILLED.** The two inherited relocation survivors. Every test read the count off the finished result rather than the accumulator's running total — which is the value the sliced GTK scan reports mid-scan, so it is worth asserting directly | 2 (tighten tests) |
| `fit_projected_bounds` ×4 (`- -> +`, two `< -> <=`, `> -> >=`) | **the four the stale `line:column` anchors were written for.** Confirmed still generated, and confirmed unprotected since the anchors moved. Three are genuinely equivalent — a zero-sized adjustment at an exact edge — but **symbol-scoping them would have matched three mutants the run had caught**, in that function's unrelated reject-outside and non-empty guards. Broadening a predicate to files or mutants it did not previously cover is a weakening the amendment forbids, so exclusion was refused at step 4 and the triage went back to step 3 | 3 (refactor) |

**The refactor.** The min-height expansion block was **duplicated verbatim**
between `fit_marker_bounds` and `fit_projected_bounds`. Extracting it into one
`expanded_to_min_height` helper achieved three things at once: it removed the
duplication, it collapsed both copies' equivalent boundary mutants into one
place, and it made the equivalence claim expressible **narrowly** — each
surviving entry now matches exactly one mutant, because the helper contains
nothing but the expansion.

**And the refactor then deleted a survivor instead of excluding it.** With the
block isolated, `.min(upper - lower)` was visibly dead code: the helper's trailing
`(top.max(lower), bottom.min(upper))` already bounds the result, and whenever the
cap would bind, the expansion fills the whole span either way. Removing it deletes
the `- -> +` mutant rather than documenting it as equivalent — which is why the
`fit_projected_bounds` `- with +` claim does not appear in the final
configuration.

**The four `fit_marker_bounds` entries were deleted, not moved.** Their recorded
reason was "the same final clamped bounds after the minimum-height expansion
settles" — and that expansion has now left the function, so the justification no
longer described the mutants the entries matched. Re-verified against the tool:
`- with +` matched **nothing**, and what remains in that function is its
reject-outside guard and its non-empty gate, killed by the co-located
edge-touching and zero-minimum tests.

**Final configuration: 4 minimap entries, down from 14, naming 0 method names.**

| Entry | Matches | Reason |
| --- | --- | --- |
| `replace < with <= in fit_native_slider_to_source_map_bounds` | 1 | pre-existing, re-verified, re-keyed |
| `replace > with >= in fit_native_slider_to_source_map_bounds` | 2 | pre-existing, re-verified, re-keyed |
| `replace < with <= in expanded_to_min_height` | 1 | `top == lower` adds a zero offset and re-assigns `lower` |
| `replace > with >= in expanded_to_min_height` | 1 | `bottom == upper` subtracts a zero offset and re-assigns `upper` |

### Confirming run, against the final formatted tree

```
MUTANTS_RE='minimap/policy' ./scripts/run-mutants.sh full
```

| | count |
| --- | --- |
| generated and tested | **412** |
| caught | **406** |
| **missed** | **0** |
| unviable | 6 |

**Survivors: zero.** The unfilterable floor tested its usual 34 with 13 missed,
all of them pre-existing survivors in `services/draft_service.rs` and
`services/file_tree.rs` — identical before and after, and unrelated to this row.

The population moved 425 → 412 between the first and final runs, and the whole
difference is triage rather than lost coverage: the extraction collapsed a block
that had been duplicated in two functions into one, removing `.min(upper - lower)`
deleted its mutants outright, and the two narrow helper exclusions account for two
more. Nine mutants that were surviving are now killed by tests; four no longer
exist because the code that generated them was either shared or dead.

**A methodology note worth carrying forward.** The first attempt at this
confirming run was invalidated by the implementer editing `policy.rs` while it was
in flight — cargo-mutants copies the tree per job, so later jobs picked up the edit
and the results were a mix of two source states. It was discarded and re-run from
the formatted final tree rather than reported. The task list's warning about
mid-run edits is about `cargo fmt`; it applies to any edit, including adding the
very tests the run is meant to validate.

## Reproduction

```
# relocation baseline, at HEAD, in a clean worktree
git worktree add -f build/parity-base HEAD
cd build/parity-base && MUTANTS_RE='model/minimap_analysis' ./scripts/run-mutants.sh full

# migrated tree
MUTANTS_RE='minimap/policy' ./scripts/run-mutants.sh full

# unexcluded population, for exclusion re-verification
cargo mutants --list --no-config -p lushtext-core \
  --file 'crates/lushtext-core/src/ui/editor_page/minimap/policy.rs'
```

`make mutants-diff` is **not** usable here: it builds its file set from a
three-dot commit range, so an uncommitted worktree tests zero mutants and exits 0.
`git add -N` does not fix it. `--re` narrows reporting but not the run; only
`--in-diff` bounds one.
