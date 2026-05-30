# GTK Runtime Debugging Playbook

## Purpose

Use this playbook when the failure only becomes visible while a GTK app is running on a real desktop session.

## Capture Strategy

1. **Confirm the session context**
   - Record `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, and `DBUS_SESSION_BUS_ADDRESS`.
   - Probe tool availability before assuming screenshot, D-Bus, or input-injection helpers will work.
   - If any tool required for the next live action is missing, pause and ask the human to install it, run `make dev-tools`, or provide a different path.
2. **Decide between fresh launch and existing-instance watch**
   - Fresh launch is best when startup logs matter.
   - Existing-instance watch is safer when the user already has unsaved work in the app.
   - `make run` is the fresh-launch path for LushText: it asks any already-running `dev.cominotti.lushtext` owner to quit before launching the freshly built debug binary, and fails if that owner refuses to close.
   - If the user may have unsaved work, prefer existing-instance watch mode instead of starting a fresh `make run`.
   - Do not treat `cargo run` printing `Running target/debug/...` as proof that you are observing a newly launched GUI process.
3. **Keep the launcher in a PTY**
   - Prefer `functions.exec_command` with `tty: true`.
   - Poll the session with `write_stdin` instead of restarting the app for every question.
   - Use the helper runner so stdout, stderr, journal output, and D-Bus traffic land in one artifact directory.
4. **Choose a tight pid pattern**
   - Prefer an anchored executable regex such as `(^| )target/debug/lushtext($| )`.
   - Avoid broad alternations or loose substrings such as `target/debug/lushtext|dev.cominotti.lushtext`.
   - After launch, inspect `process-before.txt`, `process-after.txt`, and `status.txt`. If they mention `run-gtk-debug-session.sh` or `pgrep`, tighten the pattern before trusting the PID-based launch note.
5. **Verify the target before interaction**
   - Before D-Bus actions, input injection, screenshots, or asking the human to type into the debug window, run `scripts/check-lushtext-live.sh`.
   - For a fresh debug launch, include `--session <artifact-dir>` and `--require-launched-instance` so the current PID must differ from the pre-launch PID set.
   - Include `--require-dbus` before `org.gtk.Actions` calls.
   - Include each tool needed for the next action, such as `--require-tool gdbus`, `--require-tool ydotool`, `--require-tool gnome-screenshot`, or `--require-tool python3`.
   - If the check fails, do not interact with the app or capture a snapshot. Resolve the missing tool, missing process, or wrong-instance proof first.
   - For `ydotool`, the daemon matters as much as the binary. If background daemons are reaped when shell commands exit, run `ydotoold` in its own PTY session for the duration of the live debug run.
6. **Prefer app D-Bus actions before synthetic input**
   - Inspect `org.gtk.Actions` on the application and window objects before using `ydotool`.
   - Use exported actions for real app behavior, such as opening search or toggling UI state.
   - For visible editable widgets, try AT-SPI editable-text automation next. Example: activate `win.begin-search`, then run `scripts/atspi-set-text.py --application-regex '^lushtext$' --role-regex '^entry$' --text needle`.
   - If a CLI-opened file restores behind older session tabs, call `org.freedesktop.Application.Open` with that file's URI to exercise LushText's duplicate-tab activation path before opening search.
   - Restored tab strips can nest the search entry deeper than a shallow AT-SPI walk. Use the helper defaults or pass `--max-depth 30 --max-nodes 20000`.
   - Use `ydotool` only when neither app actions nor AT-SPI can express the operation.
   - If a workflow needs repeatable automation often and AT-SPI is too ambiguous, consider adding an explicit app action or test-only helper instead of relying on keyboard injection.
7. **Let the human reproduce the bug**
   - The human can interact with the real window while the capture session stays open.
   - When no input injection tool is installed, this is the most reliable path.
   - After the first manual proof, check whether the app exposes a window-scoped `org.gtk.Actions` object such as `/dev/cominotti/lushtext/window/1`. Driving that action path over D-Bus is often the cleanest way to rerun the exact behavior after a rebuild.
8. **Summarize before opening everything**
   - Read `summary.md` first.
   - Open raw logs only around the relevant timestamps or repeated signatures.
9. **Match the warned widget to the real widget tree when geometry is involved**
   - If the warning includes a widget pointer such as `GtkBox 0x...`, match that pointer against the actual widgets in the live tree before deciding what is broken.
   - In paned animations, the warned end-child widget can be only the symptom. A start-child `GtkPicture` or other snapshot wrapper that under-reports the live child's minimum width can cause the end child to be measured illegally.
   - The warned widget can also be the real paned child host. If the pointer resolves to a `GtkStack` or similar host sitting directly under `GtkPaned`, verify that the host itself carries the same legal minimum width as the live pane it wraps.

## Preferred Live Debug Loop

When the user can reproduce the issue interactively, prefer this loop:

1. Launch a fresh real app instance under the helper.
2. Let the human reproduce the bug in the actual window.
3. Keep the PTY open and watch the first live warning burst or symptom.
4. Add one narrow trace that answers the next missing question.
   Good traces:
   - widget pointer identities
   - measured minima and allocation widths for the specific widgets in play
   - snapshot or wrapper identity swaps
5. Relaunch and reproduce again on the real app instance.
6. Compare the new traces to the warning timestamps and only then narrow the fix.

This workflow is usually superior to broad speculative edits. It is also usually better than starting with synthetic action calls when the user already has a reliable manual repro path.

## What the Helper Captures

- `app.typescript`: PTY transcript from the launcher command, preserving stdout and stderr ordering.
- `dbus.log`: filtered D-Bus traffic, focused on GNOME Shell and portal activity by default.
- `journal.log`: `journalctl --user -f` output from the capture window.
- `session.env`: capture metadata and tool paths.
- `process-before.txt` and `process-after.txt`: snapshots of the process state around the run.
- `summary.md`: condensed report generated by the summarizer.

## Fresh Launch Heuristics

- If the launcher exits and `pid-pattern` still matches a process:
  - Compare the PID set before and after launch.
  - If the sets are identical, inspect launcher output for a handoff, failed relaunch, or stale pid pattern before deciding what happened.
  - If the post-launch set includes a new PID, treat it as a newly launched app that detached from the launcher.
- These heuristics are only trustworthy if the pattern matched the app and not the debugging machinery. A contaminated `pid-pattern` can make the helper shell look like the target process.
- Do not kill matching processes automatically. Surface the finding and let the human choose.
- During an active session, use `check-lushtext-live.sh --session <artifact-dir> --require-launched-instance` before interaction. It compares the current matching PID set with the pre-launch PID set captured in `session.env`, which catches the common mistake of driving a stale instance that was already running.

## Screenshot Strategy

- For repeatable agent-owned visual checks, prefer the headless Mutter helper first:
  `scripts/capture-lushtext-mutter.py --file PATH --search TEXT --enable-minimap --output PATH`.
  It runs LushText inside an isolated `dbus-run-session` and `mutter --headless`
  Wayland monitor, stores app state in temporary XDG directories with
  `GSETTINGS_BACKEND=keyfile`, drives exported window actions over D-Bus, sets
  visible editable text through a private AT-SPI registry, and captures the
  existing Mutter monitor through PipeWire/GStreamer. This matches the CI
  compositor family and avoids live-desktop focus, Shell Overview, and portal
  approval races.
- The proven Mutter screenshot path is `org.gnome.Mutter.ScreenCast.Session.RecordMonitor("Meta-0", {"is-recording": true})` followed by `Start` and `gst-launch-1.0 pipewiresrc path=<node> num-buffers=1 ! videoconvert ! pngenc ! filesink location=...`. Do not use `RecordVirtual` for screenshots of the app monitor; during the minimap investigation it created a separate 1x1 stream instead of the visible virtual monitor.
- The isolated runtime directory must be mode `0700`, and PipeWire plus WirePlumber must run in the same private D-Bus/XDG runtime session as Mutter. A looser runtime dir can leave PipeWire unavailable to `pipewiresrc`.
- For stripped headless sessions that need AT-SPI, activate `org.a11y.Bus`, set `org.a11y.Status.IsEnabled` to true, and start `/usr/libexec/at-spi2-registryd --dbus-name org.a11y.atspi.Registry` on the normal session bus. Do not run registryd with `DBUS_SESSION_BUS_ADDRESS` pointed at the accessibility bus; registryd itself needs the normal session bus to discover that address.
- If headless Mutter is unavailable, fall back to `scripts/capture-lushtext-xvfb.sh --file PATH --search TEXT --enable-minimap --output PATH`. It runs LushText inside an isolated `dbus-run-session` and Xvfb display, stores app state in temporary XDG directories with `GSETTINGS_BACKEND=keyfile`, drives exported window actions over D-Bus, confines `xdotool` typing to the private display, and captures with `xwd` + ImageMagick. This is less compositor-faithful than Mutter.
- For the human's live desktop, prefer desktop-approved capture paths, not compositor-specific hacks.
- Before calling the screenshot helper, run `check-lushtext-live.sh` with `--require-launched-instance` for fresh sessions and `--require-tool gnome-screenshot --require-tool python3`.
- Bound both direct `gnome-screenshot` and portal attempts with timeouts. Desktop screenshot tools can hang before a portal response is available.
- In Fedora Toolbx on Wayland, prefer `capture-screenshot.py --portal-only --non-interactive` once the target app is visually frontmost. This path saved a PNG without opening GNOME Shell's screenshot UI during the LushText minimap regression run.
- Direct `gnome-screenshot -f` can fail in Toolbx after `Unable to use GNOME Shell's builtin screenshot interface, resorting to fallback X11`, followed by zero-sized GDK surfaces. Treat that as a known-bad path for the session and move to the portal helper.
- The helper subscribes to the portal request `Response` signal before issuing the screenshot call. Keep that ordering if changing the script; a post-call `gdbus monitor` can miss fast portal responses.
- Present the debug-owned LushText instance through `org.gtk.Application.Activate`, `org.freedesktop.Application.Activate`, `org.freedesktop.Application.Open`, or AT-SPI editable focus before opening the portal when possible. `org.gnome.Shell.FocusApp`, `GetWindows`, and `GetRunningApplications` may return `AccessDenied` from unprivileged Toolbx clients.
- Remember that both portal screenshots and GNOME Shell screenshots are D-Bus calls, but they still obey compositor security. `AccessDenied` and approval timeouts are expected possible outcomes, not tool syntax failures.
- The portal approval button is a UI surface. If the human consents to agent approval, use AT-SPI D-Bus only when the visible control exposes a real invokable action.
  - Before AT-SPI automation, run `check-lushtext-live.sh ... --require-tool pyatspi`.
  - Use `scripts/atspi-click-button.py --name-regex 'Screenshot|Share|Allow'` for visible portal approval buttons only when it reports a real action invocation.
  - Do not use coordinate fallback for portal approval. During the LushText minimap run, a visible GNOME Shell `Take Screenshot` accessible exposed coordinates but no action, and synthetic coordinate clicks triggered the wrong Shell surface.
  - Do not use coordinate clicks to focus app windows. GNOME Shell may expose broad frame extents, and clicks near the top of a maximized frame can activate Shell UI such as Overview or quick settings. If activation does not focus the target, ask the human to focus it.
- Expect one of three outcomes:
  - success and a saved image path
  - a permission or policy denial
  - a timeout because the portal prompt was not accepted
- If screenshots fail, keep using the runner plus the human’s verbal confirmation. Terminal logs and D-Bus output are still valuable.

## D-Bus Reading Guide

- `org.gtk.Actions` on `/.../window/N`
  - Useful for replaying the exact action path after the first manual repro, especially for single-instance apps that are awkward to relaunch and click through repeatedly.
  - Prefer this over ad hoc synthetic input when the bug is already narrowed to a specific exported action such as `toggle-sidebar`.
  - Current LushText search automation can open the search UI with `begin-search`, but D-Bus alone cannot set the visible search query. Use AT-SPI editable text today, and report a missing app action such as `begin-search-with-text` if a pure D-Bus path is required.
- `org.gnome.Shell.Introspect.WindowsChanged`
  - Often spikes during map, unmap, focus, and workspace transitions.
  - Useful for correlating “something on screen changed” with GTK warnings.
- `org.freedesktop.portal.*`
  - Indicates screenshot or screencast request lifecycles.
  - Helpful when visual capture prompts appear or stall.
- `NameOwnerChanged`
  - Useful when a service restarts or disappears during the repro.

## Mapping Logs Back to Code

1. Identify the dominant signature from `summary.md`.
2. Search for the affected widget or action with `rg`.
3. For geometry warnings, inspect size negotiation, revealers, paned positions, min-content widths, and animation endpoints.
   - If a snapshot surface replaced a live child during the animation, verify that the snapshot host preserves the live child's minimum width contract.
   - If the warning names a `GtkStack` or similar stable host directly, verify that you are fixing the width floor on that host and not only on a descendant such as the inner `GtkBox`.
   - If a stable host such as `GtkStack` only exists to swap live and frozen children, verify that its own transition type and duration are disabled unless a second animation is intentional.
   - If the frozen image shows up as black or empty, do not assume the swap logic is wrong first; confirm the cached snapshot itself is visually valid and not merely non-null.
   - If a one-shot capture still yields a black frozen pane, compare it against a persistent `GtkWidgetPaintable` observer and its warmed `current_image()`. The issue may be snapshot validity, not only swap timing.
   - Also ask when the snapshot is generated. A synchronous snapshot on the click path can remove one bug while introducing visible hide-time stutter.
   - Do not assume both panes need the same freeze strategy. If only the sidebar subtree is expensive, freezing the content pane can introduce stretching, black frames, or end-of-animation seams without buying meaningful smoothness.
   - Prefer adding surgical traces and rerunning the live repro over trying to infer the entire widget tree from static code alone.
4. For lifecycle warnings, inspect weak refs, signal disconnects, and object disposal paths.
5. For portal or shell-related symptoms, inspect whether the app is waiting on user-session state instead of its own business logic.
