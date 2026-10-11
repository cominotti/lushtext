## Context

The long-line programme splits the reviewed proposal (`docs/next/long-line-responsiveness.md`, not committed) into C0–C5:

| Change | Delivers |
|---|---|
| C0 | `probe-long-line-toolkit-costs`: probes and measurement (this change) |
| C1 | `document-scale-policy` |
| C2 | `long-line-protected-view` |
| C3 | `sliced-protected-install` (conditional) |
| C4 | `bound-window-construction-dispatch` (window-construction stall) |
| C5 | `explore-editing-long-paragraphs` (optional editing of giant paragraphs) |

C0 depends on no other change. C1–C4 depend on C0's recorded values, budgets, fixtures, watchdog and axioms, and C1–C4 are already drafted, so C0 gates their *implementation*, not their proposal. `stop-expander-bind-time-child-scans` is outside the programme and shares no code with C0. No other open change claims a ledger id: they use provisional names that are not ledger ids (C1's four D8 axioms, C2's P1–P7, C3's S1–S3, C4's G1–G4, and `stop-expander-bind-time-child-scans`' T1); each takes the next free id when its change is applied. If the stop-expander change is applied first, it takes A23 and C0 renumbers its range (task 1.2).

The maintainer decided on 2026-10-11:

1. Protected View now, which makes giant-paragraph documents view-only, with editing deferred to C5.
2. C0 measures the syntax total-size cutoff instead of fixing it in advance.
3. The budgets are stricter than the proposal's 16/50 ms. The working values are:
   - B_interact = 8 ms per interaction dispatch;
   - input-to-presented-frame ≤ 2 frames;
   - B_settle = 16 ms per settle dispatch.

   C0 confirms them.

The current state, as the exploration measured it:

- **Open.** gtrib.html (5.1 MB, 446 lines, longest paragraph 1,894,586 characters) stalls the main loop for seconds at open:
  - Pango lays out each giant paragraph in 285–1,256 ms;
  - the GtkSourceView context engine runs into its 2000 ms cut-off and turns highlighting off for the whole buffer;
  - the minimap re-lays out the paragraph again.
- **Interaction.**
  - Typing costs 300–440 ms per character.
  - In-tab find blocks for 25–28 s.
  - Window construction alone takes 188–380 ms for every file.
- **Unmeasured:**
  - cost with the AT bridge on;
  - cost with a real IM;
  - SpaceDrawer cost over long lines;
  - tag-removal re-layout;
  - the GL renderer.

All of these figures came from scratch harnesses under `target/`. None is reproducible from the repository today.

Repository facts this design relies on:

- **The axiom ledger** (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`) ends at **A22**. A23 onward is free, and no open change claims those ids.
- **`gtk-lush-axioms`** (`crates/gtk-lush/axioms/`) depends only on `glib`, `gtk4` and `libadwaita`.
  - Each probe returns an `Observation`.
  - Each sample has an interactive mode and a `--check` mode, which exits 0, 1 or 2, and 77 outside the headless session.
  - The runner is `gtk-lush-adoption-lab`'s `axiom_probes` test, launched by `scripts/run-gtk-axioms.sh`, which sets the harness's pre-GTK environment, including `GTK_IM_MODULE=gtk-im-context-simple`.
  - `make check-gtk-axioms` keeps the ledger and the catalogue in agreement.
  - `make gtk-axioms` runs on every PR as the `GTK Axioms` job of `ci.yml`, whose `timeout-minutes` is **20** (the repository ceiling `check-workflow-timeouts` enforces is 30).
  - The catalogue has no notion of a slow probe today.
- **Smoke-only cargo features exist.** `crash-kill-points` compiles to an empty inline function in every shipped build (`services/kill_point.rs`). Default `make test-unit` and widget runs do not enable such a feature, so code compiled only under it is tested only where a lane builds with it.
- **Headless sessions pin the input method.** The proof harness and `run-gtk-axioms.sh` set `GTK_IM_MODULE=gtk-im-context-simple` as a crash guard: headless Mutter advertises `zwp_text_input_manager_v3`, and destroying a focused editable races it into a SIGSEGV (`docs/next/persistent-format-hardening.md` S7B-6).
- **No repository script drives Mutter's RemoteDesktop API yet.** Input injection through it is new in this change.
- **Smoke infrastructure exists:**
  - `scripts/smoke-common.sh`, which isolates XDG state and writes the environment report;
  - `scripts/lushtext-automation.py`, the reusable D-Bus client;
  - `scripts/run-accessibility-smoke.sh`, the AT-SPI bus plus `pyatspi`;
  - `scripts/with-temp-scope.sh`, which detects temp leaks;
  - `.github/workflows/end-user-smoke.yml`, a weekly smoke matrix whose lanes `check-end-user-smoke-workflow` pins.
- **Degradation today is by byte size only** (`services/file_limits.rs`). There is no GSettings key for syntax. A fixture's extension selects the language: `.html` enables syntax, `.txt` disables it. `word-wrap`, `show-minimap` and `invisible-characters-mode` are GSettings keys.

## Goals / Non-Goals

**Goals:**
- **Pin every toolkit belief that C1–C3 rely on** (A23–A36) with an isolated probe, or record why it cannot be pinned.
- **Measure the real binary reproducibly** on a named reference lane, and derive candidate values with a written derivation:
  - L_syntax, L_minimap, L_invisibles, L_search_highlight, L_protect and N_visible;
  - V_batch, R_batch and N_dense, and the bound on C2's N_inspect;
  - the syntax total-size cutoff S_syntax;
  - a confirm-or-revise verdict on B_interact, B_settle and input-to-frame.
- **Make the budgets named constants now**, so that C1 and C2 cite one source of truth, and so that C0 can revise their values from measurement.
- **Leave shipped behaviour unchanged.** Every new runtime path is compiled out of default, release, Meson, Flatpak and Snap builds.
- **Record the C0–C5 programme** in `docs/next/long-line-documents.md`.

**Non-Goals:**
- Any change to load, display, editing, syntax, minimap or search behaviour (C1/C2).
- PR-gated precondition widget tests (each later change adds its own).
- New D-Bus automation members, snapshot fields or readiness predicates. The lane uses only existing ones.
- Fixing the minimap detach CRITICAL, the `\n`-only slicer, or the window-construction stall. C0 measures them and the record assigns them to C1 and C4.
- Real IBus over Wayland text-input-v3: `mutter --headless` without gnome-shell provides no IM bridge (see Risks).
- Measuring on, or touching, the live desktop session.

## Decisions

### D1. The axiom ids are A23–A36, as the proposal lists them

The ledger ends at A22, so no renumbering is needed. Each axiom carries an intended observation, chosen so that the **verdict is structural**: it compares positions, counts, tags, log captures or signal emissions. Timings are observation *values* and never verdict inputs. A timing-based verdict would be a flaky probe on a shared CI runner, which the pre-existing-blockers rule treats as a blocker.

| Id | Statement (short) | Structural observation (verdict) | Recorded values | Pinnable |
|---|---|---|---|---|
| A23 | A paragraph is one PangoLayout over its visible text, with no partial layout | With wrap on, `iter_location` of the paragraph's last character is valid, and the paragraph height matches the full-paragraph wrap count once any part is shown | µs per visible character at 2k/20k/200k, wrap on and off | yes |
| A24 | Invisible chunks are left out of the layout text | A hidden tail adds no height and no x-advance; the head's geometry equals that of an untagged head-only buffer | tag-apply and tag-remove re-layout time | yes |
| A25 | Text inserted at a tagged run's start or end boundary is untagged; text inserted strictly inside is tagged | Tag membership of the inserted characters at the start, the end and the interior | — | yes |
| A26 | Log attrs cover the whole logical paragraph, hidden text included, and any character change invalidates them | `forward_word_end` from the visible head lands inside a hidden run when a word spans the boundary | first-call and repeat-call cursor-movement time before and after a one-character edit elsewhere | yes, for coverage. The invalidation half has no structural observation (the cache is private), so the ledger row states it as cost evidence only |
| A27 | Tag apply and remove emit no `changed`, leave `modified` unset, and record no undo history | Signal counts, `modified`, `can-undo` | — | yes |
| A28 | Pango's glyph width overflows int32 above about 238k monospace characters | The x position at the line end is non-monotone or negative past the overflow length | the measured overflow length | yes |
| A29 | The context engine analyses a line atomically, hits a 2000 ms cut-off, then disables highlighting, and reads hidden text | A captured `g_critical` names the cut-off; afterwards highlighting is off; highlight tags appear inside a hidden range before the cut-off on a smaller line. Whether a given line reaches 2000 ms depends on the runner's speed, so the probe doubles the line length until the cut-off fires, up to a fixed cap; reaching the cap without it is `FixtureInvalid`, never `Violated` | time to cut-off, and the line length that reached it | yes. Expected to be slow; **measured** at 32,768 characters of dense synthetic markup (≈8 s per run), which is consistent with gtrib.html's own lines (cut-off between 8k and 32k characters for its script and markup lines, 64k–128k for its CSS lines), so it is not marked slow |
| A30 | IM spot and surrounding-text work grows with the cursor's paragraph, armed by caret moves, keys, clicks and scroll | A probe `GtkIMContext`, registered on the `gtk-im-module` extension point and selected for the probe's view only through the view's `im-module` property (the runner's `GTK_IM_MODULE=gtk-im-context-simple` stays untouched), counts `retrieve-surrounding` and `set_cursor_location` calls per action, and checks that the surrounding text spans the cursor's whole paragraph (`gtk_text_view_retrieve_surrounding_handler` slices from line start to line end, gtktextview.c:8996-9059) | per-call surrounding-text length and time | yes |
| A31 | Validation runs in 2000 px batches per idle | The number of idle iterations until the adjustment's `upper` settles, for a document of known height, is about height ÷ 2000 px | rows per batch, wrap off | yes, indirect (the ledger row says so) |
| A32 | `gtk_source_map_set_view(NULL)` with a pending tick causes a CRITICAL (a defect axiom) | A captured `Gtk-CRITICAL` or `GLib-GObject-CRITICAL` after a detach with the tick pending | — | yes. "Holds" means the defect is present; an upstream fix makes the probe *violated*, which is an axiom change |
| A33 | `GtkSourceSearchContext`: the non-regex path sees hidden text, the regex path sees visible text only. **Corrected by the probe:** `SearchSettings:visible-only` defaults to true, so both paths skip invisible text by default; with it off, the non-regex path sees hidden text and the regex path still does not | Match counts over a hidden range with each mode and each `visible-only` value | per-match time against line length | yes |
| A34 | With the cursor invisible, `move-cursor` moves only the viewport; `gtk-keynav-use-caret` overrides that | Insert-mark offset unchanged and adjustment value changed, then the opposite with caret browsing on | — | yes |
| A35 | AT-SPI contents and offsets over invisible text, and `TextChanged` payloads on insert | — | lane AT-case payload sizes and offsets | **no in-crate probe**: GtkTextView's `GtkAccessibleText` implementation is reachable only through an AT-SPI bus, which the probe runner does not start. Pinned by the lane's AT case, and the ledger row says so |
| A36 | The btree merges adjacent character segments by copying both strings | — | per-insert time growth when appending next to a large segment, at 1/10/100 MB | **no structural observation**; cost-only, recorded by the probe and by the lane's install case. The ledger row says so |

**Alternatives considered:**
- Renumbering from A40 to leave room: rejected, because ids are stable and dense, and nothing reserved them.
- Timing verdicts with generous thresholds: rejected as flake-prone.

### D2. `gtk-lush-axioms` gains `pango` and `sourceview5`

A29, A32 and A33 need GtkSourceView fixtures. A23, A24 and A28 read Pango geometry. Both crates are already workspace dependencies. The family policy forbids depending on GTK Lush or LushText crates, not on toolkit crates. Adding them keeps one catalogue and one runner.

**Alternative considered:** a cargo feature `sourceview` on the crate. Rejected: catalogue entries would need cfg-gated probe pointers, and `check-gtk-axioms` would have to understand features, for no gain inside an internal `0.0.0` crate.

### D3. Probe runtime stays inside the PR cap

Long-line probes use the smallest lengths that show the structure: 20k–300k characters, except A29, which needs a line that reaches the 2000 ms cut-off. Size-scaling timings at 1.9M characters and above belong to the lane, not the PR job. The change records the measured added runtime of `make gtk-axioms` against the `GTK Axioms` job's 20-minute timeout. If A29 makes the job approach it, A29 becomes a probe flagged `slow`. No slow mechanism exists today, so this change adds one: a `slow` field on the catalogue entry, a `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1` switch the runner honours (set by local `make gtk-axioms` and by the nightly responsiveness workflow, unset in the PR job), a `skipped (slow)` line in the PR output, and a `check-gtk-axioms` rule that a slow entry's ledger row says where it runs. Raising the job's `timeout-minutes` is not an option.

### D4. Budgets are integer-microsecond constants in a pure model module

`crates/lushtext-core/src/model/responsiveness_budget.rs` is GTK-free and has `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`. It holds:

- `B_INTERACT_US = 8_000`, `B_SETTLE_US = 16_000`, `REFERENCE_FRAME_US = 8_333`, `INPUT_TO_FRAME_FRAMES = 2`, and `SCROLL_P95_FRAME_GAP_US = 8_333`. Every frame-relative limit uses a fixed 120 Hz reference frame (maintainer decision, 2026-10-11), not the virtual monitor's refresh interval, so input-to-presented-frame is at most 2 × 8,333 = 16,666 µs however fast the lane's virtual monitor runs;
- the `DispatchKind` (`Interaction` | `Settle`) classification of driver phases;
- `classify_dispatch(lateness_us, tick_us, slack_us, budget_us) -> Within | Over | Indeterminate`, where `slack_us` is the watchdog's measured idle wake-up slack (D5);
- log2 histogram bucketing;
- the integer nearest-rank percentile.

It lives in `model/` because C1's `DocumentScale` and C2's policies will cite the same constants. Placing it under a `ui/` workflow role home would make two later workflows import another workflow's policy.

Kani (`model/responsiveness_budget/kani_proofs.rs`) proves, for every `u64` input:

- the classification is sound: `lateness + tick ≤ budget ⇒ Within`, `lateness > budget + slack ⇒ Over`, and otherwise `Indeterminate`;
- the arithmetic saturates;
- bucket indices are monotone and in range;
- the percentile of a bounded array (≤ 4 elements) lies between its minimum and maximum, and is monotone in the rank.

The harness joins an existing non-PR Kani shard with a measured budget (`make check-kani-shards`), and gets a proof-strength oracle entry.

The working values stay at 8/16 ms unless the C0 report revises them. A revision edits these constants in this change, citing the run id.

**Alternative considered:** floats in milliseconds. Rejected under the repository's whole-unit policy convention, and so that Kani can prove the arithmetic.

### D5. In-process measurement, compiled only under `responsiveness-probes`

`ui/responsiveness_probe.rs` is cross-cutting instrumentation, like `services/kill_point.rs`. It has no workflow role and no `WFR-*` row. Without the feature, `install(&app)` is an empty inline function. With the feature, and only when `LUSHTEXT_RESPONSIVENESS_LOG` names a path, it installs four things.

1. **The lateness watchdog.**
   - It is a custom `GSource` at `G_PRIORITY_HIGH` on the default main context whose ready time is set in microseconds with `g_source_set_ready_time(now + T)`, tick `T` (default 2 ms, overridable through `LUSHTEXT_RESPONSIVENESS_TICK_US`). A plain `timeout_source_new` is not used: it has millisecond granularity and re-arms from the iteration's cached time, not from the dispatch.
   - On each dispatch it records `lateness = g_get_monotonic_time() − ready_time`, then sets the next ready time from that same `now`.
   - **What it measures is a main-loop stall, not one dispatch.** GLib cannot preempt a running dispatch, and it dispatches every ready source of the winning priority in one iteration back to back. So the largest lateness `L` bounds the longest *stall* `S` (one or more consecutive dispatches) as `S − T ≤ L ≤ S + ε`, where `ε` is the poll wake-up slack (GLib rounds the poll timeout to whole milliseconds). A stall is what delays input, so the budgets are read against it; it is never shorter than the longest single dispatch, which makes the reading conservative. The report states `[L − ε, L + T]`, and `classify_dispatch` returns `Within` only for `L + T ≤ B` and `Over` only for `L − ε > B`.
   - The driver runs one idle-app calibration case with tick 1, 2 and 4 ms. It records the watchdog's own overhead and `ε`, the largest lateness seen while the app is idle, which every later classification uses.
2. **Frame timing.**
   - Each toplevel's `GdkFrameClock` `after-paint` records `frame_time`.
   - Once a `GdkFrameTimings` is complete, it also records `presentation_time` and `refresh_interval`.
   - Frame gaps are computed from presentation times when they are present, and from frame times otherwise, with the source labelled.
3. **Input stamps.** A capture-phase `EventControllerLegacy` on each window records, for key, scroll and button events, both the handler's monotonic time and the event's own timestamp (`gdk_event_get_time`, milliseconds from the compositor's monotonic clock). The handler time alone would hide exactly the latency being measured: while the main loop is stalled the event waits in the Wayland socket, and the handler stamps it only afterwards. Input-to-presented-frame is therefore measured from the earliest available origin: the driver's injection stamp (`CLOCK_MONOTONIC`, recorded around each RemoteDesktop call), else the event timestamp. It is the number of frames, and the µs, to the first presentation whose frame started after the handler ran. The calibration case checks that the driver stamp, the event timestamp and the handler time agree within a few milliseconds on an idle app; if the compositor's event clock is not monotonic-based, the event timestamp is dropped and the report says so.
4. **The probe IM** (`LUSHTEXT_PROBE_IM=1`).
   - A `GtkIMContext` subclass is registered on the `gtk-im-module` extension point as `lushtext-probe` before the first window is built, and selected with `GTK_IM_MODULE=lushtext-probe`.
   - It is not the Wayland text-input IM, so the probe-IM variant does not reach the S7B-6 crash race.
   - On focus-in, `set_cursor_location`, `filter_keypress` and `reset`, it emits `retrieve-surrounding`, as an IBus engine with the surrounding-text capability does.
   - It records the call count and the surrounding-text length. It never records the text.

The evidence is bounded:

- a fixed-capacity ring of raw samples;
- per-second histograms;
- the top-K worst stalls, with their timestamps.

The records travel over a bounded channel to one writer thread, which appends JSONL through `services::filesystem::write`. A smoke-only append helper is added there if none fits, so `check-filesystem-boundary` stays clean. When the channel is full, records are dropped and counted, never blocked on. No document content is recorded. The driver aligns its phase markers with the samples through `CLOCK_MONOTONIC`, which is both Python's `time.monotonic()` and GLib's `g_get_monotonic_time()` on Linux.

The measurement binary is built with `--features responsiveness-probes` into its own target directory (`target/responsiveness/`), so `target/debug/lushtext`, which other smoke lanes use, never carries it. `make check` already builds all features, so clippy covers the gated code.

**Testability.** `make test-unit` and the widget shards build without the feature, so anything compiled only under it would be tested nowhere routinely. The module is therefore split: the watchdog source, the evidence ring, histograms, top-K and the JSONL encoder compile under `cfg(any(test, feature = "responsiveness-probes"))` and are unit-tested in `make test-unit`, including a watchdog test on a private `glib::MainContext` (no display) that runs a deliberately blocking source of known length and checks the recorded lateness lies in `[S − T, S + ε]`. Only `install()` and its GTK hooks (frame clock, event controller, probe IM registration) are feature-only, and the lane's preflight exercises them in the measurement binary.

**Alternatives considered:**
- An out-of-process D-Bus heartbeat: rejected by the review, because it measures queueing rather than dispatch length.
- `sysprof` marks: not available headless in CI, and harder to post-process.
- A GTK Lush crate for the watchdog: deferred. There is one consumer and its use is smoke-only. If C1 or C2 grows a second consumer, `gtk-lush-stewardship` decides then.

### D6. Input reaches the app through the automation client and Mutter's RemoteDesktop API

- **Actions.** Open goes through CLI file arguments. Find goes through `win.set-search-query`. Tab selection, preferences and readiness waits use existing catalogued actions and `WaitForReady`.
- **Input with no action** (scroll wheel, Page Down, arrows, typing, primary clicks): the driver opens an `org.gnome.Mutter.RemoteDesktop` session inside the private `dbus-run-session` and uses `NotifyKeyboardKeysym`, `NotifyPointerAxis` and `NotifyPointerButton`. Relative motion positions the pointer, so no screencast stream is needed.
- **When RemoteDesktop is unavailable** on a runner, the input cases record `unavailable` with the reason, and action-driven cases still run.
- **The driver never uses `ydotool` or `xdotool`** against a real seat, because that would reach the live desktop.

### D7. The lane: sweep, variants, caps, derivation

`scripts/run-responsiveness-smoke.sh` is the wrapper: temp scope, private `dbus-run-session -- mutter --headless --virtual-monitor`, `GSK_RENDERER=cairo` by default, and an affinity of at most 16 CPUs taken from the inherited mask (the first 16 CPUs the process may already use; never widened) whenever more than 16 are available, with the affinity recorded. `scripts/responsiveness-smoke-driver.py` does the work.

**Cases.** Fixtures come from `scripts/generate-long-line-fixtures.py`. A seed and a kind produce byte-identical output, so the artifacts record the seed, not the bytes.

| Group | Cases |
|---|---|
| paragraph | single giant paragraph of 2k/10k/50k/250k/1.9M characters × {`.html` syntax, `.txt` plain} × {wrap on, wrap off} × {minimap on, off}; bisection between the last passing and first failing point, at most 4 steps per series |
| dense | many lines just under each candidate threshold, filling ≥ 3 viewports, wrap off and wrap on |
| total-size | short-line HTML at 1/2/5/10 MB (syntax on) |
| delimiters | the same paragraph sets split by CR-only, CRLF and U+2029 |
| session | 3+ giant-paragraph documents open → graceful quit → relaunch restore → tab switching |
| eviction | enough synthetic documents, sized from the `model/editor_memory.rs` estimate, to cross the live budget; then reselect an evicted tab |
| reload | the driver rewrites an open fixture so the file monitor reloads it |
| install | one giant paragraph at 100/250/500 MB, `full` tier only (C3's gate) |
| invisibles | `invisible-characters-mode` on over a 250k paragraph (SpaceDrawer) |
| baseline | an empty document and a small document: idle calibration and window construction (C4's evidence) |

**Interactions per case:**
- open, until load readiness;
- 2 s of wheel scrolling;
- Page Down ×10;
- Right arrow ×5 and a 10-character type burst at a giant paragraph's start, middle and end;
- one primary click inside the paragraph;
- find, for a term present near the end;
- tab switch, where there are several documents.

Each interaction has a wall cap: 20 s by default, `RESPONSIVENESS_CASE_CAP_S`. Past the cap the driver records `exceeded_cap` with the partial evidence, kills and relaunches the app, and continues. A hang is a measurement result, not a harness failure.

**Variants.** Every case runs three times:
1. plain;
2. with the AT bridge and a `pyatspi` listener registered for `object:text-changed`, `object:text-caret-moved` and `object:text-selection-changed`, reusing the accessibility-smoke bus setup;
3. with the probe IM.

Real IBus runs as a fourth variant only when `ibus-daemon` is present. It uses `GTK_IM_MODULE=ibus` inside the private bus, and records `skipped` otherwise. A `RESPONSIVENESS_RENDERER=ngl` variant is optional and records the renderer it actually got.

Every variant sets `GTK_IM_MODULE` explicitly: `gtk-im-context-simple` for plain and AT, `lushtext-probe` and `ibus` for the IM variants. None leaves GTK's default Wayland text-input IM selected, because headless Mutter advertises text-input-v3 and that path segfaults on focused-entry teardown (S7B-6); a crash from it would be a harness artifact, not a measurement.

**Tiers and filters.**
- `RESPONSIVENESS_TIER=quick` (CI): fewer lengths, one repetition, no `install` group.
- `full` (local reference): every group, three repetitions, median and maximum.
- `RESPONSIVENESS_CASES=` filters the cases.

**Noise.** A case whose maximum is more than twice its median is re-run once, and the result is marked `noisy` if the spread persists.

**Data integrity.** The driver records the SHA-256 of every fixture before launch, and after each case that the app had open. A changed hash fails the lane, unless the driver made the change itself, as in `reload`, in which case it compares against what it wrote. Typing happens in buffers that are never saved:
- the driver never sends a save chord or activates a save action, and its key bursts contain printable characters, arrows and Page Down only;
- an app that holds typed edits is ended with `SIGKILL`, never a graceful quit, so no Save Changes dialog can appear and no automated answer to one is needed; the `session` group performs its graceful quit before any typing, and types only after the relaunch;
- every case starts from a fresh isolated XDG state, so drafts and session files from one case never restore into another, and the drafts a killed case leaves are removed with its scope.

**Storage evidence.** Fifteen seconds after the last case, the wrapper runs `btrfs-stall-report --since <start − 60 s> --until <end> --json` and, per case window, `--no-below`. A case whose window the report attributes to a storage stall is marked `storage-contaminated` and excluded from the derivation. A missing tool or log directory is recorded as `unavailable`. The lane never installs it.

**Warnings.** Runtime warnings are classified with `smoke_warning_classifiers.py`. Known pre-existing defects, such as the minimap detach CRITICAL, are reported as findings with an owner change (C1). They are **not** allowlisted as accepted.

**Exit.** The lane exits 0 when the harness ran and the artifacts are complete. It fails on a fixture hash change, missing artifacts, a driver error, or an app crash outside an `exceeded_cap` kill. Budget overruns are *results* in C0, because the stock app is known to exceed them.

**Derivation** (`report.json` and `report.md`). Every derived value lists the cases, repetitions and variant that decide it, and the worst variant decides. The names match the constants C1 (`document-scale-policy`) and C2 (`long-line-protected-view`) declare, so each can copy its values from the record without renaming.

| Value | Rule |
|---|---|
| L_syntax | the largest paragraph length whose `.html` case shows no syntax-attributable dispatch over B_interact while scrolling and over B_settle during open, where syntax-attributable means the `.html`-minus-`.txt` difference at the same length |
| L_minimap | the largest paragraph length whose minimap-on case shows no minimap-attributable stall (minimap-on minus minimap-off, same length and wrap) over B_interact while scrolling and over B_settle during open |
| L_invisibles | the largest paragraph length whose `invisibles` case shows no SpaceDrawer-attributable stall (invisibles on minus off) over B_interact while scrolling |
| L_search_highlight | the largest paragraph length whose in-tab find case keeps every stall during find and match navigation ≤ B_interact (find over a stock `GtkSourceSearchContext`, with A33's per-match times) |
| N_visible | the largest visible paragraph length whose plain, wrap-on and wrap-off scroll cases keep every dispatch ≤ B_interact; checked against A23's µs-per-character values |
| L_protect | the largest paragraph length whose caret, type and click interactions (log attrs, IM surrounding text, AT) stay ≤ B_interact and ≤ 2 frames of input-to-frame in the worst variant |
| V_batch | the largest visible-character count per 2000 px validated window (from the dense cases, with A31's batch height) that keeps validation dispatches ≤ B_interact |
| R_batch | rows per 2000 px validation batch, wrap off, at the smallest supported line height (A31's batch height divided by the measured row height at the minimum editor zoom) |
| N_dense | V_batch ÷ R_batch |
| N_inspect bound | min(N_visible, L_protect): the Line Inspector (C2) has a caret, so its window must stay inside both bounds; C2 chooses its value at or below this |
| S_syntax (syntax total-size cutoff) | the largest total size whose short-line HTML background highlighting keeps every dispatch ≤ B_interact |
| Budgets | **confirmed** when the baseline cases (an empty and a small document, excluding window construction, which C4 owns) fit 8/16 ms and 2 frames on the reference lane with the watchdog's upper bound; otherwise **revised** to the smallest values the baseline fits, with the reason |

The reference values come from a `full` run on the reference machine (the maintainer's desktop, inside its toolbox, headless, ≤ 16 cores). The run id, toolkit versions, kernel and CPU model are recorded. CI runs are trend evidence only.

### D8. CI: a separate nightly workflow, sharded

`.github/workflows/responsiveness-smoke.yml` runs nightly and on `workflow_dispatch`, with a matrix of case groups. Each job has `timeout-minutes ≤ 30` (`check-workflow-timeouts`), runs the `quick` tier, uploads artifacts with `if: always()`, and is never a required check. One extra job runs `make gtk-axioms` with `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`, so a probe marked slow (D3) still runs on CI every night. It is separate from `end-user-smoke.yml`, which is weekly, so `check-end-user-smoke-workflow`'s pinned lane set is unchanged. `make end-user-smoke` does not include the lane, because of its length. `docs/end-user-coverage.md` says so.

### D9. The programme record

`docs/next/long-line-documents.md` holds:
- the C0–C5 table, with each gate;
- the maintainer decisions, dated;
- the measured values, with their derivation and run ids, filled in by this change's final task;
- the axiom ids each later change cites;
- the pre-existing blockers C0 reproduced, each with its owning change: the minimap CRITICAL and the `\n`-only slicer (C1), and the window-construction stall (C4);
- the open decisions carried forward: L_protect as a GSettings key, "Copy whole line", and upstream reports.

`docs/next/formal-verification.md` records A23–A36 and the new Kani harness.

## Risks / Trade-offs

- **[A timing-heavy probe flakes in the PR job]** → Verdicts are structural only (D1), and slow probes have a `slow` escape (D3).
- **[The watchdog perturbs what it measures]** (500 wakeups/s) → It runs only in the feature build; the idle calibration with 1/2/4 ms ticks records its overhead; and the report gives both dispatch bounds.
- **[Headless Mutter is not the user's session]** (no gnome-shell IM bridge, cairo renderer, a virtual monitor refresh rate) → The probe IM reproduces the surrounding-text demand deterministically; IBus runs when present; GL is an optional variant; and the record lists validation in the user's session, through an isolated-XDG launch with consent, as a C2 precondition, not a C0 deliverable.
- **[The input-to-frame refresh assumption]** "2 frames" is fixed at 2 × the 120 Hz reference frame (16,666 µs), whatever the virtual monitor's `refresh_interval` (maintainer decision, 2026-10-11). A slower virtual monitor can only make the lane fail more often, never pass a slower app. → The report states the µs, the reference frames and the monitor's own refresh interval; the lane runs a 120 Hz virtual monitor when the installed Mutter accepts it (task 6.0 checks) so frame gaps are observable at that rate.
- **[RemoteDesktop is unavailable on a runner]** → No repository script uses it yet, so the first lane task is a spike that opens a RemoteDesktop session under the private `dbus-run-session -- mutter --headless` and injects one key, one axis and one button into a test window. The input cases record `unavailable` when it fails, and the reference run must have it (the derivation refuses to derive L_protect without it).
- **[The watchdog reads stalls, not single dispatches]** → A stall of several back-to-back dispatches can read `Over` while each dispatch alone fits. That is the latency the user feels, so the budgets are applied to it (maintainer decision, 2026-10-11).
- **[Lane duration]** The 1.9M cases can hit the 20 s caps repeatedly. → Tiers, filters, per-interaction caps, and CI sharding.
- **[Disk use]** The `full` tier generates up to 500 MB. → The fixtures are generated into the temp scope and removed per group, and `with-temp-scope.sh` fails a run on leaks.
- **[Pre-existing blockers reproduced but not fixed in C0]** → The record assigns each one to the next change in the same work stream (C1 or C4), as the split decision requires. None is allowlisted. The stream does not close while one remains.
- **[Reproducing gtrib-like structure without the file]** → The generator's `minified-html` kind mimics a long single-line markup and script mix with a realistic tag density. Its parameters, but no content, are derived from the profile already measured.

## Migration Plan

Nothing to migrate: no persisted format, setting or shipped behaviour changes. Rollback is a revert. Order of work:

1. Axioms.
2. The budget module with Kani.
3. The feature-gated probe.
4. The fixture generator.
5. The lane and CI.
6. A reference `full` run.
7. Record the values.
8. Docs.

## Open Questions

1. **Capability placement** (maintainer decision 6). This change creates `responsiveness-measurement` instead of folding it into `performance-regression-coverage`. Is that confirmed?
2. **Frame reference — resolved 2026-10-11:** a fixed 120 Hz reference frame for both the input-to-frame limit and the scroll frame gap.
3. **The reference machine — resolved 2026-10-11:** the maintainer's desktop (`sophia`, inside the toolbox, headless, ≤ 16 cores) is the reference lane for derived values; CI runs are trend evidence only and never fail on budgets.
4. **The A29 probe in the PR job.** If A29 alone adds more than about 1 minute, should it run as a `slow` probe outside the PR job?
5. **Upstream reports** (maintainer decision 7): file the A32, A28, A26, A30 and A33 behaviours upstream now, or after C1?
6. **Archiving C0 while the reproduced blockers are open.** May C0 be archived while the minimap CRITICAL and the window-construction stall remain open and owned by C1 and C4, or must the stream wait?
7. **Stall or dispatch — resolved 2026-10-11:** the budgets are read against main-loop stalls, as this design does.
8. **The scroll frame-gap limit — resolved 2026-10-11:** 8,333 µs (120 Hz).
9. **IBus on the reference run.** The proposal's maintainer item 8 asks for the C0 run with IBus present. Must the reference `full` run have `ibus-daemon`, or is the probe IM sufficient when IBus is absent?
