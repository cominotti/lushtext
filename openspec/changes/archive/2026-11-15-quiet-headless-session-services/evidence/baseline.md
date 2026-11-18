# Measured baseline (spec stage, 2026-11-14)

Host: `sophia`, Fedora 45 toolbox, mutter 51.1, GTK 4.24, Libadwaita 1.10,
PipeWire 1.6.11, WirePlumber 0.5.19, xdg-desktop-portal-gtk 1.15.3. All spikes
ran under `taskset -c 8-23`, each in its own `dbus-run-session` with a private
`XDG_RUNTIME_DIR` under `/run/user/1000/qhs.*`. The spike scripts are throwaway
and are not part of the change. The system journal was read with
`flatpak-spawn --host journalctl` (read-only).

## Host journal (what the desktop pays)

| Signal | Window | Count |
| --- | --- | --- |
| `rtkit-daemon`: "Reached burst limit for user '1000', denying request" | 2026-11-07 15:05 – 2026-11-14 21:18 | 7,544 |
| `rtkit-daemon`: "Successfully made thread … owned by '1000'" | 2026-11-14 00:00 – 21:18 | 6,155 |
| `bluetoothd`: "Endpoint registered" | 2026-11-14 00:00 – 21:20 | 27,495 |
| distinct D-Bus senders of those registrations | same | 1,833 |

Each grant pattern in the journal is one process at nice -11 followed by one
or two threads at RT priority 20, repeated per session.

## Per-session spikes

"Bare" is what the screenshot lanes do today: `pipewire`, then `wireplumber`
with no profile (`main`: `hardware.audio`, `hardware.bluetooth`,
`hardware.video-capture`) and the distribution configuration. "Quiet" is the
candidate configuration: `wireplumber --profile=policy`; `pipewire.conf.d` and
`client.conf.d` fragments `context.properties = { module.rt = false }`; a
WirePlumber fragment that replaces `context.modules` (via `override.`) and the
`client-context` component's `arguments` (via `wireplumber.components.rules`)
with the same module lists minus `libpipewire-module-rt`; private
`XDG_STATE_HOME`. The system bus was replaced by a private `dbus-daemon`
(`DBUS_SYSTEM_BUS_ADDRESS`) watched by `dbus-monitor`, so every system-bus
message of every process in the session was recorded.

| Measurement | Bare | Quiet |
| --- | --- | --- |
| Messages naming `org.freedesktop.RealtimeKit1`, daemons only | 48 | 0 |
| Same, with a real mutter ScreenCast + `gst-launch pipewiresrc` capture | 66 | 0 |
| Messages naming `org.bluez` | 19 | 0 |
| bluetoothd "Endpoint registered" (real system bus, daemons only) | 15 | 0 |
| `pw-dump` objects with `device.api` = alsa / v4l2 / libcamera / bluez5 | 8 / 2–3 / 1 / 1 | none |
| Threads at SCHED_RR (`ps -L`), daemons only | 3 (pipewire data loop, 2 WirePlumber data loops) | 0 |
| WirePlumber main thread nice | -11 | 0 |
| Screenshot through `pipewiresrc` (640x480 PNG, one buffer) | yes | yes |

So the quiet configuration still links mutter's ScreenCast stream to
`pipewiresrc` (the `policy` profile keeps `policy.standard`). Bare WirePlumber
also opens the host's ALSA cards and cameras from a test session.

**WirePlumber's module-rt cannot be turned off by `module.rt.args`.** A
fragment `module.rt.args = { rtkit.enabled = false … }` changed nothing: the
module loaded with the arguments from `wireplumber.conf`, fell through to
RTKit, and set the main thread to nice -11. WirePlumber 0.5.19 loads
`context.modules` itself (`lib/wp/private/parse-conf-section.c`) and has no
`condition` support, so the supported overrides are `override.context.modules`
and `wireplumber.components.rules`.

**WirePlumber state is the user's.** The lanes give the app a private
`XDG_CONFIG_HOME` but no `XDG_STATE_HOME`, so their WirePlumber reads and writes
`~/.local/state/wireplumber`. The bare spike itself rewrote the real
`~/.local/state/wireplumber/default-nodes` at 21:19:23 (the quiet spike, run
with a private `XDG_STATE_HOME`, did not). Every bare screenshot-lane case does
the same.

## Who activates `org.a11y.Bus` and the portals

`dbus-monitor --session "destination=org.a11y.Bus"`, senders resolved with
`GetConnectionUnixProcessID`; names from `ListNames` 2.5 s after a window was
presented under `mutter --headless`.

| Environment set before `dbus-run-session` | Client | `org.a11y.Bus` activated by | Portals running |
| --- | --- | --- | --- |
| none | GTK 4 | the GTK 4 client and `xdg-desktop-portal-gtk` | yes |
| `GTK_A11Y=test` | GTK 4 | `xdg-desktop-portal-gtk` | yes |
| `GTK_A11Y=test NO_AT_BRIDGE=1` | GTK 4 | nobody | yes |
| `NO_AT_BRIDGE=1 GDK_DEBUG=no-portals GTK_USE_PORTAL=0` (the widget runner's) | GTK 4 | the GTK 4 client | no |
| same | Libadwaita | the client | yes: Libadwaita starts them |
| same + `ADW_DISABLE_PORTAL=1` | Libadwaita | the client | no |
| same + `ADW_DISABLE_PORTAL=1 GTK_A11Y=test` or `=none` | Libadwaita | nobody | no |

GTK 4 ignores `NO_AT_BRIDGE`; `xdg-desktop-portal-gtk` (GTK 3, atk-bridge)
honours it only when it is in the bus's activation environment, that is, set
before `dbus-run-session`. Headless mutter activates neither.

`GTK_A11Y=none` against `GTK_A11Y=test` (a button with a description and
`hidden` state):

| `GTK_A11Y` | AT context | `test_accessible_has_property(DESCRIPTION)` | `has_state(HIDDEN)` |
| --- | --- | --- | --- |
| unset | `GtkAtSpiContext` | true | true |
| `test` | `GtkTestATContext` | true | true |
| `none` | none | false | false |

Twelve files assert accessible state with
`gtk4::test_accessible_has_*` (ten widget-test modules, the widget `common.rs`
helper and `lushtext-core`'s `ui/accessibility.rs`), so `none` would break them;
`test` keeps them.

## Mutter and rtkit

Headless mutter 51.1 alone (private bus, no PipeWire) produced no rtkit-daemon
journal line for its PID and no burst-limit line in the window; its threads
stayed SCHED_OTHER or SCHED_BATCH. Under the private system bus it asks for
`org.freedesktop.ColorManager`, `org.freedesktop.locale1`,
`org.freedesktop.login1` and `org.freedesktop.UPower`, and logs two new warnings:
`mutter-WARNING **: Failed to retrieve locale properties: …ServiceUnknown…`
and `libmutter-WARNING **: There is no colord server available`.

## Review-stage spike: the portal frontend under the private system bus

Spec review, 2026-11-14, same host and versions. One throwaway
`dbus-run-session` with private XDG homes and a runtime directory under
`/run/user/1000/qhsrv.*` (removed afterwards, its document-portal FUSE mount
unmounted first), a private system `dbus-daemon` on an abstract socket watched
by `dbus-monitor`, and `mutter --headless` presenting a virtual monitor. Inside
it, `gdbus call` read `org.freedesktop.portal.Settings` `version` (reply
`(<uint32 2>,)`), which activated `xdg-desktop-portal`,
`xdg-desktop-portal-gtk`, the permission store and the document portal. No
PipeWire, no WirePlumber, no GTK client.

| Measurement | Value |
| --- | --- |
| System-bus messages naming `org.freedesktop.RealtimeKit1` | 18, all from one sender, the `xdg-desktop-portal` frontend |
| Of those, calls whose destination is `org.freedesktop.RealtimeKit1` | 3: `Properties.Get` of `MaxRealtimePriority`, `MinNiceLevel`, `RTTimeUSecMax` |
| The rest | `AddMatch` (arg0 / sender `org.freedesktop.RealtimeKit1`), `GetNameOwner`, `StartServiceByName` naming it |
| `MakeThreadRealtime*` / `MakeThreadHighPriority*` calls | 0 |
| New warnings | 3 × `xdg-desktop-portal-WARNING **: Failed to load RealtimeKit property: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.RealtimeKit1 was not provided by any .service files`, plus mutter's two lines above |
| Other system-bus destinations | `org.freedesktop.locale1` (mutter, 1 call) |
| Session-bus names at the end | the portals above and `org.freedesktop.a11y.Manager` (mutter's own a11y manager, not `org.a11y.Bus`) |

So the portal frontend reads rtkit's properties at startup in every
portal-on session. On the host bus that is a property read that grants
nothing and counts toward no burst limit, but a guard that fails on *any*
message naming RealtimeKit1 would fail every `plain`, `screencast` and
`accessibility` session. Design D7 keys the `rtkit` class accordingly.
