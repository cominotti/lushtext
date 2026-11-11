Rules for every task:

- **Headless only.** Never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`. Axiom samples only with
  `--check`.
- **CPUs and heavy work.**
  - Use CPUs 8–23 (`taskset -c 8-23`).
  - Heavy work goes through `scripts/heavy.sh`, which runs
    `scripts/with-memory-cap.sh` inside it. `make test` admits its own
    scopes.
  - Timing verdicts go through `scripts/measure.sh`.
  - One CBMC at a time.
  - Never `pgrep -f`, never `find /`.
- **Measurements.** Run only at a 1-minute load below 4 (wait if it is
  higher). Interleave `main` and branch on the same toolkit (GTK 4.24), and
  record the load before and after each run.
- **Evidence** goes in `.claude/handoffs/adopt-gnome-51/`.
- **Failing first** means the test or probe is committed, or recorded red,
  before the fix that turns it green.
- **Never relax a check.** Never widen a benign-noise list or a warning
  filter, and never adjust a probe before its ledger entry is reviewed and
  restated (design D3).

## 1. Axioms first (toolkit-update procedure)

- [x] 1.1 Record the spec-stage evidence in `runs.log` with the `main`
      revision: B1 (`gtk-axioms-f45-main.log`), B2
      (`make-test-f45-main.log`), B3 (`lane-f45-main/`) and B4. Fill in the
      design's "Spec-stage lane baseline". If `main` moved, rerun
      `make gtk-axioms` on the branch's base before any other work.
- [x] 1.2 Measure every sub-check of A7 and A20 on GTK 4.24.1 without
      editing the probes. Use a scratch copy of the probes, or an observation
      mode that runs all checks and reports each one. Cover:
      - A7's host-`set_value` half (pages 300/250/200/150);
      - A7's `scroll_to` half (4.22: 6534 → 6622, align 0.887);
      - every A20 row (40, 150, …), with and without re-announcement.

      Read the `gtklistbase.c` diff between 4.22.5 and 4.24.1 and confirm
      or replace the `!9778` attribution. Record the figures and the
      attribution in the design.
- [x] 1.3 Review the dependents before restating:
      - `classify_child_scroll_in_frame`, `classify_child_scroll` and the
        bin's re-announcement (`viewport_slice_bin/imp.rs`);
      - the Kani slice loop's A7/A20 `kani::assume` clauses
        (`crates/gtk-lush/widgets/src/kani_proofs.rs`);
      - the consumer tests named in the A7/A20 rows;
      - for A32: the minimap retirement (`ui/editor_page/minimap/retirement.rs`),
        `document_scale::test_minimap_retirement_after_scrolling_emits_no_critical`,
        and the A45 probe.

      Write down per dependent whether it is still needed on 4.22, a no-op on
      4.24, or affected (design D4, D5). Check each 4.24 figure from task 1.2
      against every A7/A20 `kani::assume` in the slice loop: `stray_anchor`
      in `[0, content]`, `stray_value` in `[-MAX_CONTENT, 2·MAX_CONTENT]`,
      `view_anchors[0]` in `[0, page]`, and `view_anchors[0] == 0` for an
      upward request. Record whether the model must be widened.
- [x] 1.4 Install `org.gnome.Sdk//51` if missing
      (`flatpak install --user flathub org.gnome.Sdk//51`; local, approved).
      Then run `make gtk-axioms-runtimes GTK_AXIOMS_RUNTIMES="org.gnome.Sdk//50 org.gnome.Sdk//51"`
      for the per-SDK verdicts, so the floor's A7/A20/A32 verdicts are
      measured, not assumed.
- [x] 1.5 Restate A7, A20 and A32 in the ledger as version-qualified
      (design D3), with attribution and both "Verified against" runs.
      Revise the A45 row's and A32 consumers' wording for 5.22.
- [x] 1.6 Add a version helper to `gtk-lush-axioms`, unit-tested on version
      tuples: the running GTK and GtkSourceView version compared against a
      catalogue-named bound. Then make the A7, A20 and A32 probes and
      samples assert the range they run on, and print it in the observation.
      `make check-gtk-axioms` must still pass. Extend
      `scripts/check-gtk-axioms.py` (with a self-test case) so a
      version-qualified catalogue entry must name its bound, and its ledger
      row must state both ranges.
- [x] 1.7 Verify the restated probes on both toolkits:
      - `make gtk-axioms` on F45: the full lane, every sample included;
      - the A7, A20 and A32 samples via `make gtk-axioms-runtimes` against
        `org.gnome.Sdk//50`.

      Then copy GTK 4.24.1 / Adw 1.10.0 (/ Pango 1.58.2 / GtkSourceView
      5.22.0 for the text rows) into "Verified against" for **every** probed
      row, with "Fedora 45 toolbox host, `make gtk-axioms`, <date>".
- [x] 1.8 Update the citations in `kani_proofs.rs` and `scroll_request.rs`
      to the version-qualified A7/A20 (design D4).
      - If task 1.3 found no assumption to widen, change comments only, and
        rerun the pull-request-gated shard once:
        `KANI_SHARD=widgets-geometry scripts/heavy.sh -- make kani`.
      - If an assumption must be widened, widen the model; never narrow it.
        Then rerun, one at a time under `scripts/heavy.sh`, every shard that
        runs the slice loop: `widgets-slice-loop-rest`, `-rest-three`,
        `-requests`, `-requests-two`, `-pairs` and `-unbounded`, plus
        `widgets-geometry`. Any shard that exceeds its recorded
        `ci_minutes`/`ci_peak_gib` margins is re-budgeted in
        `scripts/kani-shards.py` (`make check-kani-shards`).

      Record the wall time and peak of each run.

## 2. Packaging, toolchain and CI

- [x] 2.1 Re-query `rust-stable//26.08`'s version (design D6). Set the pin
      to it in:
      - `rust-toolchain.toml`;
      - all `dtolnay/rust-toolchain@` sites (19);
      - `snap/snapcraft.yaml`'s rustup bootstrap;
      - `scripts/lint-advisory-policy.toml`.

      Install it with rustup if missing. Keep `rust-version = "1.98.0"`.
      Then run `make lint-advisory` and classify any new category the
      policy file requires.
- [x] 2.2 In the Flatpak manifest, set `runtime-version` 50 → 51 (the SDK
      follows). Update `scripts/test-release.sh`'s manifest fixture and any
      verifier expectations. `cargo-sources.json` changes only if
      `Cargo.lock` does, and it should not.
- [x] 2.3 CI:
      - confirm that `registry.fedoraproject.org/fedora:45` resolves
        (`skopeo inspect` or `podman pull`);
      - every `fedora:44` container image (28) → `fedora:45`, and every
        `shared-key: fedora-44*` cache key (21) → `fedora-45*`;
      - `flatpak-github-actions:gnome-50` → `gnome-51`, with
        `flatpak-builder-gnome-50-*` → `-gnome-51-*`;
      - the existing `gtk-axioms` job keeps its id and its check name
        `GTK Axioms` and moves to `fedora:45`. A new job `gtk-axioms-floor`
        (`GTK Axioms (GNOME 50 floor)`) runs the same steps on `fedora:44`
        with its own `fedora-44` cache key. Not a matrix, so the existing
        check name keeps reporting (design D2);
      - the `snap.yml` comments stay on GNOME 50.

      Run `scripts/check-workflow-timeouts.py`,
      `make check-end-user-smoke-workflow`, and any workflow policy check in
      `make check-policy`.
- [x] 2.4 Builder-diagnostics runtime (design D2):
      - `build-aux/builder-diagnostics/Containerfile` and
        `builder-diagnostics-runtime.yml`: `fedora:45`, `GTK_REF=4.24.1`,
        `LIBADWAITA_REF=1.10.0`, `GTKSOURCEVIEW_REF=5.22.0`, a
        blueprint-compiler ref checked against the Fedora 45 toolbox, tag
        `gnome-51-debug`;
      - consumers: `end-user-smoke.yml`, the default in
        `scripts/run-builder-diagnostics.sh`, and
        `docs/blueprint-validation.md`.

      Build the image locally with `podman` (under `scripts/heavy.sh`), then
      run `make builder-diagnostics-smoke` with
      `LUSHTEXT_BUILDER_DIAGNOSTICS_IMAGE` set to the local tag. Any new
      builder diagnostic on Libadwaita 1.10 is a task 3 regression. List the
      post-merge `workflow_dispatch` of `builder-diagnostics-runtime.yml` in
      the handoff as a maintainer action.
- [x] 2.5 `scripts/run-gtk-axioms.sh runtimes` defaults to
      `org.gnome.Sdk//50 org.gnome.Sdk//51 org.gnome.Sdk//master`. Update
      the usage text and the Makefile comment.

## 3. Toolkit regressions on GTK 4.24 (failing first, fixed at the cause)

- [x] 3.1 W1, the `GtkSourceMapSlider` snapshot without allocation:
      1. Find the smallest LushText test that emits it, by running a single
         test with `make test-widget-shard` filters or the harness's
         `--exact`.
      2. Isolate the toolkit behaviour as a new `gtk-lush-axioms` probe and
         sample on a bare `GtkSourceMap`, with a ledger row. Run it on both
         toolkits: does it occur on 4.22?
      3. Fix the minimap lifecycle (show/hide, retire, attach, reflow) so
         the slider is allocated whenever it is snapshotted. Add a widget
         test that fails on the warning before the fix.
      4. If the cause is a GtkSourceView/GTK defect, check upstream with the
         `upstream-gtk-reports` skill and record it in
         `docs/upstream-reports.md`. Draft only; nothing is filed without
         the maintainer's approval.
      5. Rerun the minimap Kani shards (`core-minimap-fit`,
         `core-minimap-binning`) only if minimap policy code changed.
- [x] 3.2 W2, the `GtkListView` measured for height 0 below its minimum
      (`workspace_section::*`):
      1. Attribute it to the exact measure call (bin `measure`/`size_allocate`
         versus a hidden or empty section).
      2. Probe the list's 4.24 minimum-height contract (candidate GTK
         `!9906`) next to A2/A3/A5, and add a ledger row if it is new.
      3. Fix the bin or section at the cause, failing first.
      4. If `slice_geometry.rs` or `scroll_request.rs` changes, rerun
         `widgets-geometry` and the six `widgets-slice-loop-*` shards, one at
         a time (task 1.8).
      5. Run the `workspace-tree-reveal` visual-geometry scenario.
- [x] 3.3 W3, `AdwEntryRow placed into AdwExpanderRow` (Libadwaita 1.10):
      1. Write a widget test that constructs Preferences and fails on the
         warning.
      2. Add the entry row through the API Libadwaita accepts, keeping the
         `excluded_names.rs` removal contract.
      3. Run `make check-blueprint` / `make blueprint-generate` if
         `preferences.blp` changes.
      4. Re-check `ui.md` l. 363's 4.22/1.9 warning note on 1.10.
- [x] 3.4 `FLAKY`
      `minimap_markers::test_minimap_replace_all_of_many_matches_keeps_one_pending_refresh`:
      1. Read the first attempt's panic and classify the wait.
      2. Rerun it in isolation 10 times on 4.24.
      3. Fix the predicate, the budget or the race; do not tolerate the
         retry.
- [x] 3.5 Lane regressions L1–L3 (design B3, D7b), in this order:
      1. **L1, typed text drawn late.** Reproduce `baseline/small` typing in
         isolation (several repetitions; minimap on and off). Read the
         probe's frame-clock records against the key handlers to tell "no
         frame requested" from "frame presented late". Add a
         `gtk-lush-axioms` probe on a bare `GtkTextView` for GTK's side,
         with a ledger row if the behaviour is new. Fix at the cause,
         failing first: in LushText (frame validation, minimap slider,
         held typing) or in the lane driver on mutter 51, with a self-test.
         Never loosen the `withheld_frame` rule.
      2. **L2, typing reached no document.** Use the `paragraph/10000/*/html`
         `type-start` delivery records: click point, focus, or held input.
         Fix the cause.
      3. **L3, `baseline/empty` exits −9 after `no-editor-point`.** Run it on
         `main` in the `hypatia` F44 toolbox, headless, to decide whether it
         predates 4.24. Fix it either way.
      4. Rerun the quick tier on the branch and confirm 0 withheld, 0
         misdirected and 0 failed cases before task 6.
      Done 2026-11-10: L1 by A105 (frame-start attribution); L2 by A107 (a
      logical step for long left-to-right paragraphs, failing first); L3 a
      driver defect (no-document phases; the lane now fails on failed cases).
      Step 4 is read from task 6's branch runs (design, "Task 3.5 decisions").
- [x] 3.6 Any other regression the gate in task 4 finds is handled the same
      way (pre-existing-blockers rule).

## 4. Full local gate on Fedora 45

- [x] 4.1 `make check` passes, including all-feature Clippy under the new
      pin, and `cargo check -p lushtext --bins --locked`.
- [x] 4.2 `make test` passes with 0 `FLAKY`, 0 `FLAKY-SHARD` and 0
      unexpected warning lines. Record per-shard minutes and peaks. The
      local timing does not feed `ci_minutes`.
- [x] 4.3 `make gtk-axioms` passes in full on F45. The A7/A20/A32 floor
      verdicts pass via `make gtk-axioms-runtimes` against
      `org.gnome.Sdk//50`.
- [x] 4.4 Kani: the shards task 1.8 selected, plus any shard covering code
      changed in task 3, one at a time under `scripts/heavy.sh`.
- [x] 4.5 Run the smokes and keep their artifacts:
      - `make visual-geometry-smoke`, which includes `workspace-tree-reveal`;
      - `make accessibility-smoke`;
      - `make visual-smoke`.

      Investigate any new warning or failure.
- [x] 4.6 `make flatpak` on GNOME 51, then `make verify-flatpak-identity`
      (installing into the user Flatpak installation as that target
      requires). Record the build time and the `flatpak info` runtime line.

## 5. Docs (per `.agents/rules/documentation.md`)

- [x] 5.1 README and AGENTS:
      - the tech stack: GNOME 51 runtime (GTK 4.24, GtkSourceView 5.22,
        Libadwaita 1.10), GNOME 50 source floor, the Rust pin;
      - the CI description;
      - the Recent Changes entry.
- [x] 5.2 `.agents/rules/build.md`:
      - Flatpak runtime 51 and rust-stable `26.08`;
      - the rust-version paragraph (l. 622);
      - CI containers and the `GTK Axioms` matrix;
      - `gtk-axioms-runtimes` defaults;
      - the toolkit-update procedure naming the shipped runtime and the
        floor.

      Also update `rust.md`'s platform-floor line, `ui.md` (task 3.3), the
      `gtk4-libadwaita-internals` skill's version-family lines, and its
      ledger preamble ("The GNOME 50 platform floor…" plus the shipped
      runtime).
- [x] 5.3 `docs/upstream-reports.md`:
      - what the 51 runtime changes: A32 is fixed in the shipped runtime,
        and the workaround stays for the floor;
      - W1 or W2 if upstream;
      - every other row's "Not in the GNOME 50 or GNOME 51 runtimes" note
        re-checked;
      - "Last checked".
- [x] 5.4 `docs/next/long-line-documents.md`: the queue entry, and an
      implementation record with the measured verdicts. In
      `docs/next/formal-verification.md`, the A7/A20 envelope note.
- [x] 5.5 `make check-agent-docs`, `make check-policy` and
      `openspec validate adopt-gnome-51 --strict` pass.

## 6. Measurement (after the gate is green)

- [x] 6.1 Build three `responsiveness-probes` binaries and keep them in
      `bin/`:
      - `main` (base revision, Rust 1.98.1);
      - `main` built with the new pin (compiler attribution only);
      - branch (pinned compiler).
- [x] 6.2 Run the quick tier interleaved, main → branch → main → branch,
      with every case. Use `scripts/measure.sh` and `taskset -c 8-23`,
      waiting for load < 4, and record the load before and after each run
      in `runs.log`. Take each branch-versus-main verdict on GTK 4.24. The
      branch must also pass outright: 0 withheld, 0 misdirected and 0 failed
      cases. Then run the compiler-attribution pair, interleaved in the same
      way: `main`@1.98.1 against `main`@new pin, on the headline cases.
      The branch-versus-main pairs mix the compiler change with the toolkit
      fixes, so they cannot attribute a compiler effect.
- [x] 6.3 Build a context table of GTK 4.22 (`main` on F44, from the
      archived `bin-minimap-markers-per-row` and `power-of-two-thresholds`
      records) versus 4.24 (task 6.2's `main` runs) for the headline cases.
      It is context only, never pass/fail.
- [x] 6.4 Any case over any lane budget (`B_interact`, `B_settle`,
      input-to-frame, scroll p95 frame gap) that is not already a recorded
      exception gets one of two outcomes:
      - fixed in this change;
      - recorded as an explicit, measured exception, with cause and
        attribution, in the design and the programme record.

      Done 2026-11-11 (design, "Measured verdict"): the branch passes the
      lane in both runs (0 withheld, 0 misdirected, 0 failed cases; `main`
      97/93 withheld, 36 misdirected); the compiler pair is context only
      (maintainer, 2026-11-11) and shows no attributable effect; every
      over-budget figure is recorded as E1–E6, none fixed in task 6.

## 7. Stage-4 review (2026-11-11)

- [x] 7.1 Fix R1 failing-first: the logical arrow step also requires Pango's
      base direction to be left to right
      (`editor_page::test_arrow_steps_in_a_long_paragraph_with_no_left_to_right_text_stay_visual`;
      A107 and the `long-paragraph-editing` delta restated).
- [x] 7.2 Measure E1's attribution (find-only interleaved run) and record the
      open decisions (design, "Stage-4 review"; programme record AG).
