## Why

`restore-main-ci` (PR #99, merged `dee22056` on local evidence) left three
items open, and `.agents/rules/preexisting-blockers.md` makes each a blocker:

1. **F1. The D2 guard fails on every CI run.**
   `editor_page::test_a_reallocation_offset_at_the_top_is_still_repaired_after_a_reflow`
   failed on both attempts of every PR run (`7da3d54c`, `fd9ce7b5`,
   `318e43a3`, `00a4c26c`). It passed on every local run in the toolbox,
   starved or not. So the `editor-page` widget shard is red on every CI run.
2. **4.2. A bookmark jump landed at the wrong line once.**
   `window::test_a_bookmark_opens_in_a_never_shown_tab_in_preview_only_mode`
   failed once on CI (run 43614093974), inside the full `window` shard: top
   line 69 for a bookmark at line 300, document shown. It was never
   reproduced locally (20/20 and 40/40 starved isolated passes). If it is
   real, the user sees a bookmark open at the wrong place.
3. **The eight-branch-run proof** for the `editor-page`, `window` and
   `surfaces` shards (design D4 of `restore-main-ci`).

**F1 is not CI-only any more.** This change's spec stage took two steps:

- It read the enriched failure message of PR run 43678642280 (`00a4c26c`).
- It reproduced the failure on this workstation: the host-built widget binary
  runs in the CI image (`registry.fedoraproject.org/fedora:45` with
  `ci.yml`'s packages) with the CPUs limited.

The findings (design "Measured baseline"):

- The guard fails because the editor does **not** rest at the top when the
  guard begins. The vertical value is 8, not 0, already after
  `present_settled_minimap_document`. The fixture's precondition
  (`visible_rect().y() < 10`) lets that through. The settle therefore finds
  the view off the top and does not repair it; the 0.5 px edge epsilon makes
  value 8 "not at top". That last link is inferred, not shown.
- In the CI image, the value is 8 with 1, 2, 4 or 8 CPUs, whether limited by
  quota or by cpuset: deterministic: 20 of 20 attempts in 10 runs. With the container's
  16 CPUs it is 0 and the test passes. In the toolbox, also at 16 CPUs, it is
  0 in every configuration tried. That includes CI's font set, and software
  GL, though the renderer used was not confirmed.
- Working condition: the CI image with a small CPU count. That the image
  matters, not just the count, rests on `restore-main-ci`'s recorded toolbox
  runs pinned to one or two CPUs, which passed. Which writer moves the view to
  8, and which property of the environment lets it, are not known yet.

What remains open is whether an editor can open 8 px below its top in
production on a small machine. Four CPUs is a common laptop. If it can, the
top-anchor repair is disarmed and the first line is clipped, so user
experience comes first. Neither tightening the fixture's precondition nor
writing `set_value(lower)` in the test is acceptable until the writer is
named and the production question is answered.

## What Changes

- Add a tracked lane that runs widget tests in the CI image with a CPU limit,
  `scripts/ci-image-widget-test.sh`. It runs one test or one shard, N times,
  through `scripts/heavy.sh`, so a CI-only failure is reproduced before CI
  rounds are spent.
- F1:
  - name the writer of the 0 → 8 move with an env-gated trace from page
    construction on (test-utils only, never committed);
  - decide whether it is a product defect: a real window, not only the bare
    test page, at the same CPU limit;
  - fix it at the cause, failing first in the container lane;
  - make the minimap D2 tests' "at the top" precondition exact (value at
    `lower`), so a fixture that is not at the top fails as a precondition
    and not as a missing repair.
- 4.2: run the full `window` shard in the container lane, which tests R5's
  lead, the interaction inside the full shard. If it reproduces, take the
  normal diagnose-and-fix path. If not, take `restore-main-ci` D4's
  "not reproducible" branch: CI-log diagnosis, a deterministic mechanism
  test, and eight branch CI runs.
- The inherited watched tests of the three shards (design B5) run in the
  lane first. A test proved there needs no CI runs. A test still not
  reproduced owes eight branch CI runs of its shard on this branch's final
  code, with every run id and attempt recorded, or, if the stage brief does
  not allow the CI-evidence exception, is recorded as owed to the
  programme-end CI check (design D5).
- Docs: the gtk-testing flake reference gets the "CI image + CPU count"
  class and the container lane, `docs/next/long-line-documents.md` the
  follow-up's outcome, `docs/end-user-coverage.md` or `build.md` the new
  lane.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `widget-test-ci-sharding`: a failure seen only on CI is reproduced in the
  CI image under the runner's CPU budget before it is treated as CI-only; a
  shard is proved on CI by runs of the branch's final code.
- `editor-minimap`: an editor presented at the top rests exactly at the top
  whatever the CPU count, and the reflow-repair guard begins from that rest
  state.

## Impact

- Tests: `crates/lushtext/tests/widget/editor_page.rs` (minimap D2 fixture
  and guard), possibly `window.rs` (4.2), and a new failing-first test of
  whatever writer the trace names.
- Product code: only where the diagnosis puts the cause. Likely candidates are
  `ui/editor_page/minimap/` (settle, rest state), `ui/editor_page/overscroll.rs`
  and `ui/editor_page/view_attachment.rs`, or none if the writer is
  test-only.
- Scripts: `scripts/ci-image-widget-test.sh` with `--self-test`, wired into
  `make check-agent-helpers` or `check-policy`.
- CI: no workflow change expected. Any proof runs are this branch's
  `pull_request` run and its reruns, under the CI-evidence exception of
  `.agents/rules/parallel-agent-work.md`, and only for proofs that exist
  only in CI.
- Smokes: `editor_page.rs` is under a UI prefix, so the test edits owe
  `visual-geometry-smoke`, `accessibility-smoke` and `visual-smoke`
  whatever the diagnosis (`.agents/rules/testing.md`).
- Kani: none expected (no proven module touched). Conditional rerun in design.
- Timing: none expected.
