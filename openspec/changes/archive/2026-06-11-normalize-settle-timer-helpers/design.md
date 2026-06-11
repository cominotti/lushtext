## Context

`docs/next/gtk-lush.md` defines Phase 0 as prerequisite simplification before
any GTK Lush API freezes. The first two Phase 0 follow-ups are archived:
Markdown preview no longer uses a hand-animated preview `GtkPaned`, and pure UI
projections have been normalized to GTK-native bindings where safe.

The remaining Phase 0 simplification is the timer surface. LushText currently
uses generation-counter `timeout_add_local_once` patterns across preview,
minimap, command palette, search panel, notes, session/workspace persistence,
status pulse cleanup, file monitors, and refresh flows. Nearby code also uses
repeating pollers, idle deferrals, chunked-yield loops, and stale async
generation checks. These look related, but they do not all have the same
lifecycle or safety contract.

## Goals / Non-Goals

**Goals:**

- Build a complete audit of GLib timer, idle deferral, generation-counter, and
  `SourceId` cancellation sites before conversion.
- Introduce a private in-tree helper shape that prototypes the future
  `gtk-lush-settle` concepts without creating or exposing a public crate API.
- Convert safe superseding one-shot timers, debounce flows, and settle-burst
  repairs so each uses one local idiom for stale callback rejection and pending
  state.
- Preserve existing timing windows, readiness blockers, focus behavior,
  persistence ordering, search triggering, notification pulse cleanup, and
  rendered geometry.
- Record deliberate exceptions for timer-like code that belongs to later
  `gtk-lush-tasks`, polling/lifecycle, or domain-token work.
- Update rules and GTK Lush roadmap text only after the helper proves a
  repeatable local pattern.

**Non-Goals:**

- No public `gtk-lush-settle` API, functional family crate, crates.io package,
  or family crate dependency is introduced.
- No replacement for GTK's main loop, Libadwaita transitions, app state, or
  message/control-flow architecture is introduced.
- No broad rewrite of recurring pollers, long-running heartbeats, async worker
  freshness tokens, service-domain generations, or chunked model population is
  required.
- No intentional changes to user-visible timing, persisted data formats, D-Bus
  automation fields, action contracts, or visual design.

## Decisions

### Use a private helper module, not the GTK Lush crate

The implementation should add a private LushText helper under
`crates/lushtext-core`, most likely near the UI scheduling or support modules.
The helper may mirror future `gtk-lush-settle` vocabulary, but it remains an
application-internal prototype.

Alternative considered: start implementing `crates/gtk-lush/settle` now. That
would skip the Phase 0 learning step and freeze public API shape before the
repo-wide audit proves which concepts survive real migration.

### Split the helper into a small vocabulary

One type is probably too vague. The helper should expose a small private
vocabulary matching the different proven contracts:

| Primitive | Contract | Example users |
| --- | --- | --- |
| `Debounce` | Bump generation and run one trailing callback after a quiet window | search entries, preview render, workspace/session persistence |
| `SettleBurst` | Track pending layout/repair work and run after the burst quiets | minimap reflow settle, preview layout/code-block settle |
| `SupersedingTimer` | Re-arm one delayed action; stale firings no-op | status pulse cleanup, focus-mode affordance hide, delayed progress visibility |

The exact type names can change during implementation, but the contracts must
remain separate enough that converted call sites stay readable.

Alternative considered: a generic `schedule_after` wrapper around
`timeout_add_local_once`. That would reduce repetition but would not encode the
staleness, pending-state, and re-arm semantics that make the pattern valuable.

### Classify before converting

Every candidate belongs to one of these audit classes before code changes:

| Class | Treatment |
| --- | --- |
| Debounce | Convert when it is a superseding trailing one-shot and tests can prove latest input wins |
| Delayed settle/repair | Convert when pending state and readiness semantics can be preserved |
| Superseding one-shot | Convert when re-arming only invalidates older callbacks and does not need explicit `SourceId` cancellation |
| Chunked yield | Inventory; convert only if the helper deliberately includes a yield/chunk primitive and TreeListModel or buffer tests prove timing unchanged |
| Heartbeat/polling | Defer; these are recurring lifecycle-owned sources, not settle helpers |
| Stale async freshness | Defer to future `gtk-lush-tasks` or freshness-token work |
| Pure model/domain generation | Leave out of scope |

Alternative considered: convert every `timeout_add_local_once` call. That would
mix unrelated scheduling motives and could break model population, background
I/O retry, or automation readiness loops.

### Preserve readiness and visual contracts at the call site

Converted helpers should make pending state easier to read, but they must not
change the readiness contract. If a workflow currently blocks
`visual-geometry-settled`, `idle`, or a narrower readiness predicate, the
converted code must report the same blocker while work is pending and clear it
only after the existing repair/action has completed.

For minimap, preview, adaptive layout, or other rendered/timed surfaces,
successful implementation requires widget tests plus the visual-geometry lane
that can see the rendered pixels, not just unit tests for the helper.

Alternative considered: verify helper behavior only with unit tests. That would
miss GTK allocation, native minimap rendering, code-block width repair, and
status-pulse timing regressions.

### Keep `SourceId` cancellation only where it is the real lifecycle

Some current sites store `SourceId` because the source is explicitly installed
and removed as part of a lifecycle, such as recurring pollers. Others store
`SourceId` to cancel a one-shot that could be expressed as a superseding timer.
The audit should distinguish these before editing.

Alternative considered: ban `SourceId` in UI code. That is too strong; GTK
poll sources and recurring lifecycle timers still need explicit ownership and
cleanup.

## Risks / Trade-offs

- [Risk] The helper overfits to today's LushText call sites. -> Mitigate by
  keeping it private and rewriting docs as "prototype source material" rather
  than public API.
- [Risk] A recurring poller is converted into a one-shot helper and loses
  lifecycle cleanup. -> Mitigate through the audit classes and explicit
  out-of-scope list.
- [Risk] Converted settle paths clear readiness too early. -> Mitigate with
  focused readiness tests and automation checks when readiness blockers or
  snapshot-visible fields change.
- [Risk] Visual surfaces appear green in unit tests but regress in pixels or
  allocation timing. -> Mitigate with widget tests and visual-geometry proof
  whenever minimap, preview, adaptive layout, or rendered timer effects change.
- [Risk] The helper adds abstraction without removing enough duplication. ->
  Mitigate by requiring migrated sites to become simpler or more explicit, and
  by documenting exceptions rather than forcing awkward conversions.

## Migration Plan

1. Create the audit inventory and classify every timer-like site.
2. Add the private helper with pure generation decision logic and tests.
3. Convert a small low-risk set first, such as direct search/palette debounces
   and status pulse cleanup.
4. Convert persistence and refresh debounces while preserving latest-state-wins
   ordering and dirty/inflight behavior.
5. Convert visual settle paths last, with the relevant widget and
   visual-geometry proof.
6. Update `docs/next/gtk-lush.md`, `.agents/rules/widget-wiring.md`, and any
   related rules after the final helper pattern is stable.

Rollback is ordinary source rollback. This change does not migrate persisted
data, publish a crate, or change public automation surfaces unless
implementation later chooses to expose new readiness diagnostics; if that
happens, automation docs and self-tests become mandatory in the same change.

## Open Questions

- Should chunked-yield sites remain entirely out of scope, or should the helper
  include a tiny private `yield_tick`/chunk primitive if the audit proves the
  pattern is repeated and tests cover it?
- Should the audit result live in `design.md`, `tasks.md`, or a short
  `docs/next/` implementation note at archive time?
- Which exact module path should own the helper so it is reachable from UI
  workflows without encouraging service/domain generation counters to depend on
  GTK scheduling?
