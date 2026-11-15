# Runtime proof (apply stage)

Host `sophia`, Fedora 45 toolbox, mutter 51.1, GTK 4.24, Libadwaita 1.10,
PipeWire 1.6.11, WirePlumber 0.5.19, under `taskset -c 8-23`. Every run used
scratch `XDG_{CONFIG,STATE,DATA,CACHE}_HOME` directories and the helper's own
private runtime directory; the helper gave the media daemons their own
private homes inside it. `~/.local/state/wireplumber` mtimes were recorded
before the first run and compared after the last: unchanged.

## 2.4 WirePlumber module lists re-verified

`/usr/share/wireplumber/wireplumber.conf` (WirePlumber 0.5.19): `context.modules`
is module-rt, protocol-native, metadata; the `client-context` component's
`arguments.context.modules` is module-rt, protocol-native, client-node,
client-device, adapter, spa-node-factory, metadata. The `policy` profile is
`inherits = [ base ]` plus `metadata.sm-settings`, `metadata.sm-objects` and
`policy.standard`. `/usr/share/pipewire/pipewire.conf` and `client.conf`
(1.6.11) guard module-rt with `condition = [ { module.rt = !false } ]`. The
fragment's header records the version.

## 2.5 Capture through the helper, media on and off

`gtk-lush-session session --profile screencast [--media off] --log-dir … --
gtk-lush-session compositor --monitor 640x480 -- python3 capture.py`, where
the throwaway child presents a Libadwaita window (so the portals start),
records `Meta-0` through mutter's ScreenCast and pulls one buffer with
`gst-launch-1.0 pipewiresrc num-buffers=1 ! videoconvert ! pngenc`.

| Measurement | media quiet | media off |
| --- | --- | --- |
| Exit status / verdict | 0 / clean | 0 / clean (no capture: no PipeWire) |
| PNG | 640 x 480 RGBA | — |
| Session threads at SCHED_RR / SCHED_FIFO (`ps -eLo cls`) | 0 (the helper's pipewire at TS, nice 0) | — |
| `MakeThread*` calls | 0 | 0 |
| Messages naming `org.bluez` | 0 | 0 |
| Senders naming RealtimeKit1 | 1, `xdg-desktop-portal` (exempt) | 1, `xdg-desktop-portal` (exempt) |
| Its records | 3 `AddMatch`, 2 `GetNameOwner`, 1 `StartServiceByName`, 1 `Properties.Get` each of `MaxRealtimePriority`, `MinNiceLevel`, `RTTimeUSecMax` | identical set |
| `module-rt` evidence in daemon maps or logs | none | — |

The frontend's set is the same with media on and off, so being a PipeWire
client adds nothing to it (one `Properties.Get` per property). The
spec-review spike counted 18 messages because it also counted signal
deliveries to the frontend; the verdict only reads messages a peer sent.

**Finding fixed during this run.** The first run reported 0 frontend records:
the session bus activates the portal frontend with the *session bus's*
environment, so a `DBUS_SYSTEM_BUS_ADDRESS` set only on the command reached
the host system bus from activated services. The helper now exports the
private system bus address before `dbus-run-session` (its socket path is
fixed by the runtime directory), and the inner stage checks it.

Warning lines of the media-on session (every one, de-duplicated):

- `(mutter:PID): libmutter-WARNING **: T: There is no colord server available` (new)
- `(mutter:PID): mutter-WARNING **: T: Failed to retrieve locale properties: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.locale1 was not provided by any .service files` (new)
- `(/usr/libexec/xdg-desktop-portal:PID): xdg-desktop-portal-WARNING **: T: Failed to load RealtimeKit property: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.RealtimeKit1 was not provided by any .service files` (new, 3 times)
- `xdg-desktop-portal-WARNING **: T: Caught PipeWire error: connection error` and `Failed connect to PipeWire: Couldn't connect to PipeWire` (teardown: the media daemons stop before the activated portal; already classified)
- `Choosing gtk.portal for … via the deprecated UseIn key` (×10) and `The preferred method to match portal implementations … portals.conf(5)` (already classified in the screenshot lanes)
- `WARNING: radv is not a conformant Vulkan implementation, testing use only.` (the throwaway gst pipeline's Vulkan probe on this host, not a lane process)

## 2.6 Failing first: the previous configuration through `--expect-dirty`

`gtk-lush-session session --profile screencast --expect-dirty --log-dir … --
sleep 3`: bare `wireplumber` (no profile, no fragments), module-rt loading,
scratch homes. Exit 0 means the guard caught all four classes:

- `rtkit`: `MakeThreadHighPriorityWithPID` and `MakeThreadRealtimeWithPID`
  from `pipewire` and both `wireplumber` contexts, plus property reads from
  `pipewire`, `wireplumber` and `pw-dump`;
- `bluez`: WirePlumber's bluez monitor (`GetManagedObjects`, `AddMatch`es,
  `GetNameOwner org.bluez`);
- `hardware-device`: PipeWire objects with `device.api` `alsa`, `v4l2`,
  `libcamera` and `bluez5`;
- `module-rt`: `libpipewire-module-rt.so` mapped in both daemons and their
  `mod.rt` log lines;
- and `config` (WirePlumber profile `main`).

None of it reached the host: the system bus was the helper's. The quiet
configuration in the same scenario (2.5, media quiet) is clean.
