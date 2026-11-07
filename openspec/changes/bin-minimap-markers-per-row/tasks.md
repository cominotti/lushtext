Rules for every task:

- Headless only: never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`.
- CPUs 8–23 (`taskset -c 8-23`). Heavy work goes through `scripts/heavy.sh`
  (`scripts/with-memory-cap.sh` inside). Timing verdicts go through
  `scripts/measure.sh`. One CBMC at a time. Never `pgrep -f`, never
  `find /`.
- Measurements:
  - at a 1-minute load below 4 (`/proc/loadavg`; wait if higher);
  - optimized widget build (`CARGO_PROFILE_DEV_OPT_LEVEL=2
    CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false`), presented;
  - interleaved with `main`, with the load recorded before and after each
    run;
  - every measurement test destroys its window between repetitions (design
    D1: leaked windows produced PT's 118–140 ms).
- Evidence goes in `.claude/handoffs/bin-minimap-markers-per-row/`. The
  spec-stage tests are in `measure/editor_page.rs.add`.
- Failing first means the test is committed or recorded red before the fix
  that turns it green.

## 1. Baseline, attribution and toolkit facts (before any fix)

- [x] 1.1 Record the spec-stage A1–A4 figures (design "Measured baseline")
      in `runs.log` with the `main` revision. Re-run A2 and A3 on the
      branch's base `main` if `main` moved, and add a minimap-off scroll
      baseline: A2's 11.3–15.9 ms scroll frames, with markers and without.
      This settles which share of a scroll frame the minimap owns.
- [x] 1.2 Extend A2 to a 1,425-row strip (a window about 1,600 px tall) and
      to a 10,000,000-byte document with markers, on `main`.
- [x] 1.3 GTK axioms (design D9), each with a probe in `gtk-lush-axioms` (a
      `slow` marker where it times 131,072 items), an interactive/`--check`
      sample and a ledger row in
      `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
      ("Verified against" the pinned GTK and GtkSourceView). Number them
      from the next free id:
      - AX1, the property-less tag: cost, no relayout, no undo entry,
        movement with edits, no inheritance by inserted text, cross-buffer
        paste;
      - AX2, `line_at_y`: monotone, and zero-height lines skipped;
      - AX3, `forward`: scanned vs unscanned cost, and `visible-only`;
      - AX5, overlay render-node caching on map scroll.

      Run `make gtk-axioms` and `make check-gtk-axioms`. If AX1 fails
      (layout invalidation, undo entries or O(lines) cost), stop and revise
      D4 before task 4.
- [x] 1.4 Measure, on `main` and with a measurement-only completed-scan
      probe, the time to the first search marker inside the band and the
      worst dispatch over the whole search, from the query to 2 s after the
      scan ends (A4 timed only after the scan, so it missed `main`'s
      synchronous scan), for three documents: ordinary (10,000 lines, 100
      matches), sparse (262,144 lines, one match on the last line, map at
      the end) and dense (131,072 matches). Decide D5 by its rule:
      - within one reference frame of `main` on every document: keep the
        completed-scan rule alone;
      - otherwise add AX4 (`forward_async` never blocks past
        `B_INTERACT_US`) and adopt the progressive fallback;
      - where `main` is earlier only through a dispatch over
        `B_INTERACT_US`, record the exception with both figures.

      Record the decision and figures in the design.

## 2. Pure policy (failing first)

- [x] 2.1 `minimap/policy.rs` unit and property tests first, for the
      integer row table and the row-group walker:
      - groups from a monotone row-to-line table;
      - probe calls at most `groups + 1`;
      - hidden-line ranges inside one group;
      - a band rounded outward and clamped;
      - markers past the band ignored.
- [x] 2.2 Unit and property tests first for row runs:
      - coalescing adjacent rows per kind;
      - integer minimum-height expansion centred and pushed inside the band;
      - never below `content_bottom` (the EOF tail);
      - agreement with `expanded_to_min_height` on whole pixels, sampled to
        2^20.
- [x] 2.3 Implement the walker and the run builder under the module's
      `deny(float_arithmetic, disallowed_methods)` with no new `expect`. The
      lanes and colours are unchanged.
- [x] 2.4 Delete `MINIMAP_SEARCH_MATCH_CAP`,
      `MINIMAP_MODIFIED_LINE_MARK_CAP`, `MINIMAP_LONG_LINE_MARK_CAP`,
      `modified_line_mark_samples`, `markers_from_lines` and
      `normalize_line_runs` if unused, `fit_marker_bounds` and
      `MarkerProjectionSpace`. `MinimapAnalysisPolicy::PRODUCTION` gets
      `marker_limit: usize::MAX`, with the bound by construction (design D6)
      in its rustdoc. Replace
      `no_loss_the_minimap_draws_every_marker_main_drew` with a test that
      shows no cap, red against the caps first.
- [x] 2.5 Kani (design D8), harnesses `marker_rows_*` in
      `policy/kani_proofs.rs`:
      - the table is monotone and every row is in exactly one group;
      - sound and complete against a model marker-line set;
      - each in-band marker line lands in exactly one group;
      - no out-of-band row is marked;
      - at most `groups + 1` probe calls and at most `ceil(rows / 2)` runs;
      - runs stay inside the band at the minimum height.

      Remove the three `fit_marker_bounds` harnesses. In
      `scripts/kani-shards.py`, update the minimap entry of the
      proof-strength oracle: drop the three harnesses and
      `fit_marker_bounds` from `functions=`, add the `marker_rows_*`
      harnesses with their measured seconds and the new walker and run
      builder functions, and run `make proof-strength-list`. Then:
      - run `make kani KANI_SHARD=core-geometry-policies` and
        `make kani KANI_SHARD=core-minimap-fit` whole, with `KANI_MEASURE`;
      - update `ci_minutes` and `ci_peak_gib` in `scripts/kani-shards.py`
        only upward, and its evidence strings;
      - run `make check-kani-shards`;
      - record the per-harness times in
        `docs/next/formal-verification.md`, and mark its
        `fit_marker_bounds` entry as retired by this change.
- [x] 2.6 Recount the minimap policy's admitted float functions. Lower the
      ceiling in `.agents/rules/workflow-convention.md` and the script if
      a retired function was admitted, and never raise it. Run
      `make check-workflow-boundaries`.

## 3. Failing-first widget tests (red on `main` behaviour)

- [x] 3.1 Search past the cap. 131,072 matches on alternating lines, map
      scrolled to the end: the strip has search-lane runs on the rows
      showing matching lines. Red on `main` (no markers past the 2,048th).
- [x] 3.2 Modified past the cap. Edit 2,049 separate lines, then one more
      near the end: a modified run appears there. Red on `main`. Also a
      restored 131,072-line draft: every content row is in a modified run,
      red on `main`'s 2,048 samples. Also the stale line-number cache: an
      edit whose line number equals an earlier, shifted edit's is still
      marked, red on `main`.
- [x] 3.3 Long lines past the cap. 65,536 alternating long lines with
      long-line markers on, map at the end: long-lane runs. Red on `main`.
- [x] 3.4 Rendered strip after map scroll. Render the strip to a texture
      (proof harness) after scrolling a document taller than the map, and
      compare its marker rows with a fresh projection. Record whether
      `main` is stale (design D7, AX5). If it is, the test is red on `main`.
- [x] 3.5 Hidden lines. A Protected View fold, or an invisible-tag range in
      the test, holding a modified line between two rows: the group draws
      the lane.
- [x] 3.6 Tracking hygiene:
      - undo history after tagged edits holds only text;
      - saved bytes equal the buffer text;
      - paste of tagged text into another tab leaves no tag there;
      - programmatic replacement (load, draft restore, local-history
        restore, save-formatting mirror-back) still suspends tracking
        exactly;
      - the clean transition clears the markers in its own dispatch.
- [x] 3.7 Budgets (`timing_verdicts_enforced`). Every marker dispatch fits
      `B_INTERACT_US`, and the frame after fits `B_SETTLE_US`, at 2,048 and
      131,072 markers of each kind, 713 and 1,425 rows: refresh, scan
      completion, scroll frames, save with 131,072 modified runs plus every
      retirement slice, and a draft restore of 131,072 lines. The
      minimap's own work is timed on its own against `B_INTERACT_US`; the
      whole frame against `B_SETTLE_US` and `main` + one reference frame
      (design D10).
- [x] 3.8 Search lane after an edit. With a search active, type, then
      scroll the map before the rescan ends: the search lane returns within
      the rescan's idle cycle (design D5), and no dispatch exceeds
      `B_INTERACT_US`.
- [x] 3.9 Retirement queue. Save twice while the first retirement is still
      slicing: both old tags are retired and removed from the table, the
      current tag is untouched, and disposing the editor mid-retirement
      cancels the slices and leaves no tag behind. Edits between slices do
      not disturb the mark cursor.

## 4. Coordination and adapter

- [x] 4.1 Review the 37 uses of `minimap_marker_count` and
      `minimap_marker_bounds` in `crates/lushtext/tests/widget/editor_page.rs`
      (16) and `window.rs` (21) for the band-only semantics (design D11).
      An assertion about a marker outside the band scrolls the map to it
      first; none is weakened or dropped.
- [x] 4.2 `projection_execution.rs`:
      - gather the band (rounded outward) and the row table once per draw
        input;
      - the per-kind probes (search on a completed scan, tag, long-line
        vector, bookmarks);
      - the cache keyed by map value, map adjustment `upper`, strip height,
        band and per-kind generations;
      - draw the integer runs with one `f64::from` per edge;
      - `minimap_marker_count` and `minimap_marker_bounds` report the drawn
        runs.
- [x] 4.3 `watch.rs`:
      - tag the inserted lines in the insert after-handler, and make the
        delete handler an after-handler that tags the joined line (design
        D4);
      - redraw the strip on the map vadjustment's `value-changed` (tracked
        in `map_signals`);
      - re-arm on scan completion (`occurrences-count`).
- [x] 4.4 `retirement.rs`: swap the tag at every site design D4 lists
      (save, both load sites, buffer replacement, local-history restore,
      memory eviction, undo to the saved state) and in
      `mark_entire_buffer_modified` (draft restore), updating the facade
      rustdoc that says "sampled under the cap". Retire old tags from a FIFO
      at `G_PRIORITY_LOW` with a generation guard, each slice bounded by
      `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES` toggles from a `GtkTextMark`
      cursor; the constant has a `Kind: cap` rustdoc line (a power of two,
      rounded down) and is measured to fit `B_INTERACT_US`. Remove each tag
      from the table when empty; cancel the queue on dispose.
- [x] 4.5 `imp.rs` `MinimapState`: drop `modified_marks`,
      `modified_lines_cache` and `markers`. Add the current and retiring
      tags, the bin cache and the generations.
- [x] 4.6 Readiness: `minimap_work_pending` also covers a scan wait and
      queued retirement (design D11). Update the `minimap-refresh` blocker
      in `docs/automation.md` and `docs/automation-reference.md` and run
      `make check-automation-docs`.
- [x] 4.7 `evidence.rs`: add
      - the binned row count;
      - the group count;
      - the probe calls per kind in the last draw;
      - whether search bins are waiting for a scan;
      - the retiring tag slices pending;
      - `long_line_warning_count` without the cap wording.

      No document text. If any field reaches the automation snapshot,
      update `docs/automation.md` and `docs/automation-reference.md` and run
      `make check-automation-docs`.
- [x] 4.8 Keep the facade `minimap/mod.rs` within 370 lines, narrating the
      new stage and resumption points (scan completion, map scroll,
      retirement slices).

## 5. Measurement and proofs

- [x] 5.1 Widget measurements interleaved with `main` (design D10):
      - A1 again (the minimap's share per phase);
      - A2 at the caps;
      - the 131,072 fixtures at 713 and 1,425 rows;
      - the save retirement;
      - the draft restore;
      - an in-tab Replace All of 131,072 matches: its dispatch and the
        peak RSS of the toggle segments (design D4, Memory);
      - a buffer replacement and a load over a buffer holding 131,072
        modified runs (their clear turns now remove toggles).

      Tabulate them in the design under "Measured verdicts". Any miss with
      a toolkit cause becomes an exception with its axiom and a bounding
      widget assertion.
- [ ] 5.2 Lane cases `minimap-markers/2048` and `minimap-markers/131072` in
      `scripts/responsiveness-smoke-driver.py` (open, find, Page Down,
      scroll, type, save). Run interleaved with `main`, then
      `make check-responsiveness-lane`.
- [x] 5.3 `make visual-geometry-smoke` (rendered-pixel proof: the
      `native-minimap-*-anchors` invariants and `minimap-search-marker-row`
      in `minimap-sidebar-mid`). If the detected marker row moves by the
      removed anti-alias row, re-record the baseline with the reason.
      Then `make check-visual-proof-policy`.
- [x] 5.4 `make visual-smoke` (`main-search-minimap`) and
      `make accessibility-smoke` (`minimap-transition`). The strip stays
      decorative.
- [ ] 5.5 `make check`, `make test` (shards parallel), `make
      check-widget-shards` (assign new widget tests),
      `make test-prop`, and `cargo check -p lushtext --bins --locked`.
      Benchmarks compile.
- [ ] 5.6 Mutation scope: remove the `fit_marker_bounds` notes from
      `.cargo/mutants.toml` and `docs/mutation-testing.md` (or reword them
      as history), and run `make mutants-diff` over the minimap policy so
      every surviving mutant of the walker and the run builder is killed or
      recorded as equivalent with its reason.

## 6. Documentation (per `.agents/rules/documentation.md`)

- [x] 6.1 `docs/thresholds.md`: remove the three cap rows; note D6's bound
      by construction (relative to the live buffer) under the minimap
      policy; add a row for `MINIMAP_MARKER_MIN_HEIGHT`, now an `i32`
      (`out: geometry`), and for `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES`
      (`cap`, with its measurement). Run `make check-thresholds`.
- [x] 6.2 `docs/workflow-readability-matrix.md`, minimap row:
      - re-derive the measured cells (size, seam counts, pure-policy
        consumers, excluding `#[cfg(test)]`);
      - name the new coordination step;
      - remove `MarkerProjectionSpace` from the row's seam value objects
        (the seam list near line 2651);
      - run `make check-workflow-boundaries`.
- [x] 6.3 `AGENTS.md` (`.claude/CLAUDE.md`): the minimap module line and the
      "Minimap byte classifier" decision gain the binning, the tag
      tracking and the completed-scan rule. Update `README.md` if a feature
      line names marker limits.
- [x] 6.4 `openspec/specs/editor-minimap` (synced at archive) and
      `docs/accessibility-matrix.md` (A11Y-EDITOR-MINIMAP, if evidence
      changes).
- [x] 6.5 `docs/next/long-line-documents.md`:
      - the PT row and queue item;
      - a "BM" implementation record with the M7 attribution and its owners
        (DL for the wrapped layout, the GtkTextView/GtkSourceMap relayout
        after a burst of edits);
      - the verdicts and any exceptions;
      - the release-notes draft: "minimap markers 2,048" becomes "every
        minimap marker";
      - the `lighten-notes-browser-rows` queue position.
- [x] 6.6 `.agents/skills/gtk-testing` references. Note that measurement
      tests must destroy their windows, or later repetitions include
      earlier ones' relayouts (design D1).

## Implementation notes (stage 3, 2026-11-07)

- 1.2: the 1,425-row strip and the 9.5 MB (66,000-pair) document are in the
  scroll comparison (design "Measured verdicts").
- 1.3: A98–A101, plus A102 for D5's fallback (`forward_async` never blocks
  but loses a superseded request with a GIO CRITICAL), each with a probe,
  a `--check` sample and a ledger row.
- 1.4: D5 required the fallback (the ordinary document was later without an
  over-budget dispatch on `main`); `forward_async` was dropped for A102, and
  an interim search of a bounded private copy of the band's text was adopted
  (design "D5 decided"). First marker 150–153 ms against `main`'s 261–348.
- 2.5: Kani domain reduced to bands of one to three rows (four exhausted the
  14 GiB cap), widened by an expansion harness on every whole-pixel band up
  to 2^20 and a merge harness; the harnesses form a new `scheduled` shard,
  `core-minimap-binning`, with derived figures.
- 3.4: not red on `main` in its scenario (design D7 note); kept as a guard.
- 3.7: also enforces the retirement slice's own time; it found the 16,384
  cap at 6.9–9.1 ms per slice and set `MINIMAP_MODIFIED_TAG_RETIRE_TOGGLES`
  to 4,096. Its debug run uses 2,048 markers.
- 4.6: the `minimap-refresh` blocker also covers a scan wait and a retiring
  tag; no evidence field reaches the automation snapshot.
- 5.2: the `markers` fixture kind and the `minimap-markers/2048` and
  `/131072` lane cases (full tier) are added and self-tested; the lane never
  saves, so the save's retirement is measured by the widget lane only.
- Pre-existing defects fixed: `gtk_lush_settle::Debounce` removes its
  superseded source (Too many GWeakRef registered), and the tag replaces the
  source marks that collapsed onto line 0.

