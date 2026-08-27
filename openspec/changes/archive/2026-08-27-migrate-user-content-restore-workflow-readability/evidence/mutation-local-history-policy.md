# `WFR-LOCAL-HISTORY` mutation evidence (task 5.9)

**File-level anchors only.**

## Gain from zero, with no relocation

Task 5.3 expected gain-from-zero rather than relocation, and that is what
happened. `model/local_history.rs` **stays in `model/`** and was not edited:
re-derived as *owning workflows* its consumer count is **2**
(`WFR-LOCAL-HISTORY` and migrated `WFR-DOCUMENT-SAVE`, which captures a snapshot
on every successful save), and `services/local_history_service.rs` depends on it,
so relocating under `ui/` would invert dependency direction — the 3b
`model/file_load.rs` precedent. **No parity numbers are owed.**

What moved into `ui/window/local_history/policy.rs` came out of two GTK adapters,
neither of which was in the mutation scope:

- **viewer geometry** — the proportional size, its clamps, the parent gutter, and
  the pre-map fallback;
- **which snapshots the user sees** — the legacy-empty-baseline rule and its
  deliberately conservative *two empty baselines plus two non-empty periodic*
  threshold;
- **the preview install plan** — empty / direct / sliced, and its completion
  predicate;
- **both capture freshness tickets** with their predicates, moved from the
  editor-page capture surface so the workflow has **one** `policy.rs` even though
  it spans two directories, and the capture surface now *calls* them;
- **the periodic reschedule rule** and **row presentation**.

## The numbers

Same working-tree invocation as the other slot-4 rows.

| Quantity | Before | After |
| --- | --- | --- |
| Mutants generated in `ui/window/local_history/policy.rs` | **0** (no `policy.rs` existed; the decisions were inline in `ui/window/local_history.rs` and `ui/editor_page/local_history.rs`, neither in `examine_globs`) | **92** |
| Missed | 0 | **0** |

## Survivor accounting — every one triaged, none excluded

The first run left **10 survivors in this module**, in three groups. The pattern
is worth naming because it recurred across all four slot-4 rows: **an assertion
that compares a value against the constant it came from cannot detect the constant
changing.**

| Survivor group | Why it survived | Closed by |
| --- | --- | --- |
| `PREVIEW_RESERVATION_BYTES`'s `64 * 1024 * 1024` (4 mutants) | every assertion compared the reservation against itself, so `*` could become `+` or `/` and both sides moved together | `preview_reservation_is_the_documented_conservative_ceiling` — pins the concrete byte count, and additionally pins that the two install bounds *are* the cross-cutting `model::buffer_replacement` values rather than copies |
| `parent_relative_dialog_axis_size`'s `parent_axis * target_fraction` (2) | the geometry tests only exercised the **clamps**, which hold for *any* fraction, so the fraction itself was untested | `viewer_size_is_actually_proportional_between_its_clamps` — a mid-range parent where the fraction decides, plus the raw axis helper at 0.5 and 0.25 |
| `current_window_dimension`'s `default_axis > 1` (1) | the tests covered the *current*-axis guard but not the default's own guard; a `>=` would return a degenerate default of 1 as itself, which is the same unusable size the final arm exists to replace | extra cases in `window_dimension_falls_back_only_when_unmapped`, including a current axis of exactly 1 as a real mapped size |
| `filter_visible_snapshots`'s periodic-count predicate (3) | no fixture had *empty* periodic snapshots or *non-empty* baselines, so neither the `&&` nor the `> 0` could be distinguished | `the_periodic_count_requires_a_periodic_snapshot_with_real_content` — empty periodics, non-empty baselines, and the one-byte boundary |

### One genuinely equivalent mutant, resolved rather than excluded

`current_window_dimension`'s second guard read `default_axis > 1`, and mutating it
to `>= 1` produced **identical behaviour for every input**: the two differ only at
`default_axis == 1`, where the taken branch returns 1 and the fallback also returns
1. No test could distinguish them.

Rather than record it as an accepted equivalent, the guard was changed to
`default_axis > 0`, which is behaviour-identical, states the actual question ("is
this a usable size?"), and **is** detectable — `current_window_dimension(0, 0)`
already asserts the fallback. The reason is recorded in the function's own doc
comment, because the next reader's instinct will be to "simplify" it back.

**Zero exclusions.** No `MUTANTS_EXCLUDE` entry was added.

The `filter_visible_snapshots` group is the one worth re-reading: those three
mutants would have made the browser hide a user's only "before edits" snapshots on
evidence that does not exist.

### Final numbers

After closing every survivor, the confirming diff-scoped run reports
**246 mutants tested, 230 caught, 16 unviable, 0 missed** across all four slot-4
policy modules; this row's share is **92 generated, 0 missed**.
