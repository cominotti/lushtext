## 1. K1 — Kani lane

- [x] 1.1 Declare `cfg(kani)` as an expected cfg in the workspace lints, and
  confirm that `make check` (all-feature Clippy) stays clean.
- [x] 1.2 Add `#[cfg(kani)] mod kani_proofs` to `gtk-lush-widgets`, porting
  the seven spike harnesses (appendix A of
  `docs/next/formal-verification-evolution.md`):
  - no panic for any `f64`;
  - containment on whole pixels;
  - rest never requests;
  - requests are at least ε;
  - exact landing on whole pixels;
  - the two general-`f64` harnesses that must fail, as documented
    counterexamples (`should_panic`) or removed in favour of the domain
    statement, with the choice recorded.
- [x] 1.3 State the whole-pixel domain in the rustdoc of `viewport_slice`,
  `classify_child_scroll` and `outer_scroll_request`. The domain-restricted
  harnesses must match that statement.
- [x] 1.4 Add `make kani` with the Kani version pinned in one Makefile
  variable, and document it in AGENTS.md Build Commands and in
  `.agents/rules/build.md`.
- [x] 1.5 Add a `.github/workflows/kani.yml` workflow:
  - schedule plus `workflow_dispatch`;
  - Fedora 44 container;
  - `cargo install --locked kani-verifier --version <pinned>` followed by
    `cargo kani setup`;
  - `timeout-minutes: 30`.
  Then pass `check-workflow-timeouts.py`.
- [x] 1.6 Update the GTK Lush widgets CHANGELOG, README and governance
  evidence, then run `make check-gtk-lush-policy check-gtk-lush-adoption
  gtk-lush-doctests gtk-lush-examples`.
- [x] 1.7 Run `make kani` and record per-harness results and times in
  `docs/next/formal-verification.md`.

## 2. K2 — GTK axiom ledger

- [x] 2.1 Create
  `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
  and move the A1–A13 table there, with id, statement, dependent designs, and
  pinning status. Link it from the programme record and the skill's index.
- [x] 2.2 Add `crates/lushtext/tests/widget/gtk_axioms.rs` with isolated
  probes for A5, A9, A11 and A13. Each uses a minimal pure-GTK fixture and
  cites its axiom id. Register the module in the widget harness.
- [x] 2.3 Record the phase-0 A8 evidence, and for every other axiom record
  its existing pinning test, or a reason it cannot be pinned.
- [x] 2.4 Run `make test-widget`, headless, with every probe green and no
  `FLAKY` line.

## 3. K3 — Draft journal decision core

- [x] 3.1 Resolve the design's open question (policy.rs or a service-level
  home) against the workflow role rules, and record the decision in design.md
  and the matrix row.
- [x] 3.2 Characterization first. List every current journal decision site
  (restore hold, preserve call sites, overwrite copy, registration,
  deletion ordering, cleanup gate), and make sure each one has an existing or
  new test that pins today's behaviour.
- [x] 3.3 Implement the pure core, with fixed-size-friendly state. It
  provides `decide` and one `ownership` check. Route every decision site
  through it.
- [x] 3.4 Make "no body without an entry" unrepresentable: the body write
  takes the registration token. Route insert-only commits through
  `update_manifest`. Give stale bodies one owner, with set-aside owning the
  bytes and local history as a view.
- [x] 3.5 Add Kani harnesses for S1–S4 and a bounded L1 over 3 ids, 3
  generations and 8 steps, including Crash and Startup. Record the bounds and
  the run time.
- [x] 3.6 Run the full suite, `make test-prop`, and the data-safety explicit
  audit over the refactor. Update `docs/workflow-readability-matrix.md`,
  re-deriving the measured cells, and run `make check-workflow-boundaries`.

## 4. K4 — ViewportSliceBin closed loop in Kani

- [x] 4.1 Implement the `cfg(kani)` step model in `gtk-lush-widgets`. It
  calls the real `viewport_slice` and `classify_child_scroll`, and restricts
  the child with `kani::assume` clauses that each cite a ledger axiom id.
- [x] 4.2 Add harnesses for N ∈ {1, 2, 3} bins and k ≤ 4 allocations,
  covering: fixed point at rest, no oscillation, request fidelity, bounded
  liveness, and render fidelity.
- [x] 4.3 Decide the learning-frame residual. If there is a counterexample,
  write the failing widget test first, then the fix, and keep every
  rendered-bounds, allocation-count and correction-count test green. If there
  is none, record the proof.
- [x] 4.4 Record the results and any envelope assumptions in the programme
  record.

## 5. K5 — Deterministic kill points in the crash-recovery smoke

- [x] 5.1 Add a `crash-kill-points` Cargo feature and a kill-point helper
  that reads `LUSHTEXT_KILL_AT` and calls `std::process::abort`. Call it at
  the `draft-body-before-commit`, `durable-renamed-before-dirsync` and
  `stale-preserved-before-retire` windows.
- [x] 5.2 Extend the smoke driver with one scenario per window. Each scenario
  relaunches the app, asserts that no work is lost, and preserves the
  artifacts.
- [x] 5.3 Verify that the release and Meson/Flatpak builds keep the feature
  disabled and contain no kill-point code. Update `docs/end-user-coverage.md`
  and the crash-smoke docs.
- [x] 5.4 Run `make crash-recovery-smoke`: every scenario must pass.

## 6. K7 — Base cleanups (before K6)

- [x] 6.1 Split the durable copy and move primitives into `copy_durable`,
  which keeps the source, and `move_durable`, which removes it after the
  destination is durable. Retire `copy_file_durable`, migrate every caller,
  and check each migration against whether that caller intended a copy or a
  move. Write the tests first.
- [x] 6.2 Give the temp-name format one owner, shared by the builder and the
  sweep predicate. Make `WriteLabel` construction closed. Update the
  predicate's known-tag set from that single owner.
- [x] 6.3 Extend the startup leftover sweep to `style-schemes/`,
  `format-upgrade-backups/` and `drafts/set-aside/`. Write the tests first.
- [x] 6.4 Add the set-aside group on `Preferences > Data`. Each row shows the
  path and time, with Open (new untitled tab) and Delete (confirmed). The
  group is hidden when empty. Add widget tests, and update the accessibility
  docs and matrix, the automation docs if actions are exported, and the
  README.

## 7. K6 — durable_write I/O-free core

- [x] 7.1 Characterization first. Confirm that the existing durable-write
  unit and fault-injection tests pin every classification, ordering and
  metadata behaviour, and add any that are missing.
- [x] 7.2 Implement `services/filesystem/write_protocol.rs`, with
  `step(state, outcome) -> (state, Action)`. Reduce `durable_write.rs` to the
  shell loop, one backend call per action. Streaming closures stay in the
  shell.
- [x] 7.3 Retarget the ported protocol harnesses (appendix B) to the real
  core, keeping the `cfg(kani)` disk abstraction. Prove crash atomicity,
  classification soundness and mode non-widening, and keep the
  skip-temp-sync `should_panic`.
- [x] 7.4 Run the full suite and the data-safety explicit audit. Update
  AGENTS.md "Async save" and the durable-write design text.

## 8. K8 — Drop axiom A6

- [x] 8.1 Add a second writer actor to the K3 harness, and record which
  invariants fail, with their counterexamples.
- [x] 8.2 Record the decision in the programme record: accept with
  documentation, or open a follow-up change for an inter-process
  data-directory lock.

## 9. Migration and cleanup

- [x] 9.1 Remove the remaining Lean and Quint plan text from `docs/`,
  keeping only the dormant-Lean note in the programme record. Mark each K
  candidate done in the evolution record.
- [x] 9.2 Uninstall the spike-only Lean toolchain (`elan self uninstall`) and
  confirm that `~/.elan` is gone.
- [x] 9.3 For each `.claude/worktrees/agent-*` worktree, verify file by file
  that its content is either identical to `main` or an ancestor that `main`
  has evolved past. Remove only the worktrees that pass, with `git worktree
  remove` (unlocking first where needed). Report anything else rather than
  deleting it.

## 10. Verification and sign-off

- [x] 10.1 Run `make check`, `make test` (headless widgets included),
  `make test-prop`, `make kani`, and the GTK Lush targets. All must pass,
  with no `FLAKY` lines.
- [x] 10.2 Run `make crash-recovery-smoke`, `make accessibility-smoke` and
  `make visual-smoke`, whichever the UI changes require.
- [x] 10.3 Update `docs/next/formal-verification.md` with the posture of each
  phase and every proof result, and README.md if user-visible behaviour
  changed (the Data page).
