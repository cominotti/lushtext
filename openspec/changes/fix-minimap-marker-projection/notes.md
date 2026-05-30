## Live Verification Notes

- A live GTK debug session was launched for the minimap search-marker scenario, and the in-tab search action was activated before the user typed `needle`.
- Visual screenshot capture could not be completed in that session because `gnome-screenshot` was not installed and the available desktop portal / Shell screenshot paths were denied or timed out.
- The live session did not show GTK geometry warnings related to the minimap scenario. At this early stage, before the screenshot and input helper tooling was installed, the strongest automated proof was the minimap widget geometry coverage that asserted markers stay inside the rendered `GtkSourceMap` content boundary and clear or reproject after search, save, edit, toggle, and resize events.

## Live Verification Retry

- Retried the scenario in debug artifact directory `/tmp/gtk-debug-20260529-220531` after installing the helper tools. `check-lushtext-live.sh` proved the active `dev.cominotti.lushtext` D-Bus owner and matching PID belonged to this debug launch before interaction.
- Kept `ydotoold` running in a PTY-backed session because the agent command runner reaps background daemons after shell commands exit. After the user focused the LushText debug window, the in-tab search was opened, cleared, and populated with `needle`.
- Automated screenshots still could not be saved: direct `gnome-screenshot` timed out, portal-only capture timed out waiting for approval, and the GNOME Shell screenshot interface returned `AccessDenied`.
- The user visually confirmed the live scenario looked correct: the search bar showed `needle`, and the orange minimap search markers stopped within the rendered document content instead of extending into the blank bottom tail.

## D-Bus and Accessibility Debugging Notes

- `org.gtk.Actions` on `/dev/cominotti/lushtext/window/1` can drive exported LushText actions such as `begin-search`; it cannot set arbitrary private GTK widget state by itself.
- System Python AT-SPI bindings can drive visible editable GTK widgets over the accessibility D-Bus bus. In this session, the search entry was set to `needle` through `EditableText.setTextContents()` after opening search.
- GNOME screenshot APIs remained permission-gated: Shell screenshot methods returned `AccessDenied`, and portal screenshot requests exposed a Shell screenshot UI rather than returning a file directly.
- AT-SPI exposed a GNOME Shell `Take Screenshot` button, but not as an invokable accessible action. Coordinate fallback without a strict owning-application filter is unsafe because it can hit GNOME Shell UI such as Overview or quick settings rather than the target app.

## Fully Automated Screenshot Retry

- Repeated the live scenario in debug artifact directory `/tmp/gtk-debug-20260529-224607`. The first liveness check correctly refused to proceed when the session PID regex was too narrow for the absolute executable path; rerunning `check-lushtext-live.sh` with `/var/home/danilo/Workspace/github/cominotti/lushtext/target/debug/lushtext($| )` proved PID `883527` belonged to the debug launch.
- Activated the scenario document through `org.freedesktop.Application.Open`, opened in-tab search through `org.gtk.Actions.Activate begin-search`, and set the search entry to `needle` through `atspi-set-text.py --max-depth 30`. The deeper AT-SPI walk was required because the restored tab layout nested the search entry below the previous default depth.
- Captured `/tmp/lushtext-auto-shot-test/lushtext-repeat.png` with `capture-screenshot.py --portal-only --non-interactive --timeout 12`. The image was inspected and showed LushText frontmost, the scenario tab active, the search entry containing `needle`, highlighted matches in the editor, and minimap search markers constrained to the visible source-map content instead of the blank tail.
- Direct `gnome-screenshot -f` remained unsuitable in this Toolbx/Wayland session because it could not use Shell's builtin screenshot interface and its X11 fallback reported zero-sized GDK surfaces. GNOME Shell `FocusApp`, `GetWindows`, and `GetRunningApplications` returned `AccessDenied`, so the reliable automated path is app D-Bus actions plus AT-SPI editable text plus non-interactive portal capture.

## Isolated Display Automation Retry

- Added `capture-lushtext-xvfb.sh` as the focus-safe fully automated path for interaction plus screenshot. It launches the debug binary inside a private `dbus-run-session` and Xvfb display, points XDG data/config/cache and GSettings to temporary state, enables the minimap through the keyfile backend, activates `begin-search` through `org.gtk.Actions`, confines `xdotool type needle` to the isolated display, and captures a PNG through `xwd` + ImageMagick.
- Verified the helper with `/tmp/lushtext-xvfb-helper-3.png`. The inspected image showed the scenario file open, search text `needle` in the in-tab search field, highlighted editor matches, the minimap visible, and orange minimap markers stopping within the rendered source-map content.
- Probed headless Mutter screencast as the higher-fidelity direction. `RecordVirtual` emitted a PipeWire node but created a separate 1x1 virtual stream, so it was the wrong API for screenshots of the visible app monitor.

## Headless Mutter Automation Retry

- Proved the higher-fidelity automated path with `capture-lushtext-mutter.py`: launch an isolated `dbus-run-session`, start PipeWire and WirePlumber in the same private runtime, run `mutter --headless --wayland --no-x11 --virtual-monitor 1600x1000`, open the scenario file in LushText, and capture the existing monitor through `org.gnome.Mutter.ScreenCast.Session.RecordMonitor("Meta-0")`.
- The runtime directory must be mode `0700`; a looser temporary runtime can prevent PipeWire clients from connecting.
- For search text, the helper opens search through `org.gtk.Actions.Activate begin-search`, starts a private AT-SPI registry on the normal session bus, and sets the search entry through `EditableText.setTextContents()`. Starting `at-spi2-registryd` with `DBUS_SESSION_BUS_ADDRESS` pointed at the accessibility bus is wrong because the registry itself needs the normal session bus to discover the accessibility bus address.
- Verified `/tmp/lushtext-mutter-helper.png` as a 1600x1000 PNG. The inspected image showed the scenario file active, search text `needle` in the in-tab search field, highlighted editor matches, the minimap visible, and orange minimap markers constrained to rendered source-map content instead of the blank EOF tail.
- Remaining app-side follow-ups were recorded in `docs/next/headless-mutter-debug-automation.md`: add a D-Bus action for setting search text, give the search entry a stable accessible name, and consider a narrow read-only debug state surface for automated assertions.
