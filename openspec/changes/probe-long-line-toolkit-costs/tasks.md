## 1. Preconditions

- [ ] 1.1 Run `make gtk-axioms` first, as the toolkit procedure requires, and record the GTK, Libadwaita, Pango and GtkSourceView versions from the printed observations.
- [ ] 1.2 Re-check that the ledger ends at A22 and that no open change claims A23–A36. Renumber the whole range consistently across every artifact if that has changed, including the citations in `document-scale-policy`, `long-line-protected-view` and `sliced-protected-install`, If `stop-expander-bind-time-child-scans` was applied first, its axiom holds A23 and this range starts after it. Every other open change uses provisional names that are not ledger ids (C1's four D8 axioms, C2's P1–P7, C3's S1–S3, C4's G1–G4, and `stop-expander-bind-time-child-scans`' T1); each takes the next free id when its change is applied.

## 2. GTK axioms A23–A36 (`gtk-lush-axioms`)

- [ ] 2.1 Add the `pango` and `sourceview5` workspace dependencies to `crates/gtk-lush/axioms/Cargo.toml`. Run `cargo hakari generate` and review `Cargo.lock`; run `make cargo-sources` only if the external dependency set changed, and note it if `build-aux/cargo-sources.json` stays byte-identical. Confirm that `make check-gtk-lush-policy`, `make gtk-lush-adoption-matrix`, `make gtk-lush-msrv` and `make gtk-lush-examples` still pass, and that the `GTK Axioms` CI container already provides GtkSourceView 5.
- [ ] 2.2 Add shared text fixtures to `src/fixtures.rs`:
  - a deterministic paragraph builder (ASCII, multibyte, minified-markup);
  - a text view in a scrolled window;
  - a source view with an HTML language;
  - a log-capture guard that restores the previous writer;
  - a probe `GtkIMContext` registered on `gtk-im-module` under a probe-local id, selected only through the probe view's `im-module` property (the runner keeps `GTK_IM_MODULE=gtk-im-context-simple`). If GTK ignores a process-registered module, the A30 probe reports `FixtureInvalid` and the ledger row records it.
- [ ] 2.3 Implement probes A23 (one layout, no partial layout), A24 (invisible chunks excluded, with tag-apply and tag-remove re-layout times recorded), A25 (toggle gravity at start, end and interior) and A27 (tag changes do not dirty the buffer). Each has structural verdicts and timings as values only.
- [ ] 2.4 Implement probes A26 (word movement across a hidden boundary; the repeat-call cost before and after an edit is recorded, and the ledger states the invalidation half as cost evidence only), A28 (the measured overflow length) and A31 (idle iterations until `upper` settles, about height ÷ 2000 px; the ledger states that the observation is indirect).
- [ ] 2.5 Implement probes A29 (cut-off CRITICAL, highlighting disabled, tags inside a hidden range; the line doubles until the cut-off fires, up to a cap, and the cap yields `FixtureInvalid`, never `Violated`), A32 (the source-map detach CRITICAL with a pending tick; a defect axiom) and A33 (non-regex sees hidden text, regex visible only, per-match time recorded).
- [ ] 2.6 Implement probes A30 (probe-IM counts and surrounding lengths for caret moves, keys, clicks and scroll) and A34 (`move-cursor` with an invisible cursor moves only the viewport; caret browsing overrides it).
- [ ] 2.7 Add catalogue entries A23–A36 in id order. A35 and A36 get `probe: None`. A36 still records its cost observation through the lane, and the ledger says so.
- [ ] 2.8 Add one sample per probed axiom (`examples/a23_…`–`examples/a34_…`). Each has an interactive mode with length controls and measured values, and a `--check` mode that exits 0, 1 or 2, and 77 outside headless.
- [ ] 2.9 Add the slow-probe mechanism, which does not exist yet: a `slow` field on catalogue entries, a `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1` switch honoured by the `axiom_probes` runner and by `scripts/run-gtk-axioms.sh` (set by local `make gtk-axioms`, unset in the `GTK Axioms` PR job), a `skipped (slow): <id>` line, a `check-gtk-axioms` rule that a slow entry's ledger row says where it runs, and a `run-gtk-axioms.sh --self-test` case. Then measure the runtime `make gtk-axioms` adds against the job's 20-minute `timeout-minutes`. If A29 or another probe pushes the job toward it, mark it slow; never raise the timeout. Record the durations.
- [ ] 2.10 Add ledger rows A23–A36 to `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`, with these cells:
  - statements written from the observations;
  - dependent designs (C1, C2, C3);
  - "Pinned by";
  - "Verified against", copied from the observation lines;
  - "Sample".

  Update the intro's scope sentence (text layout, input method and GtkSourceView designs from A23 on). Run `make check-gtk-axioms`.
- [ ] 2.11 Update the `gtk-lush-axioms` README ("Adding an axiom" history, text fixtures, the slow marker) and CHANGELOG, the crate's adoption-evidence entry under `docs/gtk-lush-adoption/` if it lists probe counts, and its public-API snapshot through `make gtk-lush-api-advisory` if the catalogue's public items changed.

## 3. Budget constants and their arithmetic (`model/responsiveness_budget.rs`)

- [ ] 3.1 Create `crates/lushtext-core/src/model/responsiveness_budget.rs` with `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`. It holds:
  - `B_INTERACT_US = 8_000`, `B_SETTLE_US = 16_000`, `REFERENCE_FRAME_US = 8_333`, `INPUT_TO_FRAME_FRAMES = 2` and `SCROLL_P95_FRAME_GAP_US = 8_333`, each with a doc comment citing the 2026-10-11 decision;
  - `DispatchKind`, `classify_dispatch`, log2 bucketing, and the integer nearest-rank percentile.

  Register it in `model/mod.rs`.
- [ ] 3.2 Write these tests first, against a stub that returns `Indeterminate`, and watch them fail: every classification edge (`L + T == B`, `L + T == B + 1`, `L − ε == B`, `L − ε == B + 1`, `L < ε`, saturation at `u64::MAX`), and a property test that the percentile is monotone in the rank and bounded by the minimum and maximum.
- [ ] 3.3 Add `model/responsiveness_budget/kani_proofs.rs`, proving:
  - classification soundness (with tick and slack) and saturation for every `u64`;
  - bucket monotonicity and range;
  - percentile bounds over arrays of ≤ 4 elements.

  Assign the harnesses to an existing non-PR shard in `scripts/kani-shards.py` with a measured budget, and add the module's proof-strength oracle entry. Run `make kani KANI_SHARD=<shard> KANI_MEASURE=…` and `make check-kani-shards`.

## 4. Feature-gated in-process probe

- [ ] 4.1 Add the `responsiveness-probes` feature to `crates/lushtext-core/Cargo.toml` and `crates/lushtext/Cargo.toml`, documented as smoke-only, never enabled by release, Meson, Flatpak or Snap. Mirror the `crash-kill-points` wording.
- [ ] 4.2 Create `crates/lushtext-core/src/ui/responsiveness_probe.rs` as cross-cutting instrumentation with no workflow role. Without the feature, `install(&app)` is an empty inline function and reads no environment. Call it once from application startup.
- [ ] 4.3 Implement the watchdog: a custom `G_PRIORITY_HIGH` `GSource` with microsecond ready times (`g_source_set_ready_time`), tick `LUSHTEXT_RESPONSIVENESS_TICK_US` (default 2000), lateness against the ready time, and re-arming from the dispatch's own monotonic time. Implement frame timing (`after-paint`, completed `GdkFrameTimings` presentation and refresh interval) for every toplevel, including windows created later. Implement capture-phase input stamps for key, scroll and button events that record both the handler time and `gdk_event_get_time`.
- [ ] 4.4 Implement the probe IM: a `GtkIMContext` subclass registered as `lushtext-probe` when `LUSHTEXT_PROBE_IM=1`. It emits `retrieve-surrounding` on focus-in, `set_cursor_location`, `filter_keypress` and `reset`, and records counts and lengths, never text.
- [ ] 4.5 Implement bounded evidence: a fixed ring, per-second histograms and top-K worst stalls, sent over a bounded channel to one writer thread that appends JSONL to `LUSHTEXT_RESPONSIVENESS_LOG` through `services::filesystem::write`. Add a smoke-only append helper there if none fits. Dropped records are counted. Run `make check-filesystem-boundary`.
- [ ] 4.6 Compile the watchdog source, evidence buffers and encoder under `cfg(any(test, feature = "responsiveness-probes"))`, keeping only `install()` and its GTK hooks feature-only, so `make test-unit` covers them. Add unit tests for the record encoder and the drop counter, and a watchdog test on a private `glib::MainContext` (no display) whose blocking source of known length `S` is recorded within `[S − T, S + ε]`; write it before the watchdog and watch it fail. Build the default-feature binary and confirm that neither the probe's environment-variable names nor `lushtext-probe` appear in it (`strings`), and that CI's `cargo check -p lushtext --bins --locked` still passes. Add a lane preflight that launches the measurement binary once and checks that the log receives watchdog, frame and input records.
- [ ] 4.7 Add a Makefile target that builds the measurement binary into `target/responsiveness/`, so `target/debug/lushtext` never carries the feature.

## 5. Synthetic fixture generator

- [ ] 5.1 Create `scripts/generate-long-line-fixtures.py`. Given a kind, a size and a seed, it produces deterministic output for every kind: giant paragraph (ASCII, multibyte), minified-markup, dense near-threshold (wrap-agnostic), short-line HTML by total size, CR-only, CRLF and U+2029 variants, session set, and eviction set sized from the `model/editor_memory.rs` budget. It writes only into a given directory and prints the seed, size and SHA-256 manifest.
- [ ] 5.2 Add a self-test: same inputs give the same digest, each kind's structure is right (longest paragraph, delimiter set, total size), and it refuses an output path outside the given scope. Wire the self-test into `make check-policy` if it is cheap; otherwise into the lane's preflight.

## 6. The `make responsiveness-smoke` lane

- [ ] 6.0 Spike first: under a private `dbus-run-session -- mutter --headless --virtual-monitor`, open an `org.gnome.Mutter.RemoteDesktop` session and inject one keysym, one pointer axis and one button into a test window, and check the virtual monitor's refresh rate and whether `@120` is accepted (open question 2). Record the result in the programme record. If RemoteDesktop is unusable, the input cases stay `unavailable` and L_protect is not derivable until an alternative is chosen by the maintainer.
- [ ] 6.1 Create `scripts/run-responsiveness-smoke.sh` on `scripts/smoke-common.sh` and `scripts/with-temp-scope.sh`:
  - a private `dbus-run-session -- mutter --headless --virtual-monitor`, with `GSK_RENDERER=cairo` by default;
  - `taskset -c 0-15` when more than 16 CPUs are online;
  - a fresh isolated XDG state per case;
  - an explicit `GTK_IM_MODULE` for every variant (never GTK's Wayland text-input default, S7B-6);
  - an environment report (toolkit versions, kernel, CPU model, affinity, renderer);
  - refusal to touch a live session.
- [ ] 6.2 Create `scripts/responsiveness-smoke-driver.py`:
  - the case table from design D7, with `RESPONSIVENESS_TIER=quick|full` and the `RESPONSIVENESS_CASES` filter;
  - launching the measurement binary with fixture arguments;
  - readiness waits and actions through `scripts/lushtext-automation.py`;
  - GSettings for wrap, minimap and invisibles;
  - Mutter RemoteDesktop input for scroll, keys and clicks, recorded as `unavailable` when absent;
  - `CLOCK_MONOTONIC` phase markers;
  - per-interaction caps with `exceeded_cap`, kill and relaunch;
  - bounded bisection of at most 4 steps per series.
- [ ] 6.3 Add the variants: AT bridge plus a `pyatspi` listener (reusing the accessibility-smoke bus setup), the probe IM, IBus when `ibus-daemon` is present (otherwise `skipped`), and an optional `RESPONSIVENESS_RENDERER=ngl`.
- [ ] 6.4 Add the data-integrity check: SHA-256 of every fixture before launch and after each case, compared with the driver's own last write for the reload case. A mismatch fails the lane. The driver sends no save chord and activates no save action, ends any app holding typed edits with `SIGKILL`, and the `session` group quits gracefully only before typing. Prove the check failing-first with a self-test in which the driver itself appends to a fixture outside the reload case.
- [ ] 6.5 Add the storage evidence: 15 s after the last case, run `btrfs-stall-report --json` over the run window, and `--no-below` per case window. Mark storage-contaminated cases. Record `unavailable` when the tool is missing, and never install it.
- [ ] 6.6 Add the warning scan through `smoke_warning_classifiers.py`. Known defects (the minimap detach CRITICAL and any other reproduced one) are reported as findings with their owning change, with no allowlist entry.
- [ ] 6.7 Write the report: `report.json` and `report.md`, with per-case worst-stall bounds `[L − ε, L + T]`, frame-gap p95, input-to-frame frames and µs, IM and AT counts, the noise re-run and its flag, and the D7 derivation of L_syntax, L_minimap, L_invisibles, L_search_highlight, N_visible, L_protect, V_batch, R_batch, N_dense, the N_inspect bound, S_syntax and the budget verdict, under the names C1 and C2 declare. Each value names its rule, cases, variants and repetitions, or is reported as not derived with the reason.
- [ ] 6.8 Add the `responsiveness-smoke` Makefile target (help text, `.PHONY`), outside `end-user-smoke`. Document every helper flag in the `automation-helper-flags` comment and the helper table of `docs/automation-reference.md`, and the lane in `docs/automation.md`. Run `make check-automation-docs`.
- [ ] 6.9 Add `.github/workflows/responsiveness-smoke.yml`: nightly cron plus `workflow_dispatch`, a matrix by case group in the `quick` tier, `timeout-minutes: 30`, and artifact upload with `if: always()`. It is not a required check. Add one job that runs `make gtk-axioms` with `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`. Run `make check-workflow-timeouts`, and confirm that `make check-end-user-smoke-workflow` is unaffected.

## 7. Reference run and recorded values

- [ ] 7.1 Run `make responsiveness-smoke RESPONSIVENESS_TIER=full` on the reference lane (headless, ≤ 16 cores, never the live desktop). Re-run cases flagged noisy or storage-contaminated until each value is derived or explicitly not derivable.
- [ ] 7.2 Apply the budget verdict. If it revises a budget, change the constant in `model/responsiveness_budget.rs`, citing the run id, and re-run `make kani KANI_SHARD=<shard>` and the unit tests.
- [ ] 7.3 Copy each derived value, with its rule, cases and run id, into `docs/next/long-line-documents.md`, under the constant names C1 and C2 declare. Record whether C3 is needed (from the `install` group and A36), and the window-construction figures for C4.

## 8. Documentation

- [ ] 8.1 Create `docs/next/long-line-documents.md`, the C0–C5 programme record, containing:
  - the sequence and its gates;
  - the 2026-10-11 decisions;
  - the measured values;
  - the axiom ids per change;
  - the reproduced pre-existing blockers with their owning changes (the minimap CRITICAL and the `\n`-only slicer for C1, the window-construction stall for C4);
  - the carried-forward decisions (L_protect as a GSettings key, "Copy whole line", upstream reports, validation in the user's session).
- [ ] 8.2 Update `docs/next/formal-verification.md` with the ledger growth (A23–A36, the slow and unpinned rows) and the new Kani harness with its shard and measured time.
- [ ] 8.3 Update the remaining documentation:
  - `docs/end-user-coverage.md`: a lane-ownership row, the scheduled expectations, and the note that the lane is not in `end-user-smoke`;
  - `.agents/rules/build.md`: the lane, the measurement feature, and its separate target directory;
  - `AGENTS.md`: the Build Commands, Testing, module layout (`model/responsiveness_budget.rs`, `ui/responsiveness_probe.rs`), the Rules Index entry for `build.md`, and Recent Changes;
  - `README.md`: the test lanes and commands;
  - `sonar-project.properties`: review `sonar.sources` and exclusions for the new scripts and workflow, per the SonarQube scope rule in `build.md`.
- [ ] 8.4 Note in `docs/accessibility.md`'s smoke-coverage section that the lane runs with the AT bridge and a listener (no new anchors).

## 9. Verification

- [ ] 9.1 Run `make check` (fmt, all-feature clippy, policy audits, including `check-gtk-axioms`, `check-kani-shards`, `check-filesystem-boundary`, `check-automation-docs` and `check-workflow-boundaries`).
- [ ] 9.2 Run `make gtk-axioms`, `make test-unit`, the new widget test's shard (`make test-widget-shard WIDGET_SHARD=<shard>`), and the Kani shard. Keep the runs within 16 cores and headless.
- [ ] 9.3 Run the lane in the `quick` tier once more after the docs land. Confirm that the artifacts are complete, no fixture digest changed, and the temp scope is clean.
- [ ] 9.4 Run `openspec validate probe-long-line-toolkit-costs --strict`.
