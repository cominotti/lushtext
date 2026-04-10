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

Prefer a capture session over ad hoc commands. Run the helper through `functions.exec_command` with `tty: true`, then keep polling with `write_stdin` while the human interacts with the app window.

```bash
.agents/skills/gtk-agentic-debugging/scripts/run-gtk-debug-session.sh \
  --cmd "make run" \
  --pid-pattern '(^| )target/debug/lushtext($| )'
```

Use a tight executable regex for `--pid-pattern`. Avoid broad alternations or loose substrings such as `target/debug/lushtext|dev.cominotti.lushtext` because they can match the helper script or even the `pgrep` probe itself, which makes the launch heuristics misleading.

After the reproduction, inspect the generated `summary.md`, then open the raw `app.typescript`, `dbus.log`, and `journal.log` files only as needed.

## Choose the Right Mode

- **Fresh launch**: Use when stdout and stderr from startup matter. First check whether the app is already running. For `gio::Application` or `adw::Application` apps, a second launch may only activate the existing instance and exit immediately. Do not treat `cargo run` or `make run` printing `Running target/debug/...` as proof that a new GUI process or a new window was created.
- **Existing instance watch**: Use when the app is already open and the user can reproduce the bug in that window. This is usually the safer and more truthful mode for single-instance GTK apps. Keep the capture session open, collect journal and D-Bus output, and let the human drive the UI.
- **Screenshot assist**: Use when you need to confirm what the human sees on screen. Run `scripts/capture-screenshot.py` and be ready for a desktop permission prompt or timeout.
- **Log triage only**: Use `scripts/summarize-runtime-logs.py` on an existing artifact directory when the session has already been recorded.

## Workflow

1. Read [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md) for the live-session workflow and tool limits.
2. Choose an exact `--pid-pattern` for the target executable before launching the helper. Prefer anchored executable matches over app IDs or loose substrings.
3. Start `scripts/run-gtk-debug-session.sh` in a PTY-backed session.
4. Inspect `process-before.txt`, `process-after.txt`, and `status.txt` early. If they mention `run-gtk-debug-session.sh` or `pgrep`, the capture is contaminated and you should tighten the pattern before drawing conclusions.
5. Let the human reproduce the bug while you poll the session and read warnings as they appear.
6. If visual confirmation matters, run `scripts/capture-screenshot.py` and note whether the desktop allowed or denied the request.
7. Run `scripts/summarize-runtime-logs.py` or read the auto-generated `summary.md`.
8. Use [references/log-patterns.md](references/log-patterns.md) to map the signature to likely GTK, GLib, Adwaita, or D-Bus causes.
9. If step 8 points to a toolkit invariant rather than an app-specific state bug, hand the interpretation to `gtk4-libadwaita-internals` before proposing a fix.

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

This is the preferred workflow over broad speculative code changes. For geometry bugs especially, "launch real app -> human reproduces -> inspect live warnings -> add narrow tracing -> pointer-match the real widgets -> rerun" is usually faster and more trustworthy than static reasoning alone.

## Required Habits

- Prefer `tty: true` for the live runner. PTY-backed sessions are the most reliable way to preserve stdout and stderr ordering.
- Tell the human before starting a fresh capture if an existing app instance may need to be closed. Do **not** kill an existing GUI instance unless the human explicitly asks for that.
- Treat `pid-pattern` choice as part of the evidence chain. A bad pattern can make a single-instance handoff look like a fresh launch, or can make the helper appear to be the target process.
- Validate the helper's process snapshots before trusting the launch note. If `process-before.txt` or `process-after.txt` contains `run-gtk-debug-session.sh` or `pgrep`, the PID heuristic is not trustworthy yet.
- For unique GTK apps, distinguish "launcher command ran" from "new instance exists". `make run` may rebuild and invoke the launcher while the already-running app window is the one still being observed.
- Prefer human-driven reproduction over synthetic action triggering when the user can reproduce the issue reliably. Synthetic actions are useful for narrowing once the live symptom is already understood, not as the default first proof.
- For geometry warnings during paned animations, do not assume the widget named in the warning is the true root cause. A snapshot wrapper that replaced the opposite pane can under-report the live child's minimum width and make GTK complain while measuring the other side.
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
- `scripts/capture-screenshot.py`
  - Tries the best available screenshot path.
  - Uses the desktop portal before failing.
  - Returns a clear error when the desktop blocks or times out the request.
- `scripts/summarize-runtime-logs.py`
  - Groups repeated warnings and criticals.
  - Extracts high-signal D-Bus bursts.
  - Emits a short markdown report for fast triage.

## References

- [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md): live capture workflow, PTY guidance, session safety, and D-Bus strategy
- [references/log-patterns.md](references/log-patterns.md): common GTK, GLib, Adwaita, and D-Bus signatures, including geometry warnings like `Trying to measure GtkBox ... needs at least ...`
- [../gtk4-libadwaita-internals/references/warnings-and-criticals.md](../gtk4-libadwaita-internals/references/warnings-and-criticals.md): authoritative GTK and Adwaita warning atlas with upstream source paths
