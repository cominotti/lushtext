Rules for every task:

- Run builds and checks pinned to CPUs 8–23 (`taskset -c 8-23`).
- Run heavy jobs under `scripts/with-memory-cap.sh`, with one CBMC at a time.
- Run headless only.
- Take measurements only when the host's 1-minute load is below 4. Interleave
  `main` and branch runs, and record the load before and after each run.
- Keep handoff artifacts in `.claude/handoffs/power-of-two-thresholds/`.

## 1. Gate first, at today's values

- [ ] 1.1 Write `scripts/check-threshold-kinds.py` (design D10):
  - find every integer `const`/`static` in the production `src/` trees,
    skipping `#[cfg(test)]` modules, `kani_proofs.rs` and `gtk-lush-axioms`;
  - evaluate simple literal and constant-reference expressions;
  - compare against `docs/thresholds.md`;
  - check that each in-scope constant's rustdoc `Kind:` line names its kind.
- [ ] 1.2 Write a self-test on a fixture tree. It must fail first on each of:
  - an unlisted constant;
  - a stale row;
  - a value mismatch;
  - a non-power-of-two cap;
  - a missing or disagreeing `Kind:` line.
- [ ] 1.3 Generate `docs/thresholds.md` at `main`'s values. Classify every row
  per D1/D2 and copy the decisions from the design's inventory. Add
  `make check-thresholds` and put it in `check-policy`.
- [ ] 1.4 At this point the in-scope rows that this change will convert are
  marked `pending-PT`, the only temporary class. The check must pass on
  `main`'s values with them. The class is removed in 8.3.

## 2. Failing-first value tests

- [ ] 2.1 Change `the_policy_constants_have_their_recorded_values`
  (`model/document_scale.rs`) to the new values (D3–D5 and D9; the open tier
  stays as is until task 3). Confirm that it fails before the conversion.
- [ ] 2.2 Add edge tests at the byte tiers: 8 MiB and 8 MiB + 1, 64 MiB and
  64 MiB + 1, for minimap, save-only history, undo and history unavailable.
  Add edge tests at each converted paragraph threshold (at it and one above)
  through `DocumentScale::for_profile`, with every delimiter kind.
- [ ] 2.3 Add retention tests that fail first:
  - a lineage holding 48 snapshots and a store holding 240 keep every
    snapshot after one more capture;
  - a 20-entry `search-history.json` keeps all 20 entries plus the new one;
  - a 200-entry recents file keeps every entry.
- [ ] 2.4 Add message tests that fail first:
  - the local-history unavailable message says "64 MiB";
  - the open refusal names the tier in MiB;
  - the reduced-features notice and the accessible description use IEC units.

## 3. The open tier (measured before it changes)

- [ ] 3.1 Measure the exclusive-load memory peak.
  - Build the `responsiveness-probes` binary on `main` and on the branch.
  - Generate short-line plain-text fixtures of 500,000,000 bytes and
    512 MiB, and a legacy-encoding variant that exercises
    `DECODED_BODY_SOURCE_MULTIPLIER`.
  - Open each headless, three repetitions per side, interleaved, each under
    `scripts/with-memory-cap.sh`. Record the scope's `memory.peak`, swap use
    and the largest dispatch.
- [ ] 3.2 Accept 512 MiB if both of these hold:
  - the branch's peak at 512 MiB is at most main's peak at 500 MB × 1.074,
    plus 5 % run spread;
  - no run swaps or is OOM-killed.

  Then set `REFUSE_TO_OPEN` to 512 MiB, and note that
  `FILE_LOAD_OVERWEIGHT_PROGRESS_CAPACITY` follows. Otherwise keep
  500,000,000 as a recorded exception with the figures. Record the result in
  the programme record.

## 4. Pure conversions (no measurement)

- [ ] 4.1 Change the D3 tiers except the open tier: `S_MINIMAP`,
  `HISTORY_SAVE_ONLY_BYTES`, `UNDO_TOTAL_BYTES`, `HISTORY_UNAVAILABLE_BYTES`,
  `BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD`, `MAX_REPLACE_FILE_BYTES`.
  - Rename `retired_byte_tiers` to `byte_tier_reference`, holding the new
    table.
  - Route the tier-naming messages through one IEC formatting helper.
- [ ] 4.2 Apply the D4 caps:
  - `L_MINIMAP`, `L_INVISIBLES`, `L_SEARCH_HIGHLIGHT`, `N_VISIBLE`,
    `N_HEAD`;
  - the twelve 10,000-entry caps and their bench mirrors;
  - `BOOKMARK_EXCERPT_SCAN_LINE_LIMIT`;
  - the minimap caps and the wrapped-layout budget;
  - `MAX_RESULTS_PER_SOURCE`, `NOTES_BROWSER_RENDER_LIMIT`,
    `MAX_RECENT_LOAD_DIAGNOSTICS`;
  - the `cargo-gtk-proof` text caps.

  Check that `thresholds_are_ordered_as_the_profile_counts_them` still holds.
- [ ] 4.3 Apply the D5 coverage windows: `R_BATCH` → 256 (and so `R_FOLD`;
  `N_DENSE` becomes 218 through its formula), `MAX_CONSTRUCTION_STAGES`,
  `MAX_FRAME_PHASE_FRAMES`, `MANIFEST_REPAIR_ENTRY_OVERHEAD_BYTES`,
  `REVEAL_SETTLE_FRAME_BUDGET`.
  - Check the compile-time assertions: `R_FOLD ≤ MAX_R_FOLD`,
    `R_FOLD · N_DENSE ≤ V_BATCH`, and the watch's
    `2·R_batch + 2` read bound.
  - Update the widget test that asserts the per-keystroke read count.
- [ ] 4.4 Apply the D6 retention tiers and retry budgets.
- [ ] 4.5 Run the corpus counts for the conditional caps (D4).
  - Write a throwaway `cargo run --example` (scratchpad, not committed) that
    counts, with `pulldown-cmark` and the app's parser options, the events
    and the largest table's cells of every Markdown file of at most 4 MiB in
    the local corpus: this repository, the crates.io registry, and
    `~/Workspace` excluding `node_modules`.
  - If any file has events in (32,768, 50,000], keep `MAX_MARKDOWN_EVENTS`
    as a recorded exception with the count. Otherwise set it to 32,768.
  - Decide the two table-cell caps the same way, with (512, 1,000].
  - Record the counts in the design's baseline and the programme record.
- [ ] 4.6 Run the D2 shipped-string check. Find the longest announcement,
  editor identity and reduced-features notice the app can produce: from the
  accessibility and widget tests, plus a 255-byte file name. A cap that
  would truncate a shipped template (beyond the file name it is meant to
  cut) becomes a recorded exception at its current value.
- [ ] 4.7 Write the D9 rustdoc. Each exception gets "Kind: …; recorded
  exception (`power-of-two-thresholds`)" with its measurement from the
  design, replacing "deferred to `power-of-two-thresholds`":
  - `L_SYNTAX`;
  - the per-language gates and `S_SYNTAX_OTHER`;
  - `L_PROTECT_EDIT`, `V_BATCH`;
  - `MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES`;
  - `MAX_PREVIEW_IMAGE_SOURCE_PIXELS`.

  Give every other in-scope constant its `Kind:` line, including N2's
  `MAX_SYNTAX_PARAGRAPH_BYTES` (already "external bound, derived": the gate
  pins it).

## 5. Measured items (lane, interleaved with `main`)

- [ ] 5.1 Check the coupled pair (D7).
  - Add a `dense/512` case to the lane's dense group (fixture generator plus
    lane case list).
  - Run it in both wrap modes and three input variants, three repetitions per
    side, against the criterion `dense/500` met: every validation dispatch
    until settle at most half of `B_SETTLE_US`, every post-open dispatch at
    most `B_SETTLE_US`.
  - If it passes, set `L_PROTECT` and `N_INSPECT` to 512. Otherwise set both
    to 256.
  - Update the Protected View inspector widget tests for the window width.
- [ ] 5.2 Measure `V_SLICE` (D8): 16,384, then 8,192 if needed.
  - Run `dense/{120,500,710}` in both wrap modes and three input variants,
    plus `baseline/small`.
  - Record the largest validation dispatch, the largest post-open dispatch,
    and activation to laid out.
  - Adopt the first candidate that meets D8. Otherwise record 11,360 as an
    exception with the figures.
  - Update C6's accepted-miss entry ("Revisit when `power-of-two-thresholds`
    re-measures `V_SLICE`").
- [ ] 5.3 Measure the direct-install threshold at 512 KiB (D8).
  - Run short-line `.txt` and Markdown at 512 KiB, 600 KiB and 704 KiB.
    Record activation to laid out and the largest dispatch.
  - Keep 512 KiB (and `PREVIEW_DIRECT_THRESHOLD_BYTES`, which follows) if
    the branch is within one reference frame of `main` at every size.
    Otherwise record 704 KiB as an exception.
- [ ] 5.4 Measure the search per-turn caps (D8).
  - Add a widget fixture, failing first on the old cap value, that fills one
    turn with 256 `Match` events and one retirement turn with 256 rows. Time
    them with the probe build's dispatch timing on the reference lane.
  - Adopt 256 if both stay within `B_SETTLE_US`. Otherwise adopt 128 and
    update the two delta specs (`main-thread-responsiveness`,
    `performance-regression-coverage`) to 128.
- [ ] 5.5 Each measured item lands in its own commit with its figures and the
  load before and after.

## 6. Kani

- [ ] 6.1 Find every shard whose harnesses (or the production code they call)
  read a changed constant: `grep` the `kani_proofs.rs` files and
  `scripts/kani-shards.py`. Expected:
  - `core-document-scale` (tier reference, thresholds);
  - `core-paragraph-profile`;
  - `core-profile-watch-step`, `core-profile-watch-reads`,
    `core-profile-watch-unsound`;
  - `core-protected-fold`, `-shape`, `-mapping`;
  - `core-protected-view`, `core-protected-view-navigation` (production
    `N_INSPECT` width);
  - `core-post-open-settle` (`V_SLICE`, the direct threshold);
  - `core-paragraph-boundary`.

  Record the list with the reason for each shard.
- [ ] 6.2 Rerun each listed shard: `make kani KANI_SHARD=<shard>
  KANI_MEASURE=…`, one at a time, under `scripts/with-memory-cap.sh`. Every
  harness must pass. Record wall time and peak memory against the shard's
  table entry. Update `scripts/kani-shards.py` budgets when a figure leaves
  its margin, and run `make check-kani-shards`.
- [ ] 6.3 `make proof-strength PROOF_STRENGTH_MODULE=model/document_scale`
  (tests arm) to confirm that the new tier-reference harness kills the tier
  mutants that the parity harness killed.

## 7. Verification

- [ ] 7.1 Toolkit regression: `make gtk-axioms`. No new axiom is relied on;
  A28 and A31 are cited unchanged. Confirm that A31's probe still reports
  112 rows per 2000 px batch at the default font, the figure `R_BATCH`'s
  doubled coverage (now 256) rests on.
- [ ] 7.2 `make check`, `make test` (unit, integration, widget shards),
  `make check-thresholds`, `make check-workflow-boundaries`,
  `make check-automation-docs`, `make check-kani-shards`.
- [ ] 7.3 Responsiveness lane, quick tier, interleaved with `main`: the
  keystroke group (the 514-read bound), the dense group, `baseline/small`,
  total-size tiers at 8 MiB ± 1 and 64 MiB ± 1. Every case passes or is
  attributed to an existing recorded exception.
- [ ] 7.4 Accessibility smoke, because the announcement and description caps
  and texts changed.

## 8. Documentation

- [ ] 8.1 Update the README:
  - the feature list (minimap 16,384 / 8 MiB, search highlighting 8,192,
    invisibles 32,768, the open, undo and local-history tiers, the Protected
    View head 8,192, the Line Inspector window);
  - the local-history section (8 MiB / 64 MiB);
  - the Markdown ceilings;
  - the set-aside bound (128 drafts or 256 MiB).
- [ ] 8.2 Update AGENTS.md: every figure in Key Design Decisions that changed
  (`R_batch`, the tiers, the 10,000 caps, 250 per turn, the search-history
  cap, local-history caps, `N_HEAD`, `N_INSPECT`, `V_SLICE`, the direct
  threshold).
- [ ] 8.3 Update the rules:
  - `.agents/rules/rust.md`: the conversion is done; point to
    `docs/thresholds.md` and `make check-thresholds`; add the retention and
    retry rounding (D2).
  - `.agents/rules/build.md`: the new check in the policy list.
  - Remove the temporary `pending-PT` class from the checker.
- [ ] 8.4 Update `docs/accessibility.md` and `docs/accessibility-matrix.md` if
  a notice, description or announcement cap changed what is spoken.
  Update `docs/automation.md` and `docs/automation-reference.md` if a
  snapshot field reports a changed threshold.
- [ ] 8.5 Update the programme record (`docs/next/long-line-documents.md`):
  - the PT row's status;
  - an implementation record with every measurement (corpus counts, memory
    peak, `dense/512`, `V_SLICE`, direct threshold, per-turn caps, Kani
    figures);
  - the follow-up candidate for displayed file sizes in IEC units.
- [ ] 8.6 Update the workflow-readability matrix only if a policy module's
  measured cells moved (they should not).
- [ ] 8.7 AppStream: no change to the historical release notes. Prepare the
  user-facing sentence about the new tiers for the next release's notes, in
  the programme record.
