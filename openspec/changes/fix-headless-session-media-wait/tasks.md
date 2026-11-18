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

- **G1:** helper code, unit tests and the GTK Lush docs (tasks 1-3, 5.1-5.4).
  None of these paths is fingerprinted by the smoke proofs.
- **G2:** the runner and capture-script edits (task 4). Those paths are
  fingerprinted (`scripts/accessibility_source_fingerprint.py`,
  `scripts/check-visual-proof-policy.py`). G2 is also the commit the smokes
  of task 6 are made for, so it lands right after them, and any edit after a
  smoke means rerunning it.
- **G3:** evidence and the records (tasks 5.5-5.6, 7), docs only.

## 1. Failing-first tests (no behaviour change yet)

- [ ] 1.1 Fixtures in `crates/gtk-lush/proof-harness/src/session/observe.rs`
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
- [ ] 1.2 A unit test of the wait helper (design D3) with a fake dump source
  and a deadline of a few milliseconds:
  - it returns `Ok` when the source becomes ready on its third call;
  - it returns `Err` carrying the last dump when the source never becomes
    ready;
  - it stops polling at the deadline.

  It fails against a stub.
- [ ] 1.3 Run the new tests (`heavy.sh -- cargo test -p
  gtk-lush-proof-harness --lib session::`) and log the failures.

## 2. The readiness wait

- [ ] 2.1 Implement `session::observe::wireplumber_policy_ready` (D2): scan
  by key, parse the bracketed list, trim each member, compare whole members.
  Write rustdoc that names WirePlumber's `session-services.lua` ordering as
  the reason and says that `pw-dump` omits metadata objects without entries.
- [ ] 2.2 Implement the wait helper. Replace the ignored wait in
  `run_inner_body` (D3):
  - on success, start the command;
  - on a miss, write `pw-dump-not-ready.json` to the log directory, print
    the bounded line, and return `TEST_FAILURE_EXIT_CODE`.

  Keep `READY_TIMEOUT` at 10 s (D5). Rewrite the comment so it states the
  real signal and why the wait exists.
- [ ] 2.3 Tests 1.1-1.2 pass. Run `heavy.sh -- cargo test -p
  gtk-lush-proof-harness` (all of it) and
  `heavy.sh -- cargo clippy -p gtk-lush-proof-harness --all-targets --all-features -- -D warnings`.
- [ ] 2.4 Real sessions, by hand. Time 3 alternating quiet/off pairs of
  `scripts/headless-session session --profile screencast [--media off] -- true`
  (baseline §1: 11.0-11.4 s vs 0.8-1.1 s). Record the times and the load.
  Expected: quiet within about 1 s of off.
- [ ] 2.5 `--expect-dirty` (D6): run it by hand once, in a throwaway private
  runtime, and confirm it still reaches the guard verdict (dirty
  configuration caught, the mode's success exit). Record the result.

## 3. Seeded real-session failure (D7.1, uncommitted)

- [ ] 3.1 Temporarily add
  `wireplumber.profiles = { policy = { policy.standard = disabled } }` to
  `crates/gtk-lush/proof-harness/src/session/wireplumber-quiet.conf` and
  rebuild the helper. Confirm `session.services` lacks the member (a probe
  session). Then confirm that a `screencast` session around `true`:
  - exits 101 with the D3 line after about 10 s;
  - leaves `pw-dump-not-ready.json`;
  - leaves no daemon running.

  If WirePlumber refuses the override, use
  `support.session-services = disabled` instead and record which seed was
  used.
- [ ] 3.2 With the seed still in place and task 4 applied, run one case each
  of `make visual-smoke`, `make visual-geometry-smoke` and
  `make accessibility-smoke` (filtered to one case each). Each must fail: no
  `SKIP:`, nonzero exit, the D3 line in the case log.
- [ ] 3.3 Revert the seed (`git diff` empty for the fragment) and rebuild.
  Log 3.1-3.2 in `failing-first.log`.

## 4. Lanes fail, never skip, on media readiness (D4)

- [ ] 4.1 Remove `PipeWire did not become ready` from the host-support skip
  patterns of `scripts/run-visual-smoke.sh` and
  `scripts/run-command-palette-notes-smoke.sh`. Keep `Missing required
  command` and the AT-SPI phrases.
- [ ] 4.2 Delete the uncalled `wait_for_pipewire` from
  `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py`
  and `scripts/crash-recovery-smoke-driver.py`. Grep that nothing calls them,
  and that the D3 line matches no remaining skip pattern of any runner.
- [ ] 4.3 `make check-session-hygiene`, `make check-end-user-smoke-workflow`
  and `make check-agent-docs` pass (each through `heavy.sh`).

## 5. Documentation (`.agents/rules/documentation.md`)

- [ ] 5.1 `crates/gtk-lush/proof-harness/CHANGELOG.md`, Unreleased:
  - quiet-media readiness now waits for `session.services ∋
    policy.linking.standard`;
  - the old predicate could never hold, which cost 10 s per media session
    since the helper's first lanes;
  - a miss fails the session with exit 101 and `pw-dump-not-ready.json`;
  - the new `observe::wireplumber_policy_ready`.
- [ ] 5.2 `crates/gtk-lush/proof-harness/README.md`, the quiet-media
  paragraph: the readiness signal, and the failure on a miss.
- [ ] 5.3 `.agents/rules/build.md`, "PipeWire and WirePlumber updates": on a
  WirePlumber update, also re-check that `session-services.lua` still runs
  last and still lists `policy.linking.standard`. A changed name surfaces as
  the D3 failure in the first screencast session.
- [ ] 5.4 `docs/next/gtk-lush.md`, the proof-harness paragraph: one sentence
  on the readiness signal. Review `crates/gtk-lush/GOVERNANCE.md`: no
  constitution exception, so nothing to record. Run
  `make gtk-lush-api-advisory` and confirm that the only public-API
  difference is the additive `observe::wireplumber_policy_ready`.
- [ ] 5.5 `docs/next/long-line-documents.md`, QH section: a follow-up
  paragraph with the cause, the fix and the lane wall times before and after
  (task 6), from `evidence/`.
- [ ] 5.6 `docs/end-user-coverage.md` and `.agents/rules/build.md`,
  "End-User Smoke Lanes": state that a media readiness failure (PipeWire or
  WirePlumber) is a failure, not a host-support skip, if either text
  describes the skip classes. Otherwise record that nothing needed changing.

## 6. Lane proof on the final tree (D8)

- [ ] 6.1 After G1 and G2 are final, run each media lane 10 consecutive
  times, green, one `heavy.sh` call per run:
  - `make visual-geometry-smoke`;
  - `make accessibility-smoke`;
  - `make visual-smoke`.

  Record each run's wall time and the load before and after in
  `evidence/lanes.md`. A failure, `FLAKY`, `OOM-KILL:`, `SESSION-HYGIENE:` or
  `SKIP:` line is a blocker: investigate it, fix the cause, and restart that
  lane's count.
- [ ] 6.2 Seeded defect (D7.2, uncommitted): a temporary change inside a
  protected region of one visual-geometry scenario must make
  `make visual-geometry-smoke` fail its invariant. A wrong accessible name on
  one anchor must make `make accessibility-smoke` fail. Revert both, and log
  them in `failing-first.log`.
- [ ] 6.3 Compare the means with the baseline (visual-geometry 24.2,
  accessibility 13.0, visual 9.8 min). If the visual lane stays clearly above
  about 3 min at a comparable load, write a `NEW CHANGE PROPOSED` line for
  the remainder in `.claude/handoffs/PARALLEL-LEDGER.md` and say so in
  `evidence/lanes.md`.
- [ ] 6.4 Commit G2 right after the last smoke run, so the smoke fingerprints
  stay current.

## 7. Final gates

- [ ] 7.1 `make fmt`, `make check` (through `heavy.sh`) and the default-feature
  `cargo check -p lushtext --bins --locked`.
- [ ] 7.2 The rustdoc gate (`testing.md`), because `observe.rs` gains public
  rustdoc.
- [ ] 7.3 `openspec validate fix-headless-session-media-wait --strict`.
- [ ] 7.4 Push per task group. The merge goes through the MERGE-LOCK
  procedure (`parallel-agent-work.md`), on local evidence.
