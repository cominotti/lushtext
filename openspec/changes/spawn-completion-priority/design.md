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
- **Bounded.** A slot is held until its completion has run, so at most eight
  completed results exist at once. The source can hold lower priorities back
  for at most as many dispatches as there are results waiting. Each dispatch is
  one callback that already ran in one dispatch before this change.

### D2. No per-spawn priority, and no time budget

- **Per-spawn priority** (loads high, others default) was rejected. A held
  completion keeps its slot, and the slot cap is shared, so one starved
  low-priority completion blocks the start of every later spawn, loads and saves
  included. Every completion has to be prompt for any of them to be.
- **A time-budgeted source** (deliver until N ms elapsed, then yield) was
  rejected. Yielding returns to the same priority, so it buys nothing over one
  per dispatch, and it would add a threshold to tune. One per dispatch has no
  threshold and the smallest worst case.

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

This is the measured save case the coordinator required (condition a). The
lane's "never saves" rule is narrowed to "never writes a fixture", and the case
verifies the fixture digests as every case does.

## Risks / Trade-offs

- **Ordering among default-idle sources changes.** A completion no longer waits
  behind `G_PRIORITY_DEFAULT_IDLE` idles queued before it. That ordering was
  never promised: a worker's finishing time was already unordered against them.
  The full widget suite is the regression net. Widget wait helpers drain every
  ready source, so they observe completions as before.
- **Completions now preempt C6's paced slices and GtkSourceView's background
  analysis** (`G_PRIORITY_LOW`, A69). That happens only while results are
  waiting, at most eight dispatches in a row, each bounded.
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
  once keeps the source busy for as long as the chain lasts. Frames and input
  still run between every two completions, and validation resumes when the
  chain ends. Before this change, the same chain at priority 200 held back only
  other default-idle work.

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

## Out of scope (queued)

`eviction/reselect`'s cause, recorded in `docs/next/long-line-documents.md` as
the next change after C3, because it touches C3's install files:

1. the transient load permit (65 MiB for 8 MiB) is held through a
   validation-paced installation;
2. hidden tabs lay out their whole document. Restoring many large files spends
   the CPU laying out tabs nobody is looking at.

`crates/lushtext/tests/widget/common.rs` still describes the completion as a
"low-priority `idle_add_once`". C3 edits that file, so its comment is corrected
after C3 merges. The mechanism the comment teaches, sleep and then drain every
ready source, is unchanged.
