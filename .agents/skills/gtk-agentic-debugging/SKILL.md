---
name: gtk-agentic-debugging
description: "Reproduce and observe a running LushText: headless-Mutter captures, live sessions kept in a PTY, journal and D-Bus logs, AT-SPI input and portal screenshots. Use for bugs that only appear while the app runs (live warnings from make run, visual, focus, sizing, animation, portal or compositor issues); hand toolkit-contract questions to gtk4-libadwaita-internals."
---

# GTK Agentic Debugging

Use this skill when static code reading is not enough and the bug only becomes obvious in a live GTK session.

## Host and Toolbx Boundary

Run desktop-session operations on the Fedora host: user-session D-Bus, AT-SPI,
portals, screenshots, `journalctl --user`, Wayland/X11 input, PipeWire, and
Mutter/Xvfb capture. Toolbx is appropriate for builds and non-session tests,
but its Python and environment may not expose the host accessibility bindings
or desktop bus correctly. If the current shell is inside Toolbx, leave it for
live helpers or use an explicit, verified host-command bridge; do not infer host
readiness from tools found only inside the container. The AT-SPI helpers use
their `/usr/bin/python3` shebang and require the host `pyatspi` package.

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

This launches LushText on a private `mutter --headless` Wayland monitor, isolates XDG data/config/cache plus keyfile GSettings, sets in-document search through the exported `win.set-search-query` D-Bus action, waits through Automation1 readiness predicates, saves `automation-snapshot.json`, and captures the monitor through Mutter's `RecordMonitor("Meta-0")` screencast stream. Prefer this before touching the human's live GNOME session or falling back to Xvfb. For renamed or relocated builds, pass its identity overrides (see [references/helper-scripts.md](references/helper-scripts.md)); never patch product identities into the helper for a one-off fixture.

For the human's live session, record a capture session instead of ad hoc commands. Run the helper in a PTY-backed session the runner keeps open (for example Codex's `functions.exec_command` with `tty: true`, polled with `write_stdin`) while the human interacts with the app window.

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

If headless Mutter is unavailable, `scripts/capture-lushtext-xvfb.sh` is the lower-fidelity isolated fallback. After the reproduction, inspect the generated `summary.md`, then open the raw `app.typescript`, `dbus.log`, and `journal.log` files only as needed.

## Choose the Right Mode

- **Fresh launch**: Use when stdout and stderr from startup matter. First check whether the app is already running because `make run` intentionally asks the existing LushText owner to quit before relaunching the fresh debug binary, and fails if that owner refuses to close. Do not treat `cargo run` printing `Running target/debug/...` as proof that a new GUI process or a new window was created.
- **Existing instance watch**: Use when the app is already open and the user can reproduce the bug in that window. This is usually the safer and more truthful mode for single-instance GTK apps. Keep the capture session open, collect journal and D-Bus output, and let the human drive the UI.
- **Screenshot assist**: Use when you need to confirm what the human sees on screen. For agent-owned repros, first try `scripts/capture-lushtext-mutter.py` so focus, portal approval, and Shell UI cannot interfere. For the human's live desktop, run `check-lushtext-live.sh` first with `--require-tool gnome-screenshot --require-tool python3`; if anything is missing, stop and ask for installation or alternative instructions. Then run `scripts/capture-screenshot.py` and be ready for a desktop permission prompt or timeout.
- **Log triage only**: Use `scripts/summarize-runtime-logs.py` on an existing artifact directory when the session has already been recorded.

## Workflow

1. Read [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md) for the live-session workflow and tool limits; its "Preferred Live Debug Loop" (human reproduces, watch live warnings, add one narrow trace, relaunch, pointer-match the real widgets, then fix) is the default investigation loop.
2. Choose an exact `--pid-pattern` for the target executable before launching the helper. Prefer anchored executable matches over app IDs or loose substrings.
3. Start `scripts/run-gtk-debug-session.sh` in a PTY-backed session.
4. Before any D-Bus action, input injection, screenshot, or request for the human to type into the window, run `scripts/check-lushtext-live.sh` with `--session`, `--require-launched-instance`, and every `--require-tool` needed for the next action.
5. Inspect `process-before.txt`, `process-after.txt` when present, and `status.txt`. If they mention `run-gtk-debug-session.sh` or `pgrep`, the capture is contaminated and you should tighten the pattern before drawing conclusions.
6. Let the human reproduce the bug while you poll the session and read warnings as they appear.
7. If visual confirmation matters, capture the relevant state extreme, not just
   any reachable screen. Prefer no items/no required context for empty-state
   regressions, and many or awkward items for overflow, clipped controls,
   virtualization, or scrolling regressions. When practical, capture the
   opposing state too so the fix is not tuned only to one end of the matrix. If
   the claim is that some chrome must remain pixel-identical while another
   surface changes, prefer the same-session visual geometry runner over
   independent one-off screenshots.
8. Run `scripts/capture-screenshot.py --portal-only --non-interactive` only after the liveness/tool check succeeds, and note whether the desktop allowed or denied the request.
9. Run `scripts/summarize-runtime-logs.py` or read the auto-generated `summary.md`.
10. Use [references/log-patterns.md](references/log-patterns.md) to map the signature to likely GTK, GLib, Adwaita, or D-Bus causes.
11. If step 10 points to a toolkit invariant rather than an app-specific state bug, hand the interpretation to `gtk4-libadwaita-internals` before proposing a fix.

## Required Habits

Session and evidence:

- For automated visual inspection, prefer the headless Mutter helper before live-desktop portal screenshots and before Xvfb. Mutter matches the CI compositor path and avoids stealing the human's focus.
- Keep headless Mutter/PipeWire runtime directories short, directly under
  `/tmp` or `$XDG_RUNTIME_DIR`, instead of nesting them under long artifact
  roots. PipeWire and Wayland sockets hit Unix socket path limits before the
  app launches, so a deep case directory can invalidate an otherwise-good
  visual proof run.
- Tell the human before starting a fresh capture if an existing app instance may need to be closed. `make run` is a fresh-run path that asks the existing LushText instance to quit and refuses to activate stale code (mechanics: `docs/runbooks/dev-desktop-staging.md`); use an existing-instance watch instead when the human has unsaved work.
- If a required interaction or screenshot tool is missing, stop the live-debug flow and ask the human to install it, run `make dev-tools`, or give alternate instructions. Do not silently fall back to a weaker interaction path after discovering a missing tool.
- Treat `pid-pattern` choice as part of the evidence chain. A bad pattern can make a single-instance handoff look like a fresh launch, or can make the helper appear to be the target process; process snapshots listing the helper or `pgrep` void the heuristic (Workflow step 5).
- Prefer human-driven reproduction over synthetic action triggering when the user can reproduce the issue reliably. Synthetic actions are useful for narrowing once the live symptom is already understood, not as the default first proof.
- Add tracing surgically and remove it after the question it answered is settled. Good traces expose widget pointers, measured minima, allocation widths, or wrapper identity without turning the whole session into noise.
- Treat D-Bus output as correlation data, not proof by itself. Use it to align focus changes, window churn, portal prompts, and lifecycle events with the terminal warnings.
- Summarize the logs before broad code changes. Repeated signatures are usually more valuable than single noisy lines.
- For GTK and Adwaita warnings, prefer official GNOME docs and official GTK or Libadwaita source for the final explanation. Use this skill to capture evidence, not to guess at toolkit contracts from log text alone.
- Keep artifact directories outside the repo unless the human explicitly wants them checked in.

Input and focus:

- Prefer exported `org.gtk.Actions` over `ydotool` for app interactions. Use `scripts/lushtext-automation.py action ...` when the reusable client can express the cataloged action; otherwise use the automation action catalog to confirm parameter and state signatures, then wait with `dev.cominotti.lushtext.Automation1.WaitForReady` using the narrowest named predicate and assert with `GetSnapshot`. Fall back to `WaitForIdle` only for broad all-workflow settling, and fall back to `ydotool` only for missing operations such as arbitrary text entry into a focused widget.
- Prefer `scripts/atspi-set-text.py` over `ydotool type` for visible editable GTK widgets. It uses the accessibility D-Bus bus and avoids depending on compositor keyboard focus. In restored or deeply nested window layouts, use the script's default deep scan; a shallow scan can miss a visible search entry.
- For in-document search setup, prefer the exported `win.set-search-query` string action. Use AT-SPI editable text only when intentionally testing the visible entry itself or an older build that lacks the target-state action.
- `ydotool` is ready only when both the binary and the daemon are usable (socket `${XDG_RUNTIME_DIR}/.ydotool_socket` unless `YDOTOOL_SOCKET` is set); if the runner reaps background processes, keep `ydotoold` in its own PTY-backed session. Before `ydotool` keyboard input, ask the human to focus the LushText debug window and rerun the liveness check: `ydotool` targets the compositor's focused surface, and the liveness helper proves the process, not keyboard focus.
- Never use mouse-coordinate fallback for focus, portal approval, screenshot controls, or Shell UI. GNOME Shell can report broad or full-screen frame extents through AT-SPI, and a coordinate click near a maximized frame's top or on an action-less accessible such as Shell's `Take Screenshot` button reaches a different Shell surface (Overview, quick settings). If D-Bus/AT-SPI activation does not focus the target window, ask the human to focus it.

Screenshots:

- Before screenshot capture, tell the human a GNOME/portal prompt may appear as a blank or white dialog for a few seconds and ask them to approve it if it appears. Treat a timeout after that as a capture permission failure, not as proof of app behavior.
- Before screenshot capture, present the debug-owned LushText instance through D-Bus when possible, then ask the human to keep that window focused. A portal prompt can flash or miss AT-SPI discovery when another window owns focus.
- If the human wants the agent to approve a visible portal prompt, treat that as UI automation, not D-Bus permission bypass. Prefer AT-SPI button invocation only when the visible control exposes a real accessibility action.
- When the symptom happens during an animation, require stream-frame evidence with mapped intermediate frames; a final settled screenshot can miss it.
- A screenshot file existing is not evidence by itself: open the PNG and check the human-visible result. Pixel invariants, the state matrix and live-size capture follow `.agents/rules/testing.md`, "Proof Obligations of a Change" (visual proof), `widget-wiring.md`, "State-extreme coverage", and `ui.md`, "State Extremes And Visibility Matrix" (prefer `make visual-geometry-smoke`, and `scripts/lushtext-automation.py visual-geometry-capture` for a live-only geometry bug).
- For installed-Flatpak animation smoothness bugs, separate package freshness from frame-budget churn: first confirm the running/installed Flatpak commit, then inspect live logs and the allocation hot path (`widget-wiring.md`, "Size-Dependent Constraints"); a low-refresh effect can come from per-frame allocation work with no warning at all.
- For geometry warnings, pointer-match the warned widget against the live tree before deciding which widget is wrong; the warned widget is often only where GTK noticed the invalid width (geometry reference, "Snapshot Surfaces Used As Paned Children").

## References

- [references/runtime-debugging-playbook.md](references/runtime-debugging-playbook.md): live capture workflow, PTY guidance, session safety, and D-Bus strategy
- [references/helper-scripts.md](references/helper-scripts.md): what each helper script does, its safety limits, and where its flags are documented
- [references/log-patterns.md](references/log-patterns.md): common GTK, GLib, Adwaita, and D-Bus signatures, including geometry warnings like `Trying to measure GtkBox ... needs at least ...`
- [../gtk4-libadwaita-internals/references/warnings-and-criticals.md](../gtk4-libadwaita-internals/references/warnings-and-criticals.md): authoritative GTK and Adwaita warning atlas with upstream source paths
