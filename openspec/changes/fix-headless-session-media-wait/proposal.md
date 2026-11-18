# Proposal

## Why

Since `quiet-headless-session-services` (PR #110, `67cc12e0`, 2026-11-15),
every headless session with quiet media spends 10 s waiting for nothing.
These are the `screencast` and `accessibility` profiles, used by the
visual-geometry, accessibility, visual, crash-recovery and command-palette
notes lanes. After starting WirePlumber, the session helper
(`crates/gtk-lush/proof-harness/src/session/mod.rs`) polls `pw-dump` until it
prints `"metadata.name": "default"`, gives up after `READY_TIMEOUT` (10 s),
and ignores the result. Its comment says the wait exists "so a capture links
on its first try".

WirePlumber's `policy` profile does publish the `default` metadata object.
`pw-dump`, though, prints no metadata object that has no entries, and in a
session without audio devices `default` has none. So the predicate can never
hold. A quiet session takes 11.0-11.4 s instead of 0.8-1.1 s, and none of the
203 saved dumps of one run of the three media lanes contains the awaited text
(`evidence/baseline.md` §1-2).

That is 203 waits per smoke trio, about 34 of its 47 minutes. The trio took
11.0 min on 2026-11-06 and 47.0 min on 2026-11-18 (visual-geometry
7.1 → 24.2, accessibility 2.4 → 13.0, visual 1.5 → 9.8). Every UI-prefix
commit owes that trio, often more than once per change. And because the
result is ignored, a WirePlumber that never loads its linking policy is not
reported: the session goes on, and the capture fails later or, if the policy
was only slow, by chance does not.

## What Changes

- The helper waits for the signal WirePlumber publishes once its profile has
  fully loaded: the WirePlumber client object's `session.services` property
  lists `policy.linking.standard`. `session-services.lua` sets that property,
  and WirePlumber orders it after every other component, the linking hooks
  included (`evidence/baseline.md` §3). A pure predicate in
  `session::observe` reads it from `pw-dump`. With it, readiness took
  61-160 ms at load 50, and 10 of 10 captures linked on their first try
  (§4).
- A missed deadline is a loud session failure. The helper exits
  `TEST_FAILURE_EXIT_CODE` before the command starts, with a bounded
  diagnostic and the last dump saved to the log directory. The diagnostic is
  worded so that no lane's host-support classifier reads it as a skip, and
  the visual and command-palette notes runners stop classifying the helper's
  own "PipeWire did not become ready" failure as a host-support skip (their
  `smoke_skip` exits 0 for the whole lane); two uncalled `wait_for_pipewire`
  helpers go.
  `READY_TIMEOUT` stays 10 s; it is now only the failure deadline.
- Proof: failing-first unit tests of the predicate and of the wait's outcome;
  a seeded real-session failure (WirePlumber's `policy` profile without
  `policy.standard`) that fails the lanes rather than skipping them; a seeded visual
  defect the lanes still catch; and 10 consecutive green runs of each media
  lane on the final tree, with per-lane wall time before and after.
- Docs: the proof-harness CHANGELOG and README, `.agents/rules/build.md`
  ("PipeWire and WirePlumber updates": re-verify the readiness signal on a
  WirePlumber update), the QH record in `docs/next/long-line-documents.md`
  (lane cost before and after), and `docs/next/gtk-lush.md`.

## Capabilities

### Modified Capabilities

- `headless-session-hygiene`: ADDED requirements "Quiet media are ready
  before the command starts", "A media readiness miss fails the session" and
  "Lanes fail on a media readiness failure".

## Impact

- `crates/gtk-lush/proof-harness/src/session/{mod.rs,observe.rs}` (GTK Lush
  family crate, internal `0.0.0` path API: one additive public function in
  `session::observe`; no new dependency).
- No application code, UI files, GTK behaviour, Kani-proven module or timed
  path. Kani: none (no proven module touched). GTK axioms: none (no GTK-family
  behaviour is relied on; the WirePlumber behaviour relied on is recorded in
  `build.md` with the existing PipeWire/WirePlumber update check). No timing
  verdict is owed: lane wall time is evidence, measured through `heavy.sh`
  with the load recorded.
- Every media lane gets about 10 s faster per session. Expected trio: about
  47 → 13-15 min.
