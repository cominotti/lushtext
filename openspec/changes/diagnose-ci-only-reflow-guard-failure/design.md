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
- No `value-changed` was bare (none moved to a value no `changed` carried),
  so the A112 classifier saw no scroll. That rules out "scroll detected".
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

The editor asks for `Adwaita Mono 14.667px`. On CI that resolves to Noto
Sans, so the document is taller: `upper` is 48,504 against 44,504 locally.

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
- At 16 CPUs, the CI fonts alone do not reproduce it in the toolbox. Software
  GL alone does not either, but the renderer GTK actually used was not
  confirmed, so "no GPU device" is **not** ruled out.
- Working condition: the CI image with at most 8 CPUs. Which property of the
  image, if any, matters is open. Candidates: no GPU device, the minimal
  session services, the font set together with timing, an absent D-Bus
  service. Task 2.3 settles it, including a toolbox run at a small CPU count
  through the lane's own limit, and a confirmed renderer.
- The unlimited pass cell has n = 1. It is a control, not a figure.

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
- **Fidelity caveat:** the lane runs the binary's own headless
  self-supervision, while CI goes through `scripts/run-widget-tests.sh`
  (`widget-shards.py run`). A "not reproduced" verdict from the lane (D4)
  carries that difference. The `--shard` mode therefore runs the shard's
  test list through `scripts/run-widget-tests.sh` inside the container when
  it can do so without `cargo`, or records why it cannot.
- Every invocation in this change runs as `scripts/heavy.sh --mem none --
  scripts/ci-image-widget-test.sh …`, with the container's `--memory` set
  inside. That way it honours the shared lock (`verification-runs.md`).
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
  - The window is a `LushtextWindow` with session restore, not the bare
    `present_editor_page_with_size` page.
  - The check is a widget test in the `editor-page` or `window` shard: a
    document opened from the window, settled, then asserting
    `vadjustment.value() == lower` within 0.5.
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
  fixture off the top fails as a precondition. The guard itself stays as it
  is: its behaviour ("Top anchor still repaired after reflow") is right.

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

- **Counts:** only `ci.yml` runs of this branch's final code for the shard
  (after the last commit that touches that shard's tests or the product code
  they cover). Each fresh attempt of a widget job counts as one run. B5's
  inherited runs are recorded and not counted.
- **How the runs are produced:**
  - `ci.yml` has no `workflow_dispatch` trigger (`on: push` to `main`,
    `pull_request`), and none is added;
  - the runs are the PR's `pull_request` run plus reruns of its three widget
    jobs: `gh run rerun <run> --job <job id>`, one at a time, each waited for
    with `timeout 540 gh run watch <run> --exit-status` in repeated
    foreground calls;
  - every run id and attempt is recorded in tasks.md.
- Before relying on reruns, check once that `gh run rerun --job` is allowed
  for the logged-in account on this repository. If it is not, the runs come
  from pushes of this branch's real commits; empty commits are not used.
- **Authority:** the CI-evidence exception of
  `.agents/rules/parallel-agent-work.md` ("Merging") covers this branch's own
  runs only. It overrides, for this change, the general "never wait for or
  poll CI" line. Waiting happens before `MERGE-LOCK` is taken, never while it
  is held. If the `origin/main` merge under the lock changes files this
  evidence covers, the change releases the lock, pushes, and waits for new
  runs.
- **Merge condition:** the `editor-page`, `window` and `surfaces` shards pass
  on CI at the final code, first attempt, with no `FLAKY` line. Eight runs
  are required for a test fixed on the "not reproducible" branch. A test
  reproduced and fixed in the lane needs the lane's 20/20 plus green CI runs,
  without the eight-run count.

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
  once, on the final code. If it changes `gtk-lush-viewport`'s rest-state
  epsilon or decision, it finds the harnesses with `scripts/kani-shards.py
  oracle` and `grep -rln`, and runs those shards once.

## Timing

- Timing: none expected. No timed path's cost changes.
- If the fix moves work into a settle or attach path, it states the added
  per-dispatch cost against `B_settle` (16 ms). It measures through
  `scripts/measure.sh` only if that cost is not O(1).

## Risks

- The writer may be GTK's, with no app-side hook. Then the fix is an
  app-side repair at the settle, with the axiom recorded, and the upstream
  question goes to `upstream-gtk-reports`.
- The lane's container differs from the runner in kernel, cgroup driver and
  virtualisation. A lane green with CI still red is possible. CI stays the
  merge proof (D5); the lane only shortens the loop.
- Docker on this host is shared. Each lane invocation is bounded
  (`--memory`, `timeout`) and goes through `heavy.sh`.
