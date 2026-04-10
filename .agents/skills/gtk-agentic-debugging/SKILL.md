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
  --pid-pattern "target/debug/lushtext"
```

After the reproduction, inspect the generated `summary.md`, then open the raw `app.typescript`, `dbus.log`, and `journal.log` files only as needed.

## Choose the Right Mode

- **Fresh launch**: Use when stdout and stderr from startup matter. First check whether the app is already running. For `gio::Application` or `adw::Application` apps, a second launch may only activate the existing instance and exit immediately.
- **Existing instance watch**: Use when the app is already open and the user can reproduce the bug in that window. Keep the capture session open, collect journal and D-Bus output, and let the human drive the UI.
- **Screenshot assist**: Use when you need to confirm what the human sees on screen. Run `scripts/capture-screenshot.py` and be ready for a desktop permission prompt or timeout.
- **Log triage only**: Use `scripts/summarize-runtime-logs.py` on an existing artifact directory when the session has already been recorded.

## Workflow

1. Read [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md) for the live-session workflow and tool limits.
2. Start `scripts/run-gtk-debug-session.sh` in a PTY-backed session.
3. Let the human reproduce the bug while you poll the session and read warnings as they appear.
4. If visual confirmation matters, run `scripts/capture-screenshot.py` and note whether the desktop allowed or denied the request.
5. Run `scripts/summarize-runtime-logs.py` or read the auto-generated `summary.md`.
6. Use [references/log-patterns.md](references/log-patterns.md) to map the signature to likely GTK, GLib, Adwaita, or D-Bus causes.
7. If step 6 points to a toolkit invariant rather than an app-specific state bug, hand the interpretation to `gtk4-libadwaita-internals` before proposing a fix.

## Required Habits

- Prefer `tty: true` for the live runner. PTY-backed sessions are the most reliable way to preserve stdout and stderr ordering.
- Tell the human before starting a fresh capture if an existing app instance may need to be closed. Do **not** kill an existing GUI instance unless the human explicitly asks for that.
- Treat D-Bus output as correlation data, not proof by itself. Use it to align focus changes, window churn, portal prompts, and lifecycle events with the terminal warnings.
- Summarize the logs before broad code changes. Repeated signatures are usually more valuable than single noisy lines.
- For GTK and Adwaita warnings, prefer official GNOME docs and official GTK or Libadwaita source for the final explanation. Use this skill to capture evidence, not to guess at toolkit contracts from log text alone.
- Keep artifact directories outside the repo unless the human explicitly wants them checked in.

## Helper Scripts

- `scripts/run-gtk-debug-session.sh`
  - Records a live session into a timestamped artifact directory.
  - Captures PTY output, user journal output, and optional D-Bus traffic.
  - Detects when the launcher exits but a matching app process is still running, which is common with unique GTK applications.
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
