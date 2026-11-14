## Why

`main` has not had a green CI run since `3dcbe682` (2026-11-09). Under
`.agents/rules/preexisting-blockers.md` every red job and every test the
per-test retry rescues is a blocker. This change was asked to look at two red
jobs. A scan of every `ci.yml` run since 2026-11-12 (main and pull requests,
`gh run view --log`) found them, the earlier failures they were asked to
confirm, and three more tests the retry hides. Two of the blockers are
product defects that CI caught by accident, not test problems:

1. **The restored text never reaches its draft (data safety).**
   `window::test_autosave_during_a_local_history_restore_keeps_the_previous_draft`
   fails with "the restored text is still owed a draft". It failed in runs
   43614093974, 43619204060, 43623373361, 43625734736 and 43629270107 (each
   time on both in-process attempts and on the job retry). Two experiments on
   `main` (`b129f378`) fail **deterministically** with the same message (patch
   in `.claude/handoffs/restore-main-ci/spec-experiments.patch`):
   - the draft had settled (autosave written, `draft_dirty` false) before a
     local-history restore: after the restore the tab is still not
     draft-dirty, so no autosave ever writes the restored text;
   - an autosave pass captured before the restore commits after the replacement
     started: its completion clears `draft_dirty`, because the replacement never
     advanced the dirty generation.

   The bounded replacement suspends the buffer handlers that mark a tab
   draft-dirty. Draft recovery already compensates for that at its terminal
   (`ui/window/drafts/restore_execution.rs`); Restore and Undo Restore do not.
   After a crash the user gets back the text from before the restore, not what
   was on screen. The CI test only fails when the window's own 5 s autosave tick
   (`AUTOSAVE_TICK_INTERVAL`) runs before the replacement: failing attempts took
   5.33–5.52 s, passing ones 3.34–4.13 s.

2. **A scroll is undone during a passive width reflow (user experience).**
   `editor_page::test_minimap_native_slider_covers_the_visible_text_with_an_overscroll_margin`
   fails its precondition "the editor stays scrolled to the middle" with
   `visible_rect.y = -6`: the view went back to the top after it had scrolled
   to line 1000. It failed on 9 runs since 2026-11-11, including `main`
   `23518e53`, `d3c0bca7` and `b129f378` (PR #94's merge).
   - The suspect is `finish_minimap_reflow_settle`. It moves the view back to
     the top whenever the rest state says "at top", and that rest state is
     paused for the whole passive burst.
   - An experiment on `main` reproduces it deterministically. At the top,
     hide the minimap (a passive width burst), then scroll to line 1,000
     before the burst settles: the view ends at `y = -6`, CI's exact
     signature. A control that changes only the content width passes.
   - The site that writes the value back is still to be pinned. Diagnosis
     comes first in the tasks.

3. **The GTK Lush API Advisory job is red** since the CI image moved to
   Fedora 45 (2026-11-11).
   - `cargo install --locked cargo-public-api@0.52.0` builds `openssl-sys
     0.9.88` from the crate's lockfile. That version aborts on OpenSSL 4.0.3:
     "only compatible with OpenSSL (version 1.0.1 through 1.1.1, or 3.0.0)".
   - 0.52.0 is the newest release, so no newer pin exists. `openssl-sys`
     supports OpenSSL 4.x from 0.9.114 on.
   - `cargo-semver-checks 0.48.0` installs fine in the same job.

4. **Tests the retry rescues on CI.** These are `ok (FLAKY: passed on attempt
   2)` lines in runs from 2026-11-14:
   - `workspace_tree_virtualization::test_directory_retention_stays_within_its_charge`,
     4 times. The growth is bimodal at 2362–2375 or 2534–2537 bytes per row,
     against a bound of 5/4 × 1944 = 2430 bytes per row.
   - `window::test_a_bookmark_opens_in_a_never_shown_tab_in_preview_only_mode`,
     once: "condition was not met within 10s".
   - `protected_view_surfaces::test_a_protected_periodic_capture_reads_the_source_not_the_buffer`,
     once: "condition was not met within 30s".

5. **Already fixed on `main`, confirmed, not in scope.**
   - The rustdoc private intra-doc links (`COST_REPETITIONS`, `RemoteInput`):
     fixed by `df573812`. Lint has passed on `9ff19f2e` and `b129f378`.
   - The `atomic_write_bytes_preserves_user_xattr_when_supported` non-widget
     failures: on branches without `0a7368d0`.
   - The Glycin sandbox warning in the surfaces shard: classified by `6bc39e93`.
   - Dependency Policy: green since `df573812`.

## What Changes

- A completed local-history Restore and Undo Restore mark the tab draft-dirty
  and advance the dirty generation through the operation draft recovery uses
  today, held once in the draft workflow so the three terminals cannot drift.
  An autosave pass captured before the replacement can no longer clear it. The
  existing window test asserts the invariant ("owed or already written"), not
  the scheduling accident.
- Diagnose the scroll reset first, failing first. Then fix the cause: a scroll
  the user or the app makes during a passive reflow burst is kept. The
  top-anchor repair for GTK-preserved offsets keeps working.
- The API advisory links the image's libcurl (`libcurl-devel`) and installs
  `cargo-public-api` from its checksum-pinned crate. Exactly one lockfile
  entry changes: `openssl-sys`, raised to a pinned 0.9.118. `--locked` is
  kept. A `fedora:45` container verified it; it fails with either half
  alone. Every pin sits in the workflow `env`, and the job stays advisory.
- Diagnose the three retry-rescued tests by class (a synchronous UI flip, an
  asynchronous `spawn_blocking_then` completion, or a production race). Fix
  each cause and prove the fix with repeated isolated runs. A retry or a longer
  timeout is never the fix.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `draft-session-recovery`: adds the rule that text a whole-buffer replacement
  installs is owed a draft like an edit.
- `editor-minimap`: adds the rule that a reflow repair never undoes a scroll made
  during the burst.
- `gtk-lush-internal-platform`: adds the reproducible advisory tool install on
  the CI image.
- `widget-test-ci-sharding`: adds the rule that a retry-rescued test is diagnosed
  and fixed, never tolerated.

## Impact

- User-visible:
  - After a Restore or Undo Restore, a crash recovers the restored text.
  - Opening, scrolling or jumping into a document during a width reflow (the
    minimap appearing, a sidebar animation, a resize) no longer snaps back to
    the top.
  - Nothing is lost compared with `main`.
- Code: `ui/window/local_history/restore_execution.rs`,
  `ui/window/drafts/` (the shared draft-owed operation), `ui/editor_page/`
  minimap reflow and overscroll, widget tests, `.github/workflows/ci.yml`, a
  small install script, `.agents/rules/build.md` pins.
- Docs: `docs/workflow-readability-matrix.md` (drafts and local-history rows if
  an operation moves), `.agents/rules/build.md`, `docs/next/minimap.md`, the
  gtk-testing flake notes.

## Non-goals

- Making the API advisory job blocking. It stays advisory, as `build.md` says.
- Changing the autosave interval or the first-dirty debounce.
- Reworking the reflow freeze or the rest-state API beyond what the diagnosis
  requires.
- The red Kani or end-user-smoke lanes, which are outside `ci.yml`.
