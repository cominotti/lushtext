## Why

LushText's headless test sessions put load on the developer's desktop. The
`cst` session found this while diagnosing desktop smoothness, and the evidence
is in [`evidence/baseline.md`](./evidence/baseline.md).

- **rtkit is exhausted.** Since 2026-11-07 the system journal holds 7,544
  "Reached burst limit for user '1000'" denials, and rtkit was SIGKILLed 5
  times, so games and browsers lose real-time audio. Each screenshot-lane case
  starts `pipewire` and a bare `wireplumber` with `libpipewire-module-rt`.
  Measured: one nice -11 request and three SCHED_RR threads per case, and 48
  RealtimeKit1 messages (66 with a capture). The quiet configuration sends 0.
- **The system bluetoothd churns.** Bare WirePlumber runs the `main` profile,
  whose bluez monitor registers every media endpoint with the host's
  bluetoothd. That is 15 endpoints per case, and 27,495 registrations from
  1,833 sessions on 2026-11-14 alone. The same profile's ALSA, v4l2 and
  libcamera monitors open the host's sound cards and cameras.
- **The user's audio state is written.** The lanes set no `XDG_STATE_HOME`, so
  every bare case reads and rewrites the real
  `~/.local/state/wireplumber/default-nodes`. The spec-stage spike did it too.
- **Thousands of accessibility buses.** Every widget-test session activates
  `org.a11y.Bus` (`at-spi-bus-launcher` and its broker). GTK 4 ignores
  `NO_AT_BRIDGE`, and Libadwaita starts `xdg-desktop-portal`, `-gtk` and the
  document portal despite `GDK_DEBUG=no-portals`, which is the opposite of
  what the harness says it does.
- **Nothing stops it coming back.** About 20 call sites spawn
  `dbus-run-session`, `mutter --headless`, `pipewire` or `wireplumber`
  independently. They are in Python lanes, shell lanes, the proof harness,
  `cargo-gtk-proof`, and two `gtk-agentic-debugging` skill scripts that the
  visual and accessibility smokes call. So far they have diverged silently.

## What Changes

- **One session owner.** `gtk-lush-proof-harness` gains a `session` module
  that is the only code allowed to start the private bus, the compositor and
  the media daemons. A `gtk-lush-session` binary exposes it, and one shim,
  `scripts/headless-session`, finds or builds that binary. Every lane migrates
  to it: the widget and axiom harnesses through the library, `cargo-gtk-proof`
  through the library, and Python, shell and skill lanes through the shim.
- **Quiet media.** WirePlumber runs `--profile=policy`, with no hardware
  monitors. PipeWire, WirePlumber and every PipeWire client (mutter's
  ScreenCast, `pipewiresrc`) run without module-rt. The daemons get private
  config, state and data homes. A real capture was measured to still work.
- **A private system bus.** The helper gives each session its own monitored
  system bus, so rtkit and bluetoothd cannot be reached from a test session at
  all, and any attempt is recorded.
- **No accessibility bus or portals unless asked for.** Sessions that are not
  for AT set `GTK_A11Y=test`, `NO_AT_BRIDGE=1` and `ADW_DISABLE_PORTAL=1`
  before the bus starts. `test` keeps the accessible state that 12 widget
  modules assert, which `none` would drop. The accessibility profile keeps
  AT-SPI. Screenshot lanes keep their portals.
- **Runtime guard.** At teardown the helper fails the run with
  `SESSION-HYGIENE:` and a distinct exit status if any of these happened:
  - a message to RealtimeKit1 or bluez;
  - an ALSA, v4l2, libcamera or bluez5 device;
  - `org.a11y.Bus` in a session that is not for AT;
  - a portal in a no-portal session.

  The widget harness, `run-widget-tests.sh`, the parallel shard summary and
  the lane manifests report it as a named failure class.
- **Static guard.** `make check-session-hygiene` joins `make check-policy`. It
  fails when any tracked script, Makefile recipe, workflow step or Rust or
  Python source names one of the session commands outside the helper. It keys
  on command names, not paths. It also fails when the helper's profile or no-RT
  configuration drifts. It is self-tested failing-first.
- **Two GTK axioms:**
  - `GTK_A11Y=test` keeps accessible state and never calls `org.a11y.Bus`;
  - `ADW_DISABLE_PORTAL=1` makes Libadwaita call no portal.
- **Docs:**
  - `build.md` and `testing.md` (gate order);
  - the proof-harness README and CHANGELOG;
  - the smoke warning classifiers (two new mutter warnings under the private
    system bus, kept in one shared family);
  - the programme record;
  - a handoff for the `cst` session once the change merges.

## Non-goals

- Portals in the screenshot, accessibility and portal-sandbox lanes. Their
  rendering baselines and confinement checks may depend on them, and they put
  no measured load on the host. Each session's artifacts record the names that
  were active.
- A per-minute session cap. A quiet session costs the host none of the
  measured resources, and the runtime guard proves that on every run. A cap
  would slow the parallel widget shards and the per-case visual lanes, and the
  only thing it would limit is what the guard already forbids. Memory and CPU
  stay bounded by `scripts/heavy.sh` and the 16-CPU rule.
- `at-spi-bus-launcher` or `at-spi2-registryd` spawns in the accessibility
  profile, which still needs them.
- Any LushText application behaviour. No persisted format, GSettings key,
  action, D-Bus member or readiness predicate changes.

## User-visible impact

Users of the app see nothing. On the developer's desktop:

- audio keeps real-time scheduling while the lanes run;
- bluetoothd stops churning;
- sound cards and cameras are no longer probed;
- audio defaults are no longer rewritten.

Compared with `main`, the visual, accessibility, crash-recovery and
`cargo-gtk-proof` lanes need the small `gtk-lush-session` binary built, which
the shim does on first use. Their mutter logs gain two expected warnings.

## Capabilities

### New Capabilities

- `headless-session-hygiene`: Every private headless test session starts
  through one helper that keeps the host's rtkit, bluetoothd, audio and video
  devices, audio state and accessibility bus out of reach, and fails the run
  when it detects otherwise.

### Modified Capabilities

- `gtk-lush-proof-harness`: owns the session helper and its binary, and sets
  the no-AT, no-portal environment before the session bus starts.
- `cargo-gtk-proof`: the live runner starts its session, compositor and media
  daemons through the helper.
- `widget-test-ci-sharding`: the parallel shard summary reports
  `SESSION-HYGIENE:` as a failure class.
- `gtk-axiom-ledger`: two new axioms, for `GTK_A11Y=test` and
  `ADW_DISABLE_PORTAL=1`.
- `smoke-warning-classification`: mutter's system-bus warnings become one
  shared family that the widget and axiom runners read instead of copying.

## Impact

- Code:
  - `crates/gtk-lush/proof-harness` (new `session` module, `gtk-lush-session`
    bin, data files, API snapshot);
  - `crates/cargo-gtk-proof` (`host.rs`, `live.rs`);
  - `crates/lushtext/tests/widget.rs`;
  - `crates/gtk-lush-adoption-lab/tests/axiom_probes.rs`;
  - `crates/gtk-lush/axioms`.
- Scripts:
  - new `scripts/headless-session` and `scripts/check-session-hygiene.py`;
  - every session-spawning lane in `scripts/`;
  - the two `gtk-agentic-debugging` capture scripts;
  - `scripts/smoke_warning_classifiers.py` and the widget and axiom
    benign-noise patterns.
- Makefile (`check-session-hygiene`, `check-policy`),
  `.github/workflows/ci.yml` and `end-user-smoke.yml` (`dbus-tools` for
  `dbus-monitor`, and the bin), and `scripts/setup-dev-tools.sh`.
- Timing: none on the app (no timed path touched).
