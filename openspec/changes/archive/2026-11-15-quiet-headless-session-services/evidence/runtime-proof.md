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

## 3.1 Who activates `org.a11y.Bus` and the portals in a widget session

The real widget binary (the 15 `background_tab_layout` tests, one GTK child
per test) under the helper with the guard's classes allowed, so nothing was
hidden: the previous harness environment (`NO_AT_BRIDGE=1`,
`GDK_DEBUG=no-portals`, `GTK_USE_PORTAL=0`, `GSK_RENDERER=cairo`,
`GTK_IM_MODULE=gtk-im-context-simple`; no `GTK_A11Y`, no
`ADW_DISABLE_PORTAL`), with the child's environment check skipped for this
run only (a temporary local patch, reverted before any commit). Requesters
read from the session bus's activation lines (`requested by … comm=`):

| Run | Name | Requested by |
| --- | --- | --- |
| a11y and portals allowed | `org.freedesktop.portal.Desktop` | the widget test binary (Libadwaita) |
| | `org.freedesktop.impl.portal.desktop.gtk`, `.PermissionStore`, `org.freedesktop.portal.Documents` | `xdg-desktop-portal` |
| | `org.a11y.Bus` | `xdg-desktop-portal-gtk` (the bus's activation environment had no `NO_AT_BRIDGE` in this run) |
| a11y allowed, `ADW_DISABLE_PORTAL=1` kept | `org.a11y.Bus` | the widget test binary (GTK 4, with `NO_AT_BRIDGE=1` set) |
| | `org.freedesktop.systemd1` (×25) | the AT-SPI bus's own `dbus-broker-launch` |

So each activator is one 3.3 removes: `GTK_A11Y=test` stops the GTK 4 client
(A115), `ADW_DISABLE_PORTAL=1` stops Libadwaita activating the portal
frontend (A116), which is what starts `xdg-desktop-portal-gtk`; and the
helper sets `NO_AT_BRIDGE=1` in the bus's activation environment as well, for
any GTK 3 service a portals-on profile still activates.

## 3.2 Failing first: the child check and the shard summary

- `gtk_lush_proof_harness` unit tests
  `a_child_without_the_session_environment_names_the_missing_variable` and
  `a_child_accepts_a_callers_renderer_and_gtk_a11y_none_only` (45 harness
  tests pass). The check exists since group 2; the binaries stopped setting
  the values in 3.3, which is what makes a child launched outside the session
  fail (`NO_AT_BRIDGE=1 is not set (found None)`) before GTK initializes.
- `widget-shards.py --self-test`: the new `SESSION-HYGIENE` case against the
  previous driver (`HEAD` plus only the test) fails with
  `AttributeError: 'ShardOutcome' object has no attribute 'hygiene'`; with
  the change, a shard that printed two classes and exited 93 has one attempt,
  fails the run and prints `SESSION-HYGIENE: h: rtkit` and
  `SESSION-HYGIENE: h: bluez`. `run-widget-tests.sh --self-test` checks that
  exit 93 is never retried and a test failure (101) may be.

## 3.5 `at_listeners` under `GTK_A11Y=test`

With `GTK_A11Y=test`, `gtk_uses_atspi()` is false, so the mirror starts
`Disabled` and the test-utils simulation's empty registry answer changed
nothing: through the migrated runner, the two `background_tab_layout` tests
that simulate a listener failed
(`test_no_view_is_detached_while_an_assistive_technology_listens`,
`test_detached_editors_attach_one_per_dispatch_when_a_listener_appears`).
The fix is in the simulation hook (`simulate_registry_answer` now stands in
for an AT-SPI backend whatever `GTK_A11Y` selects), not in the environment;
production behaviour is unchanged.

## 3.7 The `surfaces` widget shard under `GTK_A11Y=test`

`make test-widget-shard WIDGET_SHARD=surfaces` (the shard holding
`accessibility.rs` and the other accessible-state modules), on the migrated
harness (`widget` profile, private system bus, `GTK_A11Y=test`,
`ADW_DISABLE_PORTAL=1`): status 0, 8.79 min, 855 of 1676 tests, `test
result: ok. all tests passed`, and no `FLAKY:`, `OOM-KILL:` or
`SESSION-HYGIENE:` line.

## 4.3 `flatpak run` and the private system bus

Decided by running it, not by reading: `flatpak run --no-a11y-bus
--command=true dev.cominotti.lushtext//master` under
`headless-session run --profile plain --inherit-runtime-dir` (a private
runtime directory under `$XDG_RUNTIME_DIR`) exits 0. It activates
`org.freedesktop.Flatpak` (the session helper),
`org.freedesktop.portal.Documents` and its PermissionStore on the session
bus, and sends **zero** messages on the private system bus
(`system_bus_records=0`); the guard's verdict is `clean`. So the portal and
sandbox lane needs no host system bus: it runs the `plain` profile, and no
`host-system-bus` profile is added. (`--command=true` exercises `flatpak run`
itself without starting the app, so the probe writes nothing into
`~/.var/app`.)

On this host the lane's own Flatpak launch cases skip: `flatpak info
dev.cominotti.lushtext` fails because both the `master` and `stable` branches
are installed, which the lane reports as "not installed". That skip predates
this change.

## Apply-time decisions (groups 3 and 4)

Each was decided autonomously, user experience first, and is recorded here
with its reason.

1. **Axiom ids A115 and A116.** The catalogue needs ascending ids, not
   contiguous ones; A112–A114 are taken by changes in flight in other
   worktrees, so the next free ids that cannot collide on merge are used.
2. **Groups 3 and 4 land in one commit.** `at_listeners.rs` (group 3) is
   under `crates/lushtext-core/src/ui/` and `smoke_warning_classifiers.py`
   (both groups) is fingerprinted, so either commit alone needs fresh
   accessibility, visual and visual-geometry smokes; one commit runs them once,
   on the final tree (`testing.md`, "Gate Order Before a Commit").
3. **`failure_status: session-hygiene` only where manifests already carry the
   field** (visual geometry, `cargo-gtk-proof`); the other drivers report the
   class in their existing `failure_reason`, and the automation client maps
   both to its `session-hygiene` status. No manifest gained a field.
4. **Crash recovery falls back to `--media off`** on a host without the media
   daemons and then takes no screenshots, instead of skipping the whole lane:
   its recovery evidence does not need pixels.
5. **The 3.1 activator census used the `background_tab_layout` subset,** not a
   whole shard: it presents real windows with focus and tab switches, which is
   what activates the portal and the a11y bus, and it kept the patched
   (temporarily unverified) harness run short. Both temporary patches were
   reverted before any commit.
6. **`setup-dev-tools.sh` checks the session's packages by package name**
   (`rpm -q`, the `session_packages` array) instead of dropping them. Task 4.3
   assumed the `packages` inventory installs them, but the script installs
   only what its command checks find missing, so dropping the four commands
   would have left a fresh host without mutter or the media daemons. An array
   whose name ends in `packages` is an exempt context of the static gate, so
   this names no session command.
7. **`check-tools --json` gained a `roles` map** (session bus, compositor,
   media, session manager). The responsiveness manifest keeps its
   `toolkit.mutter` field name, but a Python literal equal to a session
   command fails the gate, so the driver reads the compositor's name and
   version by role (`smoke_session.compositor_version`).
8. **`cargo-gtk-proof` uses the harness library, not the binary.** It
   supervises each case with its own bounded log and timeout, so the library
   now builds the `session` and `compositor` stages as `Command`s
   (`session_command`, `compositor_command`, `SessionLaunchError`) and
   `cargo-gtk-proof`'s `main` serves as the inner stage. It no longer starts
   or waits for PipeWire and WirePlumber: the `screencast` profile does.
9. **The portal/sandbox lane's portal inventory asks a private session**
   (`ListActivatableNames` in a `plain` helper session) instead of the
   desktop's own session bus, which the lane used to query; headless lanes
   never talk to the live desktop.
10. **The A116 control child relaunches itself** with `GDK_DEBUG=no-portals`
    instead of setting it with `set_var`: the axioms crate forbids unsafe code,
    and the nested `--portals on` session clears that variable by design.
11. **Socket paths are bounded by `dbus-daemon`'s 99 bytes, not the kernel's
    107** (`DBUS_SOCKET_PATH_MAX`). The A115/A116 sample `--check` runs nest a
    `widget` session under `with-temp-scope.sh`'s TMPDIR
    (`/tmp/lt-scope-<pid>-axioms-<rand>`), and its owned runtime directory put
    the private system bus socket at 101 bytes: within `sun_path`, but
    `dbus-daemon` refused it ("Socket name too long"; measured on dbus 1.16.2:
    99 binds, 100 fails). An owned runtime directory that would not fit falls
    back to `/tmp`, and `--inherit-runtime-dir` refuses one that would not, so
    a session never starts a bus it cannot bind. The probe runner's nested
    controls passed only because their TMPDIR was shorter.
12. **A nested control's stderr is captured, not inherited.** Turning the
    AT-SPI bus or the portals on is the control's point, and the services it
    starts warn about the headless session they find (the portal frontend's
    backend choice, the registry's activation, PipeWire's absence). Those are
    fixture output, so the axiom lane's warning scan does not read them; a
    failed control reports their last lines as its `control_error`
    measurement instead.
13. **Announcements skip a `GtkTestATContext` (new axiom A117).** The first
    full widget run under `GTK_A11Y=test` failed 21 tests (17 in `window`,
    4 in `editor-page`), all through the Save Changes dialog. Each child died
    with no panic text: a SIGSEGV in `gtk_accessible_announce`, at a NULL
    function pointer (gdb). `GtkTestATContext` implements no `announce`
    vfunc and `GtkATContext` gives it no default (GTK 4.24.1 and `main`
    source). Going back to `atspi` would bring the bus activations back, and
    `none` drops the accessible state the widget tests read. So the one
    announcement helper, `ui::accessibility::announce`, skips the call when
    the widget's AT context is a `GtkTestATContext`. Nothing listens through
    that context, so no announcement is lost. GTK also falls back to it when
    no backend builds a context, so the guard ships rather than being
    test-only. Under `GTK_A11Y=none` (the automation lane's app override) it
    never asks for the context: GTK creates none, and the gtk-rs
    `at_context()` binding is non-nullable. A117 pins the crash in a probe
    child, and
    `test_announcing_through_a_realized_test_context_is_skipped_not_fatal`
    was shown to fail (child killed, guard forced off) before passing. The
    upstream report is drafted only (`docs/upstream-reports.md`).
14. **The Protected View edit-entry capture cancels itself on a text
    change.** The first full widget run after merging `main` (`cfee0c48`)
    failed
    `editor_page::test_buffer_replacement_evidence_reads_stay_side_effect_free_across_replacement_mutation`
    in both attempts. The output was `Invalid text buffer iterator`,
    `gtk_text_buffer_get_text: assertion … failed`, then an abort in
    `capture_execution::begin_protected_source_capture`'s dispatch. The
    test sets a 600,000-character paragraph, which is edit-entered into
    Protected View and starts the source capture, and then runs a
    test-workflow replacement. That replacement has no profile, so it never
    calls `begin_protected_install`, which is what cancels a capture. The
    capture's kept iterator was invalidated by the replacement's deletions.
    It reproduced alone, twice, and under `GTK_A11Y=none` as well as `test`,
    so the AT backend is not the cause. The capture now watches
    `insert-text` and `delete-range` with after-handlers, which do not run
    for an edit the edit lock stopped, and cancels at its next dispatch.
    The test then passed, with no iterator warning. All 56 `protected_view`
    widget tests pass, including
    `test_an_edit_entered_session_finds_and_copies_its_hidden_tail`, which
    needs the captured source after an edit entry. No production
    replacement reaches the gap: draft and local-history restores carry a
    profile, eviction never takes a modified page, and save formatting never
    runs on a protected buffer.
15. **The Python drivers' runtime root falls back to `/tmp` when the scope's
    `TMPDIR` is too deep.** The crash-recovery lane failed on the final tree:
    its runtime root is created in the `with-temp-scope.sh` scope, whose name
    carries the scope's PID and label, and with a seven-digit PID the private
    system bus socket
    (`/tmp/lt-scope-2366576-crash-recovery-mg9Kpb/lt-crash-rt-…/runtime/gtk-lush-session/system-bus`)
    came to 100 bytes, one over `dbus-daemon`'s limit, so the helper refused
    the inherited directory. The helper was right to refuse; the driver chose
    the directory. `smoke_runtime_dir.create_runtime_root` now applies the
    helper's rule (`runtime_dir_fits_sockets`, 99 bytes) per run and falls
    back to `/tmp`, as the helper does for a directory it owns. The drivers
    already remove the root on every exit path, so it needs the scope's leak
    check no more than before. `smoke_runtime_dir.py --self-test` (in
    `make check-session-hygiene`) pins the boundary and the crash lane's path;
    the old function produced the 100-byte path (red), the new one `/tmp`.
    `with-temp-scope.sh --session-runtime` checked `wayland-0` against 100
    bytes, which let through a scope the helper then refused; it now checks
    the system bus socket against 99.
16. **The builder lane treats an unknown `GtkLush` type in standalone
    validation as the tool's limit.** The builder-diagnostics lane reported
    one actionable finding: `gtk4-builder-tool validate` of
    `workspace-section.ui` does not know `GtkLushViewportSliceBin`. That
    predates this change (the template has used it since 2026-09-15). The
    standalone tool cannot load any type the app registers, and the
    classifier already said so for `Adw` and `Lushtext` types, so `GtkLush`
    joins them. The runtime probes, which instantiate the template under
    the app, still treat the same line as actionable; the classifier was
    shown red (`actionable`) before and green after on that line.

## 4.5 The `cargo-gtk-proof` warning scanner needs no copy of the family

The Rust scanner (`crates/cargo-gtk-proof/src/warnings.rs`) flags a line only
when it names a toolkit source (`gtk`, `gdk`, `gsk`, `adwaita`, `at-spi`,
`accessibility`) and a severity. None of the three new family lines
(mutter's `Failed to retrieve locale properties: …ServiceUnknown…` and
`There is no colord server available`, the portal frontend's
`Failed to load RealtimeKit property: …`) names one, and the portal line is
also exempt by the scanner's `portal` needle, so the scanner never flagged
them and needs no new entry. The proof corpus (`fixtures/proof-corpus/`)
compares outcome classes only, not warning lines, so there was no line-level
parity test to extend. Instead,
`private_system_bus_session_service_lines_are_not_toolkit_warnings` pins the
three lines as benign against a broadened predicate, and makes the PipeWire
remote-error needle (now built from `PIPEWIRE_COMMAND`) load-bearing with a
`Gdk-WARNING` sample. The scanner also reads the media daemons' logs from the
helper's `--log-dir` (`session-hygiene/pipewire.log`,
`session-hygiene/wireplumber.log`), since the helper, not the runner, now
starts them.

## 5.1 The real scan is in `check-policy`

`make check-session-hygiene` (a prerequisite of `make check-policy`) now
runs `./scripts/check-session-hygiene.py` after its self-test. On the
migrated tree it prints `session hygiene: no session spawns outside the
helper` and exits 0. Every 1.3 finding is gone without a pragma or an
allowlist entry. The gate's only exemptions are the helper itself, the
self-test, and the `*packages` inventory arrays.

Reverting one migrated site makes it red again. Re-adding
`dbus-run-session -- "$@"` to `confined_session` in
`scripts/run-portal-sandbox-smoke.sh` gave exit 1 with this finding:

```
scripts/run-portal-sandbox-smoke.sh:150: `dbus-run-session` is named in a shell command; start sessions through scripts/headless-session or the gtk-lush-proof-harness session library
```

The file was then restored from its copy, and the scan went green again.

## 6.5 Host evidence during one `make visual-geometry-smoke`

The run went from 2026-11-15 03:36:53 to 04:01:33 (24 min 40 s, 99 cases, all
passing, no `failure_status`). It ran on the final group 3+4 tree, commit
`67cc12e0`. Other worktrees ran their own, unmigrated, visual and
accessibility lanes on the same host at the same time. The host journal
therefore cannot read zero for the window. The branch's share is
attributed instead: a sampler recorded every `pipewire` and `wireplumber`
process, with its `HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME` and
`XDG_RUNTIME_DIR`, once per second from 03:12 to 04:02.

| Measure, in the window | Host total | This branch |
| --- | --- | --- |
| rtkit-daemon grants ("Successfully made thread") | 674 | **0** |
| rtkit-daemon burst-limit denials | 424 | **0** (it made no request to deny) |
| bluetoothd "Endpoint registered" | 2,745 | **0** (no session of the branch can reach the host system bus) |

How the branch's share was attributed:

- **rtkit.** The sampler saw 400 daemons from this branch, each with
  `XDG_STATE_HOME`/`XDG_CONFIG_HOME` under
  `<runtime>/gtk-lush-session/homes/`, and 1,363 from the other worktrees,
  each with the real `HOME` and no private state directory. Of the 674
  grants, 664 name a process from the other worktrees and none names one
  of the branch's. The remaining 10 name three processes too short-lived
  for the sampler, with the same nice -11 / RT 20 pattern as the other
  worktrees' bare PipeWire daemons.
- **Private system buses.** On the 99 cases' private system buses
  (`session-hygiene/system-bus-monitor.log`) there are **0** `MakeThread*`
  calls and **0** calls to `org.bluez`. The rest of the rtkit traffic is
  the portal frontend's startup `Properties.Get` of
  `org.freedesktop.RealtimeKit1` (297 calls, 3 per case) and its
  bus-daemon lookups of the name. The private bus refuses each one ("not
  provided by any .service files"): this is the bounded exemption of
  design D7, and no case printed `SESSION-HYGIENE:`.
- **Mutter** requested no realtime scheduling (0 `MakeThread*`), as at the
  spec stage.
- **`~/.local/state/wireplumber`.** `default-nodes` changed at 03:10:30
  and 03:47:21. The writers were the other worktrees' WirePlumber
  instances: a live sample at 03:10:35 showed one from
  `scale-file-index`'s visual lane with the real `HOME` and no
  `XDG_STATE_HOME`. Every WirePlumber this branch started wrote its state
  under the helper's private `homes/state`, which is removed with the
  session.

The spec-stage baseline (`evidence/baseline.md`) put an unmigrated case at
15 endpoints and 1 nice plus 3 RT grants. On the same 99 cases that is
about 1,485 endpoints and 396 grants, against 0 for the branch.

## 7.4 Sonar scope review

`sonar-project.properties` needs no change. `sonar.sources` already lists
`crates` and `scripts`, so it covers the helper binary
(`crates/gtk-lush/proof-harness/src/bin/gtk-lush-session.rs`), the session
module, the new axiom probes and samples, `scripts/headless-session`,
`scripts/smoke_session.py` and `scripts/check-session-hygiene.py`.
`sonar.exclusions` names none of them. Nothing was suppressed, so no
first-scan triage is owed for a suppression.

## 6.2 Full widget runs

| Run | Tree | Result |
| --- | --- | --- |
| 1 | groups 3+4 before A117 | 21 failures, all through the Save Changes dialog (17 `window`, 4 `editor-page`): a SIGSEGV in `gtk_accessible_announce`, which led to A117 and the announcement guard (decision 13) |
| 2 | after A117, merged with `main` `cfee0c48` | 1 failure: the Protected View capture abort (decision 14). 1 FLAKY: `markdown_preview::test_root_and_nested_code_blocks_repair_after_resize_at_constrained_width` |
| 3 | final tree, merged with `main` `14332d69` | 1690 tests in all three shards, 0 failures, 0 `OOM-KILL:`, 0 `SESSION-HYGIENE:`. 1 FLAKY: `workspace_section::test_large_reconciliation_is_batched_supersedable_and_preserves_state` |

**Both FLAKY lines are open, not explained.** Each is a GLib-loop wait that
timed out on the first attempt and passed on the retry. Neither reproduced
afterwards:

- **The markdown preview test (10 s wait after a window resize):**
  - 6 solo runs and 8 runs side by side all passed.
  - Three single-session runs of the whole `markdown_preview` module
    (125 tests each) gave 0 FLAKY.
  - The shard's load at the time was 6 to 8, so CPU starvation does not
    explain it.
  - The wait now reports the window width, the text view width, its margins,
    and each block's `width_request` against the expected value. The next
    recurrence will tell a window that never shrank from a stale repair.
- **The workspace reconciliation test (30 s wait):** three solo runs
  passed, at a load of 15 to 18.

Both tests are older than this change, and neither depends on the session's
buses. They are listed as open decisions for the maintainer, and 6.2 stays
unchecked.

The run-3 `editor-page` shard peaked at 2,011 MiB, inside its 2 GiB memory
scope. Its run-2 peak, on the tree merged with `cfee0c48`, was 386 MiB.
Between the two runs the tree gained `main` `14332d69`
(`rust-1-100-adoption`), the capture fix and the test instrumentation. The
cause is not attributed. The one test whose behaviour changed, the
replacement test that run 2 aborted early, peaks at 116 MiB alone. The peak
is close enough to the cap to OOM under a different test order, so it is
recorded as an open decision too.

The `editor-page` shard alone afterwards, on the same tree under a 3 GiB
scope (`make test-widget-shard WIDGET_SHARD=editor-page`): 408 tests, all
passed, 0 FLAKY, peak **401 MiB**, 0 OOM kills. So the 2,011 MiB peak is not
a property of the shard's tests. It came from the parallel run, where
`memory.peak` also counts page cache charged to the scope; which pages is
not attributed, so it stays open.

### 6.2 Stage 4: discrimination, causes and fixes (2026-11-15)

Both FLAKY lines are **pre-existing on `main`**, and both are now explained
and fixed. Method: `main` `14332d69` (the branch's merge base, a detached
checkout) against the branch, one side at a time. Each round ran the full
parallel widget suite (`widget-shards.py parallel --retries 1`) as load,
plus four concurrent loops. Each loop ran the two tests alternately, one
fresh session per run, with no runner retries, until the suite ended. Every
command went through `scripts/heavy.sh`. A run is `FLAKY` when the
harness's own second attempt passed, and `FAILED` when both attempts failed.

| Round | Side | Test | Runs | FLAKY | FAILED | Load (1 min) |
| --- | --- | --- | --- | --- | --- | --- |
| 1 (1 loop) | `main` | markdown / workspace | 85 / 84 | n/c | 0 / 0 | 4.5–9.3 |
| 1 (1 loop) | branch | markdown / workspace | 66 / 66 | n/c | 0 / 0 | 5.5–11.1 |
| 2 (4 loops) | `main` | markdown / workspace | 567 / 566 | n/c | **3** / 0 | 8.3–17.1 |
| 2 (4 loops, stopped early) | branch | markdown / workspace | 52 / 49 | n/c | 0 / 1 (session start, below) | 9.1–22.5 |
| 4 (4 loops) | `main` | markdown / workspace | 669 / 667 | **42 / 3** | **2** / 0 | 8.8–21.0 |
| 4 (4 loops) | branch with the fixes | markdown / workspace | 559 / 556 | 0 / 0 | 0 / 0 | 9.8–18.4 |

"n/c": not counted. Rounds 1 and 2 deleted the logs of runs that exited 0, so
first-attempt failures were lost. From round 4 on every run is classified.
The suite runs used as load passed on both sides, with 0 FLAKY. Both fixed
tests then passed 10 of 10 times in isolation, and the axiom samples A115,
A116 and A117 hold (`make gtk-axiom-sample AXIOM=<id> CHECK=1`).

**Markdown preview: a defect in the widget.** Code-block widths were never
repaired after the preview's own width changed. In the app the window shell
masks most of it, because it queues a repair after its transitions and when
the side-by-side width clamp moves.

- `LushtextMarkdownPreview` is a `GtkBox` subclass, so `GtkBoxLayout`
  allocates it, and GTK does not call its `size_allocate` override.
  Instrumented, the override printed nothing in two runs.
- `GtkWidget` has no `width` property, so its `notify::width` handler never
  ran either.
- So the only repair after a resize was the render's own 50 ms deferred
  pass. The test passed only when the resize landed inside that pass.

Made deterministic, by settling the render's deferred passes before the
resize, the test failed 3 of 3 runs, and every run reported the stale
repair: `window 320x420, text view 320 … blocks [(820, 240, …), (868, 288,
…)]`.

Fix: the preview observes its text view's horizontal adjustment through
`gtk_lush_viewport::ViewportObserver`, whose page size GTK sets from the
text view's allocated width. On a page-size change it runs only the deferred
passes, because the change arrives inside the text view's allocation
(ledger A13). The dead override and the `width` notify are removed. The
deterministic test now passes, and so do all 117 `markdown_preview` tests
and the `window_preview` and `focus_mode` modules.

**Workspace reconciliation: the test sampled a transient state.** Its
second wait needed the first reconciliation in flight between two batches,
20 ms apart.

- `wait_until` drains the main loop to exhaustion. On a loaded host each
  dispatch outlasts the gap, so one drain applies every batch and the
  in-flight state is never seen.
- Fault injection reproduced it at will: a drain that adds 25 ms per
  dispatch timed out after 30 s, with 5,674 polls.
- The panic line of the `main` FLAKY runs is that same wait.

Fix: the test holds the first plan with a one-hour batch delay until the
latest refresh supersedes it, and the latest plan then runs at 20 ms. It
passed 6 of 6 runs under the same slowed drain. The `gtk-testing` reference
records the idiom.

**Found by the discrimination, branch only: a session could exec a replaced
binary.**

- Every `cargo build`, even a no-op one, re-uplifts
  `target/debug/gtk-lush-session`, giving it a new inode.
- Concurrent sessions in one worktree then failed in three ways:
  - `dbus-run-session: failed to exec '…/gtk-lush-session (deleted)'`, from
    the helper's `current_exe()`;
  - `Text file busy` or `No such file or directory`, from the shim's
    `exec`.
- Reproduction: four session loops beside a no-op build loop failed 2 of
  152 sessions.

Fix:
- The helper starts its inner stage through `/proc/<pid>/exe`. The
  `GTK_LUSH_SESSION_BIN` it exports stays the file path, because a consumer
  in another pid namespace (the axiom runner's Flatpak SDK sandbox) would
  resolve the link to another process or to none.
- The shim retries a failed `exec` ten times, 0.1 s apart. This needs
  `execfail` and `set +e`, because bash 5.3 ends an errexit shell on a
  failed exec even inside `||` or `if`.

Result: 0 failures in 441 sessions.

**Review hardening.**
- A per-test widget child now refuses to run unless
  `GTK_LUSH_SESSION_ACTIVE` is set. Only the helper's inner stage sets it.
  A child started by hand in a desktop shell with the runner variable
  printed the refusal and touched no display.
- The widget lane no longer allowlists
  `Gtk-CRITICAL … org.a11y.atspi.Registry`: under `GTK_A11Y=test` it cannot
  occur, and the round-4 suite proves zero. The `document_scale` criticals
  filter drops the same exemption.
- `cargo-gtk-proof` never retries a session-hygiene exit (93) as an AT-SPI
  start-up failure.

**Open decision 2 (the 2,011 MiB `editor-page` peak): not reproduced.**
Across four parallel suite runs, two per side, the `editor-page` scope
peaked at 384 to 393 MiB. Five-second `memory.stat` samples showed about
117 MiB anon and 19 MiB file. The one-off peak stays unattributed.

## 6.3 GTK axioms on the final tree

- `make gtk-axioms` on the final tree (merged with `main` `14332d69`): the
  probe binary passed and 104 sample checks held, 0 slow skipped. The load
  was 3 to 7 (an earlier run at load 25 saw the timing probes A70 and A79
  violate; both held alone and hold here).
- `make check-gtk-axioms`: passed.
- `make gtk-axioms-runtimes`, once:
  - `org.gnome.Sdk//50` (GTK 4.22.2, Adw 1.9.0): 96 holds, 5 skipped (needs
    RemoteDesktop), 2 skipped (needs a session bus: A115 and A116, because
    the lane runs every sandbox with `--no-session-bus`), 1 skipped (slow).
    A117 holds on the floor, so its ledger row now records the floor.
  - `org.gnome.Sdk//51` (GTK 4.24.1, Adw 1.10.0): the same counts.
  - `org.gnome.Sdk//master` (GTK 4.25.1, Adw 1.11.alpha): 6 violated (A4,
    A70, A83, A85, A86, A112) and 1 fixture-invalid (A29). None of them is an
    axiom this change adds or relies on, and A115 to A117 do not fail there.
    A nightly verdict is a preview of the next adoption, never a ledger
    restatement (`build.md`), so the ledger is unchanged for them. The lane
    exits 1 because of the nightly SDK alone.
- A115 and A116 therefore have no floor verdict from this host. CI's
  `GTK Axioms (GNOME 50 floor)` job runs their probes on `fedora:44`.

## 6.4 Lanes on the final tree

Each lane ran once, headless, through `scripts/heavy.sh` (lane logs in the
handoff directory, `lanes/`). No lane log has a `SESSION-HYGIENE:` line.

| Lane | Result |
| --- | --- |
| `make crash-recovery-smoke` | first run **failed**: the helper refused the driver's runtime directory, one byte over the system bus socket limit (decision 15). After the fix: passed (both draft markers restored) |
| `make automation-smoke` | passed (103 catalogue entries, 12 action-state checks) |
| `make command-palette-notes-smoke` | passed |
| `make portal-sandbox-smoke` | passed (the Flatpak launch cases skip as in 4.3, which predates this change) |
| `make builder-diagnostics-smoke` | first run **failed** on one actionable line, `Invalid object type 'GtkLushViewportSliceBin'` from standalone validation (decision 16). After the fix: passed, 9/9 runtime probes, 0 actionable |
| `make editor-glyph-live-smoke` | passed |
| `cargo gtk-proof run --case-filter open-popover` | 32/32 cases passed, but the first run exited 143: `with-temp-scope.sh` records a SIGTERM it received as 143. Nothing in the run sends one (every signal the runner and the helper send goes to a child's own PID), and the rerun of the same 32 cases exited 0, as did a 2-case `command-palette-overlay` run. The TERM came from outside the run and is not attributed |
| `make accessibility-smoke` | passed (run right before the commit) |
| `make visual-smoke` | passed (run right before the commit) |
| `make visual-geometry-smoke` | passed (run right before the commit) |
| `make responsiveness-smoke RESPONSIVENESS_TIER=quick` (functional) | passed (433 rows: 324 ok, 108 skipped by design, 1 `exceeded_cap` in `eviction/reselect`); guard verdict `clean`, `system_bus_records=0`. The private system bus carried bus housekeeping (Hello, AddMatch from the dconf clients) and Mutter's start-up `StartServiceByName` of `locale1` and `colord`, which fail fast there. No app call |

### 6.4 Interleaved measurement (`scripts/measure.sh`)

| Run | 1-min load at start / end | Status |
| --- | --- | --- |
| `qh-main-1` | 2.96 / 3.94 | valid, unpaired |
| `qh-branch-1` | **4.03** / 2.82 | non-verdict (load), retried as round 3 |
| `qh-main-2` | 3.00 / 2.76 | verdict |
| `qh-branch-2` | 2.76 / 2.36 | verdict |
| `qh-main-3` | 2.78 / 2.72 | verdict |
| `qh-branch-3` | 2.73 / 3.28 | verdict |
| `qh-main-4`, `qh-branch-4`, `qh-main-5` (`dense/2000` only) | 4.02, 4.54, 4.46 | attribution rounds, non-verdict (load) |
| `qh-branch-5` (`dense/2000` only) | 3.63 / 2.80 | attribution round |

The campaign waited about 45 minutes for the measurement lock behind another
worktree's `make proof-strength`. The host's `jetbrains-toolbox` ran at about
150 % CPU in every round on both sides. The runbook's reference precondition
asks for no process at 80 % or more, so that is a recorded deviation, shared
by both sides.

**Verdict: no budget shift** (design, "Measurement", has the figures). The
side report of the four verdict rounds flagged
`dense/2000/wrap-off@plain` background `post_open_install_slice`, but only as
a median of two reps in which one rep has a single 22 ms dispatch. Elsewhere
that dispatch is attributed to `validation`. With the attribution rounds
added (four reps a side), nothing is flagged, and the worst dispatch and the
settle time per rep match. The absolute over-budget `dense/2000/wrap-off`
dispatches are on `main` as well, and predate this change.

