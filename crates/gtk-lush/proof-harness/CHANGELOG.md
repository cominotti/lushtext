# Changelog

## Unreleased

- **Behaviour (quiet media readiness):** with media on, the session helper
  now starts the command once WirePlumber's client object lists
  `policy.linking.standard` in its `session.services` property, which
  WirePlumber's `session-services.lua` sets after every other component of
  its profile, the linking hooks included
  (`fix-headless-session-media-wait`). The old wait looked for
  `"metadata.name": "default"` in `pw-dump`, which can never hold:
  `pw-dump` prints no metadata object without entries, and in a session
  without audio devices `default` has none. So every media session ran the
  blind 10 s `READY_TIMEOUT` and ignored the result, since the helper's first
  lanes; readiness now takes tens of milliseconds.
- **Behaviour (readiness miss):** a session whose WirePlumber does not list
  its linking policy within `READY_TIMEOUT` no longer starts the command. It
  saves the last `pw-dump` as `pw-dump-not-ready.json` in the `--log-dir`
  (without `--log-dir` the helper's own log directory is removed at
  teardown, and the line says so), prints one
  `gtk-lush-session: WirePlumber did not load its linking policy within 10 s (…)`
  line, and exits with `TEST_FAILURE_EXIT_CODE` (`101`): a failure, never an
  unsupported-host (`77`) or session-hygiene (`93`) exit.
- New `session::observe::wireplumber_policy_ready(pw_dump)`: the pure
  readiness predicate, a whole-member match in the bracketed
  `session.services` list, independent of member order.
- **Breaking (behaviour):** a per-test child of `run_registered_tests()` exits
  with `TEST_FAILURE_EXIT_CODE` before GTK initializes unless
  `session::SESSION_ACTIVE_ENV` is set, which only the session helper's inner
  stage does. A consumer that set its runner variable and started the harness
  in a session of its own must start it through the helper (or let the
  harness relaunch itself).
- The session helper starts its inner stage through `/proc/<pid>/exe`, not
  `current_exe()` (the `GTK_LUSH_SESSION_BIN` it exports keeps the file path,
  which also works in another pid namespace). Every `cargo build` re-uplifts
  `target/debug/gtk-lush-session`, so with several sessions starting in one
  worktree `current_exe()` could name a replaced file (`… (deleted)`) and
  `dbus-run-session` failed to start the inner stage (2 of 152 sessions under
  a concurrent build loop; 0 of 441 after this and the shim's exec retry).
- `recommended_pre_gtk_environment()` now selects `GTK_A11Y=test`, and its
  documentation states the hazard that comes with it: announcing through a
  `GtkTestATContext` kills the process (GTK ledger A117), so consumers
  announce through a helper that skips that context.
- The session's sockets are bounded by `dbus-daemon`'s 99-byte socket path
  limit, not the kernel's 107: an owned runtime directory that would not fit
  falls back to `/tmp`, and an inherited one is refused.
- New `session` module and `gtk-lush-session` binary: the one implementation
  of the private headless session (`quiet-headless-session-services`).
  Profiles `widget`, `plain`, `screencast` and `accessibility`; a private,
  monitored system bus for every session; quiet media (WirePlumber `policy`
  profile, no module-rt in the daemons or the clients, helper-private homes);
  and a runtime guard that prints `SESSION-HYGIENE: <class>: <detail>` and
  exits with `SESSION_HYGIENE_EXIT_CODE` (`93`) on an rtkit, bluez,
  hardware-device, module-rt, accessibility-bus, portal or config violation.
  The harness's headless relaunch now goes through it (`widget` profile)
  instead of its own `dbus-run-session` command line.
- `session::session_command()` and `session::compositor_command()` build the
  `session` and `compositor` stages as `Command`s for a caller that supervises
  the processes itself; they return `SessionLaunchError` instead of starting
  anything on a host without the tools. `check-tools --json` also reports a
  `roles` map (session bus, compositor, media, session manager).
- **Breaking (array length):** `recommended_pre_gtk_environment()` returns
  **seven** settings, adding `GTK_A11Y=test` and `ADW_DISABLE_PORTAL=1`. The
  launcher applies them before the session bus starts, and a per-test child now
  only verifies them (`verify_recommended_environment()`), failing before GTK
  initializes when one is missing. Callers that set the values themselves in
  `main` can stop doing so.
- `wait_until` is `#[track_caller]`: its timeout panic now names the
  waiting call site instead of the harness's own line, so a test with several
  waits reports which one timed out.
- The supervising parent now owns every child's temporary files. It creates
  one `<prefix><pid>-<random>` root in the system temp dir per run, gives each
  child attempt its own empty subdirectory as `TMPDIR`, removes that
  subdirectory when the child exits, and removes the root when the run ends,
  on failing runs too. A child that leaked one fixed-name directory per process
  had accumulated 45,978 directories and exhausted a tmpfs's inodes. New
  `HarnessConfig::with_run_scratch_prefix()` / `run_scratch_prefix()` and
  `DEFAULT_RUN_SCRATCH_PREFIX` (`gtk-lush-proof-run-`) choose the prefix; the
  parent's startup sweep reclaims roots under it (and legacy `<prefix><pid>`
  names) whose PID is gone and which have been idle for an hour, owned by the
  current user only. Additive API; the behaviour change is that children now
  run with a harness-owned `TMPDIR`.
- The headless relaunch's compositor runtime directory is now named
  `gtk-lush-proof-runtime-<pid>-<random>` and is removed only after no process
  still carries it as `XDG_RUNTIME_DIR` (bounded at ten seconds).
  `dbus-run-session` returns while `xdg-document-portal` still holds its FUSE
  mount on `doc/`, so the previous immediate removal failed and leaked the
  directory on every plain `cargo test` run. Stale directories of both the new
  and the legacy PID-less form are swept.

- `recommended_pre_gtk_environment()` now returns **five** settings instead of
  four, adding `GTK_IM_MODULE=gtk-im-context-simple`. Headless Mutter advertises
  `zwp_text_input_manager_v3` with no input method behind it, so GTK enabled its
  Wayland text-input backend for every focused editable and destroying a focused
  entry raced the compositor's reply into a **SIGSEGV** in
  `wl_proxy_get_version`. Measured 9 failures in 40 isolated runs before, 0 in 40
  after. The return type changes from `[RecommendedEnvironment; 4]` to
  `[RecommendedEnvironment; 5]`, which is a breaking signature change for a crate
  that has not published.

## 0.0.0

- First functional in-tree pre-publication implementation for headless GTK
  test harness orchestration and wait helpers.
- Adoption-lab and matrix evidence before functional publication.
