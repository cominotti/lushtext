## Context

`restore-main-ci` merged on local evidence (maintainer decision, 2026-11-15)
and handed three items to this follow-up (archive
`openspec/changes/archive/2026-11-15-restore-main-ci/`, design R5 and R6,
tasks "Merge record"):

- F1, the D2 guard that fails on every CI run;
- 4.2, the bookmark preview-only test, which failed once on CI;
- the eight-branch-run proof for `editor-page`, `window` and `surfaces`.

The brief describes F1 as "CI-only". The spec stage found that this is no
longer true: it reproduces on the workstation in the CI image under a CPU
limit (B2 to B4). The design is built on that reproduction.

Evidence lives in `.claude/handoffs/diagnose-ci-only-reflow-guard-failure/`.
The directory is untracked, so every figure the design relies on is also
written here.

## Measured baseline

**B1. The enriched CI message (F1).** PR run 43678642280 (`00a4c26c`,
`restore-main-ci`'s final commit), job 132704248540, failed both attempts
with the same text. Log: `ci-logs/43678642280-editor-page.log`.

```
the settle must restore the top a reallocation offset moved
(Rectangle { x: 0, y: 114, width: 808, height: 664 }); settle pending false,
vertical emissions C(8,u48504,p664) C(120,u48744,p664) V(120) C(120,u48504,p664)
```

- The first recorded `changed` carries **value 8**: before the test's action,
  the view was 8 px below the top. `visible_rect().y` is the value minus the
  6 px top margin, so `y = 2`, which passes the fixture's `y < 10` "at the
  top" precondition.
- No `value-changed` was bare (none moved to a value no `changed` carried).
  The test's recorder applies the same rule as the product's classifier
  (`note_scroll_during_reflow`, `overscroll.rs`: a value more than 0.5 px
  from the last `changed` value while paused), so "scroll detected" is
  unlikely. **Inferred, not shown:** `reflow_scrolled` itself was not
  printed.
- The settle had run ("settle pending false"). That rules out "still
  pending".
- What is left is "settled without repairing". With the value at 8,
  `RestState::at_lower` is false (`LOWER_EDGE_EPSILON` 0.5,
  `gtk-lush-viewport`), and the repair runs only at the top. **Inferred,
  not shown:** `at_lower` and `reflow_scrolled` were not printed.
- The same job's `window` shard passed with no `FLAKY` line, and so did
  `surfaces`. The directory-retention figure was 1,753 B/row against a
  1,944 B/row charge.

**B2. Environments compared.** Toolkit and fonts, recorded in
`toolbox-env.txt` and `ci-image-env.txt`:

| | CI image (`fedora:45` + `ci.yml` packages, no weak deps) | toolbox |
| --- | --- | --- |
| GTK / Libadwaita / GtkSourceView | 4.24.1 / 1.10.0 / 5.22.0 | same |
| Pango / GLib / mutter / HarfBuzz | 1.58.2 / 2.90.1 / 51.1 / 15.0.0 | same |
| fonts | Cantarell VF, Noto Sans VF only | 61 families |
| `fc-match monospace` | Noto Sans (proportional) | Noto Sans Mono |
| GPU | none (`/dev/dri` absent, llvmpipe) | `/dev/dri` present |
| GTK renderer | cairo | cairo |

The editor asks for `Adwaita Mono 14.667px`. On CI that resolves to Noto
Sans, so the document is taller: `upper` is 48,504 against 44,504 locally.
In the toolbox it resolves to Adwaita Mono itself (`adwaita-mono-fonts`).

GTK draws with cairo in every run, in the image and in the toolbox: the
widget harness sets `GSK_RENDERER=cairo` before GTK starts unless the caller
overrides it (`configure_widget_test_environment`,
`crates/lushtext/tests/widget.rs`), and `run-widget-tests.sh` sets the same
default. The GPU difference can therefore reach only mutter, the headless
compositor, whose GL backend (hardware or llvmpipe) paces the frames GTK
waits for.

**B3. The guard, reproduced.** The host-built widget binary
(`widget-1417af255dc67c7d`) ran in the CI image through
`ci-container-guard.sh`. The paths were bind-mounted at the same locations,
with SELinux labels disabled and the worktree read-only. A temporary
diagnostic patch (`diag-prints.patch`, never committed) printed the value
after the fixture, after the hide, and at the end.

| Where | CPUs | Value after the fixture | Guard | Attempts | Log |
| --- | --- | --- | --- | --- | --- |
| CI image | `--cpus=4` (quota) | 8 | **red**, CI's exact emissions | 6/6 + 4/4 | `c1-…`, `c4-…--cpus=4.log` |
| CI image | `--cpuset-cpus=8-11` | 8 | **red** | 4/4 | `c4-…--cpuset-cpus=8-11.log` |
| CI image | cpuset 1, 2 and 8 CPUs | 8 | **red** | 2/2 each | `c5-…` |
| CI image | unlimited (16 visible) | 0 | green | 1/1 | `c2-…-allcpus.log` |
| toolbox | inherited (`nproc` 16) | 0 | green | 1/1 each | `l1-…`, `l3-diag-default.log` |
| toolbox, 16 | CI's font set (`fontconfig-ci/fonts.conf`; the test printed it, and `upper` was 48,504 as on CI) | 0 | green | 1/1 | `l2-…`, `l3-diag-ci.log` |
| toolbox, 16 | `LIBGL_ALWAYS_SOFTWARE=1` (and `LP_NUM_THREADS=4`); env set, **renderer not confirmed** | 0 | green | 1/1 each | `l4-…` |

What this shows:

- The failure is deterministic in the CI image with at most 8 CPUs. It does
  not depend on the kind of limit: a CFS quota and a cpuset both fail. So it
  is not quota throttling.
- Every passing run in the toolbox **and** the passing container run had 16
  CPUs. This change ran nothing in the toolbox with fewer CPUs, so "a small
  CPU count alone" is not refuted by its own runs.
- The only evidence that the image matters, and not just the CPU count, is
  `restore-main-ci`'s recorded toolbox runs pinned to one or two CPUs (20/20
  and more, on the guard's reworked shape, design R6). They all passed. That
  is recorded evidence, not a run of this change.
- At 16 CPUs, the CI fonts alone do not reproduce it in the toolbox.
  `LIBGL_ALWAYS_SOFTWARE=1` alone does not either. GTK was on cairo in that
  run whatever the variable said, so it could only have moved mutter to
  llvmpipe, and mutter's backend was not recorded. "No GPU device" is
  therefore **not** ruled out.
- No run crossed fonts with the CPU count: the CI font set at a small CPU
  count outside the image, or Adwaita Mono at a small CPU count inside it.
  Task 2.3 runs that 2×2 first. It also matters for D3: production ships
  Adwaita Mono, so a result that holds only on the Noto Sans fallback
  answers a different product question.
- Working condition: the CI image with at most 8 CPUs. Which property of the
  image, if any, matters is open. Candidates: no GPU device, the minimal
  session services, the font set together with timing, an absent D-Bus
  service. Task 2.3 settles it, including a toolbox run at a small CPU count
  through the lane's own limit, and a confirmed renderer.
- The unlimited pass cell has n = 1. It is a control, not a figure; task 1.3
  repeats it at least five times before D2 diffs a trace against it.

**B4. 4.2, the bookmark test, isolated in the CI image.** `--cpus=4`: 5/5
green (`c3-container-bookmark.log`). R5's lead is untested: CI failed it
inside the full `window` shard, after about 400 tests.

**B5. The proof tally inherited from `restore-main-ci`** (tasks, "Stage 4
record", plus B1). These runs are not this branch's: they record history and
do not count toward D5's proof.

- `surfaces`: 4 runs with no `FLAKY` line.
- `window`: 4 runs with no bookmark `FLAKY` line (one failed on R7, since
  fixed).
- `editor-page`: 0, because every run failed the guard.

The tests these proofs watch, with their shard from `scripts/widget-shards.py
list` (verified at spec review) and the reason each is owed CI runs:

| Shard | Watched test | Owed because |
| --- | --- | --- |
| `editor-page` | `editor_page::test_a_reallocation_offset_at_the_top_is_still_repaired_after_a_reflow` (F1) | red on every CI run; now reproduced in the lane |
| `editor-page` | `editor_page::test_minimap_native_slider_covers_the_visible_text_with_an_overscroll_margin` | `restore-main-ci` 2.2: the original minimap failure ("stays scrolled to the middle"), never reproduced locally |
| `editor-page` | `protected_view_surfaces::test_a_protected_periodic_capture_reads_the_source_not_the_buffer` | `restore-main-ci` 4.3: fixed from the CI log on D4's "not reproducible" branch |
| `window` | `window::test_a_bookmark_opens_in_a_never_shown_tab_in_preview_only_mode` (4.2) | not reproduced, not diagnosed |
| `surfaces` | `workspace_tree_virtualization::test_directory_retention_stays_within_its_charge` | `restore-main-ci` 4.1: diagnosed locally, CI's figure with the fix unmeasured |

## Decisions

### D1. Reproduce in the CI image before spending CI rounds

The container reproduction gives a local red in about 90 s. A CI round takes
20 to 30 minutes, and every one so far was spent on a run that could only
pass or name a branch. The lane becomes a tracked script, because handoff
directories are untracked:

- `scripts/ci-image-widget-test.sh [--cpus N | --cpuset LIST] [--runs N]
  [--shard NAME | -- <test-binary args>]`:
  - reads the image from `.github/workflows/ci.yml`, so it cannot drift;
  - installs that job's `dnf` line;
  - bind-mounts the worktree read-only, and the cargo target directory and
    one output directory;
  - runs the already built widget binary through its own headless
    self-supervision. A `--shard` runs `scripts/widget-shards.py`'s test list
    for the shard.
- It builds nothing: the binary is built first with `scripts/heavy.sh`.
- The host-built binary runs in the image only because the toolkit versions
  match (B2). The script compares the image's `gtk4`, `libadwaita`,
  `gtksourceview5`, `pango` and `glib2` versions with the host's and fails
  clearly when they differ. Without Docker it exits with the widget runner's
  unsupported-host code (77).
- **Fidelity caveat:** CI runs `widget-shards.py run <shard>`, which calls
  `scripts/run-widget-tests.sh --headless --retries 1 -- --exact <names>`,
  and that script always ends in `cargo test -p lushtext --test widget`. The
  container has no Rust toolchain, so the lane cannot use it. The lane runs
  the prebuilt binary as `<widget binary> --exact <names>`, with the names
  from `widget-shards.py list` for `--shard`. The binary's own
  self-supervision then starts `dbus-run-session` and `mutter --headless`
  with the same 2560x1600 monitor and the same pre-GTK environment
  (`GSK_RENDERER=cairo` included). What the lane lacks is the runner's temp
  scope, its warning scan and its outer retry. A "not reproduced" verdict
  from the lane (D4) carries that difference, and the lane says so in its
  summary.
- Every invocation in this change runs as `scripts/heavy.sh --mem none --
  scripts/ci-image-widget-test.sh …`, with the container's `--memory` set
  inside. That way it honours the shared lock (`verification-runs.md`).
- **One lock hold per command.** A `window` shard run takes about 11
  minutes, so five of them under one hold would keep a measurement waiting
  for an hour. `verification-runs.md` forbids that. A `--shard` invocation
  therefore takes `--runs 1`, and the five runs are five `heavy.sh` calls.
  Single-test batches stay under about 15 minutes per call: the 20-run
  proofs are split into several calls.
- **Limits:** the lane's default is `--cpus N`, a CFS quota, which does not
  pin. B3 shows a quota reproduces the failure as a cpuset does. `--cpuset`
  stays for a diagnostic that needs it, and its list is chosen inside the
  CPUs the toolbox inherits. Neither replaces `taskset`, which no run in
  this change uses.
- `--self-test` checks argument parsing, the image extraction from `ci.yml`
  and the `dnf` line, without Docker. It joins `make check-agent-helpers`.
- **Rejected:** a CI-side matrix with more CPUs. It would hide the defect,
  which is the product question in D3, not a CI nuisance.

### D2. Name the writer of 0 → 8 before choosing any fix

The 8 px move happens during `present_settled_minimap_document`, before the
existing recorder is connected, so no log holds it yet.

- Stage 3 adds an env-gated trace (test-utils only, never committed, as
  `restore-main-ci` stage 3 did). It logs every vertical `changed` and
  `value-changed` from page construction on, with a backtrace at each value
  move, and the rest state's `at_lower`, `is_paused` and `reflow_scrolled` at
  each settle.
- It runs in the lane at `--cpus=4` and unlimited, and the two traces are
  diffed.
- The discriminator for "which property of the image" is chosen from the
  trace, not tried blind. For example, if the writer is a GtkTextView
  first-line keep after a revalidation, the variable is when validation ran
  relative to the settle. The trace then also shows whether, and why, the
  toolbox at a small CPU count does not reach it.
- The same trace prints `at_lower` at the guard's settle. That confirms or
  refutes the inferred link in B1.

**Rejected before the diagnosis:**

- tightening the fixture's precondition alone, which turns a disarmed repair
  into a precondition failure and fixes nothing;
- `set_value(lower)` in the fixture, which papers over a writer that may be
  production's;
- a CI matrix change or a longer wait. No wait is involved: the value is
  wrong at rest.

### D3. Product defect or fixture artefact is a measured decision

- Stage 3 runs a real window in the lane at `--cpus=4` and checks whether
  the editor rests at `lower` after opening a document at its top.
  - The window is a `LushtextWindow`, not the bare
    `present_editor_page_with_size` page. The document is opened from a
    file the way a user opens one (`open_document`), not restored from a
    session, which would restore a saved scroll offset instead.
  - Every other variable stays the fixture's: 2,000 lines, word wrap off,
    the minimap shown, a comparable editor size. Otherwise a green window
    check could come from a different document, not from the window.
  - No test code writes the adjustment. The existing window top-anchor
    tests (`run_minimap_top_anchor_sidebar_reflow_case`, `window.rs`) call
    `put_editor_at_top_left`, which does `set_value(lower)`, so their CI
    passes say nothing about where presentation leaves the view.
  - The check is a widget test in the `editor-page` or `window` shard: a
    document opened from the window, settled, then asserting
    `vadjustment.value() == lower` within 0.5. It runs with the image's
    fonts and once more with Adwaita Mono installed in the image
    (`adwaita-mono-fonts`), the font production uses.
- **If the window rests off the top too,** it is a product defect. The user
  opens a document whose first line is clipped and whose top-anchor repair is
  disarmed.
  - It is fixed at the writer, failing first in the lane.
  - The new requirement "An editor presented at the top rests at the top" is
    the guard.
- **If only the bare test page does,** it is a fixture artefact. The
  fixture gets the same settled-at-top contract the window has, and the
  diagnosis is recorded. Possible forms: waiting for the event that ends the
  move the trace names, or presenting the page the way the window does. A
  test-only fix is acceptable only with the writer named and the window
  check green.
- Either way, the D2 tests' "at the top" precondition becomes exact
  (`value - lower <= 0.5`, the rest state's own epsilon), so a future
  fixture off the top fails as a precondition. The guard's behaviour ("Top
  anchor still repaired after reflow") is right and stays, but its final
  assertion becomes exact too. Today it is `visible_rect().y() < 10`; with
  the 6 px top margin, that accepts any value up to 15, so a "repair" that
  left the view at 8 would pass. The failure message keeps the burst state
  and the emissions.

### D4. 4.2 runs the full shard in the lane first

- Run the whole `window` shard in the lane at `--cpus=4`, five times. This
  is R5's lead: state or timing left by about 400 earlier tests. Each test
  runs in its own child process, so the state could be shared session
  services, the data directory or settings, or simply a slower, warmer
  runner.
- **If it reproduces:** the normal path. Read the enriched message (attach
  and detach counts, vadjustment), trace, classify, fix failing first, and
  get 20/20 in the lane.
- **If the F1 writer explains it:** the two tests share the fix. Top line
  69 for line 300 is a view resting where an unvalidated estimate put it,
  which a first-line keep against a stale rest state could produce. That is
  a hypothesis to test, not a finding.
- **If it does not reproduce:** `restore-main-ci` D4's branch. CI-log
  diagnosis, a deterministic test of the diagnosed mechanism red before the
  fix, and eight branch runs of the `window` shard without its `FLAKY` line.

### D5. The eight-run proof is this branch's final code on CI

- **Which tests owe what** (B5's table):
  - each watched test first runs isolated in the lane at `--cpus=4`, five
    times (the ADDED scenario "Inherited CI-only failures"). That includes
    the three inherited ones, which were "not reproduced" only in the
    toolbox;
  - a test reproduced and fixed in the lane is proved by the lane's 20/20.
    Its CI result is read in the runs below when they happen, and otherwise
    at the programme-end CI check. F1 is in this class: its evidence is no
    longer CI-only;
  - a test still not reproduced in the lane owes eight CI runs of its shard
    with no `FLAKY` line for it (`restore-main-ci` D4; the base requirement
    "A retry-rescued test that does not reproduce locally is still
    diagnosed"). The directory-retention test owes eight runs for its CI
    figure in any case, since the lane is not the runner.
- **Counts:** only `ci.yml` runs of this branch's final code for the shard
  (after the last commit that touches that shard's tests or the product code
  they cover). Each fresh attempt of a widget job counts as one run. B5's
  inherited runs are recorded and not counted.
- **How the runs are produced:**
  - `ci.yml` has no `workflow_dispatch` trigger (`on: push` to `main`,
    `pull_request`), and none is added;
  - the runs are the PR's `pull_request` run plus reruns. A run cannot be
    rerun while an attempt of it is in progress, so attempts are
    sequential. While more than one shard still owes runs, a whole-run
    rerun (`gh run rerun <run>`) serves every shard in one attempt: about
    30 minutes each, so about four hours for eight. Rerunning the three
    widget jobs one at a time (`gh run rerun <run> --job <job id>`) would
    take 24 attempts, about ten hours. A single-job rerun is used only when
    one shard alone still owes runs;
  - each attempt is waited for with `timeout 540 gh run watch <run>
    --exit-status` in repeated foreground calls;
  - every run id and attempt is recorded in tasks.md.
- Before relying on reruns, check once that `gh run rerun` is allowed for
  the logged-in account on this repository. If it is not, the runs come from
  pushes of this branch's real commits; empty commits are not used. `ci.yml`
  cancels an in-progress run of the same pull request on a new push
  (`cancel-in-progress`), so each push waits for the previous run to finish.
- **Authority:** the CI-evidence exception of
  `.agents/rules/parallel-agent-work.md` ("Merging") lets a change whose
  evidence exists only in CI dispatch and read its own runs. It covers the
  eight-run proofs above, which are CI runs by the base requirement's own
  definition, and nothing that the lane proves. Waiting happens before
  `MERGE-LOCK` is taken, never while it is held. If the `origin/main` merge
  under the lock changes files this evidence covers, the change releases the
  lock, pushes, and waits for new runs.
- **Conflict with the stage brief:** the programme's stage briefs say never
  to wait for, poll or rerun GitHub CI (maintainer, 2026-11-14), and name no
  exception. The rule file names one. Stage 3 does not resolve this itself.
  It runs task 6 only if its own brief allows the exception. Otherwise it
  records every eight-run proof as owed to the programme-end CI check, under
  the shard and test names in B5, and the change merges on its lane
  evidence like the rest of the programme.
- **Merge condition:** every watched test is either proved in the lane
  (20/20, plus the full shard once at `--cpus=4`) or has its eight CI runs,
  or is recorded as owed to the programme-end check under the brief
  conflict above. When CI runs were read, the `editor-page`, `window` and
  `surfaces` shards passed in them at the final code, first attempt, with
  no `FLAKY` line.

### D6. Scope

- In: F1, 4.2, the proof, the lane, and any further failure the lane or the
  proof runs reveal in these three shards (`preexisting-blockers.md`).
- Out: other `ci.yml` jobs, which the programme's final CI sweep owns.

## Axioms

- None decided yet. If the writer (D2) is a toolkit behaviour the fix relies
  on, it gets the next ledger entry (**A113**) in
  `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`,
  with a `gtk-lush-axioms` probe and sample. Its sample runs on the shipped
  toolkit and on the GNOME 50 floor (`make gtk-axiom-sample AXIOM=A113
  CHECK=1`; `scripts/run-gtk-axioms.sh runtimes --only A113`).
- If the behaviour needs the small-CPU condition, the probe documents it.
  The CI axiom jobs run on the same 4-vCPU runners.
- A112 is unchanged: B1 shows the classifier behaved as A112 says.

## Kani

- **Kani: none (no proven module touched)**, unless the fix changes one.
- If the fix adds or changes a pure decision in
  `ui/editor_page/minimap/policy.rs`, it reruns `core-geometry-policies`
  once, on the final code. `gtk-lush-viewport`'s rest state
  (`LOWER_EDGE_EPSILON`, `rests_at_lower`) has no harness today: it is no
  `scripts/kani-shards.py oracle`, and no `kani_proofs.rs` names it (the
  `RestState` in `ui/window/geometry/kani_proofs.rs` is a local model).
  Changing it needs no shard, but it is a GTK Lush API change, so
  `gtk-lush-stewardship` and the crate's CHANGELOG apply.

## Timing

- Timing: none expected. No timed path's cost changes.
- If the fix moves work into a settle or attach path, it states the added
  per-dispatch cost against `B_settle` (16 ms). It measures through
  `scripts/measure.sh` only if that cost is not O(1).

## Commits and proofs

`.agents/rules/testing.md` keys the three smokes on path prefixes, and
`crates/lushtext/tests/widget/` and `crates/lushtext-core/src/ui/` are both
on the list. Task 3.2 edits `editor_page.rs` whatever the diagnosis, so every
commit of this change that touches those directories owes
`make visual-geometry-smoke`, `make accessibility-smoke` and
`make visual-smoke`, current for the final contents
(`check-visual-proof-policy` and `check-accessibility-policy` in the commit
hook refuse otherwise).

- The lane script, its self-test and its docs (tasks 1.x, 7.2) commit
  first, alone: no UI prefix.
- The trace (2.1) is reverted before any smoke runs. It is never committed,
  and its diff would stale every digest.
- The test and product edits (3.1–3.3, and 4.2's if any) land in as few
  commits as possible, each after one run of the three smokes on its final
  contents.

## Risks

- The writer may be GTK's, with no app-side hook. Then the fix is an
  app-side repair at the settle, with the axiom recorded, and the upstream
  question goes to `upstream-gtk-reports`.
- The lane's container differs from the runner in kernel, cgroup driver and
  virtualisation. A lane green with CI still red is possible. CI stays the
  merge proof (D5); the lane only shortens the loop.
- Docker on this host is shared. Each lane invocation is bounded
  (`--memory`, `timeout`) and goes through `heavy.sh`.

## Spec review record (stage 2, 2026-11-15)

The baseline was checked against the evidence logs (`c1`–`c5`, `l1`–`l4`,
`ci-logs/`) and the code. CI's message and the lane's emissions are
identical, the attempt count (20 in 10 runs) and the directory figure
(1,753 B/row) recompute, and `ci.yml` has no `workflow_dispatch`. Corrections
made, with the reason for each:

- B1: "scroll detected" downgraded to inferred. The recorder mirrors the
  classifier's rule but does not read `reflow_scrolled`.
- B2/B3: GTK renders with cairo in every run, because the harness forces
  it. The software-GL run could only have changed mutter. The font × CPU
  cross was never run, so task 2.3 now starts with it.
- B5/D5: the watched tests are named per shard. Three inherited ones were
  missing: the minimap original, the protected capture and directory
  retention. Each runs in the lane before it is left on CI runs. The
  CI-evidence exception is scoped to proofs that exist only in CI, and F1
  is no longer one. The conflict with the stage brief's "never rerun CI"
  is handed to stage 3 with a fallback. Whole-run reruns replace 24
  sequential job reruns.
- D1: the lane's invocation is definite, because `run-widget-tests.sh`
  always runs `cargo`. A shard run takes one lock hold, and `--cpus` is the
  default over a cpuset.
- D3/3.1: the window check opens a file the way a user does, with the
  fixture's variables and no adjustment write. The existing window
  top-anchor tests write `set_value(lower)`. It also runs with Adwaita
  Mono, the font production uses.
- 3.2 and the spec: the guard's final assertion becomes exact too. Its
  `y < 10` accepted a repair that left the view at 8.
- 3.5/5.3 and "Commits and proofs": `editor_page.rs` is under a UI prefix,
  so all three smokes are owed whatever the fix touches. The trace is
  reverted before they run.
- 3.4: "starved" now means a lane run at `--cpus=1`, because the
  programme's briefs forbid `taskset`. The unlimited control now needs
  five runs.
