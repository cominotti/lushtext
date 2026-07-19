# Baseline Evidence

Captured on 2026-07-18 before implementation edits for this change.

## Checkout and environment

- Baseline commit: `95d171a911e792991bc1aa7446f0ea7e6ffe404a`
- Worktree: the existing dirty post-portfolio checkout was retained as the
  same-checkout baseline; no implementation file for this change had been
  edited before these runs.
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`, LLVM 22.1.2
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`
- Host: x86_64 Fedora Toolbx, Linux `7.1.3-200.fc44.x86_64`
- Storage: Btrfs on `/dev/dm-0[/home/danilo]`
- Criterion profile: Cargo `bench` profile, optimized
- Load at baseline start: `11.13 / 8.97 / 7.33`
- Load after the baseline and focused tests: `18.37 / 18.50 / 15.73`

The host was busy throughout the run. Final comparison must use the same
toolchain, profile, host, and storage class, and must review Criterion
distributions/effect sizes rather than treating this run as an idle-machine or
cross-machine absolute threshold.

## Criterion baseline

`make bench-baseline` completed successfully and saved the Criterion baseline
named `main`. The saved tree contains 28 `main` artifact paths. Criterion
extended target time for several large cases because the requested sample
count did not fit the default five-second window; those sampling notices were
not benchmark failures.

The complete suite included file-index search/rebuild/incremental cases,
directory-only traversal, end-to-end boundedness, 1/10/50 MiB editor I/O,
Replace All/Undo, Notes, minimap, Markdown, content search, transient load,
watch pressure, recovery, and disposal/admission policy groups.

## Focused test baseline

- Unit lane: the first filtered `rtk make test-unit` invocation exited opaquely
  without naming a failing test. The immediate unfiltered rerun completed with
  1,246 passed and 10 intentionally skipped. Treat the first result as a
  runner anomaly until the lane is repeated during closeout; it is not evidence
  of a product-code failure.
- Integration lane: 73 passed, 0 skipped.
- Focused headless widget cases passed:
  - release-bounded search-result retirement and current-generation isolation;
  - chunked snapshot mutation cancellation;
  - large-save snapshot consistency;
  - document-sized Local History restore and Undo Restore;
  - incremental file-index publication/readiness;
  - bookmark activation through Browse Notes;
  - startup-loaded recent documents with no tabs;
  - minimap stale-generation cancellation (slow under the recorded load, but
    terminal and successful);
  - Markdown image-flood admission; and
  - Replace/Undo transaction gating.

## Pre-existing blocker found

`scripts/run-widget-tests.sh --headless -- --list --format terse` returned a
false warning failure because the warning scanner matched legitimate listed
test names ending in `_warning:`. The scanner is corrected in this change to
require a line or whitespace boundary before lowercase Cargo `warning:`
diagnostics. The exact list command is the focused regression gate.
