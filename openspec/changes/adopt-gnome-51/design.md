## Context

LushText ships one artifact today: the Cominotti Flatpak, built on
`org.gnome.Platform//50` (GTK 4.22, GtkSourceView 5.20, Pango 1.57,
Libadwaita 1.9, GLib 2.88). Development and CI run the same family in
Fedora 44 containers and the `hypatia` toolbox (GTK 4.22.5). The Snap
scaffold targets the GNOME 50 platform snap, which is not published yet
(`core26`). The gtk-rs feature floor is GNOME 50: `gtk4` `gnome_50`,
`libadwaita` `v1_9`/`gtk_v4_22`, `sourceview5` `v5_18`, `glib`/`gio`
`v2_88` and `pango` `v1_57`.

On 2026-11-09 the maintainer approved moving development and the shipped
Flatpak to GNOME 51. The new toolbox `hypatia-f45` (Fedora 45) carries GTK
4.24.1, GtkSourceView 5.22.0, Pango 1.58.2, Libadwaita 1.10.0, GLib 2.90.1
and mutter 51.1. `org.gnome.Platform//51` is installed system-wide (GTK
4.24.1). `org.gnome.Sdk//51` is not installed yet (875 MB download; installing
it locally is approved). The old toolbox stays for rollback.

Rules that shape this change:

- **Toolkit-update procedure** (`.agents/rules/build.md`): run
  `make gtk-axioms` first. A failing probe is an axiom change. Revisit the
  entry, its dependent designs and every envelope that cites it before any
  probe is edited. Copy "Verified against" from the printed observation.
- **Compiler pin** (maintainer, 2026-10-29): the binary we test and measure
  is built by the same compiler as the binary we ship.
- **Pre-existing blockers**: any warning or flake the bump exposes is fixed in
  this work stream.
- **Budgets**: `B_interact` 8 ms and `B_settle` 16 ms. Measurements are
  interleaved `main` versus branch on the same toolkit, at a 1-minute load
  below 4.

## Measured baseline (spec stage, `main` `3dcbe682`, toolbox `hypatia-f45`)

All evidence is in `.claude/handoffs/adopt-gnome-51/`.

### B1. `make gtk-axioms` on GTK 4.24.1 / Adw 1.10.0 (`gtk-axioms-f45-main.log`)

88 probes held and 3 were violated. The sample `--check` runs for those
three also exit 1 (violated): `sample-A7-f45.log`, `sample-A20-f45.log` and
`sample-A32-f45.log`. Because the probe binary failed, the runner never
reached the other samples. The implementation reruns the full lane once the
three probes are version-qualified.

| Axiom | GTK 4.22.5 (ledger) | GTK 4.24.1 (measured) | Attribution |
|---|---|---|---|
| A7 | after a host `set_value(1360)`, the values at pages 300/250/200/150 are 1360/1139/918/697 (one line, align −4.42) | 1360/1360/1360/1360, implied align 0: a page change no longer moves the value | GTK 4.23.4 `!9778` "listbase: Sanity-check alignment values" (`gtk-4.24.1-NEWS.txt` l. 128); to be confirmed against the 4.22→4.24 `gtklistbase.c` diff |
| A20 | a `value-changed` before allocation leaves the anchor stray (align outside [0, 1]) | row 40: anchor 1360, align 0, value unchanged after shrink (re-announced or not); row 150: the same pattern (5100) | same as A7 |
| A32 | detach with a pending tick logs two CRITICALs | no CRITICAL captured (the defect is fixed) | GtkSourceView 5.22.0, commit `8d679ccd` (already tracked in `docs/upstream-reports.md`) |

The probe stops at its first failed check. On 4.24 we therefore have only
A7's host-`set_value` half. Its `scroll_to` half (6534 → 6622 on 4.22) and
the remaining A20 checks are unmeasured. Task 1.2 instruments every sub-check
before any statement is rewritten.

### B2. `make test` on Fedora 45 (`make-test-f45-main.log`, 17.8 min, load 1.38 → 1.24)

- **Non-widget:** all 2,424 tests pass.
- **Widget:** all 1,658 tests pass. Even so, every shard fails the gate on
  its first and second attempt with `unexpected warning output`. One test is
  `FLAKY`. The warnings fall into three new classes; none of them appears on
  GTK 4.22:

| Class | Count | Shards | First attribution (from the log; to be confirmed per test) |
|---|---|---|---|
| W1 `Trying to snapshot GtkSourceMapSlider … without a current allocation` | 1,786 | window 1,576, editor-page 202, surfaces 8 | the native minimap's slider (`GtkSourceMap` child), across many minimap-on editor tests (`editor_page`, `document_scale`, `window_composition`, `command_palette*`, `search_panel`) |
| W2 `Trying to measure GtkListView … for height of 0, but it needs at least N` (N from 10 to 19,124; most often 48 (268 lines), 124 (138), 86 (108) and 162 (84)) | 830 | surfaces | `workspace_section::*` tests: the sidebar file tree inside `ViewportSliceBin`. GTK emits this text when a widget is measured **horizontally** for a height (`for_size`) below its own minimum height, so something measures the list's width for height 0; the large N values track the trees' content heights. No allocation warning was logged. Candidate cause: GTK 4.23.2 `!9906` "Fix Gtk.ListView/Gtk.GridView measure with for_size != -1", or `!9778` |
| W3 `AdwEntryRow … placed into AdwExpanderRow …, can only be placed into GtkListBox` | 20 | surfaces | Libadwaita 1.10: the template `[row]` `workspace_excluded_add_row` in `workspace_excluded_names_row` (`resources/ui/preferences.blp` l. 214–230); seen in `preferences::*` and the sections that build the preferences dialog |
| `FLAKY` | 1 | window | `minimap_markers::test_minimap_replace_all_of_many_matches_keeps_one_pending_refresh` passed only on retry |

The surfaces shard took 15.9 min, against 7.8–8.0 min for every shard on
2026-10-30. Much of that is likely the warning volume and the reruns. It is
re-timed once the warnings are gone (`widget-shards.py` `ci_minutes` is never
fed from local timing).

### B3. Responsiveness lane, quick tier, `main` on Fedora 45 (`lane-f45-main/`)

This is one run of the measurement build of `main` `3dcbe682`, built with
Rust 1.98.1, on mutter 51.1: 109 cases, 17:12–18:44, load 1.69 → 1.29. The
ibus variants were skipped (no ibus), as before.

**The lane fails.** On GTK 4.22 / mutter 50.4, `main` passed the quick tier.
The last runs were `power-of-two-thresholds` `v-main` and `v-branch` on
2026-11-05, both with 0 withheld and 0 misdirected typing phases. Here the
same lane:

- **L1, typed text drawn late, in 100 phases**, including `baseline/small`
  (`summary.json` `withheld_typing_frames`). A key reaches the editor and
  inserts its character, but the first frame drawn after it starts only
  after the next key. Same case and variant
  (`typing-f44-vs-f45.txt`):

  | `baseline/small` plain | p50 | max | withheld keys |
  |---|---|---|---|
  | GTK 4.22 | 5.3 ms | 16.4 ms | 0 |
  | GTK 4.24 | 10.0 ms | 60.1 ms | 33 |

  Some withheld keys take 7 frames (56–58 ms); one has no frame at all. The
  worst input-to-frame is 378 ms. The dense cases are unchanged (p50 11.0 →
  10.5 ms).
- **L2, typing reached no document, in 36 phases.** These are the
  `paragraph/10000/*/html` `type-start` phases: keys arrived outside an
  editor or inserted nothing.
- **L3, `baseline/empty` fails** in every variant: `no-editor-point` on
  click, then exit −9. No archived run with this case was found, so whether
  L3 predates GTK 4.24 is unknown (task 3.5.3).
- **Budget verdict `revise`.**
  - `B_interact` is within: worst interaction stall 6.0 ms.
  - `B_settle` is over: 17.8 ms at `baseline/small@at#0:background`, against
    10.1 ms on 4.22 in PT's run.
  - input-to-frame is over.
  - scroll frame-gap p95 is 8.6 ms, against a limit of 8.33 ms.
- **Unchanged:** the fill-in median, 18.4 ms against a 25 ms target, and
  every hand-off dispatch within `B_SETTLE_US`.

L1 is a user-visible regression candidate: text that appears one or more
frames late while typing in a small plain document. It may also be an
artifact of the lane's injection or presentation timing on mutter 51; task
3.5.1 decides which. Either way the lane must pass before the change is
accepted. The rule is the existing `responsiveness-measurement` requirement
that a withheld typing frame fails the run.

### B4. Rust in the GNOME 51 SDK

`org.gnome.Sdk//51` declares `org.freedesktop.Sdk.Extension` version
`26.08`. On 2026-11-09, `org.freedesktop.Sdk.Extension.rust-stable//26.08`
ships Rust **1.98.0** (2026-09-03), while `//25.08` ships **1.98.1**
(2026-09-07). The manifest names the extension without a branch, so the
SDK's `26.08` is what builds the shipped binary.

## Goals / Non-Goals

**Goals:**

- The Cominotti Flatpak ships GNOME 51, and every local and CI lane that
  measures or tests behaviour runs GTK 4.24 / GtkSourceView 5.22 /
  Libadwaita 1.10.
- The GNOME 50 floor stays buildable, and its toolkit behaviour stays pinned,
  for the Snap and source builds.
- Every axiom whose verdict changed is restated per toolkit, attributed, and
  reviewed against its dependent designs and envelopes.
- The full gate is green on Fedora 45: zero unexpected warnings, zero
  `FLAKY`, and a passing responsiveness lane (no withheld or misdirected
  typing). Each toolkit regression is fixed at its cause, failing first.
- No user-visible regression: measured interleaved, the budgets hold or any
  exception is recorded.

**Non-Goals:**

- Raising the gtk-rs feature floor or adopting `v4_24` / `v1_10` / `v5_22`
  APIs.
- Dropping any workaround (A45 retirement, slice-bin write-back, A37 frame
  validation, A79 stand-in terminator, C5 surrounding bound, A52 exception)
  while a supported runtime needs it.
- The GNOME nightly re-test of upstream fixes (next queued item).
- Publishing the Snap.

## Decisions

### D1. Ship GNOME 51; keep GNOME 50 as the source floor

The Flatpak manifest moves `runtime-version` from 50 to 51. `sdk` stays
`org.gnome.Sdk` and follows the runtime version, and `rust-stable` follows
the SDK's extension version. The gtk-rs features stay at the GNOME 50 set.

*Rationale (user experience first):* Flatpak users get the newer toolkit,
including upstream fixes such as A32. The Snap's only planned platform
(`gnome-50-2604`, GTK 4.22) and distribution builds still need the floor.
Compiling on 4.24 with the GNOME 50 feature set already proves at compile
time that no newer API is used. No 4.24-only API is needed today; enabling
`v4_24` would only narrow the floor.

*Alternative rejected:* moving the floor to 51. It would let us drop A45 and
the slice-bin paths, but it would strand the Snap.

### D2. Test what ships; keep the floor's behaviour pinned

- **Local development:** toolbox `hypatia-f45`.
- **CI test, measurement and lint jobs:** `registry.fedoraproject.org/fedora:44`
  → `fedora:45`: 28 container `image:` references in eight workflows, plus
  21 `shared-key: fedora-44*` cache keys that become `fedora-45*`. The
  implementation checks that the `fedora:45` image resolves
  (`skopeo inspect` or `podman pull`) before it edits the workflows.
- **Flatpak jobs:** `ghcr.io/flathub-infra/flatpak-github-actions:gnome-50` →
  `gnome-51`. The tag exists; it was checked against the registry's tag list
  on 2026-11-09. The `flatpak-builder-gnome-50-*` cache keys become
  `-gnome-51-*`.
- **`GTK Axioms`** runs on both toolkits as two jobs, not a matrix. The
  existing job keeps its id `gtk-axioms` and its check name `GTK Axioms`, and
  moves to `fedora:45` (shipped, GTK 4.24). A new job `gtk-axioms-floor`,
  named `GTK Axioms (GNOME 50 floor)`, runs the same steps on `fedora:44`
  (GTK 4.22) with its own `fedora-44` cache key. A matrix would rename the
  check to `GTK Axioms (<entry>)`; if `GTK Axioms` is a required check on
  `main`, merges would then wait on a check that never reports. Keeping the
  name avoids a branch-protection change, which is a maintainer action.
- **Builder-diagnostics runtime image.** This was missing from the spec-stage
  inventory. `build-aux/builder-diagnostics/Containerfile` builds a debug GTK
  4.22.4 / Libadwaita 1.9.1 / GtkSourceView 5.20.0 stack on `fedora:44`,
  published as `ghcr.io/cominotti/lushtext-builder-diagnostics-runtime:gnome-50-debug`
  by `builder-diagnostics-runtime.yml` and consumed by the weekly
  `end-user-smoke.yml` `builder-diagnostics` lane and by
  `scripts/run-builder-diagnostics.sh`. Builder diagnostics exist to catch
  builder and template warnings on the toolkit users run, and W3 is such a
  warning on Libadwaita 1.10. The image therefore moves to `fedora:45`, GTK
  4.24.1, Libadwaita 1.10.0 and GtkSourceView 5.22.0. Its blueprint-compiler
  ref is re-checked against the Fedora 45 toolbox's version, and its tag
  becomes `gnome-51-debug`. Every consumer moves with it:
  `end-user-smoke.yml`, `run-builder-diagnostics.sh`'s default and
  `docs/blueprint-validation.md`. The image is built locally with `podman`,
  and `make builder-diagnostics-smoke` runs against it through
  `LUSHTEXT_BUILDER_DIAGNOSTICS_IMAGE` before the change is accepted.
  Publishing the `gnome-51-debug` tag needs a `workflow_dispatch` of
  `builder-diagnostics-runtime.yml` on `main` after merge. That is an outward
  action for the maintainer, listed in the handoff, and it must happen before
  the next weekly End-User Smoke run. Until then, that lane cannot pull its
  image. The lane is not a pull-request gate.
- **`make gtk-axioms-runtimes`** defaults to `org.gnome.Sdk//50`,
  `org.gnome.Sdk//51` and `org.gnome.Sdk//master`.

*Rationale:* the shipped binary is the only one users run, so widget tests,
smokes and the lane run on 4.24. The floor's behaviour differs from 4.24 only
where an axiom says so, and the axioms are what the probes pin, so running
the probes on the floor is enough to notice a floor-only break. The cost is
one more `GTK Axioms` job (about 10 min).

*Residual, recorded:* widget tests no longer run on GTK 4.22, so a
regression seen only on the floor and outside the ledger would go unseen
until the Snap ships. The Snap change must run the widget suite on its
platform before publishing (a note for `snap-ci-and-testing`).

### D3. Version-qualified axioms

When a ledger entry's verdict differs between supported toolkits, the entry
states each verdict with its version bound. The probe and the sample assert
the verdict that matches the running toolkit's version, through one helper in
`gtk-lush-axioms`: `ToolkitVersion` from `gtk4::major_version()` and the
`sourceview5` version functions, compared against a bound named in the
catalogue entry. A probe still fails on any further change on either side,
and its observation prints which branch it asserted.

- **A7 / A20:** the GTK < 4.24 statement is unchanged. The GTK ≥ 4.24
  statement records what task 1.2 measures for every sub-check, for example:
  "a host `set_value` anchors the list at the view's top edge, alignment 0;
  a page change keeps the value; `scroll_to` keeps alignment in [0, 1]".
  The exact wording comes from the measurement, not from this design.
- **A32:** holds (defect present) on GtkSourceView < 5.22.0 and is violated
  (fixed, `8d679ccd`) on ≥ 5.22.0. On ≥ 5.22 the probe asserts that no
  CRITICAL is logged. A regression back to the defect would then fail the
  probe.

"Verified against" lists both runs:

- `fedora:45` host (`make gtk-axioms`);
- the GNOME 50 floor, from `make gtk-axioms-runtimes` against
  `org.gnome.Sdk//50`, or the `hypatia` F44 toolbox, or the `fedora:44` CI
  job.

*Order (ledger rule):*

1. Attribute and measure (task 1).
2. Review the dependent designs and envelopes (D4, D5).
3. Then rewrite the entries.
4. Only then edit the probes.

*Alternative rejected:* removing the 4.22 branch. The floor still runs it,
and the floor job would then fail.

### D4. `ViewportSliceBin` keeps its paths; the Kani envelope already covers 4.24

On 4.24, a host `set_value` no longer leaves a stray anchor. The bin's
re-announcement after an emitting publish, and
`classify_child_scroll_in_frame`'s write-back, become no-ops there: they
write the value the child already holds, and A11 emits nothing for an
unchanged `set_value`. On the floor they are still needed.

The Kani slice loop (`crates/gtk-lush/widgets/src/kani_proofs.rs`) models a
stray anchor's re-derivation as a nondeterministic `stray_value` in
`[-MAX_CONTENT, 2·MAX_CONTENT]`. That range includes the unchanged value, and
the 4.24 anchor (the published value, alignment 0) lies inside the
`stray_anchor` range `[0, content]`. So the measured half of the 4.24
behaviour is a subset of the modelled reactions, and every proved property
holds for it.

The subset claim is **conditional on task 1.2**. The model also assumes, for
a `scroll_to`, that the anchor lies inside the view (`view_anchors[0]` in
`[0, page]`). It further assumes that a request moving the view up anchors
at alignment 0 (`view_anchors[0] == 0`, citing A7). Those halves are not yet
measured on 4.24. If task 1.2 measures any 4.24 behaviour outside an A7/A20
`kani::assume`, the model is widened, never narrowed. Every shard that runs
the slice loop then reruns, one CBMC at a time, under `scripts/heavy.sh`:
`widgets-slice-loop-rest`, `-rest-three`, `-requests`, `-requests-two`,
`-pairs` and `-unbounded`, all `scheduled`-gated, 11–16 min each on CI.
Those shards, not `widgets-geometry`, exercise the A7/A20 anchor model. The
`widgets-geometry` shard holds the pure band, containment and request
harnesses.

If the model is not widened, the change only rewrites the citations as
version-qualified comments, so CBMC sees the same program. It then reruns
only the pull-request-gated `widgets-geometry` shard once, as a sanity check.
Any code change in `scroll_request.rs` or `slice_geometry.rs` (for W2)
reruns `widgets-geometry` and the six slice-loop shards.

W2 is the open risk here. If 4.24 changed the list's minimum-height
contract (A2/A3), the bin's measure may now hand the list a height below its
minimum. Task 3.2 attributes W2 with an isolated probe and fixes it at the
cause. That cause may be a new ledger axiom, for example "GtkListView ≥ 4.24
reports a non-zero minimum for `for_size`". Silencing the warning is never
the fix.

### D5. Keep the minimap's retire-and-rebuild (A45) on 5.22

The detach CRITICAL is gone on 5.22, but the retire path stays:

- **It is still required on every supported runtime older than 5.22:** the
  GNOME 50 floor, the Snap, and distribution builds.
- **It is correct on 5.22 too.** A45 held on 4.24, and unparenting a map
  never relied on the defect.
- **It does more than avoid the CRITICAL.** "A retired native map is
  released" requires the map to be finalized and to stop validating, laying
  out or drawing document text. A `set_view(NULL)` in place would keep a live,
  parented map.

The `editor-minimap` requirement becomes version-qualified: the failing-first
reproduction of the detach CRITICAL applies on GtkSourceView < 5.22. On 5.22
the regression test still asserts that the retire path emits nothing. Revisit
this decision once 5.22 is the floor (recorded in `docs/upstream-reports.md`,
"When resolved").

### D6. The Rust pin follows the Flatpak SDK's extension

The pin moves to the version
`flatpak remote-info --user flathub-user runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08`
reports at implementation time. On 2026-11-09 that is 1.98.0. The pin
appears in:

- `rust-toolchain.toml`;
- every `dtolnay/rust-toolchain@` (19 sites);
- `snap/snapcraft.yaml`'s rustup bootstrap;
- `scripts/lint-advisory-policy.toml` `toolchain`;
- README, AGENTS, `build.md` and `rust.md`.

The MSRV stays at 1.98.0. If `26.08` ships ≥ 1.98.1 by then, the pin does
not move.

*Rationale:* the maintainer's rule is about the shipped binary, and the GNOME
51 SDK builds it with `26.08`. A patch-level step down within 1.98 changes no
language or lint surface the workspace depends on. That claim is checked,
not assumed: the GTK Lush MSRV lane already builds the family at 1.98.0, and
task 4.1 runs the whole workspace's all-feature Clippy and `make check` under
the new pin. The branch carries the toolkit fixes as well as the compiler
change, so a `main`-versus-branch pair cannot separate the two. Task 6.2
therefore adds a compiler-attribution pair: `main` built with 1.98.1 against
`main` built with 1.98.0, both on GTK 4.24, on the headline cases.

*Alternative rejected:* naming `rust-stable//25.08` inside the 51 SDK. That
pairs extensions across freedesktop branches, which Flathub does not support,
and the build would no longer be what the 51 SDK builds.

### D7. Toolkit regressions are fixed at their cause, failing first

For each of W1–W3 and the `FLAKY` test:

1. Find the test, or the smallest fixture, that emits it.
2. Write a failing test or probe: the widget test fails on the warning (the
   runner already does), or a new `gtk-lush-axioms` probe isolates the
   toolkit behaviour.
3. Fix LushText at the cause.
4. If the cause is a toolkit defect, use the `upstream-gtk-reports` skill to
   check whether it is already reported or fixed, and record it in
   `docs/upstream-reports.md`. A report is drafted only, never filed without
   the maintainer's approval.

Benign-noise lists are not widened.

Working hypotheses, to be confirmed:

- **W1:** GtkSourceView 5.22's slider is snapshotted in a frame where the
  map's overlay child was not allocated. This is likely the minimap's
  hide/show or the retire path, analogous to A37. It needs a probe on a bare
  `GtkSourceMap`.
- **W2:** see D4.
- **W3:** Libadwaita 1.10 checks the parent of an `AdwPreferencesRow` added
  as a buildable `[row]` child. The fix is probably to add the entry row with
  `add_row` from code, keeping the `remove` contract recorded in
  `excluded_names.rs`.
- **`FLAKY`:** classify the wait as a synchronous flip or an async
  completion. Rerun it in isolation; fix the predicate or the race.

### D7b. The lane's typing regressions (L1–L3) are attributed before anything else is measured

L1 decides whether users type into a GNOME 51 LushText with visibly late
text. It is therefore treated as a regression, not as lane noise, until
proven otherwise:

1. **Reproduce in isolation.**
   - Run `baseline/small` typing alone, several repetitions, on `main`'s
     measurement binary on F45, without any case filter that changes
     ordering.
   - Run the same case with the minimap off and on (W1 involves the map's
     slider being snapshotted unallocated).
   - Run it with `GSK_RENDERER` as the lane sets it.
2. **Separate toolkit from harness.** Compare the probe's frame-clock
   records:
   - `before-paint`/`after-paint` and presentation times against the key's
     handler time.
   - Did GTK not schedule a frame after the insertion, or did mutter 51
     headless not present it?

   A `gtk-lush-axioms` probe settles the GTK side without LushText: a bare
   `GtkTextView` with a key injected through the same path. A frame not
   requested after an insert is an axiom (and possibly an upstream report).
   A frame requested but presented late is mutter's (lane environment), and
   is recorded as such with evidence.
3. **Fix at the cause, failing first.**
   - If LushText suppresses or defers the redraw on 4.24 (for example the
     A37 `frame_validation.rs` queue, the minimap slider allocation of W1,
     or the held-typing path), the fix lands with the widget or lane case
     red first.
   - If the cause is the harness on mutter 51, fix the driver's frame
     attribution or injection, with its self-test, never by loosening the
     `withheld_frame` rule.
4. **L2:** check the `html` `type-start` phases' delivery records:
   - Was the click point covered (sidebar or properties geometry on 4.24)?
   - Was the editor not focused?
   - Was the language attach still holding input?

   Fix the cause.
5. **L3:** check whether `baseline/empty` also fails on GTK 4.22 (run
   `main` in the `hypatia` F44 toolbox, headless). If it does, it is a
   pre-existing driver defect, still fixed here (pre-existing-blockers
   rule). Otherwise, attribute it like L1.

Only after L1–L3 are resolved does task 6 run the interleaved verdict.

### D8. Measurement

These steps run after the gate is green:

1. Build the `responsiveness-probes` binary from `main` (Rust 1.98.1) and
   from the branch (pinned compiler).
2. Run the quick tier interleaved: main, branch, main, branch. Use
   `scripts/measure.sh`, `taskset -c 8-23`, load < 4, and record the load
   before and after each run.
3. Run every quick-tier case. The headline cases are keystroke, dense,
   paragraph, total-size, session, minified-html and the construction
   profiles.

The verdict compares branch with `main` on GTK 4.24 only. GTK 4.22 → 4.24
deltas (`main` on F44, from the archived records of
`bin-minimap-markers-per-row` and `power-of-two-thresholds`, against this
change's B3) are a context table, not pass/fail.

The branch must pass the lane outright: 0 withheld typing frames, 0
misdirected typing phases and 0 failed cases. A green comparison with a
failing `main` is not enough. Every lane budget is judged, not only
`B_interact` and `B_settle`: input-to-frame (2 reference frames, 16.67 ms)
and the scroll p95 frame gap (8.33 ms) count too. `main` on 4.24 is already
over three of them: `B_settle` 17.8 ms, input-to-frame 378 ms and scroll
p95 8.6 ms. Each case over any budget that is not already a recorded
exception has two possible outcomes:

- a fix in this change;
- an explicit, measured, recorded exception, with cause and attribution.

The visual-geometry, accessibility and visual smokes run once each on the
branch. They are artifacts, not interleaved timing.

### D9. Docs and records

- **README and AGENTS:**
  - tech stack: "GNOME 51 runtime (GTK 4.24, Libadwaita 1.10, GtkSourceView
    5.22); GNOME 50 source floor";
  - Rust pin;
  - CI images;
  - Snap gate unchanged.
- **`build.md`:** Flatpak runtime, rust-stable `26.08`, CI containers, the
  two `GTK Axioms` jobs (shipped and floor), the builder-diagnostics image
  tag, the `gtk-axioms-runtimes` default, and the toolkit-update procedure,
  which now names the shipped runtime and the floor.
- **`docs/blueprint-validation.md`:** the builder-diagnostics image tag and
  its toolkit (D2).
- **`rust.md`:** the platform feature floor line stays GNOME 50 and adds the
  shipped runtime.
- **`ui.md`:** l. 363, the 4.22 / 1.9 warning note, is re-checked on 1.10.
- **`gtk4-libadwaita-internals` skill:** the version-family lines, where
  4.24 is the shipped runtime and 4.22 the API floor.
- **Ledger:** A7, A20 and A32 restated, "Verified against" for every row
  rerun on 4.24, and any new axiom from W1/W2.
- **`docs/upstream-reports.md`:** what the 51 runtime changes. A32 is fixed
  in the shipped runtime; every other row is unchanged. Note that the
  shipped Flatpak no longer needs the A32 workaround, but the floor does.
- **`docs/next/long-line-documents.md`:** the queue entry, then the
  implementation record.
- **`docs/next/formal-verification.md`:** the A7/A20 envelope note.

## Risks / Trade-offs

- [L1 is a real GTK 4.24 regression in how quickly typed text is drawn, and LushText cannot avoid it] → D7b separates the toolkit from the harness with an isolated probe. If GTK is at fault and no LushText-side cure exists, the shipped-runtime move is reconsidered with the maintainer, user experience first, rather than accepted with late typing. The decision and evidence are recorded in the design and the programme record.
- [W2 hides a real sidebar geometry regression on 4.24 (rows drawn wrong, scroll jumps)] → The rendered-bounds slice-bin tests pass, but warnings fail the gate. Task 3.2 adds an isolated probe, and the `workspace-tree-reveal` visual-geometry smoke checks real pixels.
- [W1 is a toolkit defect we cannot fix in LushText] → Prove the cause with a bare-`GtkSourceMap` probe, work around it in the minimap's lifecycle if possible, and track it upstream. The gate stays strict.
- [The Rust pin steps down to 1.98.0] → The MSRV is already 1.98.0, and the interleaved lane would show a codegen effect.
- [The floor is untested by widget tests] → Recorded residual (D2). The axioms run on the floor in CI, and the Snap change must reinstate floor widget runs.
- [`org.gnome.Sdk//51` download (875 MB) and the first Flatpak build time] → One-time, local, approved.
- [The builder-diagnostics lane cannot pull `gnome-51-debug` until the image is published after merge] → The lane is weekly and not a pull-request gate. The local `podman` build plus `make builder-diagnostics-smoke` proves the image and the lane before merge. Dispatching the publishing workflow is listed in the handoff as a maintainer action.
- [The `gnome-51` CI image or `fedora:45` packages differ from the toolbox (e.g. a `gtksourceview5-devel` micro version)] → "Verified against" records where each run happened. The first CI run after merge is checked, but not waited on in this stage.

## Migration Plan

The implementation lands as one branch.

- **Rollback:** revert the branch. The `hypatia` F44 toolbox and
  `org.gnome.Platform//50` stay installed.
- **Users:** a Flatpak update pulls the 51 runtime automatically. App data in
  `~/.var/app/dev.cominotti.lushtext` is unaffected.

## Open Questions

None blocking. Two questions are settled by measurement inside the change:

- the exact 4.24 statements for A7 and A20 (task 1.2);
- the causes of W1 and W2 (task 3).

## Spec-stage lane baseline

See B3. Artifacts are in `lane-f45-main/`:

- `report.md`, `summary.json`, `results.jsonl`;
- the meta file `lane-f45-main.meta` (load before and after);
- the binary `bin/main-3dcbe682-f45`;
- the comparison `typing-f44-vs-f45.txt`.

GTK 4.22 context comes from `power-of-two-thresholds/v-branch/lane`
(2026-11-05, mutter 50.4, a case subset).

## Spec review (2026-11-09)

An independent review checked the artifacts against the code, the CI
workflows, `scripts/kani-shards.py` and the spec-stage logs. It made these
corrections:

- **Inventory gap: the builder-diagnostics runtime image.** It was built on
  GTK 4.22.4 / `fedora:44` with the tag `gnome-50-debug`, and the spec stage
  had not listed it. It now moves to the GNOME 51 stack (D2, task 2.4).
  *Rationale:* builder diagnostics exist to catch template warnings on the
  toolkit users run, and W3 is one.
- **Kani shards.** The A7/A20 anchor model is exercised by the six
  `widgets-slice-loop-*` shards, not by `widgets-geometry`. The envelope's
  subset claim also holds only for the measured host-`set_value` half; the
  `scroll_to` half carries its own assumptions. D4 and tasks 1.3, 1.8, 3.2
  and 4.4 now make the claim conditional on task 1.2, and rerun every
  slice-loop shard if the model is widened.
- **`GTK Axioms` check name.** Two jobs replace the planned matrix, and the
  existing job keeps its name. *Rationale:* a matrix renames the check, and a
  required check that never reports would block merges until the maintainer
  changes branch protection.
- **Compiler attribution.** The branch-versus-`main` pairs mix the 1.98.1 →
  1.98.0 step with the toolkit fixes. A `main`@1.98.0 arm now separates them
  (D6, task 6). The claim that the "MSRV lane builds at 1.98.0" was narrowed:
  that lane covers only the GTK Lush family.
- **Every lane budget is judged.** Input-to-frame and the scroll p95 frame
  gap count as well as `B_interact` and `B_settle`. The branch must also pass
  the lane outright (D8, task 6.4).
- **W2 facts.** N ranges from 10 to 19,124, not only 162. The warning comes
  from a horizontal measure for height 0, and no allocation warning was seen.
  The `gtk-lush-viewport-slice` requirement no longer implies that the list
  may never be allocated below its minimum height. Read literally, that would
  have conflicted with band allocation if the list's minimum height had
  become its content height.
- **Counts corrected:** 28 `fedora:44` images plus 21 `fedora-44*` cache keys,
  and 19 toolchain pins. Two references to task 3.6 now point to task 3.5.

The review found no renamed scenario under a MODIFIED requirement, and
`openspec validate adopt-gnome-51 --strict` passes. Verdict: sound.

## Implementation record (2026-11-10)

Evidence: `.claude/handoffs/adopt-gnome-51/` (`runs.log` lists every run with
the 1-minute load before and after).

### Task 1.2: A7 and A20 measured sub-check by sub-check on GTK 4.24.1

| Sub-check | GTK 4.22 (ledger; re-measured on the GNOME 50 SDK) | GTK 4.24.1 |
|---|---|---|
| A7 host `set_value(1360)`, pages 300/250/200/150 | 1360/1139/918/697, align −4.42 | 1360/1360/1360/1360, align 0 |
| A7 `scroll_to(200)`, 300 → 200 px | 6534 → 6622, align 0.887, one `value-changed` | the same |
| A20 rows 40 and 150, plain | align −4.42 / −16.89, values 918 / 3411 | align 0, values 1360 / 5100 unchanged |
| A20 rows 40 and 150, re-announced | align 0 | align 0 |

**Attribution.** The `gtklistbase.c` diff 4.22.5 → 4.24.1 is `!9778`'s
zero-area guard and `isfinite` checks, the keypad bindings and
`G_PARAM_STATIC_NAME`; `gtklistview.c` adds `get_request_mode`
(height-for-width, `!9906`, A104); `gtklistitemmanager.c` is unchanged. The
exact change behind the new host-`set_value` anchor is not bisected; it is
attributed to the 4.23 list-base and request-mode work (`!9778`, `!9906`) with
that caveat.

### Task 1.3: dependents

| Dependent | GNOME 50 floor | GNOME 51 (shipped) |
|---|---|---|
| `classify_child_scroll_in_frame` write-back (A7) | needed | a no-op (the value is unchanged), harmless |
| the bin's re-announcement after an emitting publish (A20) | needed | a no-op (A11: no emission for an unchanged value) |
| `classify_child_scroll`'s `reconfigure_shift` bound (A7 `scroll_to` half) | needed | needed, unchanged |
| Kani slice loop: `stray_anchor ∈ [0, content]`, `stray_value ∈ [−MAX, 2·MAX]` | covers the stray anchor | covers it: the 4.24 anchor is the published value, a row of the content, and the value stays (inside the range) |
| Kani slice loop: `view_anchors[0] ∈ [0, page]`, alignment 0 upward | as measured | unchanged on 4.24 |
| minimap retirement (A45) and `test_minimap_retirement_after_scrolling_emits_no_critical` (A32) | needed | correct, kept (D5) |

No assumption had to be widened: comments only, `widgets-geometry` rerun once.

### Toolkit regressions

| Item | Cause | Fix | Failing first |
|---|---|---|---|
| W1, 1,786 `GtkSourceMapSlider` snapshot warnings | GtkSourceView 5.22 skips allocating an unchanged slider rectangle even when the slider queued a resize (A103, `8d679ccd`); the minimap's `minimap-wide-editor-slider-offset` class moves the slider's margins on every projection | the class applies only below GtkSourceView 5.22 (`native_slider_needs_wide_editor_correction`); upstream report drafted, not filed | `editor_page::test_minimap_slider_correction_class_follows_the_sourceview_release` red with the gate disabled |
| W2, 830 `GtkListView` measure warnings | `GtkListView` is height-for-width on GTK 4.24 (`!9906`, A104); `ViewportSliceBin` forwarded its own height to the child's width measure | the bin measures the child's width for no height (the child is only allocated the band) | `gtk_lush_adoption::test_adoption_slice_bin_width_is_measured_for_no_child_height` red (2 warnings) |
| W3, `AdwEntryRow … placed into AdwExpanderRow` | a Blueprint `[row]` child reaches `AdwExpanderRow`'s buildable as `type="row"`, which it does not handle: it fell through to `GtkListBoxRow`'s `set_child` and replaced the expander's header and list on **every** Libadwaita; 1.10 added the parent check that reports it | the entry row is an untyped template child (appended through `add_row`) | `workspace_entry_visibility::test_excluded_names_add_row_is_a_row_of_the_expander_list` red (parent `AdwExpanderRow`) |
| `FLAKY` `test_minimap_replace_all_of_many_matches_keeps_one_pending_refresh` | `wait_for_scan` accepted `occurrences_count() >= 0` of a context whose search text was still empty (the entry's search delay), so Replace All replaced nothing (4 of 11 first attempts in isolation) | wait for the context's search text to be the needle too | 10 of 10 isolated runs pass |
| L1, typed text "drawn late" | GTK 4.24's frame clock aligns the frame time to its refresh grid (A105: −8.2 to +8.1 ms from `before-paint`); the lane driver matched keys to frames by frame time | the probe records `began_us` (`before-paint`), the driver attributes by `frame_start_us`, with a self-test | `baseline/small`: 52 withheld phases before, 0 after |
| L2, `paragraph/10000/*` type-start misdirected | pre-existing (identical on GTK 4.22 in `power-of-two-thresholds/am14-main`: about 1.3 s per key, nothing inserted within the phase) | none here: covered by C2's recorded toolkit-floor exception for editable paragraphs of 3,500–50,000 characters and queued as DL | — |
| L3, `baseline/empty` exit −9 | not reproduced on the branch (2 repetitions) | — | — |
| V1 (task 3.6), `make visual-geometry-smoke`: the 8 `minimap-sidebar-mid` cases fail `pixel-anchor-failed` on `main` **and** the branch on GTK 4.24 (the search marker drawn 25 px above the native slider's top edge, required inside it) | GtkSourceView 5.22 places the slider from the two views' content heights, margins included (A106); the minimap copied the editor's end-of-document overscroll margin (hundreds of pixels) 1:1 onto its map, so the slider drew below the text the editor shows (a 600 px margin: 279 px off in the probe) | from 5.22 the map's bottom margin is the editor's margins scaled by the map's text scale (`scaled_source_map_bottom_margin`, integer, whole-pixel); below 5.22 it is still copied. A constant residual remains: the map keeps its 5 px top inset where the editor has 6 px, so the slider sits about 4 px above the text on 5.22 | `editor_page::test_minimap_native_slider_covers_the_visible_text_with_an_overscroll_margin` red with the copy (186 px), green; 44/44 `minimap-sidebar*` visual cases pass |
| V2 (task 3.6, pre-existing), the sample lanes skipped every sample from A100 on: `scripts/run-gtk-axioms.sh` listed samples with `a[0-9][0-9]_*`, so `make gtk-axioms` and `make gtk-axioms-runtimes` ran 88 of 95 samples and A100–A102 had never had a sample `--check` in any lane | the two-digit glob | the list matches any number of digits (`sort -V`), and the runner's self-test fails when the list and the samples on disk differ or no three-digit id is listed | the self-test (red with the old glob: 88 of 95) |
| V3 (task 4.6, pre-existing), `make verify-flatpak-identity` failed after `make flatpak-install` ("not installed or its metadata cannot be read") whenever the published `stable` branch is installed beside the local `master` build: a bare app id is ambiguous to `flatpak info` | the verifier queried the app id, not a ref | the verifier reads the branch the active export launches (`Exec=… --branch=<b>`, overridable with `LUSHTEXT_FLATPAK_BRANCH`) and queries `app//branch` | `scripts/test-flatpak-identity-verifier.sh` gains the two-branch case (red without the fix: no `//master` ref) |

**A104 restated (the floor half).** With V2 fixed, the GNOME 50 SDK run
checked A104's floor branch for the first time and violated it: on GTK 4.22.2 a
`GtkListView` width measured directly for height 0 logs the same measure
warning (one), so "below 4.24 the same measure is silent" was wrong. What
differs is the request mode (`ConstantSize` on 4.22.2, `HeightForWidth` on
4.24.1): `ViewportSliceBin` reports its child's request mode, so only on 4.24
does GTK measure the bin's width for a height that the bin then forwarded. The
ledger row and the catalogue statement were restated first, then the probe: the
direct warning is a control on both toolkits and the version branch asserts
the request mode. W2's fix and its failing-first test are unchanged.
