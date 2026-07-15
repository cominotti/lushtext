# Bounded Interactive Pipeline Evidence

This note records the resource contracts and focused measurements for editor
saves, Markdown preview, workspace search, result retirement, live-editor
residency, and lossy-encoding analysis. Wall-clock values calibrate one
development host; the direct counters and terminal-state assertions are the
portable regression gates.

## Environment and artifacts

The measurements were captured on 2026-07-15 with cargo/rustc 1.96.0, Linux
7.1.3 x86_64, GTK 4.22.4, Libadwaita 1.9.2, and GtkSourceView 5.20.0. Criterion
used the optimized bench profile, one-second warm-up and measurement windows,
and 10 samples except for the 20-sample editor I/O matrix.

Each pass used a distinct artifact directory so the initial comparison evidence
was not overwritten:

- `build/smoke/performance/bound-remaining-interactive-pipelines-baseline-20260715`
- `build/smoke/performance/bound-remaining-interactive-pipelines-encoding-after-20260715`
- `build/smoke/performance/bound-remaining-interactive-pipelines-memory-after-20260715`
- `build/smoke/performance/bound-remaining-interactive-pipelines-markdown-after-20260715`
- `build/smoke/performance/bound-remaining-interactive-pipelines-policy-after-20260715`

Reproduce the focused policy measurements with:

```sh
scripts/run-performance-smoke.sh \
  --artifact-dir build/smoke/performance/bounded-interactive-pipelines \
  --filter 'search_interactive_policies save_admission_policy markdown_render_planning editor_memory_policy editor_file_io'
```

## Direct resource bounds

- Save requests retain weak/scalar identity until admission. Ordinary payloads
  share a 256 MiB byte budget, at most eight are active, and one supported
  overweight request may run exclusively. A close batch snapshots and saves one
  selected editor at a time. The smoke burst admitted eight payloads at exactly
  268,435,456 bytes; widget probes additionally assert queued compact requests,
  close priority, permit release, and sequential close high water.
- Markdown planning accepts at most 4 MiB of source, 50,000 parser events, 128
  structural levels, 256 embed descriptors, and 8 MiB of retained event data.
  Each preview retains one active planner plus one replaceable latest source.
  Inline-footnote lowering separately caps source, parser events, replacements,
  retained/output bytes, and source-relative close-scan work; malformed dense
  suffixes therefore reach a deterministic limited terminal.
  Projection applies at most 256 complete-block events and 256 KiB of retained
  event text per GTK turn. Replacing a render detaches its text buffer in O(1),
  then retires at most 64 Ki characters and 64 widget/link references per GTK
  turn. Local images retain at most four compact work descriptors under a
  conservative byte ceiling, with only one decoder active. Image ingestion is
  bounded before allocation and revalidates identity, size, and mtime before
  handing bytes to the decoder.
- Workspace query ownership stays at one active worker group and one replaceable
  latest compact request. Accepted matches are sealed once into
  `Arc<Vec<SearchMatch>>`;
  Replace Preview recorded one shared handoff and zero whole-result clones.
- Replacing, clearing, or closing a 10,000-result panel detaches the current
  model immediately. Retirement released at most 250 rows or cache references
  per GTK turn and took 81 arithmetic policy slices for the configured
  result-cap fixture. Non-empty query churn applies latest-query backpressure
  at two detached generations; the close/clear escape path keeps the absolute
  retained-generation ceiling at three. The live GTK widget fixture recorded a
  250-reference slice high water and two retained generations.
- Ordinary below-threshold editor edits update one residency record and perform
  zero full scans. Full freshness scans remain reserved for enforcement,
  reconciliation, attach/detach, and exceptional uncertainty.
- UTF-8 and UTF-16 analysis returns lossless immediately. Windows-1252 and
  Shift_JIS use one reusable exact no-replacement encoder while retaining total
  issue count and only the first eight diagnostics.

## Focused measurements

| Fixture | Measured interval |
| --- | ---: |
| 1,000 rapid compact workspace queries | 16.712-18.013 us |
| Retirement-budget arithmetic for 10,000 result rows plus caches | 152.28-157.81 ns |
| Eight-request save-admission burst | 282.34-307.73 ns |
| Markdown plan, 10,000 paragraphs / 30,000 events | 1.2737-1.3455 ms |
| Dense single Markdown block, explicit limited terminal | 46.910-50.187 us |
| One incremental edit in a 100,000-record residency ledger | 11.456-11.967 ns |
| Full clean 100,000-tab memory-policy scan | 1.1499-1.2423 ms |

The Windows-1252 CRLF end-to-end write comparison changed from
8.7098-9.0391 ms to 1.7795-1.8555 ms at 1 MiB, from 88.023-91.861 ms to
18.670-20.564 ms at 10 MiB, and from 471.85-482.18 ms to 116.45-121.18 ms at
50 MiB. The closeout run's intervals remain within ordinary host variance of
the earlier after-measurements. The semantic gate is exact equivalence with no-replacement encoding;
the timing improvement comes from eliminating per-scalar encoder construction
and avoidable intermediate body copies.

## Terminal and limited states

Hitting a documented Markdown source, event, structure, embed, retention, code
block, table, or image limit produces an accessible limited/fallback state;
it does not expose a silently partial complete state. Stale planning,
projection, image, save, search, and retirement generations are discarded by
identity checks. Automation readiness remains pending through current Markdown
planning/projection/image work, detached Markdown render retirement, and
detached search-result retirement.

Elapsed-time variance is accepted across developer hosts. A regression is
actionable when a direct ownership/slice counter exceeds its configured bound,
a stale generation publishes, a terminal state is ambiguous, semantic
equivalence fails, or a focused timing changes materially without an explained
fixture or environment difference.
