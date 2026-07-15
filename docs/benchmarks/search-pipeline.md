# Command-Palette Search Pipeline

This note records the behavioral and ownership baseline for the command-palette
hardening work and explains the accompanying Criterion evidence.

## Pre-change baseline

- Each file, open-tab, note, and command source scored every matching candidate,
  collected all matches, used an unstable score-only sort, and truncated after
  sorting. Equal scores therefore had no explicit source-order contract.
- All-mode grouping prioritized Open Files, Workspace Files, Bookmarks, Folder
  Notes, Document Notes, and Commands. Canonical file paths were deduplicated
  across open and workspace groups; source labels and action identities were
  assigned during UI-side grouping. The visible limit was 50 per source.
- Empty queries still traversed the general result construction path. Direct
  mode changes and 150 ms debounced text changes could launch independent
  generic workers. Stale results were rejected only after a worker completed,
  so obsolete scans still consumed CPU and memory.
- Automation tracked command-palette index debounce but not in-flight query
  ownership. Replace Preview warned with an invalid row's document substring,
  while its outcome exposed only untyped skipped counters.

## Current contract

- One GTK-free selector retains at most `limit` candidates per source in a
  worst-first heap. It final-sorts by score descending, then original source
  ordinal ascending. A test-only full-sort reference uses the same total key.
- Empty queries take the first `limit` source entries directly without fuzzy
  scoring. Non-empty queries reuse one `FuzzyQuery` and check cancellation at
  most every 256 candidates, plus source and grouping boundaries.
- Group priority, canonical path deduplication, labels, and action identities
  are unchanged. Only a complete current generation may be grouped and spliced
  into GTK.
- Direct and debounced entry points share one coordinator: at most one active
  request and one latest pending request. Replacement cancels the active token
  and overwrites the pending snapshot. Closing the palette invalidates both UI
  projection and readiness ownership.
- Invalid Replace Preview rows expose only fixed typed counts for truncated
  sources and regex range mismatches. Diagnostics, UI summaries, automation,
  and debug representations contain no source or replacement content.

## Criterion coverage and interpretation

Run the focused benchmark with:

```sh
cargo bench -p lushtext-core --bench benchmarks palette_pipeline_hardening_100000 -- --noplot
```

The generated corpus contains 100,000 files, Unicode names, and repeated
equal-score names. Cases cover high, medium, Unicode, tie-heavy, and no-hit
queries at limits 1, 10, 50, and 500. Each case compares bounded selection with
the full-sort reference. Additional cases cover cancellation before and during
scan and rapid latest-query replacement.

The benchmark emits a `palette-pipeline-evidence` line. Acceptance requires:

- `retained_peak <= requested limit` for every bounded source;
- bounded results exactly equal the full-sort reference in property tests;
- `active_high_water = 1` and `pending_high_water = 1` under rapid replacement;
- the final started request is the latest submitted query;
- cancellation returns a typed cancelled outcome rather than groupable partial
  rows.

Wall-clock values are machine-dependent and are comparative evidence rather
than a portable release threshold. `make performance-smoke` includes this group
as a coarse regression tripwire; full review uses `make bench-report`.

## Local calibration evidence

On 2026-07-14, a release-profile focused run with 20 samples, 100 ms warm-up,
and 200 ms measurement reported:

- the high-hit query examined 100,000 candidates, matched 99,800, and retained
  at most 50 candidates for a limit of 50;
- ownership high-water stayed at one active and one pending request, and the
  handoff started only two searches and selected `file_99999`, the last
  submitted query;
- high-hit limit-50 bounded selection had a 2.28 ms median versus 2.38 ms for
  the full-sort reference;
- Unicode limit-50 had a 0.48 ms bounded median, tie-heavy limit-50 had a
  0.52 ms bounded median, and no-hit limit-50 had a 0.81 ms bounded median;
- cancellation already set before scan returned in roughly 25 ns, while the
  deterministic during-scan case observed cancellation at exactly 256
  examined candidates and returned in roughly 7.08 µs;
- rapid latest-query replacement plus the final 100,000-file query completed in
  roughly 1.93 ms.

These short calibration timings are useful for cancellation-interval and gross
regression review, not for release comparisons across machines.
