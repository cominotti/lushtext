# Bounded install stays linear, measured rather than asserted

`.agents/rules/rust.md` states the paragraph-boundary contract normatively, and
the failure mode it prevents is not a wrong answer but a hang: a slice that stops
mid-paragraph makes `GtkTextBuffer` re-lay-out everything already installed in
that paragraph on **every later slice**, which is the quadratic behavior that
once froze crash recovery of a 33 MB single-line draft for minutes.

That is a performance contract with a user-visible failure mode, so this change
verifies it by measurement, not by reading the code.

## What the contract says

- An install slice cuts **just after a newline** (`next_install_boundary` in
  `model/file_load.rs`, shared with `WFR-BUFFER-REPLACEMENT`).
- A clear slice takes at most one budget of characters and then **extends to the
  next line start** (`policy::clear_slice_char_count` plus
  `policy::clear_slice_extends_to_paragraph_end`).
- **A paragraph larger than the slice budget installs or clears in one turn.**
  That single long turn costs no more than the first render of that paragraph
  pays anyway.

## Fixtures

Both files decode to the same **1,572,864 bytes** (6 × 256 KiB), which is over
the 1 MiB synchronous-install threshold, so both take the bounded path. The
insert slice budget is 256 KiB.

| Fixture | Shape |
| --- | --- |
| `single-paragraph.txt` | one line of 1,572,864 `x` characters, **no newline** — a single paragraph six times the slice budget |
| `many-paragraphs.txt` | 24,576 lines of 63 `y` characters plus a newline — same byte count, paragraph-rich |

## Measured result

From `editor_page::test_a_paragraph_larger_than_the_slice_budget_installs_in_one_turn`,
run under headless Mutter through `scripts/run-widget-tests.sh --headless`:

```
load-install-linearity single_slices=1 single_ms=1080 many_slices=6 many_ms=552
```

| Fixture | Install slices | Wall clock |
| --- | --- | --- |
| single paragraph, 6× the slice budget | **1** | 1,080 ms |
| many paragraphs, same bytes | **6** | 552 ms |

## Reading it

- **The oversized paragraph installed in exactly one slice.** That is the
  contract's own clause, asserted directly (`installation_slice_count == 1`)
  rather than inferred. A boundary that cut mid-paragraph would have produced six
  slices here, each re-laying-out a growing prefix of the same paragraph.
- **The paragraph-rich document sliced into exactly the expected six**, which is
  `1,572,864 / 262,144`. So the budget is being applied, and the single-paragraph
  result is a boundary decision rather than a disabled slicer.
- **Cost is linear in slice count, not quadratic.** Six slices took *less* wall
  clock than the one-turn install of the same bytes — 552 ms against 1,080 ms —
  because each slice lays out one small paragraph while the single turn lays out
  one 1.5 MB paragraph. Under a quadratic regression the six-slice case would
  instead grow with the slice count and exceed the single-turn case by a wide
  margin. The test's guard is deliberately generous
  (`many < single * 40 + 10s`) so it fails on a regression of that shape without
  flaking on shared-runner noise; the recorded numbers, not the guard, are the
  evidence.

## Related coverage that did not move

`editor_page::test_large_unicode_load_installs_in_exact_bounded_slices` (which
predates this change) still asserts exact slice counts, main-loop progress
between slices, and worker-thread body disposal for a multibyte fixture.
`editor_page::test_small_reload_of_large_buffer_uses_bounded_clear_phase` still
covers the clear half. Both pass unchanged, which is the behavior-equivalence
half of this claim: the contract was preserved by the migration, not re-derived
by it.
