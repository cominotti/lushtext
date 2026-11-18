# Design

## Context

The session helper is the one implementation of LushText's private headless
sessions (`gtk-lush-proof-harness`, `session` module; `scripts/headless-session`
execs its `gtk-lush-session` binary). With quiet media, `run_inner_body`
starts PipeWire, waits for it (`pipewire_ready`: its socket plus a
successful `pw-dump`), starts WirePlumber with `--profile=policy`, and then
runs:

```rust
// WirePlumber publishes the session-manager metadata once its policy
// runs; wait for it so a capture links on its first try.
let _ = wait_until(|| {
    pw_dump().is_some_and(|dump| dump.contains("\"metadata.name\": \"default\""))
});
```

`wait_until` polls every 50 ms until `READY_TIMEOUT` (10 s). That code came
with the helper (`2843a6d3`). It started costing every lane when
`67cc12e0` (PR #110) moved them onto the helper. Before that, the lanes waited
only for PipeWire and never for WirePlumber.

The wait's purpose is sound. `pipewiresrc path=<node>` names a target, and it
is WirePlumber's linking hooks (`policy.linking.standard`) that create the
link. A capture that starts before they load could stall or fail on its first
try. The predicate is what is wrong.

## Measured baseline

Details and loads are in `evidence/baseline.md`. All figures are non-verdict
wall times on a loaded host (1-minute load 18-54).

- A quiet `screencast` session around `true` takes 11.00-11.37 s; with
  `--media off` it takes 0.78-1.06 s.
- The `default` metadata exists: `pw-cli ls` lists it (id 37, owned by the
  WirePlumber client) and `pw-metadata -n default` finds it. `pw-dump` omits
  it, as it omits every metadata object without entries
  (`persistent-sm-settings`, `sm-objects`, `default-profile`, `default`,
  `route-settings`, `filters`). The helper therefore waits through a tool
  that never prints the object it waits for. 0 of 203 saved dumps of one
  trio contain it.
- Waiting for `session.services ∋ policy.linking.standard` takes
  61-160 ms (2-3 polls). The session drops to 1.3-1.9 s, and with it 10 of 10
  ScreenCast captures linked on their first try (link plus first buffer
  0.14-0.18 s).
- Lane cost on 2026-11-18 (round 1, load 4.0-9.5): visual-geometry 24.2 min
  (99 media sessions), accessibility 13.0 min (59), visual 9.8 min (45);
  trio 47.0 min, of which about 33.8 min are these waits. On 2026-11-06,
  before PR #110: 7.1, 2.4 and 1.5 min.

## Decisions

### D1. Wait for `session.services` to list `policy.linking.standard`

WirePlumber's `session-services.lua` sets the WirePlumber client object's
`session.services` property from the features that loaded:
`policy.linking.standard` is listed when the standard linking policy is
loaded. `base`, which every profile inherits, requires the component, and
`wireplumber.components.rules` puts it `before`-last: "session-services.lua
must execute at the very end". When the property lists the linking policy,
every component of the profile has loaded, the linking hooks included. It
sits on a Client object, which `pw-dump` always prints with its properties.

Alternatives rejected:

- **Delete the wait, or shorten it.** Either would hide the race the wait
  exists for, and would still not notice a WirePlumber that never loads its
  policy. Shortening it keeps a blind wait.
- **Wait for the `default` (or `filters`) metadata through the registry**
  (`pw-cli ls`, `pw-metadata`). The object exists, but the component that
  creates it loads before the linking hooks (`policy.default-nodes` and the
  linking scripts require it). So its presence does not prove that linking is
  loaded, and it would need a second tool in the hygiene allowlist.
- **`wpctl status` succeeding.** This gives no ordering guarantee, and it is
  another tool.

### D2. A pure predicate in `session::observe`

`pub fn wireplumber_policy_ready(pw_dump: &str) -> bool`, a sibling of
`device_apis`, scans the dump by key, as `device_apis` does (the crate has no
JSON dependency, and adds none). For each `"session.services"` string value
it parses the bracketed list `[a, b, c]` and trims each member. It returns
true iff a member equals `policy.linking.standard`. The match is by member,
not by substring:

- the saved dumps list the members in different orders;
- a hypothetical `policy.linking.standard-extra` must not count.

Only the session manager sets `session.services`. In a private session no
other client sets it, so the scan needs no object association. The
function is additive public API of an unpublished `0.0.0` crate. It sits
where the session's other `pw-dump` readings live, and its unit tests use
fixtures taken from real saved dumps.

### D3. A missed deadline fails the session, loudly, before the command

The wait keeps `READY_TIMEOUT` (10 s) as its deadline (D5). On a miss the
helper:

- writes the last successful dump to `<log-dir>/pw-dump-not-ready.json`;
- prints one bounded line:
  `gtk-lush-session: WirePlumber did not load its linking policy within 10 s (session.services never listed policy.linking.standard; last pw-dump in <path>)`;
- returns `TEST_FAILURE_EXIT_CODE` (101) without starting the command.

`run_inner` already tears down the media daemons, the monitors and the
system bus on every return from `run_inner_body`, so the early exit leaks
nothing. The exit code is neither 77 (unsupported host: the tools are
there, `check-tools` said so) nor 93 (session hygiene: the host was not
touched), so lanes report a plain failure.

The wait's mechanics become testable without daemons. A small helper takes a
deadline and a dump source (`FnMut() -> Option<String>`) and returns
`Ok(())`, or `Err` carrying the last dump. Production passes `READY_TIMEOUT`
and `pw_dump`; the unit tests pass a fake source and a deadline of a few
milliseconds.

### D4. The WirePlumber miss is never a host-support skip; the PipeWire skip class is deferred

The visual (`scripts/run-visual-smoke.sh`) and command-palette notes
(`scripts/run-command-palette-notes-smoke.sh`) runners grep a failed
capture's session log for host-support phrases. A match records the case as
`skipped` and calls `smoke_skip`, which **exits 0 for the whole lane**. The
accessibility, visual-geometry and crash-recovery lanes decide host support
before any session, from `check-tools` and the commands they need, and fail
a case whose session fails.

The D3 line is worded to match none of those phrases. The implementation
greps it against every runner's skip patterns, and the seeded failure (D7.1)
proves the lanes fail on it.

**Deferred: `PipeWire did not become ready`.** One of the skip phrases came
with `9a483f80` (2026-06-07). Then the capture script started PipeWire
itself (`wait_for_pipewire`), and a host where PipeWire could not start was
a host-support skip. Since PR #110 only the helper prints the phrase, and
both `wait_for_pipewire` definitions (`capture-lushtext-mutter.py`,
`crash-recovery-smoke-driver.py`) are uncalled. So a helper PipeWire
readiness failure now turns the visual lane green with `SKIP:`.

Whether that should become a failure depends on hosts this stage cannot
observe: the scheduled `end-user-smoke.yml` containers run the lane, and CI
is not polled during the programme. It also touches two fingerprinted
scripts. So it is not part of this change. It is recorded as a
`NEW CHANGE PROPOSED` line (`media-skip-classification`) in
`.claude/handoffs/PARALLEL-LEDGER.md`.

### D5. `READY_TIMEOUT` stays 10 s

Readiness took 61-160 ms at load 50, so the margin is 60x or more and costs
nothing on a healthy session. A healthy session now reaches the deadline
only when it is broken, and then it fails. The constant keeps its kind and
rustdoc.

### D6. `--expect-dirty` keeps working

The by-hand `--expect-dirty` mode runs the bare `main` profile. `main` also
inherits `base`, which requires `support.session-services`, and it requires
`policy.standard`, so the same signal applies. The implementation verifies by
hand that `--expect-dirty` still reaches its guard verdict (the dirty
configuration caught, exit 0 from the mode). If `main` ever stops listing the
linking policy, the mode fails at readiness with the D3 line. That would be
visible, not silent.

### D7. Seeded failures

Two proofs, neither committed:

1. **The awaited signal absent, in a real session.** A temporary edit of
   `wireplumber-quiet.conf` adds
   `wireplumber.profiles = { policy = { policy.standard = disabled } }`.
   `session.services` then exists without `policy.linking.standard`, which
   proves the member semantics in a real session. If WirePlumber refuses the
   override, the seed is `support.session-services = disabled`, which leaves
   the property unset. Either way, expected:
   - `scripts/headless-session session --profile screencast -- true` exits
     101 with the D3 line after about 10 s;
   - one case each of `make visual-smoke`, `make visual-geometry-smoke` and
     `make accessibility-smoke` fails, and none skips.

   These runs use their own artifact directories, or they run before the
   green runs of D8, so the last summary of each lane before the commit is a
   passing one.

   The edit is reverted before any gate runs. `make check-session-hygiene`
   never sees it; its drift check covers module-rt and `hardware.*`, not
   these features.
2. **The lanes still catch a real defect.** A temporary, uncommitted visual
   change inside a protected region of one visual-geometry scenario (for
   example a CSS colour of a protected surface) makes
   `make visual-geometry-smoke` fail that invariant. A wrong accessible name
   on one AT-SPI anchor does the same for `make accessibility-smoke`.

### D8. Lane proof and the record of lane cost

On the final tree, each media lane runs 10 consecutive times, green, one
`scripts/heavy.sh` call per run:

- `make visual-geometry-smoke`;
- `make accessibility-smoke`;
- `make visual-smoke`.

Each run's wall time and `/proc/loadavg` before and after go in
`evidence/lanes.md`, beside the baseline. Any failure, `FLAKY` or `SKIP:`
line is a blocker (`preexisting-blockers.md`). It is investigated, fixed,
and the count restarts.

**Expected after:** visual-geometry about 8 min, accessibility about 3.5
min, visual about 2.5 min. If the visual lane stays clearly above about
3 min at a comparable load, the rest of its growth since 2026-11-06 has
another cause. In that case the implementation writes a `NEW CHANGE PROPOSED`
line in `.claude/handoffs/PARALLEL-LEDGER.md` rather than widening this
change.

The before and after figures go into the QH record
(`docs/next/long-line-documents.md`, "QH `quiet-headless-session-services`").
That record is where the lanes' session cost is tracked; no separate
lane-cost record exists.

## Scope boundaries

- **Kani: none** (no proven module touched).
- **GTK axioms: none.** The change relies on no GTK-family behaviour. The
  WirePlumber behaviour it relies on (`session-services.lua` ordered last;
  `pw-dump` omitting empty metadata) is not a GTK axiom. It is recorded in
  `.agents/rules/build.md`, "PipeWire and WirePlumber updates", as a
  re-check owed on every WirePlumber update, beside the module lists.
- **No timing verdict.** Lane wall time is not a budgeted path.
- **No application code, UI file, smoke runner or rule outside build.md
  changes**, so neither the UI-prefix nor the fingerprinted-script proof
  obligations apply. The lanes run anyway, as D8.

## Risks

- **A capture race hidden by the 10 s.** The capture probe linked 10 of 10
  on its first try under load 50. The lanes' 10 green runs are the
  full-scale proof. A race that surfaces there is a real defect, fixed in
  this change.
- **A future WirePlumber that renames the service or the property.** The
  session then fails loudly at readiness (D3), and the build.md update check
  points at the predicate.
- **A scheduled CI container where WirePlumber never loads its policy**
  turns from a silent pass, or a later capture failure, into a session
  failure. That is intended, and is read once at the end of the programme
  with the rest of CI.
