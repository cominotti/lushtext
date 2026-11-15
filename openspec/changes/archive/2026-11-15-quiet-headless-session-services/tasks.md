Kani: none (no proven module touched).
Timing: no budgeted path changes (apply added two app-code edits off budgeted paths, design "Measurement"); the responsiveness lane's session gains a private system bus, so task 6.4 runs one interleaved main-vs-branch quick-tier check (design, "Measurement"). Host-load evidence is journal counts (task 6.5).
Rules for every group: run heavy commands through `scripts/heavy.sh`, one command per call; work headless only, never on the live desktop and never with the real `~/.local/state/wireplumber`; end each group in a signed commit, staged by name, then push the branch.
Fingerprinted files (design, "Risks"): the accessibility proof covers the capture and atspi skill scripts, `run-accessibility-smoke.sh`, `run-visual-smoke.sh`, `scripts/smoke_warning_classifiers.py`, `docs/automation*.md`, `docs/end-user-coverage.md`, `crates/lushtext-core/src/ui/` and `crates/lushtext/tests/widget/`; the visual-geometry proof (`check-visual-proof-policy.py`) covers `crates/cargo-gtk-proof/src/`, `scripts/visual-geometry-smoke.py`, `scripts/lushtext-automation.py`, `crates/lushtext-core/src/ui/`, `crates/lushtext/tests/widget/` and the `.blp`/`.css`/`.ui` files. The hook refuses a commit that leaves one changed without a fresh smoke. Every edit to those files lands in group 4 (move any group-3 edit there, such as a 3.5 simulation-hook fix or a widget-test change from 3.2), group 4's commit is preceded by `make accessibility-smoke`, `make visual-smoke` and `make visual-geometry-smoke` (task 4.7), and groups 5–8 plan to touch none of them; a later fix that does touches one re-runs both smokes right before its own commit.

## 1. Static gate, failing-first (design D8)

- [x] 1.1 Write `scripts/check-session-hygiene.py`:
  - the forbidden names;
  - file-kind classification of `git ls-files`: a `.sh` file or a `sh`/`bash` shebang, `.githooks/*`, Makefile recipe lines, workflow `run:` blocks, `Containerfile`/`Dockerfile` `RUN`/`CMD`/`ENTRYPOINT`, Python through `ast` (docstrings excluded), and Rust string literals after comment stripping;
  - the fail-closed shell word rule with the D8 exempt contexts (comments, package-manager installs and their continuation lines, `*packages` arrays, `case` labels, quoted text except `sh -c`/`bash -c`/`eval` strings, heredoc bodies except those fed to a shell);
  - the Python and Rust literal rule (equal, `/name` suffix, or first word);
  - the classifier-copy rule over the shared session-service family's message texts (D4);
  - the path exemptions for the helper and for the gate script itself (both fail-closed) and the zero-sanctioned-spawn check;
  - the drift rules over the helper's data files.
- [x] 1.2 Write `--self-test` before the scan logic is complete. Fixtures go in the run's temp scope, never in the repo:
  - every spawn form fails: wrapped `exec env A=1 timeout 5 wireplumber &`, `taskset -c 0 pipewire`, `scripts/heavy.sh -- mutter --headless`, `if ! dbus-run-session -- x; then`, `sh -c 'wireplumber &'`, `$(dbus-run-session -- x)`, a Makefile recipe, a workflow `run:` step, a `Containerfile` `RUN mutter …`, Python `["mutter", "--headless"]`, `"/usr/bin/pipewire"` and `"mutter --headless"`, and Rust `Command::new("dbus-run-session")` and `"wireplumber".to_string()`;
  - every argument form fails: `require_command mutter`, `smoke_require_command wireplumber`, `for c in dbus-run-session mutter; do`, `command -v pipewire`, and a `required_commands=(…)` array naming one;
  - a runner fixture holding a copy of a family text (`There is no colord server available`) fails;
  - a fixture at a new, never-listed path fails;
  - every benign form passes: `dnf install -y pipewire wireplumber`, the same install split over backslash continuation lines with `mutter \` alone on a line (as `ci.yml` has it), a multi-line `packages=(…)` array, a `case` label `dbus-run-session)`, `printf 'dbus-daemon'`, `echo "running under mutter --headless"`, a `cat <<EOF` usage heredoc naming `mutter --headless` (while `bash <<EOF` with a spawn inside fails), comments, docstrings, `.md` files, `"missing required command: mutter"`, `pipewiresrc`, `pipewire.log` and `pipewire-gstreamer`;
  - each drift mutation of a copy of the helper data fails: profile `main`, `hardware.bluetooth = required`, `module.rt = true` or missing, `libpipewire-module-rt` in the WirePlumber fragment, and a user state home;
  - an empty helper (zero sanctioned spawns) fails, and so does a renamed copy of the gate script (its exemption is path-keyed).

  Confirm each fixture fails for the stated reason before the rule that catches it exists.
- [x] 1.3 Run the real scan on the unmigrated tree. It must be red. Save its finding list to `openspec/changes/quiet-headless-session-services/evidence/red-scan.txt`: it is the migration worklist for group 4 and must include the Context table of design.md, both `gtk-agentic-debugging` scripts, and the non-spawn mentions the Context section lists (version queries, report and capability data, `required_commands`, the `for` loops, the harness's tooling check). Classify each finding as spawn, prerequisite, version or data, and name its resolution (D2, D8); none may be an allowlist entry.
- [x] 1.4 Add `make check-session-hygiene`. For now, wire only its `--self-test` into `make check-policy`; the real scan joins in 5.1, because the pre-commit hook would refuse every commit before the migration. Commit and push.

## 2. The helper in `gtk-lush-proof-harness` (design D1–D4, D7)

- [x] 2.1 Write the verdict function's unit tests first: one per class (`rtkit`, `bluez`, `hardware-device`, `module-rt`, `accessibility-bus`, `portal`, `config`), the clean case, and a bluez-named restored-metadata entry that must **not** fire `hardware-device`. For `rtkit` (design D7): the portal frontend's lookups and property reads in a portals-on profile pass; the same records from another executable, the frontend's records in a portals-off profile, any `MakeThread*` call from any sender (the frontend included), and an unresolved sender all fire. For `accessibility-bus`: `org.freedesktop.a11y.Manager` (mutter's) does not fire, and an `org.a11y.Bus` that was acquired and released before teardown does. For `config`: `GTK_A11Y` `test` and `none` pass, and any other value or an unset variable fires. Then write the GTK-free, I/O-free verdict over bounded observations (monitor-log lines, `pw-dump` `device.api` values, `ListNames`, log-topic lines, configuration facts).
- [x] 2.2 Add `src/session/` with:
  - the profiles table (`widget`, `plain`, `screencast`, `accessibility`), whose WirePlumber profile constant is `policy`;
  - the data files `session/pipewire-quiet.conf` (used for both `pipewire.conf.d` and `client.conf.d`), `session/wireplumber-quiet.conf` (`override.context.modules` plus the `client-context` `wireplumber.components.rules` override, design D3) and the private system-bus `dbus-daemon` configuration, compiled in with `include_str!`;
  - the session and compositor stages, with environment set before `dbus-run-session`;
  - the private monitored system bus;
  - quiet media with private homes, and the refusal when `XDG_CONFIG_HOME` is not private;
  - the teardown check, then reverse-order teardown;
  - the dedicated exit status and the `SESSION-HYGIENE:` output.

  Reuse `scratch.rs` for runtime directories.
- [x] 2.3 Add `[[bin]] gtk-lush-session` (`session`, `compositor`, `run`, `check-tools [--json]` with each session command's path and `--version` line, `--expect-dirty` for the by-hand runtime proof, the `GTK_LUSH_SESSION_BIN` export, and nested sessions, design D2) and `scripts/headless-session`, a shim that builds the bin and execs it, contains no session command, and forwards the exit status. Pick an exit status that the harness's existing statuses (`UNSUPPORTED_HOST_EXIT_CODE` and others) do not use, and document it.
- [x] 2.4 Re-verify design D3 against the installed `/usr/share/wireplumber/wireplumber.conf`: the `client-context` component's module list and the main `context.modules`. Record the WirePlumber version in the data file's header comment.
- [x] 2.5 Run one capture through the helper (`screencast` profile, mutter ScreenCast, then `gst-launch pipewiresrc`, one buffer, with a Libadwaita client presented so the portals start) and record in `evidence/runtime-proof.md`: the PNG size, zero SCHED_RR threads, zero `MakeThread*` calls and zero bluez records, the portal frontend's RealtimeKit records (expected: the 18 of the spec-review spike, all exempt) with no other sender naming RealtimeKit1, and every warning line of the session, so 4.5 adds exactly the texts seen. Run it once with media off and once with media on: the frontend is also a PipeWire client, so with media on its RealtimeKit records must still be exactly the media-off set (one `Properties.Get` per property, plus the lookups). If any other sender, any extra frontend record, or a `MakeThread*` call appears, stop and amend design D7 before going on.
- [x] 2.6 Failing-first runtime proof: run the helper once with the previous configuration (bare `wireplumber`, no fragments, a scratch state home — never the user's, as the spec scenario says) through `--expect-dirty`. It must report `rtkit`, `bluez`, `hardware-device` and `module-rt`. Then run once with the quiet configuration, which must be clean. Record both in `evidence/runtime-proof.md`.
- [x] 2.7 Update the crate's README (the new "Session hygiene" section: profiles, guard, gate, exit status), CHANGELOG, API advisory snapshot and examples. Run `make check-gtk-lush-policy`, `make gtk-lush-doctests`, `make gtk-lush-examples`, `make gtk-lush-msrv` and `make gtk-lush-api-advisory`. Commit and push.

## 3. Widget harness, axioms and the new GTK axioms (design D5, D6)

- [x] 3.1 Identify on the real widget session, not on the spec-stage probe, which processes activate `org.a11y.Bus` and the portals before the change. Use `dbus-monitor` on the private bus and `GetConnectionUnixProcessID`, in one shard, on the unmigrated harness. Record the senders in `evidence/runtime-proof.md` and confirm that 3.3 removes each one.
- [x] 3.2 Failing-first: add a harness test showing that a widget child started without the recommended environment fails before GTK initializes and names the variable. Add a `widget-shards.py` self-test case showing that a `SESSION-HYGIENE:` line and status become a `SESSION-HYGIENE: <shard>: <class>` summary line, fail the run, and are not retried. Both must fail on the current tree.
- [x] 3.3 Extend `recommended_pre_gtk_environment()` with `GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1`. Route `run_under_headless_compositor` through the session library (`widget` profile). Change `configure_widget_test_environment` and `configure_probe_environment` to verify the values rather than set them, in the per-test child only (list mode and the relaunching parent verify nothing; a caller's `GSK_RENDERER` still wins), and stop the axiom runner setting `GTK_A11Y=none` (design, "Risks": a probe whose verdict changes under `test` is investigated in 6.3, and keeps `none` for its child only with the reason in its ledger row).
- [x] 3.4 Migrate `scripts/run-widget-tests.sh` (`run` with the `widget` profile, replacing `dbus-run-session -- mutter …` and the require checks) and the `widget-shards.py` summary. Migrate `scripts/run-gtk-axioms.sh` the same way.
- [x] 3.5 Check that `at_listeners` behaves with `GTK_A11Y=test` (`gtk_uses_atspi()` is now false). Run the `background_tab_layout` tests that simulate listeners. If a simulated answer no longer reads as listening, fix it in the test-utils simulation hook, not in the environment.
- [x] 3.6 Add the two GTK axioms (next free ids at apply time):
  - ledger rows in `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`;
  - catalogue entries;
  - probes that read `ListNames` from inside the probe, with control runs in a nested helper session with a11y or portals on, started through `GTK_LUSH_SESSION_BIN` (the axioms crate stays a leaf: no `gtk-lush-*` dependency, no session command); with the variable unset or a control prerequisite (`at-spi-bus-launcher`, `xdg-desktop-portal`) missing, the probe returns `FixtureInvalid`, and a unit-level check of that path is part of the probe's tests;
  - in an SDK sandbox (`make gtk-axioms-runtimes`, no session bus), the samples report the existing no-session-bus skip form;
  - samples with interactive and `--check` modes;
  - README "Adding an axiom" history, and CHANGELOG.

  Run `make gtk-axioms` (headless) and `make check-gtk-axioms`, and copy the printed versions into "Verified against".
- [x] 3.7 Run the `widget` shard that holds `accessibility.rs` (`make test-widget-shard WIDGET_SHARD=<shard>`) to check the 12 accessible-state modules under `GTK_A11Y=test` before the full run in 6.2. Commit and push.

## 4. Migrate every other session spawner (design D9; worklist from 1.3)

- [x] 4.1 `cargo-gtk-proof`: in `host.rs`, replace the `dbus-run-session` prefix and report with the helper library and its profile, and route the host probe through `check-tools`. In `live.rs`, replace `LivePrograms` `pipewire`/`wireplumber`/`mutter`/`dbus_run_session` and `run_mutter_for_case` with the library's media and compositor stages. A hygiene verdict fails the case (`failure_status: session-hygiene`) and verifies no invariant from it. Update the unit tests that assert the old command line so they assert the helper's.
- [x] 4.2 Python lanes: `visual-geometry-smoke.py`, `crash-recovery-smoke-driver.py`, `automation-smoke-driver.py` (keep its `GTK_A11Y=none` app override, which the helper accepts), `builder-diagnostics.py`, `editor-glyph-live-smoke.py` and `responsiveness-smoke-driver.py` (design D2: `accessibility` profile whenever the `at` variant is selected, `plain` otherwise, always with `--media off`; keep its per-variant app environment exactly as it is; take its manifest's `toolkit.mutter` version from `check-tools --json`, keeping the manifest field name). Use `scripts/headless-session` for `session`/`compositor`, `check-tools` for prerequisites, and `failure_status: session-hygiene` in manifests. `lushtext-automation.py`'s `session_bus` report value names the helper: update `docs/automation.md`/`docs/automation-reference.md` if the field is documented, and run `make automation-client-self-test`. In `cargo-gtk-proof` `artifacts.rs`, the missing-capability test data takes its command name from the helper's exported constant (design D8). Move `proof-harness/src/lib.rs`'s tooling check and its tests into `src/session/`.
- [x] 4.3 Shell lanes: `run-visual-smoke.sh`, `run-accessibility-smoke.sh` (`accessibility` profile), `run-command-palette-notes-smoke.sh`, `run-automation-smoke.sh`, `run-crash-recovery-smoke.sh`, `run-responsiveness-smoke.sh` and `run-portal-sandbox-smoke.sh`. For the last, decide by running it whether `flatpak run` needs the host system bus; if it does, mark its profile `host-system-bus` with the reason and no media. `run-responsiveness-smoke.sh`'s `mutter=` version line comes from `check-tools --json`. Also `setup-dev-tools.sh`: its `packages=(…)` inventory stays (an exempt context) and gains `dbus-tools`; drop `dbus-run-session`, `mutter`, `pipewire` and `wireplumber` from `required_commands`, because the inventory already installs their packages and `check-tools` cannot run before the packages that build it exist; keep the `case` map only for the commands left.
- [x] 4.4 Skill scripts: `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` (the `visual-smoke`/`accessibility-smoke` capture engine) and `capture-lushtext-xvfb.sh` (`session --profile plain` around its Xvfb run). Then run `python3 -B scripts/test-gtk-debug-config.py` and `make check-agent-skills`.
- [x] 4.5 Classifiers (design D4, `smoke-warning-classification`):
  - extend the existing "Headless container session services" family of `scripts/smoke_warning_classifiers.py` (no parallel family) with the two exact private-system-bus mutter warnings, the portal frontend's `Failed to load RealtimeKit property: …ServiceUnknown…` line, and any other text the 2.5 run recorded, each with a sample line in its self-test and a near-miss defect sample;
  - add a `--print-regex <family>` entry point that emits POSIX ERE, and make the module's self-test feed it to `grep -E` over the benign and defect samples;
  - make `run-widget-tests.sh` and `run-gtk-axioms.sh` read the family from it instead of their copies of those lines, which removes the pre-existing duplicates, and make their warning self-tests take the family's sample lines from the module (a `--print-samples <family>` entry point) instead of embedding them, or the classifier-copy rule flags them;
  - update the `cargo-gtk-proof` warning scanner's copy under its Python-oracle parity test;
  - add only the exact texts seen in the 2.5 run and the spec-review spike; the gate's classifier-copy rule (1.1) then keeps the runners from regrowing copies.
- [x] 4.6 Prerequisites (design D1):
  - the shim honours `GTK_LUSH_SESSION_BIN` and fails with the lane's unsupported-host status when neither that nor cargo is available;
  - `check-tools` reports `dbus-monitor` (Fedora `dbus-tools`);
  - add `dbus-tools` to every install list that installs `dbus-daemon`: `.github/workflows/ci.yml` (widget, both axiom and other session jobs), `.github/workflows/end-user-smoke.yml`, `.github/workflows/responsiveness-smoke.yml` (both jobs), `build-aux/builder-diagnostics/Containerfile` and `scripts/setup-dev-tools.sh`;
  - add `at-spi2-core`, `xdg-desktop-portal` and `xdg-desktop-portal-gtk` to both axiom jobs (`gtk-axioms`, `gtk-axioms-floor`), which install with weak dependencies off, so the new probes' controls can run (design, "Axioms relied on");
  - make sure every job and lane that starts a session either can build the bin (cargo and GTK development headers) or is given `GTK_LUSH_SESSION_BIN`, starting with `portal-sandbox-smoke` and `capture-lushtext-xvfb.sh`;
  - run `make check-end-user-smoke-workflow` and `make check-workflow-timeouts`.
- [x] 4.7 Move the `docs/end-user-coverage.md` update of 7.4 here (it is fingerprinted). Then, as the last step before the group's commit, run `make accessibility-smoke`, `make visual-smoke` and `make visual-geometry-smoke` (headless) and commit right after them, with no edit in between (`testing.md`, "Gate Order Before a Commit"). Commit and push.

## 5. Gate fully on

- [x] 5.1 Wire the real scan of `make check-session-hygiene` into `make check-policy`. It must now be green, and every 1.3 finding must be gone with no pragma or allowlist entry. Confirm that it fails again when one migrated site is temporarily reverted, then restore the site.
- [x] 5.2 Run `make check` (fmt, all-feature Clippy, `check-policy`) and `cargo check -p lushtext --bins --locked`. Commit and push.

## 6. Verification on the final tree

- [x] 6.1 Run `make test-unit`, `make test-int`, and `cargo test -p gtk-lush-proof-harness -p cargo-gtk-proof`.
- [x] 6.2 Run the full widget suite once (`make test-widget`): the environment changes for every widget test. Any `FLAKY:`, `FLAKY-SHARD:`, `OOM-KILL:` or `SESSION-HYGIENE:` is a blocker (`preexisting-blockers.md`). Rerun a failed test in isolation before classifying it. Do not touch the two tests that PR #99 fixes. *(Stage 4: both FLAKY lines were pre-existing on `main`; causes found and fixed, `evidence/runtime-proof.md` "6.2 Stage 4".)*
- [x] 6.3 Run `make gtk-axioms` and `make check-gtk-axioms`. Run `make gtk-axioms-runtimes` once as well (local only: `flatpak run` of an SDK inside the migrated `run-gtk-axioms.sh` session), deciding as in 4.3 whether `flatpak run` needs the host system bus, or record the host skip and its reason in `evidence/runtime-proof.md`.
- [x] 6.4 Run every migrated lane once, headless:
  - `make visual-geometry-smoke`, `make visual-smoke`, `make accessibility-smoke`;
  - `make crash-recovery-smoke`, `make automation-smoke`, `make command-palette-notes-smoke`, `make portal-sandbox-smoke`;
  - `make responsiveness-smoke RESPONSIVENESS_TIER=quick` (a functional run, not a verdict). Check its app stderr and probe logs for any system-bus access. Then, unconditionally (design, "Measurement"), run the interleaved main-vs-branch quick-tier measurement of the affected cases through `scripts/measure.sh`, run detached (`verification-runs.md`; a verdict run needs a 1-minute load under 4, otherwise label it non-verdict and retry), and record the verdict in design.md "Measurement" and `evidence/runtime-proof.md`;
  - the builder-diagnostics and editor-glyph lanes;
  - a `cargo gtk-proof run` of one visual-geometry case.

  Record each status, or the host skip and its reason, in `evidence/runtime-proof.md`. Run the visual and accessibility smokes last, right before the commit (proof fingerprints; `testing.md` gate order); no fingerprinted file may change after them, in this group or later.
- [x] 6.5 Collect host evidence, read-only: during one `make visual-geometry-smoke` run, count rtkit-daemon grants and burst-limit lines and bluetoothd "Endpoint registered" lines with `flatpak-spawn --host journalctl -u rtkit-daemon`/`-u bluetooth --since/--until`. Compare with the same window length of an unmigrated run, using the spec-stage figures in `evidence/baseline.md` (15 endpoints and 1 nice plus 3 RT per case) or a fresh `main` run. The pass bar is zero for the branch. Also record that `~/.local/state/wireplumber` mtimes did not move during the run, and whether mutter requested rtkit (spec stage: it did not). Commit and push.

## 7. Documentation (documentation.md)

- [x] 7.1 `.agents/rules/build.md`: in "End-User Smoke Lanes", state that every session starts through `scripts/headless-session` or the harness library, list the profiles, and describe `SESSION-HYGIENE:`. In the policy-gate list, add `make check-session-hygiene` and what it keys on.
- [x] 7.2 `.agents/rules/testing.md`: add the gate to "Gate Order Before a Commit" among the cheap gates, and add `scripts/check-session-hygiene.py` to the existing "Path-keyed gates" list (its helper and self exemptions are path-keyed and fail-closed, and it fails on zero sanctioned spawns).
- [x] 7.3 `.agents/rules/build.md` "GTK Axioms and Toolkit Updates": on a PipeWire or WirePlumber update, re-read the module lists the WirePlumber fragment repeats (design D3), and expect the runtime guard to catch changed defaults.
- [x] 7.4 Update:
  - `AGENTS.md` (Testing: widget environment set by the launcher, `GTK_A11Y=test`, `ADW_DISABLE_PORTAL=1`; Build Commands: `make check-session-hygiene`);
  - `README.md` if it lists test commands;
  - (`docs/end-user-coverage.md` moved to 4.7: it is fingerprinted, and no fingerprinted file changes in this group);
  - the `gtk-testing` skill references (`GTK_A11Y=none` drops the AT context, so use `test`);
  - `crates/gtk-lush/proof-harness/README.md` ("Host Contract": profiles, guard, gate, exit status);
  - `docs/next/gtk-lush.md` (the harness gained a binary and the session module).

  Review `sonar-project.properties` for the new script and bin.
- [x] 7.5 Record the outcome in the programme record `docs/next/long-line-documents.md`: a short host-hygiene entry with the baseline, the decisions, the deferrals (screenshot-lane portals and the session cap, with reasons) and the proof.
- [x] 7.6 Run `openspec validate quiet-headless-session-services --strict` and `make check-agent-docs`. Commit and push.

## 8. Merge handoff

- [x] 8.1 Once merged, write `.claude/handoffs/quiet-headless-session-services/for-cst.md`: the merge SHA, what changed for the host (no rtkit or bluez from test sessions, no device probing, no WirePlumber state writes, no a11y buses outside AT sessions), and how the `cst` session verifies it. The verification is `journalctl -u rtkit-daemon` and `-u bluetooth` counts during a `make visual-geometry-smoke` run, which should be zero for LushText sessions, plus `make check-session-hygiene` and a `SESSION-HYGIENE:` grep in lane logs. *(Done: merged as `12af80a9`; the note is in the handoff directory and adds the sccache findings.)*
