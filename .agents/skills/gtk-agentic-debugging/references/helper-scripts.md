# Helper Scripts

## Contents

- [Live Sessions](#live-sessions)
- [AT-SPI Helpers](#at-spi-helpers)
- [Isolated Captures](#isolated-captures)
- [Screenshots and Log Summaries](#screenshots-and-log-summaries)

The helper catalogue of the [gtk-agentic-debugging skill](../SKILL.md). Every
path is relative to the skill directory. Each script's role, precedence and
safety limits are here; the per-flag table for the scenario helpers (for
example `capture-lushtext-mutter.py`) is in
[docs/automation-reference.md](../../../../docs/automation-reference.md#scenario-helper-flags),
which `make check-automation-docs` keeps in sync with the scripts.

## Live Sessions

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

## AT-SPI Helpers

- `scripts/atspi-click-button.py`
  - Uses system Python AT-SPI bindings to find and invoke a visible button by accessible name.
  - Useful for human-approved portal dialogs where the button is visible but D-Bus cannot bypass the permission prompt.
  - Does not perform coordinate fallback. `--fallback-mouse` is kept only as a disabled compatibility flag because GNOME Shell can route exact-looking coordinates to Overview or top-bar controls.
- `scripts/atspi-accessible-action.py`
  - Finds one visible accessible by application, name, and role regex, then
    focuses it, invokes its exported action, sends a requested key, or performs
    an explicit AT-SPI context click.
  - Prefer `focus` or `activate`; use `context-click` only on an isolated or
    human-approved surface because it synthesizes pointer input from reported
    accessible extents.
- `scripts/atspi-dump-tree.py`
  - Writes a bounded accessibility-tree dump and a separate focused-node report
    for one application regex.
  - Use it to prove roles, names, visibility, actions, text state, and focus
    before choosing an AT-SPI interaction. Keep bounds finite on large trees.
- `scripts/atspi-set-text.py`
  - Uses system Python AT-SPI bindings to set text on visible editable widgets, scoped by application, role, and optional accessible-name regex.
  - Preferred over `ydotool type` for entries such as LushText's in-tab search field because it does not require keyboard focus to move.

## Isolated Captures

- `scripts/capture-lushtext-mutter.py`
  - First-priority automated visual inspection path.
  - Launches LushText inside an isolated `dbus-run-session` plus `mutter --headless` Wayland monitor with temporary XDG state and keyfile GSettings.
  - Starts PipeWire and WirePlumber in the same session, captures Mutter's existing virtual monitor with `org.gnome.Mutter.ScreenCast.Session.RecordMonitor("Meta-0")`, and saves one PNG through `gst-launch-1.0 pipewiresrc`.
  - Discovers its repository through Git and resolves bundled AT-SPI helpers relative to its own script directory. For renamed or relocated builds, pass `--repo-root`, `--binary`, `--app-id`, `--app-object-path`, `--automation-interface`, `--gsettings-schema`, and `--gsettings-schema-dir`, or use the matching `LUSHTEXT_DEBUG_*` environment variables. Never patch product identities into the helper for a one-off fixture.
  - Drives scenarios through exported actions, Automation1 readiness predicates, AT-SPI text waits and ordered `--step KIND:VALUE` sequences; see the flag table. AT-SPI text waits keep one fixed deadline (`ATSPI_TEXT_WAIT_SECONDS`) and bound each probe only by the time left, never by a shorter per-probe cap (reasons and measurements: the script's header comment). `automation-waits.txt` records `probe_ms=` per probe, and the probe's stdout records `walk_ms=` and `call_failures=`: swallowed AT-SPI call failures, which mean the dumped tree may be incomplete.
- `scripts/visual-geometry-smoke.py`
  - Runs same-session visual invariant scenarios under isolated headless Mutter.
  - Captures before/after screenshots and bounded Automation1
    `visual_geometry` snapshots from one process, waits for
    `visual-geometry-settled` plus scenario-specific final geometry predicates,
    compares protected regions with explicit masks, and verifies declared pixel
    anchors for rendered code, CSS, or native toolkit effects.
  - Writes per-case manifests, comparison reports, warning scans, and a root
    summary under the requested artifact directory. Keep generated artifacts
    under ignored `build/` paths unless the human explicitly asks to check them
    in.
- `scripts/lushtext-automation.py visual-geometry-capture` (repository root)
  - Captures the current live Automation1 visual-geometry state into bounded
    artifacts, writes a generated replay scenario, and records the exact
    `scripts/visual-geometry-smoke.py --scenario-dir ...` command. Treat any
    portal screenshot as context-only; proof comes from the generated headless
    replay and screenshot-derived pixel anchors.
- `scripts/capture-lushtext-xvfb.sh`
  - Fallback isolated display when headless Mutter or PipeWire capture is unavailable; lower compositor fidelity than `mutter --headless`.
  - Launches a debug-owned LushText process in an isolated `dbus-run-session` + Xvfb display with temporary XDG state and keyfile GSettings.
  - Uses D-Bus window actions for exported app behavior (`win.begin-search` through `org.gtk.Actions`), then confines any `xdotool` typing to that private display.
  - Captures screenshots with `xwd` and ImageMagick, avoiding live GNOME focus and portal permission races.
  - Accepts the same repository, binary, app-ID, object-path, and GSettings overrides where those identities affect its D-Bus flow.

## Screenshots and Log Summaries

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
