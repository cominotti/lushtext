## Context

`spawn_blocking_then` (gtk-lush-tasks) runs work on a worker thread, holds one of
`DEFAULT_MAX_CONCURRENT_SPAWNS` (8) slots until the main loop has consumed the
result, and delivers the result from a `glib::idle_add_once` idle, at
`G_PRIORITY_DEFAULT_IDLE` (200). GtkTextView validates the lines below the
viewport from an incremental idle at `GTK_TEXT_VIEW_PRIORITY_VALIDATE`
(`GDK_PRIORITY_REDRAW + 5`, 125), 2000 px per dispatch (A31), and re-arms itself
until the buffer is laid out. GLib dispatches only the highest ready priority per
iteration. So a priority-200 source waits for every validating view's whole
backlog, and a view on a hidden page validates too (A80, this change).

C6 (`bound-post-open-settle`) relies on that ordering on purpose:
`PACED_SLICE_PRIORITY` is `G_PRIORITY_DEFAULT_IDLE`, so each install slice waits
until the view has validated the previous one (A70). That stays.

## Attribution (main cdfff201, 2026-11-02)

`eviction/reselect`, plain, one repetition under
`perf record -e cycles:u --call-graph dwarf --user-callchains -F 999`, launched as
a child under `scripts/measure.sh` (load 0.9–2.9). Artifacts are in
`.claude/handoffs/lush-priority/perf-main-evict/`.

- **69.5 %** of the application's cycles are `incremental_validate_callback` →
  `gtk_text_layout_validate` → `pango_layout_check_lines`. That is HarfBuzz
  shaping (28 %), line breaking (14 %) and itemizing (15 %) of every line of the
  loaded documents.
- All ten activations open within 90 ms. Loads install **three at a time**. Each
  load charges `8 × 8 MiB + 1 MiB` = 65 MiB of the 256 MiB transient budget
  (`model/file_load.rs`), and its permit survives the installation.
- The installs are 5,432 paced slices of 0.12 ms (median) at
  `G_PRIORITY_DEFAULT_IDLE`. They end only when every validating view has laid
  out its buffer. The first three documents complete at 6.1 s, the next three
  at 17.0 s, and the case hits its 20 s cap.
- Each next load's installation begins **26 ms** after the previous permit is
  released, worker read and decode included. So since C6 merged, the
  completions are not what holds `eviction/reselect` back. Its cause is the
  permit held through a validation-paced install, plus hidden tabs that
  validate fully. That cause is queued separately (see "Out of scope").

The starvation class is still real wherever a completion arrives during a
validation backlog. A80 pins it. A probe of 16,384 lines of 40 characters
showed that a priority-200 source waited 327 iterations (164 ms) on screen and
150 iterations on a hidden stack page, each time until the last line was laid
out. A priority-124 source ran in iteration 3 (on screen, after the on-screen
validation at a higher priority) or iteration 2 (hidden).

## Goals / Non-Goals

**Goals:** a completion reaches GTK promptly while validation is pending.
Completion dispatches stay bounded (`B_interact` 8 ms for input, `B_settle`
16 ms per dispatch). Completions never delay input or frames, and C6's pacing
keeps working. The GTK Lush API change is minimal and governed.

**Non-goals:** making hidden tabs skip validation, or changing the load permit's
charge model. Both are queued (see "Out of scope").

## Decisions

### D1. One completion source, one step above validation

Completions go into a process-wide FIFO (`Mutex<VecDeque>`) and are delivered by
**one** GLib idle source at `GDK_PRIORITY_REDRAW + 4` (124). The first push arms
the source. The source runs one completion per dispatch and detaches when the
queue is empty, all under the queue's lock, so exactly one source is armed at a
time.

- **Above 125**, so the completion runs before the next validation batch (A80).
- **Below 120** (`GDK_PRIORITY_REDRAW`, the frame clock's paint), 110 (GTK's
  resize), the view's on-screen validation (108, `GTK_PRIORITY_RESIZE - 2`) and
  0 (input), so a completion never delays a frame or an event.
- **One per dispatch.** GLib dispatches every ready source of the top priority in
  one iteration (A69). Per-completion idles at 124 would run eight ready
  completions back to back, with no frame or event between them. One source
  returning `Continue` puts the main loop's priority check between every two
  completions.
- **Bounded by aging, not by the cap** (corrected after review). A slot is held
  until its completion has run, so at most eight completed results exist at
  once, but that does not bound how long the source stays ready: a FIFO deeper
  than the cap, or a completion that spawns its successor, refills the queue as
  fast as it drains. The first version claimed "at most eight dispatches in a
  row"; the reviewer's experiment (`starve-experiment.rs`, a 64-task chain)
  kept the source armed for the whole chain and starved every priority from 125
  to 200 — validation, C6's paced slices, composition stages, restore turns, the
  `ViewportSliceBin` idles. D6 adds the bound. Each dispatch is one callback
  that already ran in one dispatch before this change.
- **Co-dispatch at 124 (A69).** D4's admission drains and permit releases are
  sources at the same priority, so GLib dispatches them in the same iteration as
  the completion source: one iteration can run a completion, a drain and a
  release back to back. The lane measures that combined dispatch (D5): the
  watchdog's worst lateness over any iteration overlapping a `task_completion`
  or `task_handoff` span.

### D2. No per-spawn priority, and no time budget

- **Per-spawn priority** (loads high, others default) was rejected. A held
  completion keeps its slot, and the slot cap is shared, so one starved
  low-priority completion blocks the start of every later spawn, loads and saves
  included. Every completion has to be prompt for any of them to be.
- **A time-budgeted source that yields to its own priority** (deliver until N ms
  elapsed, then yield) was rejected: yielding back to 124 buys nothing over one
  per dispatch. The review showed one per dispatch is not enough on its own,
  because nothing below 124 runs while the source stays ready. D6's aging yields
  to a **lower** priority instead, which is a different mechanism with the two
  thresholds the review fixed (eight completions, `B_settle`).

### D3. Public API: `completion_priority()`

A function, because `glib::Priority` has no `const` constructor from an `i32`.
It documents the contract, and it lets an application schedule its own
hand-offs between bounded stages at the same priority (D4). Nothing else in the
public API changes. `spawn_blocking_then` and `spawn_blocking_then_weak` keep
their signatures.

### D4. LushText's admission hand-offs move to the same priority

`load/admission.rs` and `save/admission.rs` schedule their drains
(`idle_add_local_once`) and permit releases (`idle_add_once`) at
`G_PRIORITY_DEFAULT_IDLE`. A Ctrl+S during a validation backlog would wait twice:
for the drain, and then for the write's completion. Both move to
`completion_priority()`. A drain admits work and starts workers. A release
updates a ledger and re-arms drains. Both are small.

C6's install slices, the buffer-replacement slices, the chunked snapshot
capture (`ui/buffer_snapshot.rs`, which C3 edits) and the composition and
restore turns stay where they are. Install pacing *must* stay below validation
(A70). The others are their own workflows' pacing decisions, not hand-offs.

### D5. Measurement: `save/during-validation`

The case opens a small scratch document, which the case writes into its own
state directory and never into the fixture ledger. It then opens two 8 MiB
short-line fixtures through a second activation of the running instance,
selects the scratch tab, types one character and activates `win.save`, the Ctrl+S action (the lane never injects a Control+letter chord). The case
reports:

- `save_latency_us`, from the save activation until the scratch file's bytes
  change on disk;
- the phase's watchdog stalls, which hold every dispatch, the completion
  dispatches included, against `B_settle`;
- the typed key's input-to-frame.

Every case also records each completion as a `task_completion` span (the
`completion-observer` feature of `gtk-lush-tasks`, enabled only by the
`responsiveness-probes` build) and each drain and release as a `task_handoff`
span. The report's "Task completions against B_settle" table gives the worst
completion, the worst hand-off, and the worst combined dispatch (the watchdog's
lateness over an iteration overlapping either, A69), with the verdict "every
hand-off dispatch within B_SETTLE_US" or the runs over it.

This is the measured save case the coordinator required (condition a). The
lane's "never saves" rule is narrowed to "never writes a fixture", and the case
verifies the fixture digests as every case does.

### D6. Aging (added after review)

After **eight consecutive completions, or `B_settle` (16 ms) of continuous
occupancy, whichever comes first**, the delivery source detaches. The lane stays
`armed`, so workers do not attach a second source. It arms two sources and the
first to run ends the yield, removes the other, and re-attaches the delivery
source at 124:

- a hop at `G_PRIORITY_DEFAULT_IDLE` (200), which runs in the first iteration
  that reaches that priority, so 125–200 each get the loop once;
- a 16 ms timeout at 124, for when a source between the two is ready after every
  dispatch. Validation over a large backlog is exactly that (A80): a yield
  waiting only for 200 would wait for the whole backlog, which is the wait this
  change exists to remove. Under a backlog, validation therefore gets up to
  16 ms per run of completions, then completions resume.

GLib dispatches only the top ready priority, so a default idle waits behind a
validation backlog whatever the completion source does. The reviewer's combined
experiment — an always-ready 125 **and** a 200 idle during one chain, with the
125 ready until the chain ends — therefore cannot show progress at 200 under any
scheduler: on `main` the chain itself would never finish. The unit tests prove
the two halves separately: a 64-task chain beside an always-ready 125 lets
validation run and still completes, and the same chain beside an always-ready
200 lets that idle run; in both, at most eight completions run with no
lower-priority dispatch in between
(`a_completion_chain_yields_to_an_always_ready_validation_idle`,
`a_completion_chain_yields_to_a_default_idle`, failing first on the unaged
source: 0 batches and 0 dispatches).

## Risks / Trade-offs

- **Ordering among default-idle sources changes.** A completion no longer waits
  behind `G_PRIORITY_DEFAULT_IDLE` idles queued before it. That ordering was
  never promised: a worker's finishing time was already unordered against them.
  The full widget suite is the regression net. Widget wait helpers drain every
  ready source, so they observe completions as before.
- **Completions now preempt C6's paced slices and GtkSourceView's background
  analysis** (`G_PRIORITY_LOW`, A69). That happens only while results are
  waiting, and for at most eight completions or 16 ms of them before the source
  yields (D6). Background analysis at `G_PRIORITY_LOW` (300) still waits for the
  whole chain: a yield reaches 200, not 300.
- **A slow completion callback now runs ahead of validation.** Its cost is the
  same as before, but it can no longer hide behind a backlog. The lane's
  `B_settle` check covers this (condition a).

- **A completion cannot nest another.** GLib does not re-dispatch a source that
  is dispatching. A completion that iterated the main loop and waited for
  another completion would therefore wait for ever, where separate idles would
  have nested. No completion does this. The adversarial review checked the
  callers: synchronous close-time saves do not go through `spawn_blocking_then`,
  and widget tests wait from the test body, not from inside a completion. The
  crate's rustdoc states the rule.
- **A chain of quick spawns.** A completion that spawns work which finishes at
  once used to keep the source busy for as long as the chain lasted (the
  review's finding; the first version of this section said validation "resumes
  when the chain ends"). With D6 a chain runs in runs of at most eight
  completions or 16 ms, each followed by one pass of the main loop down to 200,
  or by up to 16 ms of validation when a backlog stops it there.
- **Aging adds latency under a validation backlog.** A ninth queued completion
  can wait up to 16 ms behind validation. That is one `B_settle`, against the
  seconds the backlog cost before this change.

## Found while verifying

The first full `make test` turned two latent ordering dependencies into
failures. Both are fixed in this change, under the pre-existing-blockers rule.

- **The opened-document focus handoff could be lost** (`window_composition`
  focus tests, about half the runs). `after_content_revealed` asked
  `gtk_widget_is_visible` whether the content stack was revealed. That
  function is false until the window itself is shown, so a handoff requested
  after the content stage but before stage P was queued after the content stage
  had already drained its queue, and never ran. The startup gate's completion
  now arrives before stage P and releases the held activation open there, so
  the keyboard stayed on the status bar's toggle. The fix asks the stack's own
  `visible` property, which only the content stage sets. The three focus tests
  then passed five times out of five.
- **`window::test_selected_restore_tab_gets_bounded_priority_after_capacity_releases`**
  observed "second is still loading when third completes" from `wait_until`'s
  next poll. That poll used to come only because the release of third's permit
  waited for the validation backlog, which left an idle gap. The test now reads
  second's state inside third's `file-loaded` callback, which is what it
  claims. Admission order is unchanged: the selected tab is still admitted
  first.

- **A74's growth check failed once under load.** In a full `make gtk-axioms`
  run at load 4–5, every small-window frame cost 12.9 ms against its usual
  3 ms. The probe measured the two windows one after the other, so a change in
  host load, or a move between performance and efficiency cores, hit only one
  of them. It now measures them in three alternating rounds and takes medians
  over all rounds. The check then held three times out of three in isolation,
  and the lane passed.

- **A composition stage shared its dispatch with a session-restore turn**
  (found by the sanity interleave after merging C3, against `main` ebdd6aa1).
  GLib dispatches every ready source of the top priority in one iteration
  (A69). The session load's completion now arrives before stage P, so the
  first restore turn (a `G_PRIORITY_DEFAULT_IDLE` idle) was ready in P's
  iteration. Present plus two page activations took 16.5–26 ms in 4 of 9
  `session/restore-3-small` runs, against at most 11.8 ms on `main`. The
  composition stages (W, C, R+P, the reveals) now run one step above
  `G_PRIORITY_DEFAULT_IDLE`, so each has its iteration to itself and
  default-idle work runs in the next. They stay below the frame clock and
  input.
- **The language catalog's build shared a turn's dispatch for the same
  reason.** The build is a one-off of about 4.5 ms that C6 meant to run in "a
  dispatch of its own". At `G_PRIORITY_DEFAULT_IDLE` it shared stage W's
  dispatch on `main` (11.3 ms together). With stage W one step up, it shared
  the first restore turn's or activation turn's dispatch instead (14–19 ms). It
  now runs at `G_PRIORITY_DEFAULT_IDLE - 2`, a priority nothing else uses.

- **The workspace path reveal could report `revealed` one frame early**
  (independent review, reproduced; pre-existing on `main`). The reveal's settle
  check ends once `ViewportSliceBin::outer_request_pending()` is false. The bin's
  forwarding idle moved the outer adjustment and cleared its pending batch
  **before** the bin re-sliced from the new value in the next layout, so a poll
  between the two read the old band: the review's failing visual-geometry run
  saw the section header at y 150 after `workspace-path-revealed`, then at
  −14,435 with the same outer value. The bin now keeps the request pending
  (`awaiting_reslice`) from the idle's move until its next `size_allocate`
  consumes it; a move the outer scroller clamps to no change leaves nothing
  pending. A further "confirm in a later frame" guard in the reveal was tried
  and dropped: for an already-visible row nothing changes, the clock produces
  no frame, and the guard ran out the wall budget (`unsettled`) in 3 of the 4
  `workspace-tree-reveal-at-rest` cases of the first loop run. Proved
  failing first by the adoption-lab test
  `test_adoption_slice_bin_reports_a_forwarded_request_until_it_reslices`
  (polled right after the idle, before any frame: not pending, 2 of 2
  attempts), and in the Kani slice loop by
  `slice_loop_settle_evidence_waits_for_the_reslice` with the `should_panic`
  pin `slice_loop_settle_evidence_without_the_reslice_is_stale` (the pre-fix
  accessor).
- **The cap did not bound the completion source** (independent review): see D1
  and D6.

## Out of scope (queued)

`eviction/reselect`'s cause, recorded in `docs/next/long-line-documents.md` as
the next change after C3, because it touches C3's install files:

1. the transient load permit (65 MiB for 8 MiB) is held through a
   validation-paced installation;
2. hidden tabs lay out their whole document. Restoring many large files spends
   the CPU laying out tabs nobody is looking at.

`crates/lushtext/tests/widget/common.rs` described the completion as a
"low-priority `idle_add_once`". C3 edited that file, so its comment was
corrected after C3 merged, in this branch. The mechanism the comment teaches,
sleep and then drain every ready source, is unchanged.
