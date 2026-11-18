# Baseline (spec stage, 2026-11-18)

Host `sophia`, Fedora 45 toolbox, PipeWire 1.6.11, WirePlumber 0.5.19,
mutter 51.1, `origin/main` `187c6299`. Every figure here is **non-verdict**
wall time: it was run through `scripts/heavy.sh` on a shared, loaded host
(1-minute load 18 to 54, recorded beside each block). Lane wall time is
evidence, not a budgeted path, so no `measure.sh` verdict is owed.

## 1. The wait costs 10 s per media session

`gtk-lush-session session --profile screencast [--media off] -- true`, with
private `XDG_{CONFIG,STATE,DATA,CACHE}_HOME` directories, alternating, three
pairs. Load before 21.40 17.92 13.63, after 21.91 18.36 13.93.

| Run | media quiet | media off |
| --- | --- | --- |
| 1 | 11.00 s | 0.92 s |
| 2 | 11.03 s | 0.78 s |
| 3 | 11.37 s | 1.06 s |

All six exited 0 with a clean guard verdict. The quiet session spends its
`READY_TIMEOUT` (10 s) in
`wait_until(|| pw_dump().is_some_and(|d| d.contains("\"metadata.name\": \"default\"")))`
(`crates/gtk-lush/proof-harness/src/session/mod.rs`), whose result is
discarded.

## 2. The awaited object exists; the observer never prints it

A probe run inside a quiet `screencast` session (inner stage polling `pw-dump`
every 250 ms for 10 s, then `pw-metadata`, `wpctl status` and `pw-cli ls`):

- `pw-dump` listed, from the first poll to the last, only the Metadata
  objects `settings` (30), `schema-sm-settings` (32) and `sm-settings` (34).
- `pw-cli ls` at the end listed ids 33 `persistent-sm-settings`,
  35 `sm-objects`, 36 `default-profile`, **37 `default`**, 38
  `route-settings` and 39 `filters`, all owned by the WirePlumber client
  (31).
- `pw-metadata -n default` printed `Found "default" metadata 37` and exited 0.
- `wpctl status` exited 0 and showed the `Settings / Default Configured
  Devices` section that the `default` metadata backs.

So WirePlumber's `policy` profile does publish the `default` metadata. Every
object `pw-dump` omitted is an empty metadata object (no entries; `default` is
empty because the session has no audio devices), and every one it printed has
entries. The helper waits for the object through a tool that never prints it.
The same holds in every saved session: 0 of 99 `pw-dump.json` files of a
visual-geometry run, 0 of 59 of an accessibility run and 0 of 45 of a visual
run contain `"metadata.name": "default"` (dumps under the worktrees'
`build/smoke/*/…/session-hygiene/`, 2026-11-18).

## 3. The signal WirePlumber does publish when its policy is loaded

`/usr/share/wireplumber/wireplumber.conf` (0.5.19):

- `base` requires `support.session-services`, provided by
  `session-services.lua`, which sets the WirePlumber client object's
  `session.services` property to the services its loaded features provide
  (`policy.linking.standard` among them when `policy.standard` loaded).
- `wireplumber.components.rules` adds `before = [ support.session-services ]`
  to every other component ("session-services.lua must execute at the very
  end"), so the property appears only after every other component of the
  profile, the linking hooks included, has loaded.

Every saved dump has it on client 31, `application.name = "WirePlumber
(policy)"`, for example
`"session.services": "[policy.device.routes, policy.device.profile, policy.default-nodes, policy.linking.standard, policy.linking.role-based]"`.
The member order differs between dumps, so it is matched as a list member.

**Experiment** (helper patched, not committed: the predicate became "the dump
contains `policy.linking.standard`", plus a timing line), 10 sessions
around `true`, load 43.65 → 49.55:

| | readiness after | polls | session wall |
| --- | --- | --- | --- |
| min | 62.2 ms | 2 | 1.26 s |
| max | 84.9 ms | 2 | 1.91 s |

## 4. A capture links on its first try with the new wait

Same patched helper; `gtk-lush-session run --profile screencast --monitor
640x480 -- python3 capture-probe.py` (a GTK window, mutter `RecordMonitor`
`Meta-0`, `gst-launch-1.0 pipewiresrc path=<node> num-buffers=1`), 10 runs,
load 50.18 → 50.44:

| | readiness after | link + first buffer | capture since app start | session wall |
| --- | --- | --- | --- | --- |
| min | 61.0 ms | 0.141 s | 0.480 s | 2.31 s |
| max | 160.2 ms | 0.180 s | 0.560 s | 3.18 s |

10 of 10 exited 0, each wrote a 640 x 480 RGBA PNG, and every guard verdict
was clean. The probe scripts are kept untracked in
`.claude/handoffs/fix-headless-session-media-wait/baseline/` (they start no
daemon themselves, but `make check-session-hygiene` scans tracked files).

## 5. Lane wall time before (from `.claude/handoffs/stream-replace-all/runs.log`)

| Lane | 2026-11-06 (before PR #110) | 2026-11-18 round 1 (09:17-10:04, load 4.0-9.5) | media sessions per run | 10 s waits per run |
| --- | --- | --- | --- | --- |
| `make visual-geometry-smoke` | 7.1 min | 24.2 min | 99 | 16.5 min |
| `make accessibility-smoke` | 2.4 min | 13.0 min | 59 | 9.8 min |
| `make visual-smoke` | 1.5 min | 9.8 min | 45 | 7.5 min |
| Trio | 11.0 min | 47.0 min | 203 | 33.8 min |

Session counts are the `session-hygiene.txt` files with `media=quiet` of one
complete run of each lane (worktrees `slice-large-directory-collapse` and
`apply-verification-altitude-redesigns`, 2026-11-18). In the visual lane,
the WirePlumber log of three captures was created 10.1-10.2 s before mutter's
first line (`modified-tab` 18:16:30.43 → 18:16:40.62, `notes-dense`
18:22:19.38 → 18:22:29.52, `dark-style` 18:25:07.81 → 18:25:17.94). The
devflow plan's "6-7 media sessions" for that lane was wrong: every one of its
45 captures starts its own media session, so the visual lane's growth
(1.5 → 9.8 min) has the same cause.
