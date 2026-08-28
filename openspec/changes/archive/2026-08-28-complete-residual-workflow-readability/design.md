# Design — slot 7, the residual sweep and programme closeout

Slot 6 was the one slot the programme record predicted would need a design
document, and the prediction was confirmed rather than obeyed. This slot needs one
for a different reason: it carries **one genuine structural question** whose answer
determines most of the change's shape, and three smaller decisions that a task list
would have to decide implicitly. Deciding them here, before any file moves, is what
keeps the task list executable.

- **§D1** — is `WFR-SHELL-LAYOUT` one workflow? (the structural question)
- **§D2** — how does a cross-cutting *lane* satisfy surface rules it owes but has
  no facade for?
- **§D3** — what terminal status does `WFR-AUTOMATION-SPINE` take?
- **§D4** — where does `WFR-STATUS-NOTIFICATIONS` put its canonical role home,
  given it spans three widget directories and a service?
- **§D5** — what does "programme closeout" have to contain to be a discharge
  rather than a claim?
- **§D6** — the path-keyed gate re-key, and why the obvious re-key is forbidden.

## Non-goals

- **No behavior change**, except where a confirmed data-safety defect makes one
  mandatory under `.agents/rules/preexisting-blockers.md`. Finding 6's
  teardown-before-close fix is the one already-committed instance.
- **No re-decomposition of `ui/markdown_preview/**`.** Two earlier changes paid for
  that split and the matrix's outlier resolution says explicitly that it "must not
  be redone". This slot adds the facade and one evidence surface.
  **Renaming `inline_footnotes.rs` to that row's `policy.rs` is not a
  re-decomposition and is therefore not excluded by this non-goal** — it is a role
  assignment: no responsibility moves between modules, no file is split, and the
  module's contents are unchanged. It is also the only resolution that satisfies
  the other two constraints in play (the hand-listed mutation entry must retire,
  and delta 3 forbids leaving an unclassified pure `ui/` module holding 214
  production lines of decision logic). See task 3.5.
- **No relocation of `model/plain_disposal.rs`, `model/encoding.rs`,
  `model/action_catalog.rs`, `model/editor_memory.rs`, or
  `model/migration_ledger.rs`.** Each has a recorded census resolution that still
  holds, and the census forbids overriding an `exempt` or `cross-cutting`
  resolution.
- **No GTK Lush extraction.** `plain_disposal` encodes LushText payload admission
  policy, which fails the family's leaf-crate test and is excluded by the
  programme's non-goals.
- **No promotion of the two programme-level deferrals** (the ~98 actuation test
  seams; state-machine reification of inverted drains). Both have justification
  bars in the record that this slot does not meet and does not try to.
- **No new actuation seams.** Slot 5b's budgeted one remains unspent and this
  change plans to leave it unspent.

## D1 — Is `WFR-SHELL-LAYOUT` one workflow?

### The question, stated so it cannot be answered by line count

The row's own stage trace calls it *"a residual grouping of 19 shell surfaces that
share the window adapter and have no coordination seam"* and licenses a split *"if
the facade work shows it holds more than one story."* Measured at authoring the row
is 19 files / 9,214 physical / 8,999 production lines with 40 `*_for_test`
functions across 71 gate sites.

The trap is that the line count makes a split look obviously correct, and a split
justified by line count is the response the facade-budget section **forbids**:
"splitting the census row to make two smaller facades" is named as unavailable. So
the decision must rest on stage-order evidence, and the evidence must be gathered
before any file moves.

### Criteria

The grouping is **one workflow** if and only if all of the following hold:

1. There is a single user-initiated operation, or a family of operations that share
   one ordered stage sequence, that a reader would name as *the* shell workflow.
2. The surfaces share coordination state — the same generation counters, the same
   admission budget, the same settle gate — rather than merely the same `imp`
   struct.
3. One facade can narrate the ordered stages, with each inversion's resumption
   point named, inside 370 physical lines **after** honest delegation.

Sharing `LushtextWindow`'s `imp` struct is explicitly **not** evidence of one
workflow: slot 4 established that one shared `imp` state group split three ways,
and slot 5a established that a file can sit in a row's size cell while being owned
by none of it.

### Evidence to gather before deciding (task 0.5)

- A derived stage trace per candidate surface: entry points, ordered stages, and
  every resumption point, **counted by actor rather than by timer type** (slot 6's
  rule: an out-of-band reveal by a *different* actor is a resumption even when no
  deferral primitive is involved).
- The shared-state map: which `imp` fields each candidate reads and writes, and
  which of those are genuinely shared versus co-located. Slot 4's rule applies —
  when a field's doc comment names a workflow, believe the comment until the code
  contradicts it.
- The external entry surface per candidate: `pub`/`pub(crate)` operations and the
  count of files outside the candidate that call them. This is slot 6's measured
  budget stressor and it is the number the facade projection rests on.
- Ownership verdicts for the **four** contested files: `dialogs.rs` (file dialogs
  plus close confirmation, whose contract is close-safety's), `focus_indexing.rs`
  (focus restoration plus palette indexing, and the palette row is migrated),
  `startup_data.rs` (which slot 5a already resolved as **cross-cutting, owned by
  none**, ordering five workflows), and **`ui/window/search.rs` (955 physical
  lines), which is attributed to no row at all** — the slot-3b recent-documents gap
  class, found in the change whose job is to prove full coverage. Its expected
  verdict is `WFR-SEARCH-REPLACE`'s window-side called presentation surface, on the
  evidence that slot 2b worked in this exact file and gave it
  `journal::begin_undo_restore` / `finish_undo_restore`; that is an expectation for
  task 0.5d-i to confirm or overturn, and either way the file must land in a row's
  file set and in the re-derived coverage proof.

### The bounded set of permitted outcomes

**(a) One workflow, one facade** at `ui/window/mod.rs` (257 physical today, 113
lines of headroom). Chosen only if criteria 1–3 all hold. The role home is flat
in `ui/window/`, which is available: the three per-workflow subdirectories there
(`drafts/`, `local_history/`, `session_restore/`, `notes/`) leave `policy.rs` and
`evidence.rs` free at the directory's top level, and `adaptive_shell.rs` becomes
that `policy.rs`.

**(b) Replacement rows, each naming one workflow.** The row is retired and replaced
by rows that each satisfy criterion 1, plus entries in the
matrix's `Surfaces With No Coordination Tier` list for surfaces with no ordered
stages. Each new
row gets a stable `WFR-*` id, a slot cell naming this change, its own measured
cells, and its own terminal status. The census **coverage proof** must be
re-derived in the same change — the matrix's proof currently reads "198 files
exist under `crates/lushtext-core/src`; 195 are attributed" and a row split changes
the attribution table, so an unre-derived proof would be a false completeness
claim at exactly the moment the programme claims completeness.

**(c) A hybrid**: one workflow row for the genuinely-shared adaptive-geometry
story, plus replacement rows for the surfaces that are separate stories, plus
no-coordination-tier entries for the rest. This is the outcome the authoring
evidence points at, and it is stated as a *candidate* rather than a conclusion
because task 0.5's trace has not been run.

The candidates that outcome (b) or (c) would name, listed so the task list is
executable and **explicitly not pre-approved**:

| Candidate | Files | Why it might be its own story |
| --- | --- | --- |
| Adaptive shell geometry | `imp.rs` (split-view half), `adaptive_shell.rs`, `sidebar/width_preset.rs`, `properties_panel/**` | one ordered stage order: action or breakpoint → property set → allocation clamp → settle-gated notify → persistence on explicit intent. Owns the `workspace-sidebar-animation` blocker |
| Tab strip | `tabs.rs`, the close/delete half of `documents.rs` | context menu, pin, bulk close, reorder; owns the two `ui/automation.rs` `tab_view` reads and Finding 6's defect |
| Focus Mode | `focus_mode.rs` | reversible chrome suppression with fullscreen ownership and preview compatibility |
| Recent-documents surface | `open_popover/**`, `recent_open.rs` | 24 `*_for_test` functions and `OpenPopoverRowLayoutSnapshot`; slot 3b already split it from the load row along the coordination/presentation line |
| Shell dialogs | `dialogs.rs` | if close confirmation is close-safety's rather than this row's, `dialogs.rs` may be a **called presentation surface** of the migrated save/draft/session rows rather than a workflow |
| No coordination tier | `zoom.rs`, `workspace_scope.rs`, `transient_surfaces.rs`, `actions.rs` | no ordered stages: a control, a shared scalar, a dismissal predicate, and action wiring |

### Two constraints on how far a split may go

**Bound the replacement-row count.** Outcomes (b) and (c) are licensed by the
grouping clause, not open-ended: a split that produces a row per file would trade
one unreadable row for eight thin ones and would inflate the matrix without making
any workflow more readable. The candidate table above is the **maximum** shape
under consideration, and each candidate must independently satisfy criterion 1 —
a single user-initiated operation or a family sharing one ordered stage sequence.
Where two candidates share one stage sequence they are one row, not two. If the
evidence supports more rows than the table lists, that is a signal to re-read the
stage trace rather than to add rows.

**The row-versus-tier-list boundary needs more than "no ordered stages".** A
surface goes to the no-coordination-tier list only when it has no ordered stages
*and* no coordination role *and* no seam value-object obligation — the three
properties that list's own preamble claims of every entry — and the verdict is
recorded per surface with the evidence, not asserted. `transient_surfaces.rs` is
the hard case: window-level Escape dismissal has a strict *order* (child
propagation first, then one topmost surface, then Focus Mode) even though it owns
no generation counter, so "ordered" must be tested against that contract rather
than against the absence of a timer. `actions.rs` is the other: action wiring looks
like plumbing, and it is also a literal key in three pixel predicates, so
demoting it to the tier list must not be a route to demoting its proof
obligations.

### Constraint that applies to every outcome

A split MUST NOT reduce protection. `ui/window/actions.rs` and `ui/window/imp.rs`
are literal keys in six predicate instances plus five self-test keys (§D6), and
`imp.rs` in particular is a **visual-sensitive** path whose changes require two
named pixel invariants and the workspace-sidebar animation matrix. A split that
moves geometry code out of `imp.rs` into a new module that no predicate names would
disarm those invariants while every gate exits 0.

## D2 — How a cross-cutting lane satisfies surface rules it owes

`WFR-BUFFER-SNAPSHOT` and `WFR-PLAIN-DISPOSAL` are lanes, not workflows. They have
no user-initiated operation, so they owe no facade and no stage narration. They do
have observable state, test-only inspection seams, and — in buffer snapshot's case
— three parallel typed observation types, which is the duplication the
evidence-surface requirement forbids.

**Decision: the lane owes the *surface*, not the facade.** Concretely:

- One `evidence.rs`-equivalent typed surface per lane, at the narrowest visibility
  its readers require, replacing every parallel typed observation type and every
  inspection seam. The file may keep the lane's own name (`buffer_snapshot.rs`,
  `plain_disposal.rs`) as its home if a separate module is not warranted; what is
  fixed is that there is exactly **one** surface, not that it lives in a file
  called `evidence.rs`, because `evidence.rs` is a *workflow role* name and these
  lanes carry no roles.
- All three mandated proofs apply unchanged — reentrancy, disposal, and
  non-materialization — because each follows from "one accessor reads the whole
  surface" plus interior mutability, none of which depends on being a workflow.
  Buffer snapshot's disposal proof is the interesting one: its whole subject is a
  live `GtkTextBuffer` reached through a widget that `dispose()` clears.
- The lanes stay `cross-cutting`. No relocation, no facade, no role names, and no
  `policy.rs` — the census resolutions hold.
- **The shared limits stay shared.** `char_count_requires_chunked_snapshot` is
  called by save and must not be forked; consolidating the surface must not move
  or duplicate it.

**Rejected alternative: leave both lanes alone.** It is the status quo and it is
what the current spec text technically permits, since every settled rule fires
"when its workflow migrates". But it leaves the programme's closing change asserting
completeness over two rows carrying nine and eight seam functions and three
parallel observation types, which is the drift the matrix exists to prevent. Hence
capability delta 2, which states the obligation rather than relying on a reader
inferring it.

**Rejected alternative: promote both to workflows so the rules fire.** This is the
"manufacture a role to obtain tooling reach" move the programme was created to
stop. A lane consumed by 10 workflows is not a workflow.

## D3 — `WFR-AUTOMATION-SPINE`'s terminal status

The row is `pending`, its `Slot` cell says "2a onward, incrementally per migrated
workflow", the Migration Order table omits it from slot 7, and the programme
record's ledger includes it. After this change there is no later slot, so `pending`
cannot stand.

Permitted outcomes, to be decided by task 8.8 on evidence:

**(a) `cross-cutting`.** The evidence pointing here: the row has no user-initiated
operation — its entry point is an external D-Bus caller; its observable state is
*by construction* a projection of other rows' evidence surfaces; it owns
`model/action_catalog.rs`, which the census confirmed as domain and staying; and
its real contract is the drift gate plus `docs/automation-reference.md`, not a
facade. On the Status Labels definition — "shared coordination or shared policy
[that] stays in a shared location" — it fits.

**(b) `migrated`.** Would require a facade over `ui/automation.rs` (2,208 physical
/ 2,077 production), a `policy.rs`, and an evidence surface *over the surface that
projects other surfaces*. The last of those is close to incoherent, and building it
would widen an internal surface for no reader.

**(c) `exempt`.** Rejected in advance: `exempt` means "must not be forced into the
convention", and the row has in fact been advanced incrementally by six slots, so
that label would misdescribe its history.

Outcome (a) is the expected answer and it is **not** pre-approved: task 8.8 records
the probe. Under capability delta 1 a non-migrating resolution must record probe
evidence, which for this row means probing `ui/automation.rs` for separable pure
decisions before concluding it owns no `policy.rs` — the same probe slot 4 was
forced into and which found five decisions where the proposal expected none. Slot
3b's mirror lesson applies: "the domain module stays" does not imply "the workflow
owns no policy".

Whatever outcome is chosen, the ledger and the Migration Order table must be made
to **agree** in the same change; today they do not, and neither is marked as
authoritative over the other.

## D4 — `WFR-STATUS-NOTIFICATIONS`'s role home

The row spans `ui/status_bar/**` (334), `ui/info_bar/**` (370),
`ui/window/notifications.rs` (183/153), and `services/notifications.rs`
(1,132/431). It has **zero** `*_for_test` functions and one gate site, so it is the
cleanest row in the slot; the only real question is where the facade lives.

**Decision: the canonical role home is the window side.** `window/notifications.rs`
already declares itself the bridge — *"bridges `NotificationBus` state to the GTK
status bar and per-editor info bars, keeping callers from touching notification
widgets directly"* — which is the coordination position. `ui/status_bar/**` and
`ui/info_bar/**` are **called presentation surfaces**: they project one
notification onto a lane or an inline alert and own no ordered stages.
`services/notifications.rs` stays a GTK-free service and is not a role.

This is the nested/two-directory resolution the convention already sanctions
(one canonical role home plus recorded called surfaces), and the honesty test slot
4 named applies: the called surfaces must import their identity types from the
canonical home rather than defining private copies. If they cannot, the split is
not clean and §D4 must be revisited rather than papered over.

Whether the home is flat (`ui/window/notifications.rs` grows a `policy.rs`
sibling — unavailable, since `ui/window/policy.rs` may be claimed by §D1's
outcome (a)) or a per-workflow subdirectory (`ui/window/notifications/`) is
therefore **dependent on §D1** and is resolved in task 0.7 after §D1 lands. The
subdirectory is the safe default because it never collides.

## D5 — What "programme closeout" must contain

A closeout that only flips statuses is a claim, not a discharge. Four components,
each with a failure mode it prevents:

1. **Terminal status on every row**, with the probe evidence for every
   non-migrating resolution. Prevents: a future session reading `pending` and
   concluding planned work was abandoned.
2. **Measured outcomes against the section 2 baseline** — workflows migrated,
   share of `ui/` + `model/`, policy relocations against the candidate
   denominator, seams retired by kind, seam value objects, automation projections,
   facades measured against the budget, data-safety defects fixed, convention
   amendments — in the same table shape every prior slot used, so the programme's
   arc is readable in one place. Prevents: the record's baseline tables ending
   mid-programme with no summary.
3. **One inventory of every remaining deferral** with its gating condition and its
   owner: the seven open `[~]` items across four archived changes, the two
   programme-level deferrals with their justification bars, `scan_execution.rs`'s
   size follow-up, slot 6's unresolved `minimap_work_pending` candidate, slot 5b's
   two unresolved candidates and two sub-critical notes, and any finding this
   slot's own pass hands to a `docs/next/` record. Prevents: the failure Finding 6
   documents — a handoff whose only home is an archived directory.
4. **An explicit statement of what is *not* claimed.** Four consecutive slots have
   shipped without the live `make run` walkthrough, and slot 5b's language is
   binding: the acceptance gap *"must be accepted by the user, not granted by this
   change"*. The closeout records the gap as awaiting the user's decision and does
   **not** write "accepted" on the change's own authority.

The matrix's stale four-row facade table is replaced by the full measured table in
the same task, because a budget claim checked against a stale number is not a check
(slot 5a).

## D6 — The path-keyed gate re-key, and why the obvious re-key is forbidden

`ui/window/actions.rs` and `ui/window/imp.rs` are literal path keys in **three**
predicates in **each** of two hand-mirrored implementations —
`scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`
— covering the native-minimap highlight invariant, the native-minimap animation
invariant, and the workspace-sidebar animation matrix. `imp.rs` appears in **six** further literal keys inside those implementations' own
self-tests — `scripts/check-visual-proof-policy.py:594`, `:786`, `:808` and
`crates/cargo-gtk-proof/src/policy.rs:69`, `:228`, `:254` — against the live
predicates at `check-visual-proof-policy.py:164`, `:191`, `:210` and
`policy.rs:824`, `:853`, `:880`.

Slot 6 solved its own half by replacing a literal `minimap.rs` path with
`NATIVE_MINIMAP_ROLE_HOME_PREFIX`, a directory prefix that survives role splits.
**The analogous move here is forbidden.** A `crates/lushtext-core/src/ui/window/`
prefix would make *every* window file — including the four migrated per-workflow
role homes underneath it — require two pixel invariants and the sidebar animation
matrix. That is broadening a predicate to files it never protected, which the
amended convention names as a scope change requiring its own justification rather
than a rename side effect.

**Decision: re-key only if §D1's outcome moves the code the predicate protects,
and re-key to the narrowest key that still selects exactly that code.** Three
sub-cases:

- If `actions.rs` and `imp.rs` keep their paths, **no re-key is required** and the
  change records that verdict with the run that proves it. A gate correctly left
  alone is a legitimate outcome and must be stated, not silently skipped.
- If geometry code moves out of `imp.rs` into a new module, the new module's path
  is **added** as a key (or a prefix scoped to the geometry role home if one is
  created), and the run must show the moved files still selected.
- If a file is split such that part of it no longer participates in the protected
  behavior, narrowing is a **scope change** and must be argued on the behavior,
  not the rename.

In every sub-case the requirement is the same and is not satisfiable by review:
run each gate against the tree being shipped, in both implementations, and show
that the protected files are still selected and the required evidence still
demanded. Both implementations get a parity assertion proved by a **deliberate
red**, because one assertion on one side is the half that passes while the other
side is wrong — slot 6's finding, and the reason its Python half's previously
unreachable self-tests now run.

## Rejected alternatives (change-level)

- **Split slot 7 up front into 7a and 7b.** Rejected as the default: the programme
  record's slot line, the Migration Order table, and the ledger all describe one
  change, and four of the eight rows are small enough that splitting would pay
  coordination cost for no risk reduction. The split path is *declared* in the
  proposal with its boundary and trigger, following the 5a/5b precedent where the
  split was forced by evidence the proposal had argued against needing.
- **Migrate the shell row first because it is the biggest.** Rejected for the same
  reason the census refused to promote the minimap: risk order is the convention,
  and the four tier-1 rows prove the no-inversion facade shape before the row with
  the heaviest behavior-contract set consumes the change's remaining capacity.
- **Mark the two cross-cutting lanes' obligations void because their status is
  already terminal.** Rejected: it is the cheapest reading and it makes the
  matrix's terminal label mean two different things (resolved, versus resolved and
  discharged). Delta 2 separates them.
- **Amend the facade budget preemptively because this slot writes the most
  facades.** Rejected: five slots have proved step one of the escalation path
  sufficient, and at eleven migrated rows the amendment's retroactive cost is at
  its maximum. The budget is amended only if an honest delegation genuinely cannot
  fit, measured rather than predicted.
- **Fold `services/notifications.rs`, `services/markdown_render.rs`, or
  `services/editor_io.rs` into their rows' size cells.** Rejected: each is shared,
  and pooling a shared service into a row's cell is the exact error slot 3a, 3b, 4,
  and 5a each had to correct. Every such population is named with the rows that
  share it instead.
