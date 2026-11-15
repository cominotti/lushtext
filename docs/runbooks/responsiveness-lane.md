# Responsiveness lane

How `make responsiveness-smoke` drives a run, which classes of result fail it,
and how to turn runs into a verdict. The rules (smoke-only feature never
shipped, synthetic fixtures only, verdicts only under the reference
precondition) are in [`.agents/rules/build.md`](../../.agents/rules/build.md),
"End-User Smoke Lanes"; the driver's subcommands are documented in
`scripts/responsiveness-smoke-driver.py`; what the lane covers is in
`docs/end-user-coverage.md`; verdicts are recorded in
`docs/next/long-line-documents.md`.

## Session and input

The lane runs under a private `dbus-run-session` and
`mutter --headless --virtual-monitor 1280x800@120` and drives input through
Mutter's RemoteDesktop API. Its CPUs come from the driver's `lane-affinity`
subcommand. Under `scripts/measure.sh` (which exports `LUSHTEXT_MEASURE_CPUS`
and `LUSHTEXT_MEASURE_CPUS_SOURCE`) the lane runs on the reference CPU set
`measure.sh` pinned (`scripts/measure.sh --print-reference-cpus`); it fails
before any case when its inherited affinity differs from that set or the set
exceeds 16 CPUs, and never trims it. Outside `measure.sh` it takes at most 16
CPUs from the inherited mask (never widened). `environment.txt` records
`affinity=`, `measure_cpus=` and `measure_cpus_source=`; `run.json` adds
`reference_run` (true only for a default reference run on exactly that set);
and `report.md`'s environment line says which of three cases the run is: the
reference CPU set, an override of it (not comparable with reference runs), or
not pinned to it (trend evidence, not a verdict). It clicks inside the text area first: keys are not
delivered before. Every editor click aims at an uncovered point of the
snapshot's `visual_geometry` `source-view` rectangle, corrects the pointer by
the probe's `motion` records (a requested screen point does not always arrive
at the same window point), and fails its phase unless the probe's record of
the press names an editor. A relaunch whose editor click cannot be proven ends
the case. Every injection is paired with its probe `input` record by mutter's
event stamp, never by arrival order (`match_injections`: the stamp must fall
between 1 ms before the injection and 1 ms before the next one, at most 20 ms
after the injection's D-Bus call returned). The mid-install phase starts only
once the window's composition has run.

The lane seeds `split-view-layout-migrated` so the case's declared layout is
the one rendered (the fresh profiles that show no pane seed the legacy
`sidebar-visible` key hidden instead, so the one-shot migration stays
exercised), disables the caret blink (under headless Mutter the blink's frames
get no presentation feedback and GTK paces them at its 60 Hz fallback, which
would add up to 16.7 ms to every input), and sets `GTK_IM_MODULE` explicitly
for every variant (never GTK's Wayland text-input default, S7B-6). It never
writes a fixture: an app holding typed edits is `SIGKILL`ed, and the one case
that saves, `save/during-validation`, writes only a scratch file it created in
its own state directory, through `win.save`, never a Control+letter chord. It
fails on any fixture digest change and records `btrfs-stall-report` evidence.

## Classes that fail the lane

Budget overruns are results, not failures. These fail the run:

- `misdirected_typing`: a phase whose text keys left no input record, arrived
  outside an editor or inserted nothing. Keys into a protected, read-only
  editor are reported apart from editable typing, and so are keys into a
  **held** editor (`held-editor`: a workflow owns its buffer or its load is in
  flight), which are drawn only when the held text is inserted; they count as
  keys into an editor, whose insertion marks are one per character, so a held
  key never inserted fails the lane.
- `withheld_typing_frames`: a key typed into an editable editor is never "no
  visible change"; drawn only after the next key was handled while the loop
  was free, it is measured to that frame.
- `undelivered_scrolls`: a phase with a wheel step that left no input record.
- `frameless_cases` (in `summary.json`): a measured case that wrote no `frame`
  record, because every launch paints and a missing frame stream would
  otherwise read as `None` first-frame and empty frame-gap figures. The probe
  writes a frame once it or any later frame is final (`FrameTimingsQueue` in
  `services/responsiveness_log.rs`), so one frame that never gets presentation
  feedback cannot hold back the stream.
- `failed_cases`: a repetition whose outcome is `failed` (a phase could not
  run). A case that opens no document records its editor interactions
  `unavailable` instead of clicking at an editor.
- A warning of a pre-existing blocker the programme fixed (`FIXED_DEFECTS` in
  the driver: the scrollbar `GtkGizmo` snapshot warning, ledger A37); an owned
  one is reported as a finding.

## Tiers and verdicts

The nightly `quick` tier is trend evidence only; verdicts come from a `full`
run on the reference machine. A long `full` sweep may run group by group, each
part into its own artifact directory, joined for one report:

```
scripts/responsiveness-smoke-driver.py merge-runs --artifact-dir <out> --from <part>...
scripts/responsiveness-smoke-driver.py report --artifact-dir <out> [--axiom-observations <make gtk-axioms log>]
```

Repetitions of a case measured by several parts accumulate. Parts whose
`affinity`, `measure_cpus` or `measure_cpus_source` differ merge as
`affinity: "mixed"` with `reference_run: false` and each part's set in
`merged_from`, and the report says the parts are not comparable; a part
written before those fields existed counts as unpinned. A side-labelled
verdict (`merge-runs --from baseline=DIR --from branch=DIR`) is computed by
`report`, with toolkit-floor exceptions passed as
`--toolkit-floor ATTR=AXIOM[:REPORT]`; input-to-frame is printed beside the
documented headless floor and never gated.

The reference values come from the reduced sweep
`scripts/responsiveness-reference-sweep.py` (bisection over length, bracketing
repetitions and variants only at each boundary; design D7 of
`probe-long-line-toolkit-costs`), which drives the lane through
`RESPONSIVENESS_LENGTHS` (probe points per group) and `RESPONSIVENESS_PHASES`
(only the interactions its thresholds read).

**The reference precondition.** A timing verdict comes only from a run where
the 1-minute load average is below 4 for 30 s, no process is at 80 % CPU or
more, and no other agent's `cargo`, `rustc`, `nextest`, `kani`/`cbmc`,
`cargo-mutants` or `mutter --headless` is running. Record the load, the
process check and `btrfs-stall-report` for the window, run it through
`scripts/measure.sh`, and discard a run that loses the precondition part-way.
Runs under load are labelled non-verdict.

## Construction and composition profiles

The `baseline/small` case runs on four settings profiles: fresh or
steady-state (a previous, gracefully quit launch), under the keyfile or the
dconf backend. Those cases keep their XDG state on the artifact filesystem
(tmpfs hides a keyfile write's `fsync`), and the dconf ones point the private
bus's `dconf-service` at the case's config directory before seeding it, so the
user's dconf database is never reached. The probe's `span` records feed the
report's "Construction spans against B_settle" table (startup, construction,
first frame, the largest dispatch before it, each construction stage, the
settings flush); `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` makes an over-budget
LushText-owned span on the release build fail the run (never in CI).

A fifth construction profile, `baseline/small/steady-populated`, is a steady
keyfile profile with one restored workspace folder of 300 synthetic entries
and both side panes shown. The composition table reports **the first painted
frame** derived the same way for every build (the end of the first
`frame_paint` that starts after `composition_present`, so a build that ran an
unseen frame on the unmapped window, ledger A51, is not credited with it), the
**first presentation** from the `frame` records, composition complete,
**activation shown** (the first frame after the activation's file opens
drained), and the **content fill-in**,
`max(complete, activation shown) − first painted frame`. Every dispatch up to
that later point is budgeted (`largest_composition_dispatch`); each run's
fill-in is judged against the maintainer's 40 ms (`ACCEPTED_FILL_IN_US`; a run
over it fails only under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1`) and the
median is reported against three reference frames. A frame's repeated layout
runs are summed into that frame's `frame_layout`. The "Composition dispatches
and first frames" table lists each stage's dispatch (`composition_<stage>`
spans), every dispatch of 4 ms or more in order, and the layout and paint
phases of the first four frames (`frame_layout`/`frame_paint`). The probe also
records the window's first measures and allocations
(`window_measure_h`/`_v`, `window_size_allocate`, measurement build only).

## Post-open cases

The post-open cases run the `background` phase until the driver-computed
settle, capped, then `type-during-settle` and, on dense cases, `wrap-toggle`,
`zoom-step` and `font-change`, and report a "Post-open settle" table
attributing every dispatch by its `post_open_*` span or mark. The driver's
docstring documents how the settle is computed.
