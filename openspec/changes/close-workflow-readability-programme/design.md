# Design — slot 7b, the programme's close

Slot 7 needed a design document for one genuine structural question. Slot 7b needs one
because that question's **answer** is now the change's shape: §D1 resolved that
`WFR-SHELL-LAYOUT` is not one workflow, and the work of *implementing* a hybrid outcome
is a set of ownership decisions that a task list would otherwise decide implicitly, one
file at a time, with no record of the criterion each one met.

Decisions are numbered `§E` so cross-references to slot 7's `§D1`–`§D6` stay
unambiguous. Slot 7's decisions are **inputs** here, not re-openable.

- **§E1** — the replacement-row set: which surfaces are rows, which are reassigned,
  which go to the tier list, and how a set of seven stays inside a declared maximum of
  five.
- **§E2** — role homes for six new rows, under the constraint that `ui/window/` is a
  crowded directory and four of its subdirectories are already migrated role homes.
- **§E3** — the §D6 re-key: which key, and why the narrow addition is not the
  forbidden broadening.
- **§E4** — the disposal lane's surface shape, its visibility, and why its reentrancy
  proof is unsound unless the lane is quiesced.
- **§E5** — `WFR-AUTOMATION-SPINE`'s terminal status and the three-way reconciliation.
- **§E6** — what "programme closeout" must contain to be a discharge rather than a
  claim.
- **§E7** — the disposition rule for inherited debt that reached no artifact.

## Non-goals

- **No behavior change**, except where a confirmed data-safety defect makes one
  mandatory under `.agents/rules/preexisting-blockers.md`.
- **No re-opening of §D1.** The verdict, the ≥12 stage orders, the 15-of-18 state
  groups, and the four contested-file findings are inputs. Re-deriving them would spend
  the change's capacity re-proving a conclusion the programme record deliberately
  stores outside the archived change directory.
- **No relocation of `model/plain_disposal.rs`, `model/action_catalog.rs`,
  `model/editor_memory.rs`, or `model/migration_ledger.rs`.** Each has a recorded
  census resolution that still holds, and the census forbids overriding an `exempt` or
  `cross-cutting` resolution.
- **No widening of `WFR-EDITOR-MEMORY`'s `exempt` resolution.** That resolution covers
  `model/editor_memory.rs` only. Stretching it over ~590 production lines of GTK
  eviction orchestration would be forcing an exempt row into a scope it was never
  granted — the inverse of the error the exempt label exists to prevent. §E1 gives the
  orchestration its own row instead.
- **No GTK Lush extraction.** `plain_disposal` encodes LushText payload admission
  policy, which fails the family's leaf-crate test.
- **No promotion of the two programme-level deferrals** (the ~98 actuation test seams;
  state-machine reification of inverted drains). Both have justification bars in the
  record that this change does not meet and does not try to. Their correct disposition
  is the closeout inventory, not the scope.
- **No new actuation seams.** Slot 5b's budgeted one remains unspent and this change
  plans to leave it unspent for the third consecutive slot.
- **No restructuring of `ui/automation.rs`.** §E5 probes it; probing is not migrating.

## E1 — The replacement-row set

### The question, stated so it cannot be answered by line count

§D1 answered *"is this one workflow?"* — no. It did **not** answer *"then what are the
rows?"*, and the second question has a failure mode of its own: a row per file. The
design that produced §D1 named it: *"a split that produces a row per file would trade
one unreadable row for eight thin ones and would inflate the matrix without making any
workflow more readable."*

So each replacement row must independently satisfy **criterion 1** — a single
user-initiated operation, or a family of operations sharing one ordered stage sequence
— and the verdict is recorded **per surface with its evidence**, not asserted from the
candidate table.

### The candidate table is a maximum, and this set exceeds it

The table listed five candidates. §D1 removed one (shell dialogs). §D1's findings then
created three attributions the table did not anticipate. Seven rows against a maximum of
five is a departure that needs its own argument, and the argument is **not** "the code
turned out bigger":

- Four rows are **inside** the maximum: geometry, tab strip, recent documents, Focus
  Mode.
- Three are **forced by §D1's own findings**, each of which is a coverage obligation
  rather than a preference:
  - `WFR-TRANSIENT-DISMISSAL` — finding 4 removed `transient_surfaces.rs` from the
    no-coordination-tier list, on the evidence that it has *"a strictly ordered
    dismissal ladder **and** a one-tick idle latch"*. A surface off the tier list and
    in no row is unattributed, which the coverage proof forbids.
  - `WFR-EDITOR-MEMORY-EVICTION` — finding 2 established that ~590 production lines of
    eviction orchestration are *"owned by no story anywhere"*. Not a new row, a
    **newly-visible** one.
  - `WFR-STARTUP-PREFLIGHT` — `startup_data.rs` is cross-cutting and owned by none. A
    terminal row with no facade is the only shape that attributes it without claiming
    it is a workflow.

The distinction that keeps this honest: **the maximum bounded the candidates that had
been enumerated**, and a maximum cannot bound surfaces a later finding discovers are
unattributed. What it *does* still bound is discretionary splitting, and it binds here:
tasks 0.3 and 0.4 must show that no two of the seven share one ordered stage sequence,
because *"where two candidates share one stage sequence they are one row, not two."*
**If the trace supports more than seven, re-read the trace rather than add a row.**

### The seven rows and their criterion-1 evidence obligations

| Row | Files | Criterion-1 claim task 0.3 must confirm |
| --- | --- | --- |
| `WFR-SHELL-GEOMETRY` | geometry halves of `imp.rs` and `actions.rs`, `window/policy.rs`, `sidebar/width_preset.rs`, `focus_indexing.rs`'s geometry story | one ordered sequence: action or breakpoint → property set → allocation clamp → settle-gated notify → persistence on explicit intent. §D1: seven entry points converging on it, smallest external entry surface. Owns the `workspace-sidebar-animation` readiness blocker |
| `WFR-TAB-STRIP` | `tabs.rs`, close/delete half of `documents.rs` | one stage order (close, including the confirmed-close terminal) plus three synchronous projections. Pin, bulk close, and reorder must be shown to share that sequence, not to be three sequences |
| `WFR-RECENT-DOCUMENTS` | `ui/open_popover/**`, `window/recent_open.rs` | **two** stage orders plus a lazy projection gate. Two stage orders in one row is permitted where they are one workflow's — the exemplar has two — but it must be argued, not assumed |
| `WFR-FOCUS-MODE` | `focus_mode.rs` | one stage order: reversible chrome suppression with fullscreen ownership and preview compatibility |
| `WFR-TRANSIENT-DISMISSAL` | `transient_surfaces.rs` | one strictly ordered dismissal ladder with one idle latch. Order is the criterion, not the presence of a timer |
| `WFR-EDITOR-MEMORY-EVICTION` | ~590 lines extracted from `focus_indexing.rs` | one stage order with its own generation counter and a bounded idle continuation |
| `WFR-STARTUP-PREFLIGHT` | `startup_data.rs` | **fails** criterion 1 by design: it orders five workflows and starts none. Terminal as `cross-cutting`, with the probe evidence delta 1 requires of a non-migrating resolution |

### The four reassignments, and the cell staling they cause

Reassignment is the half of a split that is easy to do silently and expensive to get
wrong, because each one lands files in a row whose measured cells were derived without
them. **Delta 1's cross-row staling statement is this change's own delta**, so the
change cannot use the "record that they are stale" escape it grants: it re-derives.

| Surface | Receiving row(s) | What must be decided, not assumed |
| --- | --- | --- |
| `dialogs.rs` (861) | `WFR-DOCUMENT-SAVE` and/or `WFR-DRAFT-RECOVERY` | §D1: five stage orders and three unrecorded freshness/identity values, with confirmed-close coordination consumed by two migrated rows. **Which row owns which stage order** is task 0.5's, and the answer may be "both, split by stage" — in which case the file is a coordination module of one and a called presentation surface of the other, not a shared role |
| `ui/window/search.rs` (955/928) | `WFR-SEARCH-REPLACE` | §D1: *more* than a called presentation surface — two of that workflow's coordination stages plus one coordination job of its own. So it takes **bounded coordination role names**, and the row's cell that reads "all under `ui/search_panel/**`" becomes false and must be corrected |
| `focus_indexing.rs`'s palette story | `WFR-COMMAND-PALETTE` | that row's cell currently says the file *"stays window code"*. Either the reassignment is right and the cell is wrong, or the cell is right and the story is the geometry row's. Task 0.5c decides on the code |
| `mod.rs`'s `setup_theme_selector` (~100) | undecided | §D1 found it in neither list. A control with no ordered stages belongs on the tier list; a control that participates in the geometry sequence belongs to that row. Task 0.5d decides |

### The no-coordination tier, and what may not be demoted to it

A surface joins the tier list only when it has **no ordered stages *and* no
coordination role *and* no seam value-object obligation** — the three properties the
list's own preamble claims of every entry — recorded per surface with its evidence.

Expected entries: `zoom.rs` (a control), `workspace_scope.rs` (a shared scalar),
`ui/open_popover/item.rs` (a row model object), and `ui/properties_panel/**` (moved
there by slot 7a on the evidence of 2 `pub fn`s, 0 test seams, and one production
caller). Possible: `mod.rs`'s theme selector, per task 0.5d.

**Two things may not be demoted**, and both are stated because demotion is the cheap
route to losing an obligation:

- **`transient_surfaces.rs`** — §D1 finding 4 removed it. Do not put it back.
- **`actions.rs`** — §D1: *"not demotable: it contains two of the geometry candidate's
  stages verbatim, and demoting it would be a route to demoting its pixel-proof
  obligations."*

## E2 — Role homes for six new rows

`ui/window/` already hosts four migrated per-workflow subdirectories (`drafts/`,
`local_history/`, `notes/`, `session_restore/`) plus slot 7a's `encoding/`,
`notifications/`, and `print/`. Its top-level `policy.rs` is **taken** — by slot 7a's
rename of `adaptive_shell.rs` — and `mod.rs` is the window module's own surface, not a
workflow facade: §D1 found a tenth story living in it.

**Decision: every new row in `ui/window/` takes a per-workflow subdirectory.** The
convention names the subdirectory as the safe default *"because it never collides"*,
and with `policy.rs` and `mod.rs` both spoken for at the directory's top level, flat
role names are not merely risky here — they are unavailable. Concretely:

| Row | Role home | Notes |
| --- | --- | --- |
| `WFR-SHELL-GEOMETRY` | `ui/window/geometry/` | `mod.rs` facade; `ui/window/policy.rs` **moves in** as `geometry/policy.rs`, staying inside the `ui/**/policy.rs` mutation glob — which task 2.4 must **verify after the move** rather than assume, per the nested-home rule |
| `WFR-TAB-STRIP` | `ui/window/tab_strip/` | not `tabs/`: the row's name and its role home should read the same, and `tabs.rs` becomes a role module inside it |
| `WFR-FOCUS-MODE` | `ui/window/focus_mode/` | one-file row today; the subdirectory exists so its `policy.rs` and `evidence.rs` cannot collide |
| `WFR-TRANSIENT-DISMISSAL` | `ui/window/transient_dismissal/` | same |
| `WFR-EDITOR-MEMORY-EVICTION` | `ui/window/editor_memory_eviction/` | the extraction target; `focus_indexing.rs`'s remaining two stories go to their owners |
| `WFR-RECENT-DOCUMENTS` | `ui/open_popover/` (canonical), with `window/recent_open.rs` as a **coordination role or called presentation surface** under it | the **nested** home the convention sanctions: one canonical role home holding the facade, the single `policy.rs`, and the single `evidence.rs`, with the module in the other directory taking a bounded role name or being recorded as a called surface |

A row-name/directory-name mismatch is worth one sentence because the convention has
already been bitten by names that describe mechanism rather than the workflow: a reader
who greps `WFR-TAB-STRIP` should land in `tab_strip/`.

## E3 — The §D6 re-key

§D6's finding is confirmed by count at authoring: `ui/window/actions.rs` and
`ui/window/imp.rs` are literal path keys in **three** predicates in **each** of
`scripts/check-visual-proof-policy.py` (`:163`/`:164`, `:190`/`:191`, `:209`/`:210`) and
`crates/cargo-gtk-proof/src/policy.rs` (`:823`/`:824`, `:852`/`:853`, `:879`/`:880`) —
six pairs — plus **six** further literal `imp.rs` keys inside those implementations'
own self-tests (`:594`, `:786`, `:808`; `:69`, `:228`, `:254`). They cover the
native-minimap highlight invariant, the native-minimap animation invariant, and the
workspace-sidebar animation matrix.

§E1's outcome **does** move the code those predicates protect: the geometry row's clamp
and breakpoint path lives in `imp.rs`, and two of its stages live verbatim in
`actions.rs`. So unlike slot 7a — whose one shell-side rename was deliberately
same-directory precisely to avoid this — a re-key is required here.

**The obvious re-key stays forbidden.** A
`crates/lushtext-core/src/ui/window/` prefix would demand two pixel invariants and the
sidebar animation matrix of **seven** subdirectories under that path, four of them
migrated role homes no predicate has ever protected. That is broadening a predicate to
files it never protected, which the amended convention classifies as a scope change
requiring its own justification rather than a rename side effect.

**Decision: add `crates/lushtext-core/src/ui/window/geometry/` as a role-home prefix
key, alongside the retained `actions.rs` and `imp.rs` literals.** Three properties make
this the narrow move rather than the broad one:

1. It selects **exactly** the geometry role home, which is where the protected clamp
   and breakpoint code lands. Slot 6's `NATIVE_MINIMAP_ROLE_HOME_PREFIX` is the
   precedent and the shape.
2. `actions.rs` and `imp.rs` keep their keys because both keep protected code —
   `actions.rs` is not demotable (§E1) and `imp.rs` retains its template-child and
   non-geometry halves. **Removing a key because some of a file's content moved is a
   scope change and must be argued on the behavior**, not on the rename.
3. It adds no path that was not already protected in substance.

The requirement is not satisfiable by review, and three steps are ordered:

- **Observe the disarm before fixing it.** With the geometry code moved and no key
  added, show the gate **passing while protecting nothing** — the property that makes
  reviewing the edit insufficient. Record the observation.
- **Re-key both implementations** and run each against the shipped tree, showing the
  moved files still selected and the required evidence still demanded.
- **Add a parity assertion to each implementation and prove each by a deliberate red.**
  One assertion on one side is the half that passes while the other side is wrong —
  slot 6's finding, and the reason its Python half's previously unreachable self-tests
  now run. Confirm the Python self-tests actually execute.

## E4 — The disposal lane's surface

`WFR-PLAIN-DISPOSAL` is a lane, not a workflow: §D2 settled that a lane owes the
**surface**, not the facade, and delta 2 states it. So no facade, no coordination role
names, no `policy.rs`, and the row stays `cross-cutting`.

**Shape.** One typed surface replacing four parallel typed observation values reached
through six accessors: `PlainDisposalLimits` × 2 lanes, `PlainDisposalSnapshot` × 2
lanes, and `DisposalPressureEvidence`. The ordinary and progress lanes are two lanes of
one mechanism, so the surface names them as components rather than exposing two
top-level accessors — the shape slot 7a used for `BufferSnapshotEvidence`'s named
components. The file may keep the lane's own name; what is fixed is that exactly one
surface exists.

**What must not be swept in.** `DisposalCapacityHold` and
`ProgressDisposalCapacityHold` are **actuation** holds — they make the lane full so a
test can observe rejection. Folding an actuator into an observation surface would make
reading the surface change the state it reports, which is the one thing the surface
rules forbid. `DisposalOwned<T>` and `DisposalPermit` are the lane's **seam values**,
in ten workflows' signatures, and stay exactly as they are.

**Visibility: measure, then conclude.** Slot 7a's task 6.8 instructs *"narrow
`DisposalPressureEvidence` from `pub`"*. Measured at authoring, the type is already
`test-utils`-gated and its only reader is a **different crate**
(`crates/lushtext/tests/widget/plain_disposal.rs:16`), for which `pub` is the narrowest
visibility that compiles. **Executing the inherited instruction literally would break
the widget lane and call it a narrowing.** The obligation is to record the reader set
and the visibility conclusion together; "already narrowest" is a legitimate outcome and
must be stated as a measurement, exactly as an unchanged cell is a legitimate
re-derivation outcome.

**The reentrancy proof is unsound unless the lane is quiesced, and this is the change's
main proof hazard.** The mandated proof drives the workflow through each operation that
takes a mutable borrow, reads the surface after each one, and asserts that repeated
reads of unchanged state are identical. This lane's state includes **process-wide
atomics and high-water marks mutated by worker threads**, so "unchanged state" is not a
property of the reader's control flow. Slot 7a hit exactly this: a no-retry widget lane
found *"an unsound assertion in this change's own evidence-surface proof, whose panic
message read exactly like a production defect"*, and a single retry would have left it
in the tree.

The reference pattern is slot 7a's buffer-snapshot fix: **quiesce the lane first** —
drain to a terminal, confirm zero running and zero queued through the surface itself,
and only then assert read-to-read identity — and assert **monotonicity** rather than
equality wherever a worker can still advance a counter. The non-materialization proof
takes the same care: read the surface with the lane empty and with it saturated, and
show admission counters, retained-byte accounting, high-water marks, and retry-source
counts identical before and after **each** read. A proof that passes only on a quiet
machine is the flake `.agents/rules/preexisting-blockers.md` classifies as a blocker.

## E5 — `WFR-AUTOMATION-SPINE`'s terminal status

The row is `pending`, and after this change there is no later slot, so `pending` cannot
stand. Permitted outcomes, decided by task 6.2 on evidence:

**(a) `cross-cutting`.** The evidence pointing here: no user-initiated operation — its
entry point is an external D-Bus caller; its observable state is *by construction* a
projection of other rows' evidence surfaces; it owns `model/action_catalog.rs`, which
the census confirmed as domain and staying; and its real contract is the drift gate plus
`docs/automation-reference.md`, not a facade. On the Status Labels definition it fits.

**(b) `migrated`.** Would require a facade over 2,084 production lines, a `policy.rs`,
and an evidence surface **over the surface that projects other surfaces** — close to
incoherent, and building it would widen an internal surface for no reader.

**(c) `exempt`.** Rejected in advance: `exempt` means "must not be forced into the
convention", and this row has been advanced incrementally by seven slots, so the label
would misdescribe its history.

Outcome (a) is expected and is **not** pre-approved. Under delta 1 a non-migrating
resolution records the **probe evidence**, which for this row means probing
`ui/automation.rs` for separable pure decisions before concluding it owns no
`policy.rs`. Slot 7a's own result is the reason this is not a formality: **four** rows
the census recorded as `policy: none` each turned out to own a `policy.rs`, with 5, 6,
and a whole dialog vocabulary of decisions found where the proposal expected none.
*"The domain module stays"* has never implied *"the workflow owns no policy"*.

**The reconciliation is three-way, not two-way** (Finding 4). All three sources — the
matrix `Slot` cell, the Migration Order table, and the programme ledger — must agree
after this change, and slot 7a's omission of `WFR-AUTOMATION-SPINE (partial)` from its
own `complete` line must be either corrected or stated as deliberate with its reason
(its baseline row's *"7, unchanged"*). Leaving a defensible omission undefended is how
the next reader re-opens it.

## E6 — What "programme closeout" must contain

A closeout that only flips statuses is a claim. Five components, each with the failure
mode it prevents:

1. **Terminal status on every row**, with probe evidence for every non-migrating
   resolution. *Prevents*: a future session reading `pending` and concluding planned
   work was abandoned.
2. **Measured outcomes against the section 2 baseline** in the same delta-table shape
   every prior slot used — workflows migrated, share of `ui/` + `model/`, policy
   relocations against the candidate denominator, seams retired by kind, seam value
   objects, automation projections, facades measured against the budget, data-safety
   defects fixed, convention amendments — plus the refreshed `Measurement Definitions`
   denominators, which are the programme's actual ratchet. *Prevents*: the record's
   baseline tables ending mid-programme with no summary.
3. **One inventory of every remaining deferral** with its gating condition and owner:
   the **nine** open `[~]` items (seven inherited plus slot 7a's two), stated together
   with the 23/16 reconciliation so a grep cannot conclude sixteen items were
   abandoned; the two programme-level deferrals with their justification bars;
   `scan_execution.rs`'s size follow-up; slot 6's conditionally-cleared
   `minimap_work_pending` condition; slot 5b's unresolved candidates; the two ratchet
   rows' residue; and whatever this change's own passes hand to a `docs/next/` record.
   *Prevents*: the failure slot 7a's Finding 6 documents — a handoff whose only home is
   an archived directory — and its recurrence in Finding 6 of this change.
4. **An explicit statement of what is *not* claimed.** Seven consecutive slots have
   shipped without the live `make run` walkthrough, and slot 5b's language is binding:
   the acceptance gap *"must be accepted by the user, not granted by this change"*. The
   closeout records the gap as awaiting the user's decision and does **not** write
   "accepted" into the matrix, the programme record, or the task file on the change's
   own authority.
5. **Every stale pointer in both documents corrected**, because a closeout is the last
   moment anyone reads them with the whole programme in view: the record's status-line
   count, its slot-6-terminated row list, *"Slots 5 through 7 remain authorable"*, the
   `| 5–7 | not yet authored |` change-name row, §7's parenthetical calling two
   migrated rows deferred, the matrix's eleven-row facade table and its anachronistic
   prose, the coverage proof's 198, and the ratchet table's two already-retired
   reach-throughs. *Prevents*: exactly what Finding 7 measured — three consecutive
   slots inheriting "a budget claim checked against a stale number is not a check".

## E7 — Disposition rule for debt that reached no artifact

Finding 6's six items share one property: they were found by a review pass and recorded
nowhere. That is a **worse** failure than a handoff to an archived directory, and it
means the items arrive as *claims*, two of which measurement has already falsified (171
hunks → 411; "~70 lines" → 8 sites binding 15 placeholders).

**Rule: re-verify before disposing, and re-derive every figure under a stated
predicate.** A finding whose only evidence is a label — the "S12" ledger-check holes —
is not inherited as three holes; it is re-derived from the gate's own documented failure
conditions, and the disposition follows what the re-derivation finds, including "no hole
exists".

**The rustfmt gate hole needs its decision made here rather than in the task.** It is
not a failing check: `cargo fmt --all --check` exits 0. It is a **coverage hole in a
gate** — the same class as delta 3's inclusion-side blind spot and slot 6's
"protection that vanishes while every command exits 0" — and the programme has fixed
that class three times rather than recorded it.

Two candidate fixes, and the choice belongs to task 8.1 on measured evidence:

- **Give rustfmt reach** and take the reformat. `cargo fmt` cannot follow
  `include!(concat!(env!("OUT_DIR"), ...))`, so the reach must come from the
  invocation, not from the source. The cost is 411 hunks across 18 files, which is
  mechanical but must be **isolated from every semantic change in this change** so
  review can separate them — the reformat and the debt fixes in tasks 8.2–8.4 touch the
  same files.
- **Record it** in a `docs/next/` record with its gating condition, if and only if the
  measured reach mechanism turns out to conflict with the harness's registry
  generation. "It is a large diff" is not that condition.

The ordering constraint is real and easy to miss: tasks 8.2, 8.3, and 8.4 edit
`tests/widget/window.rs`, `tests/widget/markdown_preview.rs`, and
`ui/window/encoding/dialogs.rs`. If the reformat lands **after** them, review sees one
diff mixing 411 mechanical hunks with three semantic fixes. The reformat therefore runs
**first** within section 8, or the semantic fixes run first and the reformat last, but
never interleaved — and whichever order is chosen is recorded.

## Rejected alternatives (change-level)

- **Create the replacement rows as `pending` and let a later slot migrate them.**
  Rejected: it is the cheapest reading of "implement the hybrid" and it is precisely
  what delta 1 forbids. A closing change that manufactures six new transitional
  statuses has not closed the programme; it has renamed the outstanding work.
- **Widen `WFR-EDITOR-MEMORY`'s `exempt` resolution over the eviction orchestration.**
  Rejected: the census forbids overriding an `exempt` resolution, and the resolution
  covers a pure `model/` module. Stretching it over GTK orchestration with its own
  generation counter and two race injectors would use an exemption granted for purity
  to excuse the least pure code in the row.
- **Split the census row along file boundaries and take one row per file.** Rejected by
  §D1's own constraint, and it would inflate the matrix while making no workflow more
  readable.
- **Amend the facade budget preemptively because this change writes six facades.**
  Rejected: six slots have proved step one of the escalation path sufficient, and at
  sixteen migrated rows the amendment's retroactive cost is at its programme maximum.
  The budget is amended only if an honest delegation genuinely cannot fit, measured
  rather than predicted.
- **Execute slot 7a's `DisposalPressureEvidence` narrowing as written.** Rejected on
  measurement (§E4): the type is already `test-utils`-gated and its only reader is
  another crate. The inherited instruction is a hypothesis, and this one is false.
- **Treat the two already-retired automation reach-throughs as still open because the
  ratchet table says so.** Rejected: the table's own instruction is to match on the
  reading expression rather than the line, and the expression is gone. A ratchet row
  left open after its subject is fixed is the same drift as a cell left stale.
- **Record the rustfmt gate hole as accepted debt because the reformat is large.**
  Rejected: `.agents/rules/preexisting-blockers.md` has no exceptions, and "the fix is
  a big mechanical diff" is not a gating condition. §E7 permits recording it only on a
  measured mechanism conflict.
- **Fold `services/notifications.rs`, `services/markdown_render.rs`, or
  `services/editor_io.rs` into a replacement row's size cell.** Rejected: each is
  shared, and pooling a shared service into a row's cell is the exact error slots 3a,
  3b, 4, 5a, and 7a each had to correct. Every such population is named with the rows
  that share it instead.
