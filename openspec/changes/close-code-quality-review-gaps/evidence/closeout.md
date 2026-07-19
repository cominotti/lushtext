# Closeout Evidence

Completed across 2026-07-18 and 2026-07-19 on the same checkout used for the
baseline and implementation.

## Environment and comparison discipline

- Baseline commit and worktree basis:
  `95d171a911e792991bc1aa7446f0ea7e6ffe404a`, retaining the existing dirty
  post-portfolio checkout.
- Rust and Cargo: 1.96.0, optimized Cargo `bench` profile.
- Host and storage: the same x86_64 Fedora Toolbx host and Btrfs workspace as
  the baseline.
- Baseline load ranged from `11.13 / 8.97 / 7.33` to
  `18.37 / 18.50 / 15.73`.
- Final closeout load was `2.04 / 1.57 / 1.65`.

Because the saved baseline was captured under materially heavier ambient load,
the comparison was reviewed through Criterion distributions, confidence
intervals, implementation controls, and boundedness evidence. No absolute
cross-machine timing threshold was introduced, and the quieter closeout run is
not presented as an absolute speedup claim.

## Targeted correctness evidence

- The release-semantics search-retirement proof passed with test-profile debug
  assertions disabled. It exercises actual before-and-after ownership across
  bounded turns rather than trusting a self-reported counter.
- Focused unit, integration, property, and headless GTK tests passed for:
  release-safe retirement; chunked snapshots; large save and Local History
  ownership; Replace All/Undo admission, ingestion, diagnostics, and journal
  accounting; file-index count and byte bounds; unified bookmark browsing;
  recent-document growth; minimap byte classification; Markdown image
  admission, cancellation, and stale disposal; and active-plus-latest request
  ownership.
- Failure-injection coverage passed for pre-rename durable-write failure,
  metadata-to-ingestion growth, stale completion, source mutation, cancellation,
  supersession, teardown, and guarded disposal.
- The new tests inspect retained collections, payload bytes, generation state,
  permit ownership, actual releases, file contents, or bounded result samples.
  They do not establish correctness solely from instrumentation counters.

## Full verification matrix

| Gate | Result |
| --- | --- |
| `make test-search-retirement-release` | 1 passed with debug assertions disabled |
| `make test-unit` | 1,267 passed; 10 intentionally skipped |
| `make test-int` | 76 passed |
| `make test-prop` | 40 passed |
| `make test-widget` | 1,061 passed; no unexpected warning output |
| `make performance-smoke` | Passed all 17 Criterion groups and all headless ownership/responsiveness proofs |
| `make accessibility-smoke` | Passed AT-SPI anchor and focus replay |
| `make visual-smoke` | Passed and regenerated every visual scenario |
| `make visual-geometry-smoke` | Passed same-session geometry proof; no case timed out |
| `make check` | Passed formatting, blocking Clippy, filesystem, architecture/UI policy, documentation, accessibility, and proof policy gates |
| `make lint-advisory` | Passed; every emitted finding was cleaned or matched a narrow reviewed classification |
| `make check-agent-docs` | Passed 19 validator tests and all 14 maintained skills |

The widget harness's documented transient headless AT-SPI registration noise was
filtered by its existing allowlist. Its post-run scanner found no unexpected GTK
warning, critical, timeout, stalled sentinel, disposal-backpressure, or readiness
leak. Performance-smoke artifacts are in `build/smoke/performance`, accessibility
artifacts in `build/smoke/accessibility`, visual artifacts in
`build/smoke/visual`, and geometry artifacts in `build/smoke/visual-geometry`.

## Criterion review

`make bench-compare` completed against the saved `main` baseline. Focused reruns
then separated the file-index policy cost from the ordinary unlimited sidebar
scan path.

### File-index construction

- New common mixed 10,000-file case: `25.596 ms .. 25.858 ms`. This benchmark
  ID did not exist in the saved baseline, so it is retained as a closeout
  distribution rather than a before/after claim.
- New near-policy long-path case: `182.30 ms .. 182.91 ms`. This benchmark ID
  also did not exist in the baseline.
- Existing 10,000-entry rebuild: `+1.81%`, confidence interval
  `+0.20% .. +3.42%`, within the configured Criterion noise threshold.
- Existing 100,000-entry rebuild: `+10.95%`, confidence interval
  `+8.11% .. +13.66%`.
- Directory-only 1,000/10,000 cases were approximately `+29%`.

The remaining large-rebuild and directory-only cost is explained by the new
required deterministic two-pass byte discovery/replay plus charge-before-retain
accounting for scan entries, directories, pending work, canonical identities,
hash capacity, and output ownership. The work is confined to finite byte-capped
file-index construction and buys an in-operation memory ceiling; it is not an
unexplained regression in the ordinary file-tree scanner.

### Unlimited scanner control

The first comparison exposed a material regression because the new two-pass
file-index discovery had accidentally reached ordinary unlimited sidebar scans.
That path was split back to deterministic single-pass top-k scanning and rerun
against `main`:

- 10 entries: approximately `-20.1%`.
- 100 entries: approximately `-25.5%`.
- 1,000 entries: approximately `-29.1%`.
- 5,000 entries: approximately `-16.9%`.
- 10,000 entries: approximately `-15.0%`, with final distribution
  `11.959 ms .. 12.196 ms`.

This control resolves the material unexplained regression while preserving the
new finite-budget behavior for command-palette indexing.

### Direct boundedness evidence

The near-policy file-index fixture reported:

- `retained_files=6428`;
- `retained_index_bytes=57,199,335`, at or below the 64 MiB installed limit;
- `peak_build_bytes=134,215,209`, at or below the 128 MiB construction limit;
- typed terminal `BuildByteLimit` with a deterministic usable partial index.

Performance smoke additionally confirmed actual limits for search retirement,
Notes inventory/query ownership, Markdown batches and detached generations,
save admission, transient loads, watcher pressure, minimap slices, Replace All
construction, draft repair, session restore, and worker-side disposal.

## Final review conclusions

- Data safety: no unaddressed persistence, save/close, recovery, Replace All,
  or async freshness finding remains in the change. Durable safety writes still
  precede mutation, ambiguous post-rename evidence remains retryable, and large
  rejected or superseded owners keep bounded off-GTK disposal.
- Performance: the responsiveness leaf's stale Markdown-image finding and all
  seven scale findings were fixed. The hot-path leaf found no blocking issue;
  its bounded-work recommendations were incorporated. The benchmark review
  leaves no material unexplained regression.
- Architecture: byte and ownership policy remains in plain model/service code;
  GTK adapters retain generation checks, weak/scalar callbacks, and bounded
  projection. No new crate, dependency, persisted-format migration, scheduler,
  or GTK Lush public API was introduced.
- GTK/accessibility: the unified bookmark browser preserves its stable
  per-bookmark open action through an icon-only suffix with an accessible label.
  The callback captures only weak state plus a scalar source index, and live
  AT-SPI, visual, constrained, dense, and geometry scenarios pass.
- Comments and policy: comments are limited to durable admission, ownership,
  accounting, generation, and release invariants; blocking and advisory lint
  policies are clean without blanket suppression.

No commit or push was performed as part of this apply workflow.
