---
name: gtk-agentic-debugging
description: "Run and observe GTK4 and Libadwaita applications the way a human does: launch real GUI sessions, keep a PTY open for stdout and stderr, capture user-session journal output, watch D-Bus traffic, request screenshots through desktop capture paths, and summarize runtime artifacts. Trigger when the user asks to reproduce a live GTK bug, inspect warnings from `make run`, watch terminal logs, follow D-Bus or portal behavior, capture what is on screen, or debug visual, focus, sizing, animation, compositor, or session-specific issues that only appear while the app is actually running. Pair with `gtk4-libadwaita-internals` when the captured symptom is really a GTK or Adwaita contract question."
---

# GTK Agentic Debugging

Use this skill when static code reading is not enough and the bug only becomes obvious in a live GTK session.

## Boundary with gtk4-libadwaita-internals

This skill is for evidence collection: reproduce the bug, capture logs, correlate timestamps, and narrow the failing phase.

When the captured symptom is a GTK or Adwaita warning, critical, geometry invariant, focus contract, builder-template issue, or `GtkListView` / `GtkTreeListModel` lifecycle question, switch from "what happened at runtime?" to "what contract is the toolkit enforcing?" and read:

- [../gtk4-libadwaita-internals/references/warnings-and-criticals.md](../gtk4-libadwaita-internals/references/warnings-and-criticals.md)
- [../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md)
- [../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md](../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md)

## Quick Start

For repeatable agent-owned inspection, make headless Mutter the first path:

```bash
.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py \
  --file PATH \
  --search needle \
  --expected-search-matches 3 \
  --enable-minimap \
  --output /tmp/lushtext-mutter.png
```

This launches LushText on a private `mutter --headless` Wayland monitor, isolates XDG data/config/cache plus keyfile GSettings, sets in-document search through the exported `win.set-search-query` D-Bus action, waits through Automation1 readiness predicates, saves `automation-snapshot.json`, and captures the monitor through Mutter's `RecordMonitor("Meta-0")` screencast stream. Prefer this before touching the human's live GNOME session or falling back to Xvfb.

Prefer a capture session over ad hoc commands. Run the helper through `functions.exec_command` with `tty: true`, then keep polling with `write_stdin` while the human interacts with the app window.

```bash
.agents/skills/gtk-agentic-debugging/scripts/run-gtk-debug-session.sh \
  --cmd "make run" \
  --pid-pattern '(^| )target/debug/lushtext($| )'
```

Use a tight executable regex for `--pid-pattern`. Avoid broad alternations or loose substrings such as `target/debug/lushtext|dev.cominotti.lushtext` because they can match the helper script or even the `pgrep` probe itself, which makes the launch heuristics misleading.

Before interacting with the window or taking a screenshot, prove the app is alive and that the current PID was not already present before the debug launch:

```bash
.agents/skills/gtk-agentic-debugging/scripts/check-lushtext-live.sh \
  --session /tmp/gtk-debug-YYYYMMDD-HHMMSS \
  --require-launched-instance \
  --require-dbus \
  --require-tool gdbus
```

Add the exact tool you are about to use to the same check, for example `--require-tool ydotool` before input injection or `--require-tool gnome-screenshot --require-tool python3` before snapshot capture. If this check fails because LushText is not running, the PID is not proven to belong to the debug launch, or a required tool is missing, pause and ask the human to install the tool, run `make dev-tools`, relaunch the debug session, or provide different instructions.

For agent-driven input, `ydotool` is only ready when both the binary and daemon are usable. The default socket is `${XDG_RUNTIME_DIR}/.ydotool_socket` unless `YDOTOOL_SOCKET` is set. If the command runner reaps background processes after each shell command, keep `ydotoold` running in its own PTY-backed session instead of starting it as a short-lived background child.

Before using `ydotool type` or `ydotool key`, ask the human to focus the specific LushText debug window you launched. Then rerun `check-lushtext-live.sh --session ... --require-launched-instance --require-tool ydotool` immediately before injection. `ydotool` targets the compositor's focused surface, so process liveness alone does not prove the keystrokes will go to LushText.

Prefer D-Bus for interactions whenever LushText exports the needed behavior as a `org.gtk.Actions` action on the application or window object. For ordinary inspection, use `scripts/lushtext-automation.py catalog`, `snapshot`, `wait`, and `action` so the action catalog, typed parameters, statuses, and result envelope stay consistent with docs. For lower-level tracing, inspect `/dev/cominotti/lushtext/Automation` with `dev.cominotti.lushtext.Automation1.GetActionCatalog`, then drive the documented app/window action and use `WaitForReady` with the narrowest named predicate plus `GetSnapshot` for bounded assertions. Fall back to broad `WaitForIdle` only when no narrower predicate matches the workflow. For text entry into visible GTK widgets that is not covered by a target-state action, prefer AT-SPI D-Bus editable-text automation before `ydotool`. Screenshot capture may also use D-Bus through the desktop portal or GNOME Shell, but those APIs are permission-gated and can return `AccessDenied` or wait for human approval.

Prefer non-interactive portal screenshots before opening the GNOME Shell screenshot UI. In this Fedora Toolbx on Wayland, `capture-screenshot.py --portal-only --non-interactive` can save a PNG without a visible prompt, while `gnome-screenshot -f` may hang after falling back to X11. If an interactive portal UI appears, do not try to approve it with coordinate clicks; only use AT-SPI actions when the accessible exposes a real invokable action.

Do not use coordinate clicks to focus LushText or any other application window. GNOME Shell can report broad or full-screen frame extents through AT-SPI, and clicking near the top of a maximized frame can activate Shell UI such as Overview or quick settings instead of the app. If D-Bus/AT-SPI activation does not focus the target window, ask the human to focus it.

If headless Mutter is unavailable, use `scripts/capture-lushtext-xvfb.sh --file PATH --search TEXT --enable-minimap --output /tmp/shot.png` as the fallback isolated display. It launches the debug binary on a private Xvfb display, uses temporary XDG data/config/cache home plus keyfile GSettings, invokes `win.begin-search` through `org.gtk.Actions`, types only inside that isolated display with `xdotool`, and captures the root window with `xwd` + ImageMagick. This path is lower compositor fidelity than `mutter --headless`.

After the reproduction, inspect the generated `summary.md`, then open the raw `app.typescript`, `dbus.log`, and `journal.log` files only as needed.

## Choose the Right Mode

- **Fresh launch**: Use when stdout and stderr from startup matter. First check whether the app is already running because `make run` intentionally asks the existing LushText owner to quit before relaunching the fresh debug binary, and fails if that owner refuses to close. Do not treat `cargo run` printing `Running target/debug/...` as proof that a new GUI process or a new window was created.
- **Existing instance watch**: Use when the app is already open and the user can reproduce the bug in that window. This is usually the safer and more truthful mode for single-instance GTK apps. Keep the capture session open, collect journal and D-Bus output, and let the human drive the UI.
- **Screenshot assist**: Use when you need to confirm what the human sees on screen. For agent-owned repros, first try `scripts/capture-lushtext-mutter.py` so focus, portal approval, and Shell UI cannot interfere. For the human's live desktop, run `check-lushtext-live.sh` first with `--require-tool gnome-screenshot --require-tool python3`; if anything is missing, stop and ask for installation or alternative instructions. Then run `scripts/capture-screenshot.py` and be ready for a desktop permission prompt or timeout.
- **Log triage only**: Use `scripts/summarize-runtime-logs.py` on an existing artifact directory when the session has already been recorded.

## Workflow

1. Read [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md) for the live-session workflow and tool limits.
2. Choose an exact `--pid-pattern` for the target executable before launching the helper. Prefer anchored executable matches over app IDs or loose substrings.
3. Start `scripts/run-gtk-debug-session.sh` in a PTY-backed session.
4. Before any D-Bus action, input injection, screenshot, or request for the human to type into the window, run `scripts/check-lushtext-live.sh` with `--session`, `--require-launched-instance`, and every `--require-tool` needed for the next action.
5. Inspect `process-before.txt`, `process-after.txt` when present, and `status.txt`. If they mention `run-gtk-debug-session.sh` or `pgrep`, the capture is contaminated and you should tighten the pattern before drawing conclusions.
6. Let the human reproduce the bug while you poll the session and read warnings as they appear.
7. If visual confirmation matters, capture the relevant state extreme, not just
   any reachable screen. Prefer no items/no required context for empty-state
   regressions, and many or awkward items for overflow, clipped controls,
   virtualization, or scrolling regressions. When practical, capture the
   opposing state too so the fix is not tuned only to one end of the matrix.
8. Run `scripts/capture-screenshot.py --portal-only --non-interactive` only after the liveness/tool check succeeds, and note whether the desktop allowed or denied the request.
9. Run `scripts/summarize-runtime-logs.py` or read the auto-generated `summary.md`.
10. Use [references/log-patterns.md](references/log-patterns.md) to map the signature to likely GTK, GLib, Adwaita, or D-Bus causes.
11. If step 10 points to a toolkit invariant rather than an app-specific state bug, hand the interpretation to `gtk4-libadwaita-internals` before proposing a fix.

## Preferred Investigation Loop

Prefer this loop for real GTK bugs that only show up in a live desktop session:

1. Launch a real fresh app instance with the helper and keep the PTY open.
2. Let the human reproduce the bug in the actual window instead of starting with synthetic action calls.
3. Watch the terminal output live and capture the first repeated warning burst or visible symptom.
4. Add the smallest possible targeted tracing to answer the next unknown.
   Good examples:
   - widget pointer dumps
   - minimum-width or allocation logs for the specific paned or revealer path
   - snapshot or wrapper identity checks
5. Relaunch the real app, reproduce again, and compare the new traces with the warning timestamps.
6. Match warned widget pointers to the real widget tree before deciding which widget is actually wrong.
7. Only then make a narrow fix, rerun the same real-app loop, and verify both correctness and UX.
8. Once the manual repro is proven, prefer driving the exact exported `org.gtk.Actions` window action for restart-to-restart verification, then call automation `WaitForReady` with the narrowest named predicate and `GetSnapshot`. This keeps the reproduction on the real application path without guessing at lower-level input injection.

This is the preferred workflow over broad speculative code changes. For geometry bugs especially, "launch real app -> human reproduces -> inspect live warnings -> add narrow tracing -> pointer-match the real widgets -> rerun" is usually faster and more trustworthy than static reasoning alone.

## Required Habits

- Prefer `tty: true` for the live runner. PTY-backed sessions are the most reliable way to preserve stdout and stderr ordering.
- For automated visual inspection, prefer the headless Mutter helper before live-desktop portal screenshots and before Xvfb. Mutter matches the CI compositor path and avoids stealing the human's focus.
- For visual UI work, inspect the state matrix through actual screenshots when
  widget assertions cannot prove legibility: empty/no-context, representative
  populated, dense/many-item, and constrained-size states. Open the PNG and
  verify the human-visible result: text readable, controls reachable, intended
  region scrolls, empty status pages do not show gratuitous scrollbars, and
  dense lists do not push chrome off-screen. A screenshot file existing is not
  evidence by itself.
- Tell the human before starting a fresh capture if an existing app instance may need to be closed. `make run` is now a fresh-run path that asks the existing LushText instance to quit and refuses to activate stale code; use an existing-instance watch instead when the human has unsaved work.
- Before interacting with LushText or capturing a visual snapshot, run `scripts/check-lushtext-live.sh`. For fresh debug sessions, include `--session` and `--require-launched-instance` so you do not accidentally drive a pre-existing LushText window.
- If a required interaction or screenshot tool is missing, stop the live-debug flow and ask the human to install it, run `make dev-tools`, or give alternate instructions. Do not silently fall back to a weaker interaction path after discovering a missing tool.
- Before `ydotool` keyboard input, ask the human to focus the LushText debug window. The liveness helper proves the process, not keyboard focus.
- Prefer exported `org.gtk.Actions` over `ydotool` for app interactions. Use `scripts/lushtext-automation.py action ...` when the reusable client can express the cataloged action; otherwise use the automation action catalog to confirm parameter and state signatures, then wait with `dev.cominotti.lushtext.Automation1.WaitForReady` using the narrowest named predicate and assert with `GetSnapshot`. Fall back to `WaitForIdle` only for broad all-workflow settling, and fall back to `ydotool` only for missing operations such as arbitrary text entry into a focused widget.
- Prefer `scripts/atspi-set-text.py` over `ydotool type` for visible editable GTK widgets. It uses the accessibility D-Bus bus and avoids depending on compositor keyboard focus. In restored or deeply nested window layouts, use the script's default deep scan; earlier shallow scans can miss a visible search entry.
- For in-document search setup, prefer the exported `win.set-search-query` string action when the app build includes it. Use AT-SPI editable text only when intentionally testing the visible entry itself or an older build that lacks the target-state action.
- Before screenshot capture, tell the human a GNOME/portal prompt may appear as a blank or white dialog for a few seconds and ask them to approve it if it appears. Treat a timeout after that as a capture permission failure, not as proof of app behavior.
- Before screenshot capture, present the debug-owned LushText instance through D-Bus when possible, then ask the human to keep that window focused. A portal prompt can flash or miss AT-SPI discovery when another window owns focus.
- If the human wants the agent to approve a visible portal prompt, treat that as UI automation, not D-Bus permission bypass. Prefer AT-SPI button invocation only when the visible control exposes a real accessibility action.
- Never use mouse-coordinate fallback for focus, portal approval, screenshot controls, or Shell UI. During this LushText minimap session, GNOME Shell exposed a `Take Screenshot` button through AT-SPI, but synthetic coordinate clicks on that exact accessible still activated the wrong Shell surface.
- Treat `pid-pattern` choice as part of the evidence chain. A bad pattern can make a single-instance handoff look like a fresh launch, or can make the helper appear to be the target process.
- Validate the helper's process snapshots before trusting the launch note. If `process-before.txt` or `process-after.txt` contains `run-gtk-debug-session.sh` or `pgrep`, the PID heuristic is not trustworthy yet.
- For unique GTK apps, distinguish "launcher command ran" from "new instance exists". `cargo run` may still hand off to an existing owner; `make run` should either relaunch the fresh debug binary or fail if the existing owner refuses to close.
- Prefer human-driven reproduction over synthetic action triggering when the user can reproduce the issue reliably. Synthetic actions are useful for narrowing once the live symptom is already understood, not as the default first proof.
- After the first confirmed repro, check whether the app exports `org.gtk.Actions` on a `/.../window/N` object. If it does, use `gdbus call ... org.gtk.Actions.SetState` or `Activate` to replay the exact window action path across fresh launches.
- For geometry warnings during paned animations, do not assume the widget named in the warning is the true root cause. A snapshot wrapper that replaced the opposite pane can under-report the live child's minimum width and make GTK complain while measuring the other side.
- Also allow for the opposite outcome: the warned widget may be the actual `GtkPaned` child host, such as a `GtkStack` used to swap live vs frozen children. In that case, preserving the descendant's width floor is not enough; the host itself must advertise the legal minimum width.
- For installed-Flatpak animation smoothness bugs, separate package freshness from frame-budget churn. First confirm the running/installed Flatpak commit when relevant; then inspect live logs and the allocation hot path. A low-refresh visual effect can come from per-frame `size_allocate()` work such as split-view synchronization, GSettings writes from notify handlers, or repeated `AdwBreakpoint` condition parsing even when no warning or blocking file I/O appears.
- Add tracing surgically and remove it after the question it answered is settled. Good traces expose widget pointers, measured minima, allocation widths, or wrapper identity without turning the whole session into noise.
- Treat D-Bus output as correlation data, not proof by itself. Use it to align focus changes, window churn, portal prompts, and lifecycle events with the terminal warnings.
- Summarize the logs before broad code changes. Repeated signatures are usually more valuable than single noisy lines.
- For GTK and Adwaita warnings, prefer official GNOME docs and official GTK or Libadwaita source for the final explanation. Use this skill to capture evidence, not to guess at toolkit contracts from log text alone.
- Keep artifact directories outside the repo unless the human explicitly wants them checked in.

## Helper Scripts

- `scripts/run-gtk-debug-session.sh`
  - Records a live session into a timestamped artifact directory.
  - Captures PTY output, user journal output, and optional D-Bus traffic.
  - Detects when the launcher exits but a matching app process is still running, which is common with unique GTK applications.
  - Warns when `--pid-pattern` is broad enough to match the helper script or the probe itself, because that invalidates the fresh-launch heuristic.
- `scripts/check-lushtext-live.sh`
  - Checks that LushText is running before live interaction.
  - Verifies required tools before D-Bus actions, input injection, or screenshots.
  - Use `--require-tool pyatspi` before AT-SPI portal-dialog automation.
  - With `--session` and `--require-launched-instance`, refuses to proceed unless the current matching PID was not present before the debug launch.
- `scripts/atspi-click-button.py`
  - Uses system Python AT-SPI bindings to find and invoke a visible button by accessible name.
  - Useful for human-approved portal dialogs where the button is visible but D-Bus cannot bypass the permission prompt.
  - Does not perform coordinate fallback. `--fallback-mouse` is kept only as a disabled compatibility flag because GNOME Shell can route exact-looking coordinates to Overview or top-bar controls.
- `scripts/atspi-set-text.py`
  - Uses system Python AT-SPI bindings to set text on visible editable widgets, scoped by application, role, and optional accessible-name regex.
  - Preferred over `ydotool type` for entries such as LushText's in-tab search field because it does not require keyboard focus to move.
- `scripts/capture-lushtext-mutter.py`
  - First-priority automated visual inspection path.
  - Launches LushText inside an isolated `dbus-run-session` plus `mutter --headless` Wayland monitor with temporary XDG state and keyfile GSettings.
  - Starts PipeWire and WirePlumber in the same session, captures Mutter's existing virtual monitor with `org.gnome.Mutter.ScreenCast.Session.RecordMonitor("Meta-0")`, and saves one PNG through `gst-launch-1.0 pipewiresrc`.
  - For search repros, drives the exported `win.set-search-query` action, waits for `search-complete`, optionally waits for `--expected-search-matches`, and records the Automation1 snapshot before screenshot capture.
  - Use repeated `--wait-predicate` flags when a scenario needs an extra Automation1 readiness gate such as `workspace-refresh-complete`.
  - Use repeated `--window-string-action ACTION=TEXT` flags when a visible workflow exposes a string-parameter action, such as filtering Browse Notes through `set-notes-browser-query=Visual note`.
  - Use repeated `--wait-window-action` flags when a dialog action must become enabled before AT-SPI tree capture, such as `set-notes-browser-query`.
  - Use repeated `--wait-atspi-text` flags when the best readiness proof is visible dialog text, such as `No notes yet`.
- `scripts/capture-lushtext-xvfb.sh`
  - Fallback isolated display when headless Mutter or PipeWire capture is unavailable.
  - Launches a debug-owned LushText process in an isolated `dbus-run-session` + Xvfb display with temporary XDG state.
  - Uses D-Bus window actions for exported app behavior, then confines any `xdotool` typing to that private display.
  - Captures screenshots with `xwd` and ImageMagick, avoiding live GNOME focus and portal permission races.
- `scripts/capture-screenshot.py`
  - Tries the best available screenshot path.
  - Applies the requested timeout to both direct `gnome-screenshot` and portal capture paths.
  - Supports `--portal-only --non-interactive` for sessions where direct `gnome-screenshot` hangs or the interactive Shell UI is unsafe to automate.
  - Uses the desktop portal before failing and subscribes to the portal `Response` signal before sending the request to avoid missing fast responses.
  - Returns a clear error when the desktop blocks or times out the request.
- `scripts/summarize-runtime-logs.py`
  - Groups repeated warnings and criticals.
  - Extracts high-signal D-Bus bursts.
  - Emits a short markdown report for fast triage.

## References

- [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md): live capture workflow, PTY guidance, session safety, and D-Bus strategy
- [references/log-patterns.md](references/log-patterns.md): common GTK, GLib, Adwaita, and D-Bus signatures, including geometry warnings like `Trying to measure GtkBox ... needs at least ...`
- [../gtk4-libadwaita-internals/references/warnings-and-criticals.md](../gtk4-libadwaita-internals/references/warnings-and-criticals.md): authoritative GTK and Adwaita warning atlas with upstream source paths
