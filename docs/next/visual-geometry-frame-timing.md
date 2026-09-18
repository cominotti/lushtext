# Visual geometry: frame timestamps are measured after encoding

**Status: open defect in the proof harness, root cause isolated, fix not
attempted.** Found 2026-09-18 while trying to certify the sidebar wheel-scroll
fix; the lane blocked that change and the blockage is not caused by it.

## Symptom

`make visual-geometry-smoke` fails four to five of its live-process cases with:

```
animation-frame proof failed: animation stream did not capture a PNG frame
mapped to intermediate sidebar geometry
```

The failing set varies slightly between runs, which reads as flakiness, but the
cases are drawn from a fixed family: `minimap-sidebar-live-threshold` and
`minimap-sidebar-workspace-animation`, at the **largest** capture resolutions
(`live-1822x1272`, `wide-desktop`).

## What the evidence says

From `animation/animation-report.json` of a failing case:

| field | value |
| --- | --- |
| PNG frames captured | 66 |
| geometry samples | 75 |
| intermediate geometry samples | 12 |
| **frames mapped to an intermediate sample** | **0** |
| `max_sample_skew_ms` (budget) | 80 |
| observed `sample_skew_ms` at the failure | 248 |
| `failure_reason` | `stale-frame-geometry-pairing` |

So both halves of the evidence exist — the animation *was* observed mid-flight,
and frames *were* captured throughout — and the proof fails purely on pairing
them in time.

## Root cause as first hypothesised (superseded — see the resolved section)

`animation_frame_elapsed_ms` in `crates/cargo-gtk-proof/src/live.rs` derives a
frame's instant from the PNG file's **modification time**:

```rust
fs::metadata(frame_path)
    .and_then(|metadata| metadata.modified())
```

That is when GStreamer finished *writing* the file, not when the frame was
*captured*. The difference is the encode-plus-write latency for one PNG, and it
is systematic rather than random.

Two independent confirmations:

- The failing cases are the largest resolutions. A bigger frame takes longer to
  encode, so the bias is larger — exactly the ordering observed.
- The geometry samples on the other side of the comparison are taken in-process
  at sample time, so they carry no equivalent lag. The comparison is biased in
  one direction by construction.

A second, smaller conflation sits next to it: the fallback path uses
`config.sample_interval`, the *geometry sampling* cadence, as if it were the
video frame cadence. Those are not the same quantity.

## Why this is not a tolerance to widen

Raising `max_sample_skew_ms` past 248 would turn the lane green without making
the pairing correct, and the budget would have to grow again with resolution.
The number is not wrong; the measurement it is applied to is.

## What a fix has to get right

The correction must recover the frame's *capture* instant, and it must not
create the opposite error. Normalising by the smallest observed lag is the
obvious idea and is unsafe on its own: if the capture genuinely starts after the
action, subtracting that offset makes frames appear earlier than they were and
can certify an intermediate frame that never existed. A false pass in the
harness that certifies visual correctness is worse than the current false
failure.

Candidates worth evaluating, in rough order of soundness:

1. Carry the frame's own stream timestamp (PTS) out of the GStreamer pipeline
   instead of reading the filesystem, so capture time is never inferred.
2. Pair by ordinal position within the known animation window rather than by
   absolute timestamps, since both series span the same window.
3. Measure the per-case write lag explicitly with a calibration frame, rather
   than inferring it from the data being judged.

Whatever is chosen needs its own tests plus repeated lane runs to show the
failing set is actually gone rather than resampled.

## Scope note

This lane is scheduled/manual in CI (`end-user-smoke.yml`), so the defect does
not red the pull-request or release path. It blocks local commits through
`check-visual-proof-policy` whenever a change touches visual-sensitive files.

## Dead end recorded: nearest-sample offset cannot recover the lag

A first fix attempt estimated the lag as the median gap between each frame and
its *nearest* geometry sample, then subtracted it. Its own unit tests killed it,
and the reason is worth keeping: once a frame is delayed by more than half the
sample interval, its nearest sample is no longer the one it was captured beside
but a later one, so the measured gap collapses to a small residual instead of
the true lag. With 16 ms sampling, nearest-neighbour can only ever recover a lag
below ~8 ms — useless against the ~250 ms it needs to. Any recovery must align
the two series over the animation *window* (or carry a real capture timestamp
out of the pipeline); a nearest-neighbour formulation is structurally incapable.
The attempt was reverted rather than shipped, because a wrong estimator inside
the judge of visual correctness is worse than the false failure it replaces.

## The trigger is also load, not only the artifact

The mtime timing is a latent fragility; what pushes it over the 80 ms budget is
CPU contention. A 1822×1272 PNG encodes in tens of milliseconds on an idle
machine — this lane passed for the v0.8.0 release on this same host — but under
contention the encode-and-write stalls to the ~250 ms measured here. So there
are two ways forward, and they are not exclusive: run the lane on a genuinely
quiet machine (it passes, as the release history shows), and separately fix the
measurement so a loaded machine no longer produces false failures. The second is
the durable fix; the first is what unblocks a commit today.

## Root cause, resolved: a startup stall plus a sampling window shorter than capture

Deeper capture inspection replaced the encode-latency story with the real one,
which has two parts, and both were fixed.

**Part one: the recorder stalls at startup.** A failing run showed inter-frame
gaps of ~33 ms (steady 30 fps) except for one ~700 ms gap right at the
beginning: `pipewiresrc` takes hundreds of milliseconds to negotiate its first
buffer. The harness slept a fixed 30 ms (`ANIMATION_RECORDING_ATTACH_DELAY`)
and then triggered the action, so the whole sidebar animation (~250-400 ms)
ran and settled *inside* that startup stall. No frame captured the motion,
which is why `mapped_intermediate_frame_count` was 0.

*Fix:* wait for the recorder's first frame to exist before triggering the
animation (`wait_for_first_stream_frame`, bounded by
`ANIMATION_FIRST_FRAME_TIMEOUT`). This is a precondition, not an estimate — an
animation triggered before capture starts cannot be recorded — so it cannot make
a wrong frame pass; it only stops the recorder from missing the event. Measured:
the startup gap disappears and mapped intermediate frames went from 0 to 8.

**Part two: geometry sampling ended before capture did.** With capture now
starting on time, the full frame budget ran past the sampling window: sampling
stopped at `stream_timeout` (1400 ms) while 48 frames at 30 fps keep being
written to ~1600 ms. Every trailing frame had no sample within budget and the
proof failed it as `stale-frame-geometry-pairing`.

*Fix:* sample geometry for as long as the recorder is capturing. The recorder
exiting is the true end of capture; `stream_timeout` (plus
`ANIMATION_SAMPLING_SAFETY_MARGIN`) remains only as a safety cap so a hung
recorder cannot sample forever. Every captured frame then has a sample near it.

## A tempting wrong fix, rejected by an existing test

Before the sampling fix, an alternative was tried: skip frames captured outside
the geometry window instead of failing them, on the argument that a frame with
no reference sample carries no evidence. The lane went green — and the existing
unit test `animation_report_builder_rejects_stale_frame_sample_pairing` failed.
That test deliberately asserts an unpairable frame **must** fail, and it is
right: silently dropping unpairable frames removes the guard that keeps a
drifted capture from sneaking through. The test was not edited to pass; the
approach was reverted. The recorder-driven sampling fix above keeps that guard
fully intact, because the trailing frames now *have* samples to pair with, so a
frame that still cannot pair within 80 ms is a genuine stale pairing and still
fails, exactly as the test demands.

The earlier "nearest-sample offset" estimator recorded below was likewise
killed by its own tests; both dead ends are kept as a record of what does not
work in this subsystem.

## The gate is all-or-nothing, which couples unrelated changes

`scripts/check-visual-proof-policy.py` treats `crates/cargo-gtk-proof/src/`,
`crates/lushtext-core/src/ui/`, `crates/lushtext/tests/widget/`, and the UI
resource trees as visual-sensitive, and requires the **entire** visual-geometry
lane to pass before any change touching them can be committed. Two consequences
worth recording:

- A change to one widget test (e.g. a sidebar-scroll regression proof) is
  blocked by an unrelated environmentally-broken minimap-animation case, because
  the gate is whole-lane rather than per-invariant.
- A fix to the harness itself lives under `crates/cargo-gtk-proof/src/`, which
  is visual-sensitive, so the pre-commit hook requires a passing lane to commit
  the very fix that would make the lane pass. The harness fix cannot be landed
  through the ordinary local gate while the lane is red.

Neither is necessarily wrong as policy, but both mean a single flaky/broken
case blocks a broad set of unrelated commits, and the harness fix has a
bootstrapping problem. A per-invariant scoping of the gate, or a sanctioned
path to land harness fixes, would remove the coupling.
