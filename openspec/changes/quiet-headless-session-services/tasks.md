Kani: none (no proven module touched).
Timing: none (no timed path touched). Host-load evidence is journal counts (task 6.5).
Rules for every group: run heavy commands through `scripts/heavy.sh`, one command per call; work headless only, never on the live desktop and never with the real `~/.local/state/wireplumber`; end each group in a signed commit, staged by name, then push the branch.

## 1. Static gate, failing-first (design D8)

- [ ] 1.1 Write `scripts/check-session-hygiene.py`:
  - the forbidden names;
  - file-kind classification of `git ls-files`: a `.sh` file or a `sh`/`bash` shebang, `.githooks/*`, Makefile recipe lines, workflow `run:` blocks, Python through `ast` (docstrings excluded), and Rust string literals after comment stripping;
  - the shell command-position rule with the D8 wrapper list;
  - the Python and Rust literal rule;
  - the path exemption for the helper (fail-closed) and the zero-sanctioned-spawn check;
  - the drift rules over the helper's data files.
- [ ] 1.2 Write `--self-test` before the scan logic is complete. Fixtures go in the run's temp scope, never in the repo:
  - every spawn form fails: wrapped `exec env A=1 timeout 5 wireplumber &`, `taskset -c 0 pipewire`, `$(dbus-run-session -- x)`, a Makefile recipe, a workflow `run:` step, Python `["mutter", "--headless"]` and `"/usr/bin/pipewire"`, and Rust `Command::new("dbus-run-session")` and `"wireplumber".to_string()`;
  - a fixture at a new, never-listed path fails;
  - every benign form passes: `dnf install -y pipewire wireplumber`, a `case` label `dbus-run-session)`, comments, docstrings, `.md` files, `pipewiresrc`, `pipewire.log` and `pipewire-gstreamer`;
  - each drift mutation of a copy of the helper data fails: profile `main`, `hardware.bluetooth = required`, `module.rt = true` or missing, `libpipewire-module-rt` in the WirePlumber fragment, and a user state home;
  - an empty helper (zero sanctioned spawns) fails.

  Confirm each fixture fails for the stated reason before the rule that catches it exists.
- [ ] 1.3 Run the real scan on the unmigrated tree. It must be red. Save its finding list to `openspec/changes/quiet-headless-session-services/evidence/red-scan.txt`: it is the migration worklist for group 4 and must include the Context table of design.md and both `gtk-agentic-debugging` scripts.
- [ ] 1.4 Add `make check-session-hygiene`. For now, wire only its `--self-test` into `make check-policy`; the real scan joins in 5.1, because the pre-commit hook would refuse every commit before the migration. Commit and push.

## 2. The helper in `gtk-lush-proof-harness` (design D1–D4, D7)

- [ ] 2.1 Write the verdict function's unit tests first: one per class (`rtkit`, `bluez`, `hardware-device`, `module-rt`, `accessibility-bus`, `portal`, `config`), the clean case, and a bluez-named restored-metadata entry that must **not** fire `hardware-device`. Then write the GTK-free, I/O-free verdict over bounded observations (monitor-log lines, `pw-dump` `device.api` values, `ListNames`, log-topic lines, configuration facts).
- [ ] 2.2 Add `src/session/` with:
  - the profiles table (`widget`, `plain`, `screencast`, `accessibility`), whose WirePlumber profile constant is `policy`;
  - the data files `session/pipewire-quiet.conf` (used for both `pipewire.conf.d` and `client.conf.d`), `session/wireplumber-quiet.conf` (`override.context.modules` plus the `client-context` `wireplumber.components.rules` override, design D3) and the private system-bus `dbus-daemon` configuration, compiled in with `include_str!`;
  - the session and compositor stages, with environment set before `dbus-run-session`;
  - the private monitored system bus;
  - quiet media with private homes, and the refusal when `XDG_CONFIG_HOME` is not private;
  - the teardown check, then reverse-order teardown;
  - the dedicated exit status and the `SESSION-HYGIENE:` output.

  Reuse `scratch.rs` for runtime directories.
- [ ] 2.3 Add `[[bin]] gtk-lush-session` (`session`, `compositor`, `run`, `check-tools`, and `--expect-dirty` for the by-hand runtime proof) and `scripts/headless-session`, a shim that builds the bin and execs it, contains no session command, and forwards the exit status. Pick an exit status that the harness's existing statuses (`UNSUPPORTED_HOST_EXIT_CODE` and others) do not use, and document it.
- [ ] 2.4 Re-verify design D3 against the installed `/usr/share/wireplumber/wireplumber.conf`: the `client-context` component's module list and the main `context.modules`. Record the WirePlumber version in the data file's header comment.
- [ ] 2.5 Run one capture through the helper (`screencast` profile, mutter ScreenCast, then `gst-launch pipewiresrc`, one buffer) and record the PNG size, zero SCHED_RR threads and zero monitor hits in `evidence/runtime-proof.md`.
- [ ] 2.6 Failing-first runtime proof: run the helper once with the previous configuration (bare `wireplumber`, no fragments, a scratch state home — never the user's) through `--expect-dirty`. It must report `rtkit`, `bluez`, `hardware-device` and `module-rt`. Then run once with the quiet configuration, which must be clean. Record both in `evidence/runtime-proof.md`.
- [ ] 2.7 Update the crate's README (the new "Session hygiene" section: profiles, guard, gate, exit status), CHANGELOG, API advisory snapshot and examples. Run `make check-gtk-lush-policy`, `make gtk-lush-doctests`, `make gtk-lush-examples`, `make gtk-lush-msrv` and `make gtk-lush-api-advisory`. Commit and push.

## 3. Widget harness, axioms and the new GTK axioms (design D5, D6)

- [ ] 3.1 Identify on the real widget session, not on the spec-stage probe, which processes activate `org.a11y.Bus` and the portals before the change. Use `dbus-monitor` on the private bus and `GetConnectionUnixProcessID`, in one shard, on the unmigrated harness. Record the senders in `evidence/runtime-proof.md` and confirm that 3.3 removes each one.
- [ ] 3.2 Failing-first: add a harness test showing that a widget child started without the recommended environment fails before GTK initializes and names the variable. Add a `widget-shards.py` self-test case showing that a `SESSION-HYGIENE:` line and status become a `SESSION-HYGIENE: <shard>: <class>` summary line, fail the run, and are not retried. Both must fail on the current tree.
- [ ] 3.3 Extend `recommended_pre_gtk_environment()` with `GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1`. Route `run_under_headless_compositor` through the session library (`widget` profile). Change `configure_widget_test_environment` and `configure_probe_environment` to verify the values rather than set them, and stop the axiom runner setting `GTK_A11Y=none`.
- [ ] 3.4 Migrate `scripts/run-widget-tests.sh` (`run` with the `widget` profile, replacing `dbus-run-session -- mutter …` and the require checks) and the `widget-shards.py` summary. Migrate `scripts/run-gtk-axioms.sh` the same way.
- [ ] 3.5 Check that `at_listeners` behaves with `GTK_A11Y=test` (`gtk_uses_atspi()` is now false). Run the `background_tab_layout` tests that simulate listeners. If a simulated answer no longer reads as listening, fix it in the test-utils simulation hook, not in the environment.
- [ ] 3.6 Add the two GTK axioms (next free ids at apply time):
  - ledger rows in `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`;
  - catalogue entries;
  - probes that read `ListNames` from inside the probe, with control runs in a nested helper session with a11y or portals on;
  - samples with interactive and `--check` modes;
  - README "Adding an axiom" history, and CHANGELOG.

  Run `make gtk-axioms` (headless) and `make check-gtk-axioms`, and copy the printed versions into "Verified against".
- [ ] 3.7 Run the `widget` shard that holds `accessibility.rs` (`make test-widget-shard WIDGET_SHARD=<shard>`) to check the 12 accessible-state modules under `GTK_A11Y=test` before the full run in 6.2. Commit and push.

## 4. Migrate every other session spawner (design D9; worklist from 1.3)

- [ ] 4.1 `cargo-gtk-proof`: in `host.rs`, replace the `dbus-run-session` prefix and report with the helper library and its profile, and route the host probe through `check-tools`. In `live.rs`, replace `LivePrograms` `pipewire`/`wireplumber`/`mutter`/`dbus_run_session` and `run_mutter_for_case` with the library's media and compositor stages. A hygiene verdict fails the case (`failure_status: session-hygiene`) and verifies no invariant from it. Update the unit tests that assert the old command line so they assert the helper's.
- [ ] 4.2 Python lanes: `visual-geometry-smoke.py`, `crash-recovery-smoke-driver.py`, `automation-smoke-driver.py` (keep its `GTK_A11Y=none` app override, which the helper accepts), `builder-diagnostics.py`, `editor-glyph-live-smoke.py` and `responsiveness-smoke-driver.py` (`accessibility` profile when AT is requested). Use `scripts/headless-session` for `session`/`compositor`, `check-tools` for prerequisites, and `failure_status: session-hygiene` in manifests. `lushtext-automation.py`'s `session_bus` report value names the helper: update `docs/automation.md`/`docs/automation-reference.md` if the field is documented, and run `make automation-client-self-test`.
- [ ] 4.3 Shell lanes: `run-visual-smoke.sh`, `run-accessibility-smoke.sh` (`accessibility` profile), `run-command-palette-notes-smoke.sh`, `run-automation-smoke.sh`, `run-crash-recovery-smoke.sh`, `run-responsiveness-smoke.sh` and `run-portal-sandbox-smoke.sh`. For the last, decide by running it whether `flatpak run` needs the host system bus; if it does, mark its profile `host-system-bus` with the reason and no media. Also `setup-dev-tools.sh` (package inventory stays; its `dbus-run-session` require entry reads the helper's `check-tools`).
- [ ] 4.4 Skill scripts: `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` (the `visual-smoke`/`accessibility-smoke` capture engine) and `capture-lushtext-xvfb.sh` (`session --profile plain` around its Xvfb run). Then run `python3 -B scripts/test-gtk-debug-config.py` and `make check-agent-skills`.
- [ ] 4.5 Classifiers: add the two exact private-system-bus mutter warnings (design D4) to `scripts/smoke_warning_classifiers.py` with sample lines in its self-test, and to the benign-noise patterns of `run-widget-tests.sh` and `run-gtk-axioms.sh` (and the `cargo-gtk-proof` warning scanner if it classifies mutter lines). Only these exact texts. Commit and push.

## 5. Gate fully on

- [ ] 5.1 Wire the real scan of `make check-session-hygiene` into `make check-policy`. It must now be green, and every 1.3 finding must be gone with no pragma or allowlist entry. Confirm that it fails again when one migrated site is temporarily reverted, then restore the site.
- [ ] 5.2 Run `make check` (fmt, all-feature Clippy, `check-policy`) and `cargo check -p lushtext --bins --locked`. Commit and push.

## 6. Verification on the final tree

- [ ] 6.1 Run `make test-unit`, `make test-int`, and `cargo test -p gtk-lush-proof-harness -p cargo-gtk-proof`.
- [ ] 6.2 Run the full widget suite once (`make test-widget`): the environment changes for every widget test. Any `FLAKY:`, `FLAKY-SHARD:`, `OOM-KILL:` or `SESSION-HYGIENE:` is a blocker (`preexisting-blockers.md`). Rerun a failed test in isolation before classifying it. Do not touch the two tests that PR #99 fixes.
- [ ] 6.3 Run `make gtk-axioms` and `make check-gtk-axioms`.
- [ ] 6.4 Run every migrated lane once, headless:
  - `make visual-geometry-smoke`, `make visual-smoke`, `make accessibility-smoke`;
  - `make crash-recovery-smoke`, `make automation-smoke`, `make command-palette-notes-smoke`, `make portal-sandbox-smoke`;
  - `make responsiveness-smoke RESPONSIVENESS_TIER=quick` (a functional run, not a verdict);
  - the builder-diagnostics and editor-glyph lanes;
  - a `cargo gtk-proof run` of one visual-geometry case.

  Record each status, or the host skip and its reason, in `evidence/runtime-proof.md`. Run the visual and accessibility smokes last, right before the commit (proof fingerprints; `testing.md` gate order).
- [ ] 6.5 Collect host evidence, read-only: during one `make visual-geometry-smoke` run, count rtkit-daemon grants and burst-limit lines and bluetoothd "Endpoint registered" lines with `flatpak-spawn --host journalctl -u rtkit-daemon`/`-u bluetooth --since/--until`. Compare with the same window length of an unmigrated run, using the spec-stage figures in `evidence/baseline.md` (15 endpoints and 1 nice plus 3 RT per case) or a fresh `main` run. The pass bar is zero for the branch. Also record that `~/.local/state/wireplumber` mtimes did not move during the run, and whether mutter requested rtkit (spec stage: it did not). Commit and push.

## 7. Documentation (documentation.md)

- [ ] 7.1 `.agents/rules/build.md`: in "End-User Smoke Lanes", state that every session starts through `scripts/headless-session` or the harness library, list the profiles, and describe `SESSION-HYGIENE:`. In the policy-gate list, add `make check-session-hygiene` and what it keys on.
- [ ] 7.2 `.agents/rules/testing.md`: add the gate to "Gate Order Before a Commit" among the cheap gates, and add a "Path-keyed gates" note: the helper exemption is path-keyed and fail-closed.
- [ ] 7.3 `.agents/rules/build.md` "GTK Axioms and Toolkit Updates": on a PipeWire or WirePlumber update, re-read the module lists the WirePlumber fragment repeats (design D3), and expect the runtime guard to catch changed defaults.
- [ ] 7.4 Update:
  - `AGENTS.md` (Testing: widget environment set by the launcher, `GTK_A11Y=test`, `ADW_DISABLE_PORTAL=1`; Build Commands: `make check-session-hygiene`);
  - `README.md` if it lists test commands;
  - `docs/end-user-coverage.md` (lane boundaries are unchanged; the guard is new);
  - the `gtk-testing` skill references (`GTK_A11Y=none` drops the AT context, so use `test`);
  - `crates/gtk-lush/proof-harness/README.md` ("Host Contract": profiles, guard, gate, exit status);
  - `docs/next/gtk-lush.md` (the harness gained a binary and the session module).

  Review `sonar-project.properties` for the new script and bin.
- [ ] 7.5 Record the outcome in the programme record `docs/next/long-line-documents.md`: a short host-hygiene entry with the baseline, the decisions, the deferrals (screenshot-lane portals and the session cap, with reasons) and the proof.
- [ ] 7.6 Run `openspec validate quiet-headless-session-services --strict` and `make check-agent-docs`. Commit and push.

## 8. Merge handoff

- [ ] 8.1 Once merged, write `.claude/handoffs/quiet-headless-session-services/for-cst.md`: the merge SHA, what changed for the host (no rtkit or bluez from test sessions, no device probing, no WirePlumber state writes, no a11y buses outside AT sessions), and how the `cst` session verifies it. The verification is `journalctl -u rtkit-daemon` and `-u bluetooth` counts during a `make visual-geometry-smoke` run, which should be zero for LushText sessions, plus `make check-session-hygiene` and a `SESSION-HYGIENE:` grep in lane logs.
