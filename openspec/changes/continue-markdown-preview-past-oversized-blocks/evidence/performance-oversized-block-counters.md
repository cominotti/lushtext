# Planning and projection counters for the oversized-block fixtures

Task 10.8 evidence. No absolute timing gate is asserted anywhere; the numbers
below are recorded so a later change can see whether the shape of the work
moved. Timings are Criterion medians on one developer machine and are not a
threshold.

## Command

```
make performance-smoke
```

Artifacts: `build/smoke/performance/criterion-markdown_render_planning.log`.

## Planning counters (GTK-free, from the benchmark's evidence lines)

| Fixture | source bytes | events | batches | max events per slice | retained bytes | omissions | limit |
| --- | --- | --- | --- | --- | --- | --- | --- |
| oversized table (300 rows x 3 columns) | 20,138 | 3,316 | 14 | 254 | 16,805 | 0 | none |
| oversized indented code (600 lines) | 20,196 | 605 | 3 | 256 | 17,794 | 0 | none |
| 10,000 paragraphs (pre-existing control) | 158,890 | 30,000 | 118 | 255 | 138,890 | 0 | none |
| one indivisible dense block plus a tail | 1,817 | 1,204 | 1 | - | - | 1 | none |

The two rows this change exists for are the first two. Both:

- plan to completion with `limit = None` and **zero omissions**, so the whole
  table and the whole code block are retained and projected;
- spread across several batches (14 and 3) rather than one, which is the
  sub-slicing this change added;
- keep every batch within `MARKDOWN_EVENTS_PER_PROJECTION_SLICE` (254 and 256
  against the 256 ceiling), which is the per-GTK-turn bound.

The indented-code fixture is the shape that genuinely sub-slices: the pinned
`pulldown_cmark` 0.13.4 emits one `Text` event per indented line (605 events for
600 lines), whereas a fenced block coalesces its body into a single event and so
cannot reach the event budget at all.

## Projection counters (per GTK turn)

Projection is GTK-bound and therefore not measurable from a benchmark. The
per-turn high water is asserted instead by the widget lane through
`projection_counters_for_test()` (task 7.9), which requires every new oversized
fixture to stay within `MARKDOWN_EVENTS_PER_PROJECTION_SLICE` events per turn.
That lane passed with 630 widget tests and no `FLAKY:` line.

## Criterion medians (informational only)

| Benchmark | median |
| --- | --- |
| `markdown_render_planning/oversized_table_sub_sliced` | 143.66 us |
| `markdown_render_planning/oversized_indented_code_sub_sliced` | 50.08 us |
| `markdown_render_planning/dense_single_block_omitted` | 49.83 us |
| `markdown_render_planning/10000_paragraphs` | 1.5929 ms |

The oversized-table fixture is the most expensive of the three new ones, which
matches its event count (3,316 against 605); planning stays linear in events
rather than degrading when a block has to be sub-sliced.
