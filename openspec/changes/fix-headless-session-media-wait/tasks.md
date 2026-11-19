## Rules for every task

- Inherit the toolbox cpuset; never pin with `taskset`. Only timing verdicts
  pin, and this change owes none: lane wall time is evidence, not a verdict.
- Run every heavy non-measurement command through
  `scripts/heavy.sh [--mem <cap>] -- <cmd>`, one command per call. That
  covers cargo build, check, clippy, test and doc, `make check` and
  `make test` (as `heavy.sh --envelope 16G -- make test`), each smoke lane,
  each helper session started by hand, and `git commit` with its hook
  (`heavy.sh --mem none -- git commit -F <file>`). `heavy.sh` sets
  `CARGO_BUILD_JOBS` and the rustc front-end threads; never override them.
- Before the first heavy command, take an implementation slot. While
  `MEASURE-QUEUE.md` holds a READY verdict campaign, at most **3** slots may
  be in use (maintainer, 2026-11-18); otherwise 5. Wait in the foreground with
  the documented bounded loop, and release the slot on every exit
  (`parallel-agent-work.md`). Read `COORDINATOR-NOTE.txt` in
  `.claude/handoffs/fix-headless-session-media-wait/` at the start and before
  each heavy command group, and honour any quiet window it names.
- Record `/proc/loadavg` before and after every lane run and every timed
  session, in
  `.claude/handoffs/fix-headless-session-media-wait/runs.log`.
- Media sessions started by hand get private
  `XDG_{CONFIG,STATE,DATA,CACHE}_HOME` directories in the scratchpad or the
  handoff directory, never the real home, because the helper refuses media
  without them. Headless only: never `make run` or the live desktop. Never
  `pgrep -f` or `find /`.
- A failing-first test is run and seen failing on the branch before its fix,
  and logged in `.claude/handoffs/fix-headless-session-media-wait/failing-first.log`.
- The spec-stage probe scripts (`probe.sh`, `capture-probe.py` in the
  handoff `baseline/` directory) stay untracked.
- Kani: **none** (no proven module touched). GTK axioms: **none** (no
  GTK-family behaviour relied on; design "Scope boundaries").

## Commit plan

- **G1:** helper code, unit tests and the GTK Lush and rule docs (tasks 1-5.4).
  No path of this change is under the UI prefixes or in the smoke proofs'
  fingerprint lists (`scripts/accessibility_source_fingerprint.py`,
  `scripts/check-visual-proof-policy.py`; checked at spec stage), so the
  commit hook owes no smoke. If an implementation fix does touch such a path,
  that commit lands right after the task 6 smokes.
- **G2:** evidence and the records (tasks 5.5, 6, 7), docs only.

## 1. Failing-first tests (no behaviour change yet)

- [x] 1.1 Fixtures in `crates/gtk-lush/proof-harness/src/session/observe.rs`
  tests, cut down from real saved dumps:
  - (a) a WirePlumber `policy` client whose `session.services` lists
    `policy.linking.standard` last;
  - (b) the same, member first;
  - (c) the same without `session.services`;
  - (d) a `session.services` without that member;
  - (e) the member `policy.linking.standard-extra` only;
  - (f) empty or malformed text.

  Assert `wireplumber_policy_ready` holds for (a) and (b) only. Add a test
  that the current predicate (`contains("\"metadata.name\": \"default\"")`)
  is false on (a) and (b), which documents the defect. Declare the function
  as a stub returning `false`, so the tests compile and fail.
- [x] 1.2 A unit test of the wait helper (design D3) with a fake dump source
  and a deadline of a few milliseconds:
  - it returns `Ok` when the source becomes ready on its third call;
  - it returns `Err` carrying the last dump when the source never becomes
    ready;
  - it stops polling at the deadline.

  It fails against a stub.
- [x] 1.3 Run the new tests (`heavy.sh -- cargo test -p
  gtk-lush-proof-harness --lib session::`) and log the failures.

## 2. The readiness wait

- [x] 2.1 Implement `session::observe::wireplumber_policy_ready` (D2): scan
  by key, parse the bracketed list, trim each member, compare whole members.
  Write rustdoc that names WirePlumber's `session-services.lua` ordering as
  the reason and says that `pw-dump` omits metadata objects without entries.
- [x] 2.2 Implement the wait helper. Replace the ignored wait in
  `run_inner_body` (D3):
  - on success, start the command;
  - on a miss, write `pw-dump-not-ready.json` to the log directory, print
    the bounded line, and return `TEST_FAILURE_EXIT_CODE`.

  Keep `READY_TIMEOUT` at 10 s (D5). Rewrite the comment so it states the
  real signal and why the wait exists.
- [x] 2.3 Tests 1.1-1.2 pass. Run `heavy.sh -- cargo test -p
  gtk-lush-proof-harness` (all of it) and
  `heavy.sh -- cargo clippy -p gtk-lush-proof-harness --all-targets --all-features -- -D warnings`.
- [x] 2.4 Real sessions, by hand. Time 3 alternating quiet/off pairs of
  `scripts/headless-session session --profile screencast [--media off] -- true`
  (baseline §1: 11.0-11.4 s vs 0.8-1.1 s). Record the times and the load.
  Expected: quiet within about 1 s of off.
- [x] 2.5 `--expect-dirty` (D6): run it by hand three times, each in a
  throwaway private runtime. Each run must report all four classes
  (`rtkit`, `bluez`, `hardware-device`, `module-rt`) and exit 0 with the
  mode's "the bare configuration was caught" line. If any run misses a
  class, apply D6's fallback (in `--expect-dirty` only, keep the bare
  daemons up for the rest of `READY_TIMEOUT` after readiness), add it to
  the CHANGELOG entry (5.1), and repeat the three runs. Record the results.
- [x] 2.6 First-try link proof (D8): rerun the spec stage's untracked
  `capture-probe.py` 10 times on the final helper, without the experiment
  patch, as `gtk-lush-session run --profile screencast --monitor 640x480
  --log-dir <dir> -- python3 capture-probe.py` (`evidence/baseline.md` §4).
  Every run must exit 0, link and deliver its first buffer on its first
  `pipewiresrc` attempt within 1 s (baseline 0.14-0.18 s), write a
  640 x 480 PNG, and keep a clean guard verdict. Record the figures and the
  load in `evidence/lanes.md`. Rerun it after any later helper change.

## 3. Seeded real-session failure (D7.1, uncommitted)

- [x] 3.1 Temporarily add
  `wireplumber.profiles = { policy = { policy.standard = disabled } }` to
  `crates/gtk-lush/proof-harness/src/session/wireplumber-quiet.conf` and
  rebuild the helper. Then confirm that
  `scripts/headless-session session --profile screencast --log-dir <dir> -- true`
  (`--log-dir` is required: without it the helper removes its log directory
  at teardown, design D3):
  - exits 101 with the D3 line after about 10 s;
  - leaves `<dir>/pw-dump-not-ready.json`, whose `session.services` value
    exists and lacks `policy.linking.standard` (with the
    `policy.standard = disabled` seed) or is absent (with the fallback
    seed). No probe can run inside such a session, since the command never
    starts, so this saved dump is the evidence;
  - leaves no daemon running.

  Also run the same command once without `--log-dir` and confirm the D3
  line ends with `rerun with --log-dir to keep the last pw-dump`.

  If WirePlumber refuses the override, use
  `support.session-services = disabled` instead and record which seed was
  used.
- [x] 3.2 With the seed still in place, run one case of each media lane
  through its runner, with the flags its make recipe passes plus a case
  filter, and **always** with an artifact directory of its own under
  `.claude/handoffs/fix-headless-session-media-wait/seeded/` (the make
  targets take no case filter, and a seeded run must never leave a lane
  summary under `build/smoke/`):
  - visual: `scripts/run-visual-smoke.sh --case <one case> --artifact-dir
    <seeded>/visual` (plus `--read-fault-binary` as the recipe passes it);
  - accessibility: `scripts/run-accessibility-smoke.sh --case <one case>
    --artifact-dir <seeded>/accessibility` (same);
  - visual-geometry: `cargo run -q -p cargo-gtk-proof -- run --case-filter
    <one scenario> --artifact-dir <seeded>/visual-geometry --scenario-dir
    scripts/visual-geometry-scenarios --binary "$PWD/target/debug/lushtext"
    --composition-hold-binary "$PWD/target/composition-hold/debug/lushtext"`;
  - command-palette notes: `scripts/run-command-palette-notes-smoke.sh
    --artifact-dir <seeded>/command-palette-notes`, the second runner with
    the skip grep.

  Each through `heavy.sh`, under the recipe's `scripts/with-temp-scope.sh`
  wrapper. Each must fail: no `SKIP:`, a nonzero exit, and the D3 line in
  the case's session log (for visual-geometry, no `session-retry.json`).
- [x] 3.3 Revert the seed (`git diff` empty for the fragment) and rebuild.
  Log 3.1-3.2 in `failing-first.log`.

## 4. The WirePlumber miss is never a skip (D4)

- [x] 4.1 Grep the D3 line against the host-support skip patterns of every
  media lane's runner and capture script. At spec stage that meant
  `scripts/run-visual-smoke.sh` and `scripts/run-command-palette-notes-smoke.sh`
  (patterns); `crates/cargo-gtk-proof/src/live.rs`, the runner of
  `make visual-geometry-smoke` (`session_status`, and the AT-SPI startup
  retry texts of `case_session_needs_atspi_startup_retry`); and
  `scripts/run-accessibility-smoke.sh`, `scripts/crash-recovery-smoke-driver.py`,
  `scripts/visual-geometry-smoke.py` (the Python oracle) and
  `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py`,
  which have pre-session checks only and pass the session's status on. It
  must match none, in each of the D3 line's four `<where>` forms (the
  fourth, a failed save, was added at implementation: design D3).
- [x] 4.2 `make check-session-hygiene` and `make check-agent-docs` pass
  (each through `heavy.sh`).
- [x] 4.3 Confirm the `NEW CHANGE PROPOSED: media-skip-classification` line
  (written at spec stage) is still in `.claude/handoffs/PARALLEL-LEDGER.md`.
  It covers the deferred `PipeWire did not become ready` skip class and the
  two uncalled `wait_for_pipewire` helpers. Do not widen this change with
  them.

## 5. Documentation (`.agents/rules/documentation.md`)

- [x] 5.1 `crates/gtk-lush/proof-harness/CHANGELOG.md`, Unreleased:
  - quiet-media readiness now waits for `session.services ∋
    policy.linking.standard`;
  - the old predicate could never hold, which cost 10 s per media session
    since the helper's first lanes;
  - a miss fails the session with exit 101 and `pw-dump-not-ready.json`;
  - the new `observe::wireplumber_policy_ready`.
- [x] 5.2 `crates/gtk-lush/proof-harness/README.md`, the quiet-media
  paragraph: the readiness signal, and the failure on a miss.
- [x] 5.3 `.agents/rules/build.md`, "PipeWire and WirePlumber updates": on a
  WirePlumber update, also re-check that `session-services.lua` still runs
  last and still lists `policy.linking.standard`. A changed name surfaces as
  the D3 failure in the first screencast session.
- [x] 5.4 `docs/next/gtk-lush.md`, the proof-harness paragraph: one sentence
  on the readiness signal. Review `crates/gtk-lush/GOVERNANCE.md`: no
  constitution exception, so nothing to record. Run
  `make gtk-lush-api-advisory` and confirm that the only public-API
  difference is the additive `observe::wireplumber_policy_ready`.
- [ ] 5.5 `docs/next/long-line-documents.md`, QH section: a follow-up
  paragraph with the cause, the fix and the lane wall times before and after
  (task 6), from `evidence/`.

## 6. Lane proof on the final tree (D8)

- [ ] 6.1 Seeded defect (D7.2, uncommitted), **before** the green runs: a
  temporary change inside a protected region of one visual-geometry scenario
  must make `make visual-geometry-smoke` fail its invariant. A wrong
  accessible name on one anchor must make `make accessibility-smoke` fail.
  Revert both, and log them in `failing-first.log`.
- [ ] 6.2 After G1 is final, run each media lane 10 consecutive times,
  green, one `heavy.sh` call per run:
  - `make visual-geometry-smoke`;
  - `make accessibility-smoke`;
  - `make visual-smoke`.

  Record each run's wall time and the load before and after in
  `evidence/lanes.md`. Then run `make crash-recovery-smoke` and
  `make command-palette-notes-smoke` once each, green, on the same tree
  (D8), and record them the same way. A failure, `FLAKY`, `OOM-KILL:`, `SESSION-HYGIENE:` or
  `SKIP:` line is a blocker: investigate it, fix the cause, and restart that
  lane's count.
- [ ] 6.3 Compare the means with the baseline (visual-geometry 24.2,
  accessibility 13.0, visual 9.8 min). If the visual lane stays clearly above
  about 3 min at a comparable load, write a `NEW CHANGE PROPOSED` line for
  the remainder in `.claude/handoffs/PARALLEL-LEDGER.md` and say so in
  `evidence/lanes.md`.
- [ ] 6.4 Before the G2 commit and the merge, the last run of each lane
  under `build/smoke/` is a green one, on the final helper (seeded runs
  live under the handoff directory, task 3.2).

## 7. Final gates

- [ ] 7.1 `make fmt`, `make check` (through `heavy.sh`) and the default-feature
  `cargo check -p lushtext --bins --locked`.
- [x] 7.2 The rustdoc gate (`testing.md`), because `observe.rs` gains public
  rustdoc.
- [ ] 7.3 `openspec validate fix-headless-session-media-wait --strict`.
- [ ] 7.4 Push per task group. The merge goes through the MERGE-LOCK
  procedure (`parallel-agent-work.md`), on local evidence.
