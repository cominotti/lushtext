> **STATE (read before planning further work).**
>
> This is **slot 7b, the programme's closing change**. Slot 7a
> (`complete-residual-workflow-readability`) migrated five rows, discharged
> `WFR-BUFFER-SNAPSHOT`, landed capability delta 3, fixed the teardown-before-close
> data-safety defect, and resolved §D1. **Three rows remain non-terminal**:
> `WFR-SHELL-LAYOUT` (`pending`, known **not** to be one workflow),
> `WFR-AUTOMATION-SPINE` (`pending`), and `WFR-PLAIN-DISPOSAL` (`cross-cutting` with
> obligations undischarged).
>
> **Baseline: slot 7a is committed and archived** (`a3b1743a`, archived `4c39c58a`).
> Its migrations, its delta 3 sync into `openspec/specs/mutation-testing/spec.md`, and
> its matrix and record edits are in the tree, not in a worktree — so this change's
> diff-aware gates and `mutants-diff` have a real base ref, which slot 5b's warning says
> they otherwise do not.
>
> **Capability deltas 1 and 2 live in this change**, relocated from slot 7a's
> directory at authoring under its own task 0.14a: a delta must not ship in a change
> that cannot discharge its obligation. Delta 1 needs every row terminal; delta 2
> needs the lane's surface. Both were re-based against **post-sync** live text and
> needed no edit — each is a strict superset with zero modified and zero removed lines
> (**+108 / +9 scenarios** and **+46 / +3**, counting each requirement block including
> its terminating blank separator; **+107** and **+45** excluding it).
>
> **Authoring inputs consumed from slot 7a, not re-derived** (its B.0): §D1's
> resolution with its ≥12 stage orders and 15-of-18 co-located state groups; the four
> contested-file verdicts; §D6's constraint re-proved intact; the corrected cells
> including `WFR-SHELL-LAYOUT` as **tier-3** and the coverage proof stale by 68 files;
> and the `[~]` reconciliation — **23** markers, **16** of them slot 5a's and closed by
> 5b, **seven** genuinely open, **plus 7a's own two** makes **nine**. That last input
> is the one that did **not** survive being consumed: it was re-grepped rather than
> carried, and it was wrong in every position (**35 / 18 / 7 / 8 / 2**, B.3).
>
> **Six inherited claims were falsified at authoring**, which is why this change
> re-derives rather than reports:
>
> 1. the rustfmt reach gap is **411 hunks across 18 files**, not 171;
> 2. slot 7a's `DisposalPressureEvidence` narrowing instruction is **unexecutable as
>    written** — the type is already `test-utils`-gated and its only reader is another
>    crate, for which `pub` is the narrowest visibility that compiles;
> 3. the disposal lane has **three** parallel observation values through **five**
>    accessors, not four through six — the other three seams are actuation holds;
> 4. `focus_indexing.rs` holds **four** stories, not three: eviction is **~406**
>    production lines (not ~590), the "palette story" is **two** stage orders, and the
>    "geometry story" contains **no geometry code** and is unowned focus restoration;
> 5. the automation ratchet row's recorded *occurrence* is retired but its **reading
>    expression persists at eight sites**, so it must not be struck retired;
> 6. `encoding/dialogs.rs`'s named "near-duplicate pair" is a **one-expression
>    default-argument wrapper** — the target does not exist.
>
> Two further inherited items are **stale halves of slot 7a's own self-contradictions**:
> its 160-untriaged-mutants claim (B.6) against its recorded run (A.12), and its
> accessibility fail-open status (A.6 against A.13a).
>
> **The recurring lesson, now seven slots deep:** a handed-on number is a hypothesis,
> a census cell can be wrong in its *kind* and not only its magnitude, and a step that
> quietly succeeds against the wrong input is the defect class this programme keeps
> rediscovering. This change's own verification cannot be read off exit codes.
>
> **What this change may not do:** re-open §D1; split a row on line-count evidence;
> create a `pending` row; write "accepted" against a user-gated gate; or claim the
> programme complete while any row, delta, or inventory item is outstanding.

> ---
>
> **IMPLEMENTATION STATE (updated during implementation — read with the above).**
>
> **All seven replacement rows are migrated, every matrix row is terminal, both
> capability deltas are landed, and `make check-workflow-boundaries` passes.**
> No split was taken; the declared boundary was passed rather than used.
>
> | Row | Facade / 370 | `policy.rs` mutants |
> | --- | --- | --- |
> | `WFR-TRANSIENT-DISMISSAL` | 199 | 10: 8 caught, 2 unviable, **0 survivors** |
> | `WFR-FOCUS-MODE` | 338 | 17: 16 caught, 1 unviable, **0 survivors** |
> | `WFR-EDITOR-MEMORY-EVICTION` | 333 | 22 generated |
> | `WFR-RECENT-DOCUMENTS` | 252 | 6: 5 caught, 1 unviable, **0 survivors** |
> | `WFR-SHELL-GEOMETRY` | 240 | parity **81 -> 81**; 77 caught, 4 unviable, **0 survivors** |
> | `WFR-TAB-STRIP` | 217 | 46: **46 caught, 0 survivors** |
> | `WFR-PLAIN-DISPOSAL` (lane surface, no facade) | n/a | n/a |
> | `WFR-STARTUP-PREFLIGHT` (cross-cutting, no facade) | n/a | n/a |
>
> **Facade figures are the post-review re-measurement.** Three were stale when
> first recorded — geometry 164, tab strip 223, recent documents 239 — because
> they were taken before the module docs were finished, which is the slot-5b
> failure mode repeating inside the closing change. The table is re-derived after
> the last edit.
>
> **Resumed at `69f78b09` after a session loss. State was re-established from the
> tree**, and that re-establishment corrected the previous banner in five places,
> which is the reason the step exists:
>
> 1. both compile configurations were green at resume;
> 2. `WFR-TRANSIENT-DISMISSAL`'s facade measures **199**, not 205;
> 3. **appendix B.0 and B.7 were stale and contradicted the banner** — written as
>    a stopping record before the session continued through three more rows. Both
>    are rewritten; the lesson is recorded in B.5;
> 4. `ui/open_popover/{policy,evidence}.rs` existed but were **not wired**, so the
>    green `cargo check` was not evidence about them;
> 5. appendix A.10 was empty although 7.4 was ticked, and A.11–A.14 recorded only
>    row 1 although rows 2–4 had landed.
>
> **Landed in this session**, beyond the three remaining row migrations: the §E3
> visual-proof re-key with its **observed disarm** and a deliberate red in both
> implementations; a terminal `superseded` status label with its own self-test
> arm and deliberate red; delta 1's mechanical half (a transitional status must
> not survive programme close) with its arm and red; both capability deltas
> landed; the four reassignments (0.5a–0.5d) with two **migrated** rows' cells
> found stale and corrected; the coverage proof re-derived under a stated
> predicate with a one-file correction; the no-coordination-tier list populated
> per surface; the rustfmt reach hole closed with a `make fmt` target; the
> startup activation-open queue bounded; **five** findings landed in
> `docs/next/persistent-format-hardening.md` (S7B-1 through S7B-5, the last two
> from the review pass); and **three pre-existing blockers
> fixed** — a default-feature dead-code gate mismatch in
> `services/content_search/replace.rs`, and a real **panic** in the geometry
> evidence surface that its own disposal proof caught before it shipped.
>
> **Everything in sections 0–11 is closed except task 11.10**, which is
> explicitly an **archive-time** step (rewriting this change's evidence pointers
> from live to archive form) and must stay open until the change is archived —
> live form is the only form that passes the gate while the change is live.
> **157 of the file's 162 checkbox items are ticked.** The other five are
> accounted for individually: **1 open** (11.10, archive-time), **2 `[-]`**
> deliberately-not-taken alternatives (0.4b-ii, 8.4c), and **2 `[~]`**
> (10.18 live-display, 10.19 manual Orca) that remain **user-gated** and are not
> written as accepted.
>
> **Resumed a second time after a session loss**, mid-way through the review-fix
> cycle's own closing chain. State was again re-established **from the tree**, and
> again the re-establishment was the step that found something: the review's four
> blockers and eighteen should-fixes had all landed, but **six measured cells had
> gone stale behind them**, because every fix that edits a file invalidates the
> figure someone measured from it. Corrected: `WFR-PLAIN-DISPOSAL`'s size cell
> (S13 moved `is_quiesced()` into the domain after the `model` half was recorded
> *unchanged*, and S12 collapsed two dead gate attributes),
> `WFR-RECENT-DOCUMENTS`'s and `WFR-STARTUP-PREFLIGHT`'s size cells, the
> programme record's facade row (still carrying the pre-review 239/223/164), its
> seam-retirement figure (9, where the matrix says 11), and both headline ratchet
> counts. The rule this change wrote for its own facades — *a figure taken
> mid-change is a figure with a shelf life* — applies to every measured cell, not
> only to facades.
>
> **Final gate matrix** is in A.13, re-run in full at the end of the review
> follow-up. Everything green, including the three scoped mutation runs, the
> no-retry widget lane (**1,203 tests, zero `FLAKY:` lines, zero `CRITICAL`
> lines**) *after* the SIGSEGV fix, all three clean-root smoke lanes re-taken,
> `make check-policy`, both rustdoc configurations, and
> `openspec validate --all --strict` at 111/0.
>
> **Four pre-existing blockers were found and fixed in-stream**, none of them
> introduced by this change:
>
> 1. a default-feature dead-code **gate mismatch** in
>    `services/content_search/replace.rs` — two declarations gated
>    `cfg(any(test, test-utils))` while every consumer is gated `test-utils`
>    alone, invisible to `clippy --all-features`;
> 2. a real **panic** in the geometry evidence surface, caught by its own
>    disposal proof before it shipped — the surface was built from three
>    production accessors that each deref a `TemplateChild`;
> 3. a smoke-lane **staleness guard that could never pass**:
>    `make performance-smoke` asserted four widget-harness logs with a pattern
>    (`test result: ok\. [1-9]`) the custom harness never emits, so the lane
>    failed on a healthy tree while the condition it guarded went unchecked.
>    Fixed with one shared `smoke_assert_widget_ran` predicate and proved three
>    ways;
> 4. a **SIGSEGV**, found by the no-retry widget lane and hiding behind a
>    `FLAKY:` line. Destroying a focused `GtkEntry` under headless Mutter races
>    GTK's Wayland input-method backend into `wl_proxy_get_version` on a stale
>    pointer — **9 failures in 40** isolated runs of the four inline-rename
>    tests. Production code is unchanged and that is the finding: three
>    application-side orderings were implemented and measured, and each was
>    neutral or **worse**. Mitigated where the cause is, by adding
>    `GTK_IM_MODULE=gtk-im-context-simple` to the harness's pre-GTK environment
>    beside `NO_AT_BRIDGE` and `GSK_RENDERER` — **0 failures in 40** after — and
>    recorded as `S7B-6` because the GTK race itself is not fixed.

> ---

---

## 0. Gates, orientation, decisions, and premise re-verification

Every decision task precedes every structural task, because §E1's ownership verdicts
determine which files move and therefore which gates need re-keying. Do not start
section 4 before section 0 closes.

- [x] 0.1 `git add -N` every new file **as soon as it exists**, before running any
  diff-aware gate. `make check-visual-proof-policy`, the diff-aware half of
  `make check-accessibility-policy`, and `make mutants-diff` build their changed-file
  set from `git diff <base>`, which does not list untracked paths at all. This change
  creates **six new role-home directories**; a green diff-aware gate computed over a
  file set that omits all of them is not evidence. Re-run the lane after the files are
  visible; if adding them changes a digest and the gate starts failing, re-run the
  lane rather than unstaging.
- [x] 0.2 **Confirm the relocated deltas still match the live specs** before planning
  around them. `diff` each delta's requirement body against the live requirement of
  the same title and confirm the delta remains a strict superset. Slot 7a's delta 3
  was synced into `openspec/specs/mutation-testing/spec.md`; confirm nothing that
  sync touched is quoted by delta 1 or 2. Record the two diffs' shapes in A.1.
- [x] 0.3 **DECISION (§E1): confirm each replacement row satisfies criterion 1**, from
  the stage trace §D1 already derived — one user-initiated operation, or a family
  sharing one ordered stage sequence — recorded **per surface with its evidence**.
  - [x] 0.3a Show that **no two of the seven share one ordered stage sequence**.
    *"Where two candidates share one stage sequence they are one row, not two."* The
    pairs to test explicitly: geometry versus transient dismissal (both react to
    window-level state), tab strip versus recent documents (both open documents), and
    Focus Mode versus geometry (both suppress chrome).
  - [x] 0.3a-ii **Apply the reciprocal thin-row test**, which the maximum does not
    catch: does the row end up **larger after migration than before**? A 202-line
    `transient_surfaces.rs` that gains a facade, a `policy.rs`, an `evidence.rs`, and a
    `test_policy.rs` can grow, and a row whose migration adds net lines without making
    a workflow more readable is the thin-row failure from the other direction. Measure
    before/after per row and record it; a growth is not automatically wrong, but it is
    automatically something to justify.
  - [x] 0.3b Show that `WFR-TAB-STRIP`'s pin, bulk-close, and reorder paths **share**
    the close stage order rather than being three stage orders. If they do not, the row
    is wrong and the trace must be re-read before a row is added.
  - [x] 0.3c Argue `WFR-RECENT-DOCUMENTS`'s **two** stage orders as one workflow's. Two
    stage orders in one row is permitted — the exemplar has two — but it is the
    measured budget stressor, so the argument and the facade projection are the same
    decision.
  - [x] 0.3d Record `WFR-STARTUP-PREFLIGHT` as **failing** criterion 1 by design, with
    the five workflows it orders named, and take `cross-cutting` with the probe
    evidence delta 1 requires of a non-migrating resolution.
- [x] 0.4 **DECISION (§E1): the row count against the declared maximum.** The candidate
  table listed five; §D1 removed one; §D1's findings created three. State the four
  inside the maximum and the three forced by findings 2 and 4 and by the coverage
  proof, each with the finding that forced it. **Do not present the departure as a
  consequence of file sizes** — if any row's only support is its line count, it is the
  forbidden budget response and must be withdrawn. State the arithmetic directly: the
  candidate table has six rows, five of them candidate stories, one resolved as not a
  row, leaving **four**; this change writes **six** new facades.
- [x] 0.4a **DECISION (§E1): re-derive `focus_indexing.rs`'s stories from the code
  before reassigning any of them.** This task precedes 0.5c and must not be folded into
  it. The inherited *"three stories"* verdict is wrong in three independent ways
  (proposal Finding 1b), so the file's decomposition is itself a measurement this change
  re-derives:
  - [x] 0.4a-i **Editor-memory eviction** — confirm **~406** production lines across
    two discontiguous ranges (`:56`–`:190`, `:380`–`:667`), state the predicate, and
    record that the inherited ~590 absorbed the focus-restoration block between them.
    **Re-project the eviction facade from ~406**, not ~590 (task 0.6).
  - [x] 0.4a-ii **Palette overlay control (~104)** and **palette file-index build
    (~171)** — confirm these are **two** ordered stage orders, not one, and that the
    index build owns its own coordinator and generation counter. Both land in
    `WFR-COMMAND-PALETTE` at 0.5c, and the index build is **coordination**, not
    presentation.
  - [x] 0.4a-iii **Focus restoration (~129, `:255`–`:380`)** — confirm it contains **no
    geometry code**, and that two of its four functions are geometry-*triggered*
    (a pane close and a breakpoint collapse call them), which is a **caller
    relationship, not ownership**. Then **give it an explicit owner**: its own
    replacement row, a coordination role of `WFR-SHELL-GEOMETRY` argued on behavior
    rather than on the caller relationship, or a called presentation surface of whichever
    row owns editor focus. **Leaving it implicit is the failure §D1 was run to prevent**,
    and assigning it to the geometry row on the caller relationship would leave a story
    unowned while every gate exits 0.
- [x] 0.4b **DECISION: how the retired `WFR-SHELL-LAYOUT` row is lawfully disposed**
  (proposal Finding 1c). Neither available mechanism is legal today: deleting the row
  fires `record_findings`, because `docs/next/workflow-readability.md:1231` names it in
  the `slot 7b (outstanding)` ledger line; and no member of `KNOWN_STATUS_LABELS` fits a
  row that was **replaced** rather than migrated, exempted, or shared. Choose:
  - [x] 0.4b-i add a terminal **retirement/superseded** label to the status vocabulary —
    a **delta 1 text edit**, paid inside this change with its retroactive re-check across
    every row — and teach the gate to accept it as terminal; **or**
  - [-] 0.4b-ii **NOT TAKEN** (the alternative to 0.4b-i, which was): retain the row with a documented terminal status plus a `superseded by`
    pointer to the seven replacement rows, arguing which existing label that is and why
    it does not misdescribe the row's history.
  Either way the ledger line is edited in the same change: a ledger naming a deleted row
  and a matrix hiding a retired one are the same defect from two sides.
- [x] 0.5 **DECISION (§E1): the four reassignments**, each on the code rather than on
  the handoff.
  - [x] 0.5a `dialogs.rs` — which of `WFR-DOCUMENT-SAVE` and `WFR-DRAFT-RECOVERY` owns
    which of its five stage orders, and whether the file is a coordination module of
    one and a called presentation surface of the other. Name its three unrecorded
    freshness/identity values and decide whether each becomes a seam value object of
    the receiving row.
  - [x] 0.5b `ui/window/search.rs` (955/928) — confirm it holds two of
    `WFR-SEARCH-REPLACE`'s coordination stages plus one coordination job of its own,
    assign **bounded coordination role names** accordingly, and correct that row's
    cell, which reads "all under `ui/search_panel/**`" and is now false by 928
    production lines.
  - [x] 0.5c `focus_indexing.rs`'s **two** palette stage orders (0.4a-ii) —
    `WFR-COMMAND-PALETTE`'s cell says the file *"stays window code"*. Decide which of the
    cell and the reassignment is wrong, on the code, and carry 0.4a-iii's
    focus-restoration owner into the matrix at the same time.
  - [x] 0.5d `mod.rs`'s `setup_theme_selector` (~100 lines), the tenth story §D1 found
    in neither list — tier list, or a stage of the geometry sequence. Decide and record.
  - [x] 0.5e **Re-derive every receiving row's staled measured cells** in this change.
    Delta 1's cross-row staling statement is this change's own delta, so the "record
    that they are stale" escape it grants is not available here.
- [x] 0.6 **Project each new facade before writing it**, against the projections in the
  proposal, and record the projection beside the measurement in A.11 so a falsified
  projection is visible as such. **The tightest repo margin is 1 line**
  (`ui/search_panel/mod.rs` at 369). Plan against 1, not against slot 7a's 105–270
  landings. `WFR-RECENT-DOCUMENTS` is the declared escalation candidate; if it exceeds
  370 after honest delegation, take escalation step 1 and record the attempt before
  considering step 2.
- [x] 0.7 **DECISION (§E2): role homes.** Confirm that `ui/window/policy.rs` and
  `ui/window/mod.rs` are both unavailable as flat role names, and take a per-workflow
  subdirectory for every new row in `ui/window/`. Confirm `ui/open_popover/` as
  `WFR-RECENT-DOCUMENTS`'s canonical role home with `window/recent_open.rs` in the
  nested position, and decide whether that module takes a bounded coordination role
  name or is recorded as a called presentation surface.
- [x] 0.8 **DECISION (§E4): the disposal surface's shape and visibility**, before any
  edit. Enumerate the **three** typed observation values and **five** accessors —
  `PlainDisposalLimits` (`:1013`, `:1020`), `PlainDisposalSnapshot` (`:1027`, `:1034`),
  `DisposalPressureEvidence` (`:1145`) — and **state the predicate**, because
  "observation value" is exactly the kind of count two readers get differently: a type
  returned by a gated accessor to be read, with the ordinary and progress lanes as two
  *instances* of one type rather than two types. State that the remaining **three**
  declarations returning `DisposalCapacityHold` / `ProgressDisposalCapacityHold` are
  actuation and stay out — 5 observation + 3 actuation accounts for all 8;
  state that `DisposalOwned<T>` and `DisposalPermit` are seam values in ten workflows'
  signatures and stay unchanged; and **measure the reader set of every type before
  choosing a visibility**. "Already narrowest" is a legitimate recorded outcome.
- [x] 0.9 **Confirm the open `[~]` items are not inherited as work.** Verify by
  path that slot 4's two, slot 5a's, slot 5b's 7.6 and 10.13, slot 6's 10.19 and 10.20,
  and slot 7a's 10.22 and 10.23 stay gated on something this change cannot supply.
  **Executed, and the "user-gated" half of that premise did not survive it**: of the
  ten markers genuinely open once this change's own two are included, **eight** are
  user-gated (live-display in slots 4, 5b, 6, 7a, 7b; manual Orca in 6, 7a, 7b) and
  **two are machine-gated, not user-gated** — slot 5b's 7.6 needs a baseline worktree
  and slot 4's 10.7 needs a quiet host. Both are reclassified in B.3 and in the
  record's inventory. This change's contribution is to **inventory** them in one
  place (task 11.4), not to discharge them. State the full reconciliation together —
  a reader who greps the markers and finds **18** of them already closed by slot 5b
  will otherwise conclude eighteen items were abandoned.
- [x] 0.10 **Re-verify Finding 6's six items against the code**, not against the brief
  that named them. Two inherited figures are already falsified (411 hunks not 171; 8
  destructuring sites binding 15 placeholders, not "~70 lines"). For the "S12"
  ledger-check holes, whose only evidence is a label that appears nowhere in the
  repository, re-derive from `check-workflow-boundaries.py`'s four documented ledger
  failure conditions and record what the re-derivation finds, including "no hole
  exists".
- [x] 0.11 **Re-verify slot 7a's own inheritances** rather than trusting them: that the
  two `ui/automation.rs` reach-throughs are gone (they are — `current_readiness_failure`
  now iterates `window.open_editors()`), that `ui/window/policy.rs` is present at
  813 physical, that §D6's six predicate pairs and six self-test keys are intact, and
  that `MinimapEvidence` is still absent from `EVIDENCE_PROJECTIONS`.
- [x] 0.12 **Resolve slot 7a's internal appendix contradiction** about
  `check-accessibility-policy`'s summary-absence fail-open: A.6 finding 1 records it
  **fixed and proved by deliberate red**, A.13a records it *"Not fixed here"*. Read the
  script. Record which is true, and if it is unfixed, fix it with its own self-test arm.
- [x] 0.13 **Record the split decision point and its trigger.** Six new facades exceeds
  slot 7a's five, and 7a split. The boundary is **after `WFR-SHELL-GEOMETRY`**; the
  trigger is the data-safety pass, the recent-documents seam retirement, or the §E3
  re-key consuming the change's capacity. It never renumbers, and a partially migrated
  row is never an acceptable outcome.
  - [x] 0.13a **If the split is taken, allocate the delta clauses rather than the delta
    files.** The two deltas are not each one obligation, and their clauses land on
    different sides of the boundary: delta 1(a) cross-row staling and 1(c) provisional
    groupings are **discharged in 7b**; delta 1(b) terminal status / probe evidence /
    reconciliation and 1(d) the closeout record are **7c's**; and delta 2's work happens
    in 7b while its own scenario says the obligation *"is discharged by the change that
    closes the migration programme"*. Select one of three lawful resolutions and record
    it: move both files to 7c and accept that 7b does work its spec text does not yet
    require; **split delta 1** along that line into two files; or keep delta 2 in 7b and
    reword its discharge clause to "the change that discharges the lane", a one-sentence
    delta edit paid with its retroactive re-check. Do not discover this mid-split.
- [x] 0.14 **Quote the behavior anchors this change must preserve verbatim in
  behavior**, before moving any geometry code, and name the rules file each is in —
  slot 7a's A.5 records that its own task list named the wrong file for two contracts.
  From `.agents/rules/ui.md`: the Split-View Rules, the `ClipBin` zero-minimum-height
  contract, the width-preset presets and their clamp, the allocation-time rule that
  paths clamp and cache but never persist GSettings or reparse an `AdwBreakpoint`
  condition, and the compact `AdwBottomSheet` bounded-natural-height contract. From
  `.agents/rules/widget-wiring.md`: the GtkPaned Position Constraints in full — restore
  then pre-clamp, the hidden-restore collapsed endpoint, per-frame animation clamping,
  the `max(measure(Horizontal, -1), measure(Horizontal, current_height))` floor,
  clamping against the real end-child, the revealer wrapper for zero-width panes,
  hide-time clamps staying live until the wrapper is hidden, and arming a `SettleBurst`
  **before** setting an animated property — plus the transient-surface dismissal order
  (Bubble phase, one topmost surface per Escape, Focus Mode last, palette click-away
  through `close_command_palette()`) and the focus-restoration-on-overlay-close
  contract.

---

## 1. Capability deltas and their retroactive re-checks

Both deltas add obligations, so both carry the retroactive cost across **sixteen**
migrated rows. That cost is the point, not paperwork: the not-a-confirmation streak
stands at seven, and slot 7a's re-check of delta 3 found two real instances.

- [x] 1.1 **Land delta 1's statements** in
  `openspec/specs/workflow-readability-boundaries/spec.md`: cross-row cell staling;
  terminal status at programme close with probe evidence; matrix/ledger reconciliation;
  provisional grouping rows and the forbidden line-count split; and the completion
  record with its deferral inventory and its no-self-acceptance rule.
- [x] 1.2 **Land delta 2's statement** in
  `openspec/specs/workflow-evidence-surfaces/spec.md`: a cross-cutting lane owes the
  surface but not the facade, under the same visibility, reentrancy,
  non-materialization, and bounded-child rules with the same three proofs; no forked
  shared limit; the surface's file may keep the lane's name; discharge by the closing
  change.
- [x] 1.3 **Retroactive re-check for delta 1(a) — cross-row staling.** For every
  migrated row, ask whether any earlier change assigned it files without re-deriving its
  cells. `WFR-SHELL-LAYOUT` was the known instance and is being retired; the question
  here is whether a **migrated** row inherited the same shape. Slot 3b's assignment of
  `ui/open_popover/**` is the template to look for.
- [x] 1.4 **Retroactive re-check for delta 1(b) — terminal status.** Sweep every row:
  does any status label carry a trailing narrative that contradicts the label? Does any
  non-migrating terminal row lack probe evidence? `WFR-EDITOR-MEMORY` (`exempt`) and
  `WFR-MIGRATION-LEDGER` (`cross-cutting`) predate the probe rule; establish whether
  each records a probe and record the finding either way.
- [x] 1.5 **Retroactive re-check for delta 1(c) — provisional groupings.** Is
  `WFR-SHELL-LAYOUT` the only residual grouping row? Test the criterion against every
  row that names more than one surface family, and record the negative findings.
- [x] 1.6 **Retroactive re-check for delta 2.** Does any migrated row expose a second
  typed observation path alongside its surface, or an evidence type wider than its
  readers need? Slot 7a named three `pub` candidates from the Evidence Surface
  Baseline: `DisposalPressureEvidence` (this change's, §E4),
  `WorkspaceScanPressureEvidence`, and `NoteScoringEquivalenceEvidence`. **Establish
  each one's reader set before concluding anything about its visibility** — §E4's
  measurement shows why: a cross-crate widget-test reader makes `pub` the narrowest
  compiling visibility, and "narrow it" would be a regression dressed as compliance.
- [x] 1.7 **Implement the mechanical half delta 1 still owes**: fail when a matrix row
  carries a transitional status while the ledger has no `outstanding` slot naming it.
  Slot 7a implemented the slot-agreement half and left this one, *"which travels with
  7b, the change that can produce a transitional status."* Prove it by **deliberate
  red** — produce a transitional status with no outstanding slot, see the gate fail,
  then close it.
- [x] 1.8 **Decide whether `pending`, `deferred`, and `partially-conforming` stay in
  `KNOWN_STATUS_LABELS`.** Delta 1 says they must not survive the closing change. If the
  labels remain accepted by the gate, the rule is enforced by review only, which is the
  class the programme keeps fixing. If they are removed, the gate must still fail
  *informatively* on an unknown label rather than silently exempting the row. Decide,
  implement, and prove both arms.
- [x] 1.9 `openspec validate --all --strict` after landing both deltas, and record the
  pass/fail counts. Slot 7a recorded 111 passed / 0 failed after delta 3.

---

## 2. Path-keyed gates, mutation scope, and drift-gate registrations

- [x] 2.1 **Observe the disarm before fixing it (§E3).** With the geometry code moved
  out of `imp.rs` and `actions.rs` and **no key added**, run
  `make check-visual-proof-policy` and the `cargo-gtk-proof` half and show each
  **passing while protecting nothing**. Record the observation. This is the property
  that makes reviewing the edit insufficient, and it is the only step that proves the
  re-key was necessary rather than decorative.
- [x] 2.2 **Re-key to the narrowest key that still selects exactly the protected code**,
  in **both** implementations: add
  `crates/lushtext-core/src/ui/window/geometry/` as a role-home prefix constant
  alongside the retained `actions.rs` and `imp.rs` literals. A
  `crates/lushtext-core/src/ui/window/` prefix is **forbidden** — it would demand two
  pixel invariants and the sidebar animation matrix of seven subdirectories, four of
  them migrated role homes no predicate has ever protected.
- [x] 2.3 **Do not remove `actions.rs` or `imp.rs` as keys** without arguing the
  behavior. Both retain protected code (§E1: `actions.rs` is not demotable; `imp.rs`
  keeps its template-child and non-geometry halves). Narrowing a key because *some* of
  a file's content moved is a scope change and must be argued on the behavior, not the
  rename.
- [x] 2.4 **Verify the mutation glob still reaches the moved `policy.rs`.** Moving
  `ui/window/policy.rs` into `ui/window/geometry/policy.rs` keeps it inside
  `ui/**/policy.rs` by convention — **verify it after the move** rather than assume, per
  the nested-home rule, and re-derive the module's mutant count from the tool. The
  sources disagree: the matrix records **80** mutants with 15 survivors triaged to zero
  for `adaptive_shell.rs` → `policy.rs`; report what the tool says now, and state the
  figure as **relocation parity** (before → after) rather than as gain, because this
  module's gain-from-zero was already paid by slot 7a.
- [x] 2.5 **Add a parity assertion to each implementation and prove each by a deliberate
  red.** One assertion on one side is the half that passes while the other side is
  wrong. Confirm the Python half's self-tests actually run — slot 6 found them
  unreachable.
- [x] 2.6 **Re-key or retire every other path-keyed or string-keyed reference this
  change's moves touch**, and verify each **by running the lane** rather than by
  reading it: `.cargo/mutants.toml` `exclude_re` entries anchored on moved symbols,
  `scripts/run-performance-smoke.sh`'s 17 Criterion group names / 20 widget test names
  / 3 module-qualified test paths, and `scripts/check-automation-docs.py`'s
  `EVIDENCE_PROJECTIONS` paths. Slot 6's fail-open lesson applies: a filter that
  matches nothing exits 0.
- [x] 2.7 **Retire the stale `ui/window/tabs.rs` calibration comment** in
  `.cargo/mutants.toml`, which records an exclusion the current `examine_globs` never
  applies. Slot 7a's proposal named it and did not reach it.
- [x] 2.8 **Register each new evidence surface that projects to automation**, or record
  that none does. Slot 7a's projection count stayed at seven because its new surfaces
  are `test-utils`-gated and report no automation field; establish the same for each of
  this change's six, per surface, and prove any new registration **rejects a real
  rename** rather than asserting the drift gate works.

---

## 3. `WFR-PLAIN-DISPOSAL` — the lane's surface (§E4)

No facade, no coordination role names, no `policy.rs`. The row stays `cross-cutting`
and advances to `cross-cutting — surface obligations discharged`, matching
`WFR-BUFFER-SNAPSHOT`'s resolved form.

- [x] 3.1 **Re-derive the row's measured cells** row-scoped, with the predicate stated
  on every figure because this is the census's only dual-gate user: `ui/plain_disposal.rs`
  at **1,542 physical / ~1,344 production** and `model/plain_disposal.rs` at
  **692 / ~465**; **8** `*_for_test` declarations; **17** `cfg(feature = "test-utils")`
  plus **13** `cfg(any(test, feature = "test-utils"))` sites in the `ui` half and
  **0** of either in the `model` half. State the direction of every correction, and
  record that an unchanged cell is a legitimate outcome.
- [x] 3.2 **Consolidate the three parallel typed observation values into one surface**,
  reached through one accessor, with the ordinary and progress lanes as **named
  components** rather than two top-level accessors — the shape slot 7a used for
  `BufferSnapshotEvidence`. Retire all five existing observation accessors — leaving the
  three actuation holds alone — and update every reader
  in `crates/lushtext/tests/widget/{plain_disposal,window,command_palette,editor_page,
  markdown_preview,search_panel}.rs`.
- [x] 3.3 **Record the visibility conclusion with its reader measurement.** Slot 7a's
  instruction to narrow `DisposalPressureEvidence` from `pub` is **unexecutable as
  written**: the type is already `test-utils`-gated and its only reader is
  `crates/lushtext/tests/widget/plain_disposal.rs:16`, in a different crate. State the
  measured reader set, state the narrowest visibility that compiles for it, and if that
  is `pub`, record "already narrowest" as the finding. Do not execute a narrowing that
  breaks the widget lane and call it compliance.
- [x] 3.4 **Discharge the reentrancy proof with the lane quiesced.** This lane's state
  includes process-wide atomics and high-water marks mutated by **worker threads**, so
  read-to-read identity is not a property of the reader's control flow. Drain to a
  terminal, confirm zero running and zero queued **through the surface itself**, then
  assert identity; assert **monotonicity** rather than equality anywhere a worker can
  still advance a counter. Slot 7a's no-retry widget lane caught exactly this class —
  an unsound assertion in its own evidence proof whose panic read like a production
  defect, which a single retry would have hidden.
- [x] 3.5 **Discharge the disposal proof.** The lane's surface is not derived from a
  `TemplateChild`, so state which stage plays the disposed-widget role for a lane —
  a torn-down owner whose pending job is cancelled — and prove the surface answers
  honestly rather than panicking.
- [x] 3.6 **Discharge the non-materialization proof.** Read the surface with the lane
  empty and with it saturated, and show admission counters, retained-byte accounting,
  high-water marks, retry-source counts, and producer terminals **identical before and
  after each read**. Prove it, do not assert it.
- [x] 3.7 **Confirm no shared limit moved or forked.** The lane's constants are consumed
  by ten workflows (`MAX_REPLACE_UNDO_RETAINED_BYTES`, `MARKDOWN_PLAN_RESERVATION_BYTES`,
  `STARTUP_PRELOAD_RESERVATION_BYTES`, `PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY`).
  Consolidating the surface must not relocate or duplicate any of them — delta 2 states
  it, and slot 3a's decision not to fork
  `char_count_requires_chunked_snapshot` is the precedent.
- [x] 3.8 **Resolve the `DisposalProducer` family's 12 default-feature `never used`
  items**: `MAX_SMALL_PENDING_DISPOSAL_BYTES`, `try_own_for_gtk`,
  `DisposalProducerInner`, `DisposalProducer` and its five associated items, and
  `retry_pending` — all inside the 13 dual-gated sites. Retire what is dead; gate or
  justify what is live. They are invisible to `clippy --all-features`, which is slot 5b's
  lesson about which configuration hides what, so **check under default features**. Do
  not leave 12 warned items in the row the closing change declares settled.
- [x] 3.9 **Record `ui/plain_disposal.rs`'s 1,344 production lines against the
  ~1,000-line target** as accepted refactor debt in the closeout inventory, with the
  reason it is not split here (the lane's contract is one admission mechanism; a split
  by line count would repeat the error §E1 forbids). Do **not** split it blind.

---

## 4. The six replacement-row migrations (§E1, §E2)

For every row: choose the role home per §E2 and record the choice in the matrix row;
assign each module exactly one role from the bounded set (`admission`, `execution`,
`retirement`, `watch`, `journal`) or record it as a **called presentation surface** in
both its own module doc and the row; and never label a module "adapter detail" — that
label was retired by slot 5a. Where a row's stages are connected by a deferred drain,
idle callback, or worker completion, the facade **documents the inversion and names the
point where control resumes**.

Order is increasing risk and proof cost. Do not reorder.

- [x] 4.1 **`WFR-TRANSIENT-DISMISSAL`** — role home `ui/window/transient_dismissal/`.
  Facade against the ≈120 projection. Preserve the dismissal contract exactly: Bubble
  phase so focused children, dialogs, popovers, dropdowns, and entries get first chance;
  exactly **one** topmost visible dismissible surface closed per Escape; Focus Mode
  after transient dismissal; palette click-away through `close_command_palette()` so
  saved-focus restoration runs; and the pointer sequence claimed so the same press does
  not activate an underlying control. Name the one-tick idle latch as an inversion with
  its resumption point.
- [x] 4.2 **`WFR-FOCUS-MODE`** — role home `ui/window/focus_mode/`. Facade against ≈150.
  Preserve fullscreen ownership, preview compatibility, the affordance-hide
  `SupersedingTimer`, and the readable-column margin path that queues the preview
  layout-settle. Probe for a seam value object and for an evidence surface **before**
  concluding either is unnecessary, and record the negative finding — slot 7a's four
  `policy: none` rows all turned out to own a `policy.rs`.
- [x] 4.3 **`WFR-EDITOR-MEMORY-EVICTION`** — role home
  `ui/window/editor_memory_eviction/`, extracted from `focus_indexing.rs`. Facade
  against ≈200. Carry its generation counter, its bounded idle continuation, its 8 test
  seams, and its **two race-injector hooks** across the move without changing timing.
  **Do not widen `WFR-EDITOR-MEMORY`'s `exempt` resolution** to cover this code
  (design non-goal); `model/editor_memory.rs` stays where it is and keeps its
  resolution.
- [x] 4.4 **`WFR-RECENT-DOCUMENTS`** — canonical role home `ui/open_popover/`, nested
  with `window/recent_open.rs`. Facade against ≈250, **the escalation candidate**.
  - [x] 4.4a Retire the row's **26 gated declarations across 37 sites** — more than
    every other shell surface combined — into one evidence surface plus one
    `test_policy.rs`. Fold `OpenPopoverRowLayoutSnapshot` in.
  - [x] 4.4b Fix the **ungated** `window.imp().recent_documents.loading` read in
    `crates/lushtext/tests/widget/open_popover.rs`, inherited from slot 3b's census gap
    and never closed.
  - [x] 4.4c Discharge the three surface proofs, with the **disposed-widget** proof read
    through `try_get()` — the popover's fields derive from template children, and slot
    5a's trap was that a transitive window accessor derefs one and panics.
  - [x] 4.4d If the facade exceeds 370, take escalation **step 1** (extract called
    presentation surfaces, push stage bodies into coordination roles) and record the
    attempt and its measurement before considering step 2. Do not split the row.
- [x] 4.5 **`WFR-SHELL-GEOMETRY`** — role home `ui/window/geometry/`, with
  `ui/window/policy.rs` moving in. Facade against ≈190.
  - [x] 4.5a Preserve every anchor quoted in 0.14 **verbatim in behavior**. The
    allocation-path rule is the one a role move is most likely to break: allocation and
    programmatic notify paths clamp runtime geometry and cache derived thresholds, and
    **never** persist fractions to GSettings or reparse an `AdwBreakpoint` condition.
    Persistence stays tied to explicit user intent, restore, or animation completion.
  - [x] 4.5b Keep the `workspace-sidebar-animation` readiness blocker with the
    **animation**, not with the row name, per slot 5a. Keep
    `ui/sidebar/width_preset.rs`'s `WorkspaceSidebarWidthPreset` in this row with its
    three consumers, already re-pointed by slot 5b.
  - [x] 4.5c Preserve the `ClipBin` zero-minimum-height contract so the status bar can
    still be allocated inside the visible height, and the compact `AdwBottomSheet`
    bounded-natural-height contract.
  - [x] 4.5d This row's files are **visual-sensitive**. Its changes require two named
    pixel invariants and the workspace-sidebar animation matrix; §E3's re-key is what
    keeps that requirement armed after the move, and task 10.13 is what proves it.
- [x] 4.6 **`WFR-TAB-STRIP`** — role home `ui/window/tab_strip/`. Facade against ≈220.
  - [x] 4.6a Take the close/delete half of `documents.rs` and leave the rest with its
    owner; state which row owns the remainder rather than leaving `documents.rs`
    unattributed.
  - [x] 4.6b Carry slot 7a's teardown-before-close fix across the move **without
    duplicating the teardown**, and confirm its regression test still fails without the
    fix by deliberate revert-and-rerun. Slot 7a's B.4 records that the handoff's word
    "move" would have duplicated it.
  - [x] 4.6c Pair every structural tab operation with explicit refresh of all
    tab-dependent UI, per `.agents/rules/widget-wiring.md`: signal ordering during
    `close_page()` is not guaranteed and `selected-page` may not fire when closing a
    non-selected tab. Also reset window-level projections that can outlive the previous
    tab (preview-only mode) before scheduling editor focus restoration.
  - [x] 4.6d **Spend zero actuation seams.** Slot 5b's budgeted one remains unspent by
    6 and 7a, and this change plans to leave it unspent. If a stage is unreachable
    without one, say so and leave the coverage gap recorded rather than spending it
    quietly.
- [x] 4.7 **One test policy per row.** Test-only timing and limit overrides belong in
  the row's single `test_policy.rs`, not in several module-level statics, and no
  override storage may compile without the test feature. Confirm each new
  `evidence.rs` and `test_policy.rs` is `test-utils`-gated with the module doc stating
  that production reads live state directly — the shape slot 6 fixed at
  `minimap/mod.rs:47`.
- [x] 4.8 **For every row, probe for a seam value object and record the finding.** A
  bundle crossing two or more function boundaries or reconstructed at two or more call
  sites is reified; a bundle used by one private helper is not. Reuse the shapes the
  codebase already uses (Ticket + Facts + predicate; coordinator generation identity)
  rather than inventing a parallel one. **A value must not be renamed while crossing a
  seam.**
- [x] 4.9 **Treat every `#[expect(clippy::too_many_arguments)]` this change would add as
  an unreified seam.** The workspace has exactly **1**, at
  `model/action_catalog.rs:178`, the exempt domain catalog constructor. Do not add a
  second.

---

## 5. Reassignments, cross-cutting rows, and the coverage proof

- [x] 5.1 **Implement 0.5a's `dialogs.rs` verdict**, and re-verify the close-coordination
  contract in `ui/window/AGENTS.md` holds **exactly** after the reassignment: input
  rejected across the selected-save pipeline and later draft/session yields; discarded
  editor identity, content generation, modified state, and path fingerprinted at
  confirmation; active saves and freshness rechecked before cleanup and destruction; and
  retryable drafts plus sensitivity restored on every aborted close.
- [x] 5.2 **Implement 0.5b's `ui/window/search.rs` verdict** with bounded coordination
  role names, and correct `WFR-SEARCH-REPLACE`'s size cell, which excludes 928
  production lines it owns.
- [x] 5.3 **Implement 0.5c's `focus_indexing.rs` verdict** and reconcile
  `WFR-COMMAND-PALETTE`'s cell.
- [x] 5.4 **Implement 0.5d's theme-selector verdict.**
- [x] 5.5 **Populate the no-coordination-tier list** with each surface's evidence for
  all three properties the list's preamble claims — no ordered stages, no coordination
  role, no seam value-object obligation — recorded **per surface**. Expected entries:
  `zoom.rs`, `workspace_scope.rs`, `ui/open_popover/item.rs`, `ui/properties_panel/**`.
  **`transient_surfaces.rs` must not be re-added** and **`actions.rs` must not be
  demoted**.
- [x] 5.6 **Resolve `WFR-STARTUP-PREFLIGHT`** as a `cross-cutting` row with no facade,
  naming the five workflows `startup_data.rs` orders and recording the probe evidence
  delta 1 requires — including a probe of the module for separable pure decisions.
- [x] 5.7 **Dispose the unbounded startup activation-open queue** (slot 5a, un-homed by
  slot 7a): a bounded-work assessment with a named budget, or a `docs/next/` record with
  its gating condition and owner. It belongs to `startup_data.rs`, which is
  cross-cutting and owned by none, so *"it belongs to another row"* is not a
  disposition here.
- [x] 5.8 **Establish whether slot 5b's M-5 format-gate fail-open site is inside this
  change's files** — the question slot 7a's task 0.10 left open — and dispose it under
  the proposal's disposition rule.
- [x] 5.9 **Retire `WFR-SHELL-LAYOUT`** from the Product Matrix, replaced by the seven
  rows, each with a stable `WFR-*` id, its own measured cells, its slot cell naming this
  change, and its terminal status. Record the retirement rather than deleting the row's
  history.
- [x] 5.10 **Re-derive the census coverage proof under a stated predicate.** Three
  figures are in circulation for one denominator — the matrix's proof reads *"198 files
  exist under `crates/lushtext-core/src`; 195 are attributed"*, slot 7a recorded
  **266**, and a count at authoring gives **286**. **Adopt none of them.** Three
  disagreeing counts is itself the finding, and its cause (which paths the predicate
  includes) must be stated. Re-derive both numerator and denominator, attribute every
  file, and show the delta against 198/195 with the cause. A split changes the attribution table, and an
  un-re-derived proof would be a false completeness claim at exactly the moment the
  programme claims completeness.

---

## 6. `WFR-AUTOMATION-SPINE` (§E5)

- [x] 6.1 **Probe `ui/automation.rs` (2,214 / ~2,084) for separable pure decisions**
  before concluding the row owns no `policy.rs`, and record the finding either way.
  Slot 7a found five decisions in editor-find, six in notifications, and a whole dialog
  vocabulary in encoding, in **four** rows the census recorded as `policy: none`.
- [x] 6.2 **DECISION (§E5): the row's terminal status.** Select `cross-cutting`
  (expected) or `migrated` on the evidence; `exempt` is rejected in advance because
  several slots have advanced this row incrementally — **eight**, counted from the
  ledger during implementation (2a, 2b, 3a, 3b, 4, 5a, 5b, 6), against the seven
  this task premised.
- [x] 6.3 **Re-derive the row's evidence cell** rather than inheriting slot 7a's
  correction. Count the registered projections in `EVIDENCE_PROJECTIONS` and state the
  figure with its source; a correction is a measurement, and this change re-derives
  measurements.
- [x] 6.4 **Do NOT strike the automation ratchet row retired** — its own predicate
  forbids it. The matrix records two open entries at `ui/automation.rs:517`/`:518`
  reading `window.imp().tab_view`. Measured at authoring: the **recorded occurrence is
  gone** (`current_readiness_failure` now iterates `window.open_editors()`), but the
  **reading expression persists at eight sites** — `:585`, `:586`, `:776`, `:777`,
  `:836`, `:837`, `:843`, and the chained read spanning `:2003`–`:2005`. Seven match
  `imp.tab_view` after a local `let imp = window.imp()`; the eighth reads through a
  multi-line `.imp()` chain that a single-line grep misses. The table's own instruction
  is *"match on the reading expression rather than the line"*.
  - [x] 6.4a Record the row as **occurrence-retired, expression persisting**, with the
    eight sites enumerated and a **new owning row** — `WFR-SHELL-LAYOUT` is being
    retired, and these sites read the tab collection, so the owner is whichever row task
    0.5 gives tab enumeration.
  - [x] 6.4b Keep the three measurements separate and do not conflate them: the recorded
    occurrence (retired), the reading expression (8 sites), and how many of the file's
    **15** total `.imp()` reads cross a workflow boundary under the table's predicate
    (a third number). **The predicate a ratchet row is checked under decides whether it
    is closed** — a row recorded by line number reads as retired the moment code moves.
- [x] 6.5 **Verify, do not inherit, the `MinimapEvidence` non-registration verdict.**
  Slot 6 called it *"a result rather than an omission"* — the minimap's ≥18
  `visual_geometry.native_minimap` fields derive from live widget geometry rather than
  workflow state. The Completion Rule says *"any automation snapshot field for this
  workflow projects from the evidence surface"*, so read the fields and record the
  verdict. This is the second slot asked to verify it.
- [x] 6.6 **Re-check slot 6's conditionally-cleared `minimap_work_pending` candidate.**
  The clearance holds only while no `mark-set` handler reads readiness; both handlers in
  the tree reach only scrolling and menu-model refresh. Record it as a **standing
  condition** in the closeout inventory rather than a closed item, so a future
  `mark-set` handler that reads readiness re-opens it visibly.
- [x] 6.7 **Reconcile all three sources** (Finding 4): the matrix `Slot` cell, the
  Migration Order table, and the programme ledger. Include the slot-label mismatch
  (`2a` versus `2`), the dropped terminal-status clause, and slot 7a's omission of
  `WFR-AUTOMATION-SPINE (partial)` from its own `complete` line — correct it or state it
  as deliberate with its reason. Leaving a defensible omission undefended is how the
  next reader re-opens it.
- [x] 6.8 **Prove the exported D-Bus contract is unchanged** by a measured diff of the
  exported schema and a before/after Automation1 capture of the same app state, not by
  assertion. Slot 2b's no-widening proof is the shape.

---

## 7. Data safety

Seven consecutive slots found at least one confirmed defect. This change carries two
tier-3 rows. Apply the proposal's disposition rule, and record the verdict, severity,
site, and owning row for every candidate **including the ones cleared** — slot 5a found
two tests that passed against broken code, and a test that cannot fail is worse than no
test.

- [x] 7.1 **`WFR-PLAIN-DISPOSAL` (tier-3)** — audit the retirement lane's terminal
  ownership: does every path either carry the permit forward or release it? A dropped
  terminal strands whoever waited on it — slot 3b fixed exactly that shape in the load
  row — and a mis-accounted permit lets the next admission overshoot the budget. Cover
  the panic path (`catch_unwind`), the retry path, and exact-owner teardown
  cancellation.
- [x] 7.2 **`WFR-TAB-STRIP` (tier-3)** — audit the **tab-pin and bulk-close paths in
  `tabs.rs`**, the neighbours two independent passes reached the teardown defect
  *without* examining. Confirm no path runs teardown before a cancellable
  `close_page()`, and that a cancelled bulk close leaves every surviving tab's load,
  monitor, and draft record intact.
- [x] 7.3 **`WFR-SHELL-GEOMETRY`** — confirm no clamp, notify, or allocation path
  gained a GSettings write or an `AdwBreakpoint` reparse in the move, and that
  persistence still runs only from explicit intent, restore, or animation completion.
  A persistence write moved into an allocation path is a live-warning and
  monitor-refresh regression the widget lane cannot see.
- [x] 7.4 **`WFR-EDITOR-MEMORY-EVICTION`** — audit eviction against in-flight saves and
  draft writes: can an eviction race a save's buffer snapshot, or evict state a draft
  autosave is about to persist? The two race-injector hooks are the instrument; use
  them.
- [x] 7.5 **`WFR-RECENT-DOCUMENTS`** — audit the lazy projection gate: can a stale
  completion publish rows for a superseded query, and does the row's activation path
  re-check the path still exists before opening?
- [x] 7.6 **`startup_data.rs`'s format-upgrade preflight** — the gate slot 5b found
  fail-open (M-5), and the surface `WFR-STARTUP-PREFLIGHT` makes terminal. Confirm the
  preflight cannot admit an unmigrated format on an error path.
- [x] 7.7 **Land every handed-on finding in a `docs/next/*.md` record** with severity,
  site re-verified against the code in this change, owning row, and close condition —
  and name each in the closeout inventory. **"Handed on" has no recipient after this
  change.** `docs/next/persistent-format-hardening.md` is the established home for this
  class; slot 7a landed four there and recorded the fifth closed.

---

## 8. Inherited debt from slot 7a's review pass (§E7)

Six items that reached **no** artifact — no OpenSpec change, no `docs/next/` record, no
rules file. Re-verify before disposing, and re-derive every figure under a stated
predicate; two inherited figures are already falsified.

**Ordering constraint (§E7):** tasks 8.2, 8.3, and 8.4 edit files the 8.1 reformat would
also touch. Run the reformat strictly first or strictly last within this section, never
interleaved, and record which order was chosen — otherwise review sees one diff mixing
411 mechanical hunks with three semantic fixes.

- [x] 8.1 **DECISION: the rustfmt gate hole.** `crates/lushtext/tests/widget.rs` reaches
  its 18 test modules through `include!(concat!(env!("OUT_DIR"), "/widget_test_registry.rs"))`,
  so `cargo fmt` cannot discover them and `cargo fmt --all --check` **passes while
  formatting nothing under `tests/widget/`**. Re-derived at authoring: **411 hunks across
  18 files** (`workspace_section.rs` 129, `window.rs` 75, `markdown_preview.rs` 70,
  `command_palette.rs` 39, `editor_page.rs` 29, `app.rs` 17, `sidebar.rs` 14,
  `status_bar.rs` 10, and ten smaller), measured per file with
  `rustfmt --edition 2024 --emit stdout | diff`; the inherited figure of 171 is wrong.
  Give rustfmt reach through the invocation and take the reformat, **or** record the
  hole in a `docs/next/` record with its gating condition — and the only admissible
  gating condition is a measured conflict between the reach mechanism and the harness's
  registry generation. *"It is a large diff"* is not one:
  `.agents/rules/preexisting-blockers.md` has no exceptions, and this is the
  gate-coverage class delta 3 and slot 6 each fixed rather than recorded.
- [x] 8.2 **Fix the proof that misdescribes its own premise.**
  `crates/lushtext/tests/widget/window.rs` asserts
  `print_evidence(&window).document.is_some()` at `:14566` and again at `:14575`–`:14578`
  under a comment claiming *"Verified: the surface still answered `Some` after
  `close()`, so a close-only test would have proved nothing"* — **the test never calls
  `close()`**. Either make the comment describe what the test does, or add the
  `close()` step the comment claims was verified, and remove the duplicate assert. A
  proof that reports a verification it did not perform is the honesty class this
  programme keeps naming, not a comment nit.
- [x] 8.3 **Remove the dead tuple ladders** in
  `crates/lushtext/tests/widget/markdown_preview.rs`: **8** destructuring sites binding
  **20** `_` placeholders — **6** of them over seven-element tuple literals and **2**
  over five-element ones — left behind when slot 7a
  retired 11 tuple-returning seams into `MarkdownPreviewEvidence` — `:1208`, `:1303`,
  `:1322`, `:1385`, `:1415`, `:2223`, `:2413`, `:2454`. **Re-derive the line count under
  a stated predicate**; the inherited "~70 lines" is untraceable to any measurement.
  Read the fields from the surface directly.
- [x] 8.4 **Re-scope or record the negative: `encoding/dialogs.rs`'s named
  "near-duplicate pair" does not exist.** `append_action_row` (`:342`) is a
  **one-expression default-argument wrapper** whose entire body delegates to
  `append_action_row_with_sensitivity(group, title, subtitle, true, ...)` — the
  idiomatic way to express a default in a language without them, not duplication. The
  inherited claim also miscounts the file's row builders as four; there are **five**
  (`dialog_row`, `static_dialog_row`, `append_action_row`,
  `append_action_row_with_sensitivity`, `append_choice_row`).
  - [x] 8.4a Look for **real** duplication in the six `present_*` shapes over
    `build_dialog` (`:36`, `:99`, `:141`, `:183`, `:217`, `:268`, `:308`).
  - [x] 8.4b If none is found, **"no near-duplicate exists" is the recorded outcome.**
    Do not refactor a correct wrapper to satisfy an inherited claim — that is the
    false-correction error slot 7a named when it withdrew its own
    `WFR-STATUS-NOTIFICATIONS` seam correction: *"a false correction in a re-derivation
    table is worse than an uncorrected census figure"*.
  - [-] 8.4c **NOT REACHED** — 8.4a found no real duplication, so 8.4b's recorded outcome applies. If real duplication is found, remove it **only** where the grouped-row
    contract in `.agents/rules/ui.md` is preserved **exactly**: one
    `AdwPreferencesGroup` per conceptual set, short row titles with clear subtitles,
    activatable rows for choices and non-activatable for facts, and the widget coverage
    asserting the grouped section labels and representative row titles/subtitles. This
    file is a called presentation surface of a row migrated one change ago; a tidiness
    edit that changes dialog geometry is a regression.
- [x] 8.5 **Audit the ledger-and-slot-column *parsing* path for fail-opens.** The
  inherited framing — "re-derive whether three holes exist" — is **wrong**: all three
  reproduce, and two sit *around* `check-workflow-boundaries.py`'s four documented
  conditions rather than inside them. The four conditions are implemented correctly; the
  holes are in the code that decides **what the conditions see**, so an audit that
  enumerated the conditions would have reported the gate sound. These three are the
  audit's **starting findings**, not its conclusion:
  - [x] 8.5a **A renamed `Slot` header silently disables the slot-dependent half of the
    matrix check.** `parse_matrix_rows` sets `slot_index` only when a header row contains
    both `row id` and `slot` — deliberately, *"so the `Slot` column is located by name
    rather than by a positional guess that a later column insertion would silently
    shift"*. Reword that header and `slot_index` stays `None`, every row's `slot` is
    `None`, every slot-dependent finding is skipped, and the gate **exits 0**. The
    defence against a positional shift became a fail-open against a rename. Fix so a
    matrix whose `Slot` column cannot be located is a **finding**, not a silent skip.
  - [x] 8.5b **A typo'd ledger verb silently drops that slot's claim.**
    `SLOT_LEDGER_RE` requires the literal `complete` or `outstanding`; a non-matching
    line is `continue`d, and the only guard is the *all-lines-failed* case
    (`if not claims`). One malformed line among eleven is invisible — and a dropped
    `outstanding` line is exactly how a row with remaining work reads as settled. Fix so
    a line that looks like a ledger line but does not parse is a finding.
  - [x] 8.5c **A `Slot` cell of `none` exempts a `pending` row from the
    outstanding-slot rule.** `none` legitimately means "never migrated" for
    `WFR-EDITOR-MEMORY`; nothing distinguishes that from a `pending` row carrying
    `none`. Fix so the combination is a finding.
  - [x] 8.5d Sweep the rest of both parsing paths for the same class, and prove each fix
    by **deliberate red**. This is not paperwork: the closeout's central claim is that
    the matrix and the ledger agree, and two of these holes let them disagree at exit 0.
    It is also the class delta 3 was written for, slot 6 fixed twice, and slot 7a fixed
    twice more — found this time in the reconciliation gate the closeout depends on.
- [x] 8.6 **Remove `git_lines`** at `scripts/accessibility_source_fingerprint.py:142`–
  `:143`: a one-line wrapper over `git_lines_checked` with **zero** callers, confirmed
  at authoring. Re-run the accessibility fingerprint lane afterwards — the module's own
  bytes are part of the relevant set, so removing dead code voids the proof.
- [x] 8.7 **Resolve the A.6 / A.13a contradiction** from task 0.12 and record which
  appendix was right, so a later reader does not re-plan a fixed item or trust an
  unfixed one.

---

## 9. Mutation coverage

Report **relocation parity** and **extraction gain** as separate figures, each naming
the exact invocation and its file-level anchors. A rename of an already-pure module that
was never in the scope has no before-count: its result is a gain from zero and must not
be dressed as parity (slot 5b's finding G7 found exactly that conflation in the live
matrix).

- [x] 9.1 **Re-derive the mutation floor from the tool**, not from recall:
  `MUTANTS_RE='ZZZ_NO_SUCH_MUTANT_ZZZ' make mutants-list`. Slot 7a measured **34**, all
  `delete field`, of which 12 in `services/file_tree.rs`. The floor is a property of the
  **name filter** only; `--in-diff` genuinely bounds a run.
- [x] 9.2 **Do not scope a focused run with `--in-diff` over a diff containing a
  rename.** A rename is a whole-file delete plus add, so it measures far more than the
  logic that changed — slot 7a saw 347 mutants where the answer was 160. This change
  contains **six** role-home creations, so the hazard is at its maximum.
- [x] 9.3 **`test -s` any diff file passed to `run-mutants.sh`.** `ensure_diff_file`
  silently substitutes a `git diff origin/main...` three-dot diff when the path is
  missing, and slot 7a got a plausible-looking `54 mutants: 1 missed` against the
  previous slot's committed diff. Check every survivor's path against the files you
  scoped.
- [x] 9.4 **Report each new `policy.rs`'s mutant count** with parity and gain separated,
  and triage every survivor to zero or to a narrow documented equivalence whose
  invariant is pinned by its own test, so the exclusion cannot outlive its
  justification.
- [x] 9.4a **Resolve slot 7a's mutation-triage self-contradiction before scoping any
  triage.** Its B.6 "explicitly not claimed" list says the **160 newly-in-scope mutants
  are untriaged** — *"`make mutants-diff` was not run, and the figures reported are
  generation counts, not kill counts"* — while its **A.12** records an actual run:
  **130 caught / 13 unviable / 17 missed**, with the 17 since triaged to a residue of
  roughly 0–2. Both cannot be true, and B.6 was written first and not revised when
  A.12's run landed. Resolve it **against the tool**, not against either appendix, and
  record which section was stale. Budgeting to triage 160 untriaged mutants when the work
  is largely done would either report a fictional accomplishment or discover the
  discrepancy with no record of it.
- [x] 9.5 **Triage the actual surviving set**, scoped from 9.4a's resolution rather than
  from the inherited 160 — it may be two mutants. A programme cannot be closed over an
  untriaged scope expansion its own last change created, but neither can it claim credit
  for triage another change already did. Run against a **committed or explicitly passed**
  diff; `make mutants-diff` proves nothing on an uncommitted worktree and exits 0 doing
  it. Slot 7a is committed (`a3b1743a`), so a real base ref exists.
- [x] 9.6 **Re-derive the two ratchet rows' current survivor counts** from the tool:
  8 survivors deleting bounded-scan telemetry fields from the published `DirectoryScan`
  (`WFR-WORKSPACE-TREE`'s) and 5 deleting orphan-cleanup continuation fields from the
  published plan and outcome (the draft row's). Both are the same operator class —
  bounded-work counters no test asserts, in two rows whose whole point is bounded work.
  Decide per row whether this change closes them (a test that asserts the counter) or
  carries them as ratchet rows, and record the decision with its reason.

---

## 10. Verification

Smoke lanes run **last**, after the tree is final, because every `ui/**` edit voids the
accessibility, visual, and visual-geometry proof fingerprints — and slot 7a paid three
lane re-runs for treating "last" as an abstraction rather than an ordering against
edits. Close every code task first.

- [x] 10.1 `cargo fmt --all --check`, plus whatever reach task 8.1 selects. Record both.
- [x] 10.2 **Both feature configurations.** The documented blocking command uses
  `--all-features`, which **hides** breaks the default-feature build reports — slot 5b
  found `origin/main` not compiling under default features while `make check` was green,
  and slot 7a's orphaned `cfg` attribute was caught only by the default-feature rustdoc
  build. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  **and** the default-feature build, and record both. Task 3.8's 12 items live in
  exactly the configuration `--all-features` cannot see.
- [x] 10.3 **The rustdoc gate, by hand** — `make check` does not run it:
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo doc --workspace --no-deps`.
  This change ships **six** new facades in new `pub` role homes, which is the exact
  shape that trips `private_intra_doc_links`: a facade naturally wants to link its own
  coordination modules and `pub(crate)` seam values. The fix is **always** to drop the
  link and keep the name in backticks; never widen visibility to satisfy
  documentation. This class has shipped three times because the local gates were green.
- [x] 10.4 `make check-policy`, and separately `make check-workflow-boundaries`,
  `make check-automation-docs`, `make check-filesystem-boundary`, and
  `make check-blueprint`. Record each. **Name each one's lie-mode beside its result**,
  because a green run is not the claim it looks like:
  `check-workflow-boundaries` validates that a declared role **path exists** and that a
  `policy.rs` is GTK-free — it does **not** validate that a role *name* fits the module's
  actual job, so an off-set or ill-fitting coordination name passes (the bounded set is a
  **review** contract, stated as such in `.agents/rules/rust.md`); and after 8.5 it also
  depends on a `Slot` header it locates by name. `check-automation-docs` compares declared
  projections against documented ones and cannot see a field nobody registered.
  **This change's verification cannot be read off exit codes** — the STATE header's rule,
  applied to its own gates.
- [x] 10.5 `make check-accessibility-policy`, after 8.6's edit and after every `ui/**`
  edit. It requires the accessibility and visual smoke summaries to carry a source
  fingerprint matching the current tree, digested from relevant-file **contents**.
- [x] 10.6 **Accessibility wiring for six new role homes.** Update
  `docs/accessibility-matrix.md` rows for the shell surfaces, route all new metadata
  through `crate::ui::accessibility`, refresh and clear row metadata in
  `connect_bind`/`connect_unbind` for the recent-documents factory, and confirm no
  hover-only affordance lost its keyboard or context-menu alternative. Slot 5b's lesson
  applies to this change's many module-doc rewrites: a module doc describing an
  affordance that has **moved away** is a real gate finding, and the fix is to name the
  owning module.
- [x] 10.7 **State-extreme coverage for every collection surface this change touches**:
  no tabs / no recent documents / no workspaces, one or a few, and many-or-awkward
  (long paths, capped results, deep nesting). Assert the user-visible contract — right
  empty copy, header and close controls visible, only the item region scrolling, no
  unintended scrollbar in an empty status-only surface — not only model state.
- [x] 10.8 `make test`, then `make test-widget-headless` with **no retries** and **zero
  `FLAKY:` lines**. A `FLAKY:` line is a blocker, not accepted noise. Task 3.4's
  quiesced-lane proof is the assertion most likely to produce one; if it does, fix the
  assertion's soundness rather than the budget.
- [x] 10.9 `make test-prop` and `make fuzz-corpus-replay`.
- [x] 10.10 `make test-workspace-row-states`, and every focused lane whose string filter
  task 2.6 touched — **content-asserted**, not exit-code-accepted.
- [x] 10.11 `make performance-smoke`, **content-asserted**: grep the lane summary for the
  asserted lines and confirm every filter this change's renames touch still matches a
  non-zero number of tests.
- [x] 10.12 `make crash-recovery-smoke` and `make automation-smoke`. Both exercise paths
  this change's rows own — draft and session recovery through the shell, and the spine.
- [x] 10.13 `make visual-geometry-smoke` from a **clean artifact root**. The geometry row
  is visual-sensitive: the run must pixel-verify both named invariant ids, include
  per-case pixel rows, final-frame rendered-anchor stability, and final
  sidebar/editor/minimap geometry. Use `pixel_verified_invariant_ids` and
  `animation_verified_invariant_ids` by their correct names — slot 6's task wording
  named a field that will never hold the animation id.
- [x] 10.14 `make visual-smoke`, `make accessibility-smoke`, and
  `make builder-diagnostics-smoke`, each from a clean root, after the last source edit.
- [x] 10.15 `make check-gtk-lush-policy` and `make check-gtk-lush-adoption` — no GTK
  Lush extraction is proposed, and these prove none happened.
- [x] 10.15a `make check-agent-docs` and `make check-agent-skills`. Task 11.11 edits
  `.agents/rules/*` and skill references, and tasks 5.1–5.4 plus 11.5a rewrite module
  docs and `AGENTS.md` files that those validators read for the filesystem-contract and
  rename-safe-topology checks. Both are cheap and neither is inside `make check-policy`
  for the paths this change touches.
- [x] 10.16 `openspec validate --all --strict`.
- [x] 10.17 **Any `ui/**` edit made during verification voids the accessibility, visual,
  and visual-geometry fingerprints.** Re-run the affected lanes or defer the edit; do not
  ship a stale proof.
- [~] 10.18 **Live-display proof — deferred for the user, planned that way from the
  start.** `make run` against restored workspaces: toggle the workspace sidebar and the
  properties pane repeatedly while watching stderr for
  `Trying to measure GtkBox ...`, `pixman_region32_init_rect`, `Gtk-CRITICAL`, and
  `GLib-GObject-WARNING`; open and close tabs, pin and bulk-close, and cancel a close on
  a modified tab (7.2's paths); press Escape with the palette above Focus Mode (4.1's
  ladder); resize across the properties and workspace breakpoints. **Do not start a live
  launch to discharge this.** Slot 4 established that isolating an app's state does not
  isolate its window: a real Wayland launch maps a surface and takes focus regardless of
  `XDG_*` isolation, and it interrupted the user's session. Widget green plus a live
  warning is a **failed** fix, not a partial success, so this gap **must be accepted by
  the user, not granted by this change** — do not write "accepted" into the matrix, the
  programme record, or this file on the change's own authority. **Seven consecutive
  slots have now shipped without it**; task 11.4 records that as the programme's
  standing gap.
- [~] 10.19 **Manual Orca check — deferred for the user**, per
  `docs/accessibility-orca-checklist.md`, for the rows this change touches:
  `A11Y-SHELL-*`, `A11Y-OPEN-*`, `A11Y-PROPERTIES-*`, and
  `A11Y-EDITOR-FOCUS-PREVIEW`.

---

## 11. Programme closeout and handoff

The closeout is written **strictly last**. It asserts that nothing is outstanding and
can only be written truthfully once everything else has landed. If any section above is
incomplete, this section records what is true rather than a discharge — slot 7a's B.2 is
the precedent and the reason it exists.

- [x] 11.1 **Advance every matrix row to a terminal status** with probe evidence for
  every non-migrating resolution, and confirm `make check-workflow-boundaries` passes
  **truthfully** rather than because a claim was weakened.
- [x] 11.2 **Update the slot ledger and the remaining-scope table together**, and close
  the `slot 7b` line. Delete nothing from the ledger's grammar section: the `(partial)`
  rules and the four failure conditions stay, because they are what makes "complete"
  checkable — and after 8.5 they are joined by whatever the parsing-path audit adds.
  - [x] 11.2a **Correct the 7b artifact cell.** The remaining-scope row at
    `docs/next/workflow-readability.md:932` records this change's artifacts as
    *"proposal + tasks + 2 spec deltas"*; it also ships a **`design.md`**, which §D1's
    precedent says a slot carrying a genuine structural question needs. A record that
    under-lists a change's artifacts is the same drift class as an under-listed file set.
  - [x] 11.2b **Carry 0.4b's disposition into both documents** — the ledger line at
    `:1231` and the matrix row — in the same edit, so no intermediate state exists where
    one names a row the other has removed.
- [x] 11.3 **Replace the matrix's facade table** with every migrated facade re-measured
  in this change — the current table lists **eleven** where **sixteen** rows are
  `migrated`, omitting all five of slot 7a's — and **re-base its prose**, which still
  reads *"only two workflows are migrated today"* and *"slot 3 must plan against 1
  line"*. Verb: **re-measure**, not confirm.
- [x] 11.4 **Write the programme completion section** in
  `docs/next/workflow-readability.md` per §E6: measured outcomes against the section 2
  baseline in the same delta-table shape prior slots used; the refreshed
  `Measurement Definitions` denominators, which are the programme's actual ratchet; the
  **single deferral inventory**; and the explicit statement of what is **not** claimed.
  The inventory must carry, each with its gating condition and owner: the **nine** open
  `[~]` items stated together with the re-derived **35 / 18 / 7 / 8 / 2** reconciliation; the two
  programme-level deferrals with their justification bars; `scan_execution.rs`'s ~2,000
  production lines; `ui/plain_disposal.rs`'s 1,344; slot 6's conditionally-cleared
  `minimap_work_pending` **standing condition**; slot 5b's unresolved candidates; the
  two ratchet rows' residue after task 9.6; and everything tasks 5.7, 7.7, and 8.1 hand
  to a `docs/next/` record.
- [x] 11.5a **Rewrite the stale `WFR-SHELL-LAYOUT` owner pointers in production source
  and docs**, which the row's retirement invalidates and which **no gate can find**
  because they are prose in doc comments. Measured at authoring: **nine sites across
  seven files** — `ui/sidebar/mod.rs:25`, `:68`, `:85`; `ui/sidebar/AGENTS.md:21`,
  `:43`; `ui/sidebar/width_preset.rs:9`; `ui/sidebar/filter_execution.rs:20`;
  `ui/editor_page/restore_position.rs:14`; `docs/automation-reference.md:681`. Each names
  the retired row as an **owner** — of the width preset, of the sidebar show/hide
  animation, of the `workspace-sidebar-animation` readiness blocker, of the open/active
  file-row projection — so leaving them behind points a reader at a row that no longer
  exists, from files the seven replacement rows now own. **Re-derive the set** rather
  than trusting this list, and rewrite each to its new owner. Slot 5b's
  accessibility-policy lesson is the same shape: a module doc naming a thing that has
  moved away is a real finding, and the fix is to name the owning module.
- [x] 11.5 **Correct every stale pointer in both documents** (§E6 component 5): the
  record's status-line count (**"eleven"** against its own baseline's **16**), its
  slot-6-terminated migrated-row list, *"Slots 5 through 7 remain authorable"*, the
  `| 5–7 | not yet authored |` change-name row, §3's *"Three are complete"* preamble,
  §7's parenthetical calling `WFR-MINIMAP` and `WFR-MARKDOWN-PREVIEW` deferred, and the
  coverage proof's 198. A closeout is the last moment anyone reads these with the whole
  programme in view.
- [x] 11.6 **Record the convention and tooling friction this change hit**, each entry hit
  while *using* the thing rather than reading it, in the programme record's friction
  section — including the class Finding 6 belongs to: a review pass whose findings reach
  no artifact is a worse handoff than one that reaches an archived directory, and the
  fix is a durable home, not a better memory.
- [x] 11.7 **Re-derive the dangling evidence-pointer set.** The matrix's finding G8
  records **twelve** dangling `mutation parity` pointers across four archived changes and
  that *"the gate cannot catch this"*. Re-derive the current set rather than inheriting
  twelve or the inherited "seven", fix each, and decide whether the gate can be made to
  catch it — if it can, that is a mechanical half worth landing in the change that
  closes the programme.
- [x] 11.8 **Correct earlier handoff text** so a later reader does not re-plan a non-item
  or inherit a wrong pointer: slot 7a's proposal says *"five self-test keys"* in one
  place and six in another (**six** is correct); its narrowing instruction for
  `DisposalPressureEvidence` is unexecutable as written (§E4); its A.11/A.14/A.15
  appendix sections read as though no facade was written while its own header reports
  five; its A.6/A.13a pair contradicts itself about the accessibility gate (task 8.7);
  its B.6 and A.12 contradict each other about mutation triage (task 9.4a); its §D1
  record's *"three stories"* verdict for `focus_indexing.rs` is wrong in three ways
  (Finding 1b); its ratchet row for the automation reach-throughs is closed by line and
  open by expression (task 6.4); and two line pointers have drifted — its `design.md:110`
  cites `ui/window/mod.rs` at **257 physical with 113 lines of headroom** where it now
  measures **269/101**, and its A.4 cites the shell trace at `:157`–`:256` where it now
  sits at `:169`–`:268`.
- [x] 11.9 **State what is terminal, on what grounds, and what is not claimed**, per row,
  in B.6's shape — and end with whether anything is recorded as accepted debt. Nothing
  may be.
- [x] 11.10 **At archive time**, rewrite this change's evidence pointers from live form
  to archive form — the step five prior changes missed. Until then they stay in live
  form, which is the only form that passes the gate while the change is live.
  **Discharged at archive (2026-09-14):** a repository-wide search found **zero**
  path-formed pointers (`openspec/changes/close-workflow-readability-programme/...`)
  outside this change's own directory; the matrix and the programme record refer to
  the change by **name** only, which is archive-stable. Nothing to rewrite.
- [x] 11.12 **Discharge the ledger's "remaining argument suppressions" item**, which the
  matrix's slot-7b row (`docs/workflow-readability-matrix.md:1304`) lists as this
  change's scope. Measured at authoring: **one** exists, at
  `model/action_catalog.rs:178` — the exempt domain catalog constructor, whose parameters
  each name a documented external contract field, which `.agents/rules/rust.md` places
  **outside** the seam rule; and **zero** `#[allow]` of the lint exist anywhere. The item
  is therefore discharged by **recording** that the single instance is the sanctioned
  exception, not by removing it. Confirm the count, and remove the item from the ledger
  row rather than leaving it to read as unfinished work.
- [x] 11.11 **Run the repository-learning review** and land whatever durable guidance
  this change earned in `.agents/rules/*`, `AGENTS.md`, `README.md`, and the affected
  skills' references. `.agents/rules/documentation.md` requires the matrix and the
  programme record to advance in the same change as the code; this task covers the rest.

---

## Appendix A — orientation record

Each section is filled by the task named in its heading. An appendix section left empty
means its task did not run.

### A.1 Delta re-basing against the live specs (task 0.2)

Both relocated deltas were diffed against the live requirement of the same title,
after slot 7a's delta 3 sync landed (`4c39c58a`). Both remain **strict supersets**:

| Delta | Live requirement | Added | Modified | Removed | Scenarios |
| --- | --- | --- | --- | --- | --- |
| `workflow-readability-boundaries` | *Every workflow is enumerated before any workflow is migrated* (`spec.md:6`–`:79`) | **+107** | 0 | 0 | 7 -> 16 (**+9**) |
| `workflow-evidence-surfaces` | *A migrated workflow exposes one typed evidence surface* (`spec.md:52`–`:173`) | **+45** | 0 | 0 | 12 -> 15 (**+3**) |

Predicate: counts are `diff` output lines, excluding each requirement block's
terminating blank separator. Including it gives **+108** and **+46**, which is the
authored figure — the one-line difference is the separator and nothing else, and
it is stated because two honest counts differing by one is otherwise read as an
error.

Neither delta quotes any text the delta 3 sync touched: delta 3 landed in
`openspec/specs/mutation-testing/spec.md`, which neither of these requirements
references.

> **SUPERSEDED — both deltas were landed.** The paragraph below was written at
> the released stopping point and is kept so the sequence stays legible.

~~**Both deltas remain UNLANDED and that is deliberate** — see B.0.~~ Slot 7a's rule
(its task 0.14a) is that a delta must not ship in a change that cannot discharge
its obligation. Delta 1 requires every row terminal; delta 2's own scenario
assigns its discharge to the change that closes the programme. This change closed
neither, so landing them would assert obligations nothing discharges. Task 0.13a's
resolution **(i)** is taken: both files stay withheld for the continuation slot.

**What actually happened:** no split was taken, every row reached a terminal
status, and **both delta files were landed** into their live specs — the
`+107 / 0 / 0` and `+45 / 0 / 0` supersets recorded above, applied verbatim.
`openspec validate --all --strict` passes **111 / 0** after. Task 0.13a needed no
clause allocation, because no boundary was crossed. Delta 1 was then **extended
once more inside this change** (B1 of the review): `superseded` was added to its
terminal-status set with a counterpart obligation — a superseded row must name
its replacements and the gate must verify each exists — because shipping the gate
half without the spec half would have left the matrix carrying a status its own
capability did not define. **Final applied sizes, measured against `69f78b09`
rather than restated from the authored delta**: `openspec/specs/workflow-readability-boundaries/spec.md` **+129 / -0** (the +108 superset plus B1's +21),
and `openspec/specs/workflow-evidence-surfaces/spec.md` **+46 / -0**, unchanged
from its authored size. Neither spec lost a line.


### A.2 Criterion-1 evidence, per replacement row (tasks 0.3, 0.4)

> **SUPERSEDED — all seven rows were carried to implementation.** The sentence
> below was written at the released stopping point.

~~Only `WFR-TRANSIENT-DISMISSAL` was carried to implementation, so only its
criterion-1 evidence is recorded as confirmed.~~

**Criterion 1 holds for all seven, and the three pairs 0.3a names were tested
explicitly:**

- **geometry versus transient dismissal** — both react to window-level state, but
  their stage sequences share no step: geometry is *read requested state and live
  width -> derive presentation -> apply -> settle -> persist*; dismissal is
  *observe every dismissible surface -> decide -> apply exactly one dismissal*.
  Neither reads the other's inputs, and no entry point of one reaches the other.
- **tab strip versus recent documents** — both can end in an open document, but
  that is a *shared callee* (`open_document`), not a shared sequence. The tab
  strip's sequence starts from a menu target it resolves itself; recent documents
  starts from a durable record it recovers and projects.
- **Focus Mode versus geometry** — both suppress chrome, and this is the closest
  pair. They are separate because Focus Mode *borrows and returns* named chrome
  state (`FocusModeEntry` / `FocusModeRestore`) while geometry *derives* the
  presentation from width and requested visibility every time. Focus Mode calls
  geometry's result; geometry never consults Focus Mode.

**0.3b — `WFR-TAB-STRIP`'s three paths share the close stage order**, confirmed on
the code: pin and the two reorders complete in one turn inside the facade, while
"Close Others" and "Close to the Right" both resolve eligible targets and hand
them to the *same* `confirm_and_close_tab_pages`. They are one row because they
share one sequence, not because they are near each other.

**0.3c — `WFR-RECENT-DOCUMENTS`'s two stage orders are one workflow's**: recover
-> project -> present -> filter -> act is one sequence whose first stage is the
journal's. The two orders share the record, the projection call, and the
generation that guards it. It was the declared budget stressor and came in at
**252** in the final re-measurement (**239** when first recorded, before the
module docs were finished), under both the budget and its own ≈250 projection.

**0.3a-ii thin-row test, per row** (before -> after production lines, re-derived
after the review pass): transient dismissal 202 -> **566**, focus mode 354 ->
**754**, eviction 407 -> **801**, recent documents -> **2,069** across its two
homes, geometry -> **1,176** in the role home, tab strip 634 + the
`documents.rs` close half -> **958**. Three of these grew again during the review
pass — eviction by 28 when its evidence surface's transitive-disposal reasoning
was written down, geometry by 127, and recent documents by 53 — which is why they
are stated as re-derived rather than as carried forward. **Every row grew**, and the growth is
justified on the same three grounds A.2 records for the first: pure decision logic
became testable and entered the mutation scope, observation seams collapsed into
one typed surface, and the module docs the convention requires are new text. No
row grew without gaining coverage it did not have.

**`WFR-TRANSIENT-DISMISSAL` satisfies criterion 1** as *a family of operations
sharing one ordered stage sequence*. The family is two gestures — Escape and a
primary pointer press outside the palette — and they share one sequence:
**observe every dismissible surface -> decide -> apply exactly one dismissal**.
They are not two workflows: both resolve the same question against the same
observation, and the pointer path's verdict is the ladder's palette arm with a
containment test in front of it. §D1's rejection of this file as a tier-list entry
is confirmed on the code: it holds a strictly ordered ladder (five surfaces, order
load-bearing) **and** a one-tick idle latch, so the tier list's "no ordered stages"
preamble is false for it.

**Thin-row test (0.3a-ii).** The row grew: **202 -> 691 physical / 566
production** lines across 4 files.
Growth is the expected shape here and is justified rather than waved through:

- The ladder's order was previously expressible only by reading five `if` blocks
  interleaved with widget calls. It is now a pure function with **7 unit tests**,
  inside the `ui/**/policy.rs` mutation scope — the file had **zero** mutation
  coverage before, because `ui/window/transient_surfaces.rs` is not in
  `examine_globs`.
- The five-boolean observation became a named seam value object. Five same-typed
  booleans is the archetype case for a value renamed while crossing a seam, and a
  swapped `primary_menu_active`/`notes_menu_active` pair was invisible to every
  existing test — none of which reached the two menu arms at all.
- Two pre-existing `*_for_test` actuation seams are now complemented by an
  evidence surface that lets a test read ladder state **without** actuating it.

Net production growth excluding tests and doc comments is far smaller than
691-202 suggests; the dominant additions are the module docs the convention
requires and the policy unit tests. The measurement is recorded because 0.3a-ii
asks for it, and the growth is accepted for the three reasons above.


### A.2a `focus_indexing.rs`'s four stories, re-derived, and focus restoration's owner (task 0.4a)

**The four-story decomposition is CONFIRMED.** The inherited *"three stories"*
verdict is wrong in the three ways Finding 1b states, and re-derivation adds a
fourth correction the proposal did not anticipate.

Every function line number the proposal cites was verified exact (`:56`, `:79`,
`:108`, `:128`, `:163`, `:190`, `:209`, `:218`, `:229`, `:237`, `:255`, `:304`,
`:325`, `:362`, `:380`, `:393`, `:407`, `:667`, `:746`, `:753`, `:770`, `:807`,
`:817`, `:843`, `:852`). The **size figures** are the cells that needed correction:

| Story | Measured range(s) | Production lines | Proposal's figure | Correction |
| --- | --- | --- | --- | --- |
| Editor-memory eviction | `:56`–`:189`, `:380`–`:666` | **407** | ~406 | confirmed (+1) |
| Focus restoration | `:255`–`:379` | **125** | ~129 | -4 |
| Palette overlay control | `:190`–`:254`, `:807`–`:849` | **108** | ~104 | +4 |
| Palette file-index build | `:667`–`:806`, `:852`–`:856` | **145** | ~171 | **-26** |

Predicate: physical lines within each story's ranges, minus whole
`#[cfg(feature = "test-utils")]`-gated **function** blocks (two, at `:630`–`:636`
and `:639`–`:645`, 14 lines). Inline `test-utils` attributes inside a production
function body (`:451`, `:485`, `:537`) are production control flow and are
counted. The four stories plus the file's 51-line import/`impl` header and its
inter-function blank lines reconcile to the file's 856 physical lines.

The inherited **~590** for eviction is refuted: it absorbs focus restoration's 125
lines, which sit between eviction's two ranges. The proposal's own range endpoints
(`:56`–`:190`, `:380`–`:667`) are each one line long at the upper bound — they name
the *next* story's opening line — but its **~406** is right to within one line.

**0.4a-iii — focus restoration contains no geometry code, and it is not one story
either.** This is the fifth correction, and it is why the ownership question had no
single answer. The four functions have three unrelated caller families:

| Function | Lines | Callers | Owner |
| --- | --- | --- | --- |
| `restore_saved_focus` (`:362`) | 18 | **one**, `close_command_palette` — it is the read half of `saved_focus`, whose write half is `toggle_command_palette` | **`WFR-COMMAND-PALETTE`**, with the palette overlay stage order |
| `restore_focus_after_secondary_pane_close` (`:304`) | 21 | `imp.rs` `sync_secondary_surfaces` | **`WFR-SHELL-GEOMETRY`** |
| `restore_focus_after_breakpoint_collapse` (`:325`) | 37 | `imp.rs` breakpoint layout switch | **`WFR-SHELL-GEOMETRY`** |
| `focus_selected_editor_after_action` (`:255`) | 49 | `actions.rs:24` (`win.new-tab`), `actions.rs:414` (`select_tab_by_index`), `recent_open.rs:59`, `recent_open.rs:77` | **cross-cutting** — shared by three owning workflows |

No function in the range reads a paned position, an allocation, a breakpoint
condition, or a GSettings width key. Two are geometry-*triggered*, which is a
caller relationship; and assigning the whole story to the geometry row on that
basis would have mis-homed the other two while every gate exited 0.

**Decision.** The two pane/breakpoint restorers go to `WFR-SHELL-GEOMETRY` as a
coordination role, argued **on behavior** rather than on the caller relationship:
`.agents/rules/ui.md` states focus return as part of the split-view contract
(*"When a utility pane closes, return focus to the active editor rather than
leaving focus stranded on a toggle button"*), so the geometry sequence is
incomplete without its final stage. Each has exactly one caller and exists only
for a geometry transition. `restore_saved_focus` goes to `WFR-COMMAND-PALETTE`.
`focus_selected_editor_after_action` is recorded **cross-cutting**, staying in a
shared location with the matrix naming its three owning workflows — the same
disposition `.agents/rules/rust.md` gives cross-cutting pure policy, and the same
shape as `startup_data.rs`.

**No eighth row is created.** Giving focus restoration its own row was the
available third option and is rejected: the design's constraint is that a trace
supporting more rows is a signal to re-read the trace, and re-reading it dissolved
the story into three existing owners rather than producing a workflow.

> **SUPERSEDED — the decision was implemented in this change.**

~~This decision is recorded but NOT implemented — see B.0.~~

**As implemented:** the two pane/breakpoint restorers moved into
`ui/window/geometry/mod.rs` as the sequence's final stage, with the split-view
focus-return contract quoted at the call site; `restore_saved_focus` stayed with
the palette overlay in `ui/window/palette_shell.rs` (renamed from
`focus_indexing.rs`), because it is the read half of `saved_focus` whose write
half is `toggle_command_palette` in the same file; and
`focus_selected_editor_after_action` became cross-cutting
`ui/window/editor_focus.rs`, whose module doc names its three owning workflows in
a table. No eighth row was created.


### A.2b The retired row's lawful disposition (task 0.4b)

**Resolution 0.4b-i was taken: a terminal `superseded` label was added to the
status vocabulary**, and the gate now accepts it as terminal.

0.4b-ii was available and is rejected on the evidence rather than on taste.
Every existing label misdescribes a **replaced** row: `exempt` and
`cross-cutting` both assert the code is deliberately unmigrated, which is the
opposite of what happened; `deferred` is transitional and would have left the
row non-terminal at programme close; `partially-conforming` claims some roles
exist, and the row has none; and `migrated` would be a false claim outright.

What landed, in one change so no intermediate state exists where the ledger and
the matrix disagree:

- `SUPERSEDED_STATUS` added to `KNOWN_STATUS_LABELS`, `SETTLED_STATUSES`, and
  `TERMINAL_NON_MIGRATING_STATUSES` in `scripts/check-workflow-boundaries.py`,
  each with a stated reason at the definition;
- a **self-test arm with two halves** — a `superseded` row with slot `none`
  must pass, and a *misspelled* terminal label must still be a finding, so the
  label stays checked rather than becoming a hole;
- the arm proved by **deliberate red**: with `SUPERSEDED_STATUS` removed from
  `KNOWN_STATUS_LABELS`, the arm failed with
  *"row WFR-REPLACED has status `superseded`, whose label ... is not one of
  pending, migrated, partially-conforming, exempt, deferred, cross-cutting"*;
- the label documented in the matrix's `Status Labels` section, including the
  rule that a superseded row is **retained, never deleted**;
- the ledger line at `docs/next/workflow-readability.md` edited in the same
  change, moving `WFR-SHELL-LAYOUT` onto slot 7b's **complete** line beside its
  seven replacements.

### A.3 The four reassignments and the cells they staled (tasks 0.5, 5.1–5.4)

| # | Subject | Verdict | Cell corrected |
| --- | --- | --- | --- |
| 0.5a | `ui/window/dialogs.rs` | **Five ordered stage orders across three workflows**, so the module takes no single role name and is classified per stage order in its own module doc: the open-file chooser (called presentation surface of `WFR-DOCUMENT-LOAD`), the Save As chooser plus `complete_save_as` (called presentation surface of `WFR-DOCUMENT-SAVE`), the discard-changes dialog plus `clear_close_discard_drafts` (called presentation surface of `WFR-DRAFT-RECOVERY`), the save-changes/close-save pipeline (**coordination** of `WFR-DOCUMENT-SAVE`), and async close safety (**coordination** of `WFR-DRAFT-RECOVERY`). Its three unrecorded freshness/identity values are named: two are already in sanctioned shapes (`CloseSafetyEditorFingerprint` + `close_discard_fingerprints_are_current` is Ticket + Facts + predicate; the close-save session identity is a coordinator generation identity, which the rule says *is* the seam value object) and are **recorded rather than re-reified**; `CloseSavePipeline` crosses one boundary and is reconstructed nowhere, so the seam rule does not require more | `WFR-DOCUMENT-SAVE`, `WFR-DRAFT-RECOVERY` |
| 0.5b | `ui/window/search.rs` (955 -> 829) | Holds `WFR-SEARCH-REPLACE`'s window side as a **called presentation surface**, plus **one coordination job of its own** — search progress, a third ordered stage sequence — extracted to `ui/window/search_progress_execution.rs` (`execution`, stage-order-qualified beside the row's existing `execution.rs` and `replace_execution.rs`). It lives in `ui/window/` because what it coordinates is the **status lane**, which the window owns and the panel does not | `WFR-SEARCH-REPLACE`'s size cell, which said the row's files were *"all under `ui/search_panel/**`"* — **false by 928 production lines** |
| 0.5c | the retired `focus_indexing.rs` | **The cell was wrong, not the reassignment.** It said the file *"stays window code"*; two of `WFR-COMMAND-PALETTE`'s ordered stage orders live there. The file is renamed `ui/window/palette_shell.rs` and classified as that row's **called presentation surface** — not a coordination role, because the `admission`/`execution`/`retirement` roles for the index already exist at the row's canonical home and this is the window-side target resolution they are reached from | `WFR-COMMAND-PALETTE` |
| 0.5d | `setup_theme_selector` (~100 lines) | **Tier list**, not a geometry stage. The check is concrete rather than definitional: it reads no paned position, no allocation, no breakpoint condition, and no width key, and nothing in the geometry sequence calls it. It is a `gio::Settings` -> `libadwaita::StyleManager` projection installed once plus a dark-notify handler | the no-coordination-tier list, now populated per surface with evidence for all three properties its preamble claims |
| 0.5e | receiving rows' staled cells | Re-derived in this change rather than marked stale, because delta 1(a)'s cross-row staling statement **is this change's own delta**, so the "record that they are stale" escape it grants is not available here | all four above |

### A.4 Role-home selections and their collision analysis (task 0.7)

`ui/window/policy.rs` and `ui/window/mod.rs` are both unavailable as flat role
names for a new row: the first was occupied by the adaptive shell geometry
workflow (and is now `ui/window/geometry/policy.rs`), and the second is the
window's own module root. **Every new row in `ui/window/` therefore took a
per-workflow subdirectory**, which is also what keeps the choice legible rather
than first-come.

| Row | Role home | Collision analysis |
| --- | --- | --- |
| `WFR-TRANSIENT-DISMISSAL` | per-workflow subdirectory `ui/window/transient_dismissal/` | flat names unavailable |
| `WFR-FOCUS-MODE` | per-workflow subdirectory `ui/window/focus_mode/` | flat names unavailable |
| `WFR-EDITOR-MEMORY-EVICTION` | per-workflow subdirectory `ui/window/editor_memory_eviction/` | flat names unavailable |
| `WFR-SHELL-GEOMETRY` | per-workflow subdirectory `ui/window/geometry/` | it **vacated** the flat `ui/window/policy.rs` it had held, which is why `WFR-PRINT`'s note about that name being "taken" is corrected in the same change |
| `WFR-TAB-STRIP` | per-workflow subdirectory `ui/window/tab_strip/` | flat names unavailable |
| `WFR-RECENT-DOCUMENTS` | **nested**: canonical `ui/open_popover/`, window half nested in `ui/window/` | confirmed as 0.7 directed. `ui/open_popover/` hosts one workflow, so the flat `policy.rs` / `evidence.rs` names are free there. The nested module takes a **bounded coordination role name** — `ui/window/recent_documents_journal.rs` (`journal`) — because it maintains a durable, generation-guarded record a later stage reads back, which is that role's definition; the remaining `ui/window/recent_open.rs` is a **called presentation surface** |
| `WFR-STARTUP-PREFLIGHT` | **none** — cross-cutting, no facade, no role home | |

### A.5 Behavior anchors quoted, with the rules file each is in (task 0.14)

Quoted **before** any geometry code moved, each with the file it is in, because
slot 7a's A.5 records its own task list naming the wrong file for two contracts.
Every anchor below is reproduced verbatim in behaviour and most are now quoted
in the owning module's doc so a later reader meets the rule where the code is.

**From `.agents/rules/ui.md`:**

- the **Split-View Rules**: `AdwOverlaySplitView` for the outer workspace shell
  and `AdwMultiLayoutView` for the adaptive properties surface; no manual
  rehosting with `set_sidebar(None)`; the left pane restores one of the
  Preferences presets then clamps it to the active desktop width; breakpoints
  switch the properties layout **before** collapsing the workspace pane;
- the **`ClipBin` zero-minimum-height contract** — the flexible region reports a
  zero minimum and clips, so the status bar can still be allocated inside the
  visible height;
- the **width-preset identities** (`20%`, `30%`, `40%`) and their clamp to a
  comfortable desktop range, with the window layer owning the math;
- the **allocation-time rule**: paths clamp and cache, and **never** persist
  GSettings or reparse an `AdwBreakpoint` condition. Quoted into
  `geometry/execution.rs`'s module doc and made **assertable** for the first
  time by the evidence surface's `persisted_*` fields;
- the compact **`AdwBottomSheet` bounded-natural-height contract**.

**From `.agents/rules/widget-wiring.md`:**

- the **GtkPaned Position Constraints** in full — restore then pre-clamp, the
  hidden-restore collapsed endpoint, per-frame animation clamping, the
  `max(measure(Horizontal, -1), measure(Horizontal, current_height))` floor,
  clamping against the real end-child, the revealer wrapper for zero-width
  panes, and hide-time clamps staying live until the wrapper is hidden;
- **arming a `SettleBurst` before setting an animated property**, because
  GTK/Libadwaita notify signals can run synchronously from the setter. Quoted
  into `geometry/mod.rs`'s inversion section;
- the **transient-surface dismissal order**: Bubble phase, exactly one topmost
  surface per Escape, Focus Mode after transient dismissal, palette click-away
  through `close_command_palette()`. Quoted into `transient_dismissal/mod.rs`;
- the **focus-restoration-on-overlay-close** contract, which is why
  `restore_saved_focus` stayed with the palette overlay rather than moving with
  the geometry restorers.

### A.6 Retroactive re-check results for both deltas (tasks 1.3–1.6)

| Clause | Question | Result |
| --- | --- | --- |
| **1(a)** cross-row staling | did any *migrated* row inherit files assigned without re-deriving its cells, in slot 3b's `ui/open_popover/**` shape? | **Yes — two, and both were corrected here.** `WFR-SEARCH-REPLACE` claimed its files were "all under `ui/search_panel/**`", false by 928 production lines; `WFR-COMMAND-PALETTE` said the window-side palette code "stays window code" while two of its ordered stage orders lived there. The shape **did** recur in migrated rows, which is the finding — the known instance was assumed to be the only one |
| **1(b)** terminal status | does any status label carry a trailing narrative contradicting it? does any non-migrating terminal row lack probe evidence? | No label contradicts its narrative after the sweep. **Two rows lacked probe evidence and now record it**: `WFR-EDITOR-MEMORY` (`exempt`) — the module is entirely pure already, has 7 consumer files across 5 workflows, and is already in the `model/**` mutation scope, so the convention would add nothing; `WFR-MIGRATION-LEDGER` (`cross-cutting`) — 5 consumer files across three areas, no separable decision belonging to one workflow. Both confirmed on measurement rather than inherited |
| **1(c)** provisional groupings | is `WFR-SHELL-LAYOUT` the only residual grouping row? | **Yes.** Tested against every row whose workflow name spans more than one surface family. `WFR-COMMAND-PALETTE` ("palette, file index, notes browse modes") and `WFR-NOTES-BOOKMARKS` ("notes, bookmarks, and the rename-driven sidecar migration") both name several families but each is **one** ordered stage sequence with one entry family, which is the criterion; neither is a grouping. Negative findings recorded so the test is not re-run from scratch |
| **2** | does any migrated row expose a second typed observation path, or an evidence type wider than its readers need? | **No, and all three named `pub` candidates were measured rather than assumed.** `DisposalPressureEvidence`: `pub` under a `test-utils` gate is the narrowest that compiles for a cross-crate reader — **already narrowest**, and slot 7a's narrowing instruction is unexecutable as written. `WorkspaceScanPressureEvidence`: **not** a second path — it is a named **component** of `WorkspaceTreeEvidence`, and its producer `child_scan_pressure_evidence` is `pub(crate)` with exactly one caller, the surface itself. `NoteScoringEquivalenceEvidence`: gated `cfg(any(test, feature = "property-tests"))`, read by `crates/lushtext-core/tests/properties/palette.rs` — a different target in the same crate, so `pub` is required — and it is a service-level equivalence-proof value, not a workflow surface. Its dual gate is **correct**, unlike the two in `content_search/replace.rs` this change fixed, because it genuinely has consumers under both |

### A.6a The ledger/slot-column parsing fail-opens, and their deliberate reds (task 8.5)

The inherited framing — *"re-derive whether three holes exist"* — is refuted: all
three reproduce, **a fourth was found by the sweep**, and **a fifth was introduced
into view by fixing the first two**. All five are the same class: a step that
quietly succeeds against the wrong input, in the gate the closeout's central claim
depends on.

Each fix carries a self-test arm, and each arm was proved a **genuine deliberate
red** by disabling only that fix and observing the arm fail:

| # | Hole | Fix | Deliberate red (fix disabled) |
| --- | --- | --- | --- |
| 1 | A reworded `Slot` header leaves `slot_index = None`, so every row parses with no slot, the outstanding-slot sweep is skipped, and the gate exits 0 | `parse_matrix_rows` now returns whether the column was located; `record_findings` reports the skip. Reported at the **consumer**, not the parse, because a Slot-less fixture matrix reconciled against no record is legitimate | `got []` — the renamed header produced **zero** findings |
| 2 | `SLOT_LEDGER_RE` requires the literal verb; a non-matching line is `continue`d, guarded only by the all-lines-failed case | `parse_slot_ledger` also returns lines matching the ledger's *shape* (new `SLOT_LEDGER_SHAPE_RE`) but not its vocabulary | fired the **wrong** finding — it blamed the matrix for an unlisted row instead of naming the typo'd line, which in the real record (where the row also appears elsewhere) would be fully silent |
| 3 | A `Slot` cell of `none` exempts a row from the outstanding-slot rule; nothing distinguished "never migrated" from "has work and no slot" | `none` is now a finding unless the row's status is terminal | `got []` |
| 4 | **Found by the sweep.** `declared_facade_path` required *exactly one* backticked token, but the established `facade:` line names its measured size and its pre-convention file — so the second token made the whole facade-budget rule inert for that row | the claim is the **first** token, and only when it looks like a Rust source path | `got []` — an over-budget annotated facade produced zero findings |
| 5 | **Introduced into view by fixes 1–2.** `record_findings`' empty-ledger branch `return`ed a **fresh list**, discarding every finding accumulated before it | the branch appends and returns the accumulated list | caught during implementation: fixes 1 and 2 appeared to do nothing on an empty-ledger fixture |

**Hole 4 was live, and it was not hypothetical.** Measured on the current matrix:
**3 of 16** migrated rows had their facade budget silently unchecked —
`WFR-MARKDOWN-PREVIEW`, `WFR-EDITOR-FIND`, `WFR-ENCODING`. After the fix all 16
resolve. The rule still passes, because all three are under budget — but two of
the three recorded figures were **stale**, which is what an unchecked rule permits:

| Row | Matrix claimed | Measured | Corrected in this change |
| --- | --- | --- | --- |
| `WFR-EDITOR-FIND` | 238 | **229** | yes |
| `WFR-MARKDOWN-PREVIEW` | 270 | **244** | yes |
| `WFR-ENCODING` | 155 | 155 | unchanged — a legitimate outcome |

`make check-workflow-boundaries` passes on the live tree after all five fixes, and
its `--self-test` passes with the four new arms.


### A.7 The gate disarm observed, and the re-key that closed it (tasks 2.1–2.5)

**The disarm was observed before it was fixed**, which is the only step that
proves the re-key was necessary rather than decorative. With the geometry code
moved into `ui/window/geometry/` and **no key added**, both implementations were
evaluated against the moved paths:

| Predicate | Before the re-key | After |
| --- | --- | --- |
| `is_visual_sensitive` | `True` (so a visual proof is still required) | `True` |
| `required_invariants_for_changes` | **`[]`** | `["native-minimap-highlight-anchors"]` |
| `required_animation_invariants_for_changes` | **`[]`** | `["native-minimap-animation-highlight-anchors"]` |
| `workspace_sidebar_animation_matrix_required` | **`False`** | `True` |

So both named pixel invariants and the sidebar animation matrix were unprotected
while the gate **exited 0**. A path-keyed gate that matches nothing does not
fail; it passes while protecting nothing.

**The key added is a role-home prefix**, in both implementations:
`SHELL_GEOMETRY_ROLE_HOME_PREFIX = "crates/lushtext-core/src/ui/window/geometry/"`,
in `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`.
A prefix rather than three literals so a later split inside the role home cannot
disarm it again. A `crates/lushtext-core/src/ui/window/` prefix was **rejected**:
it would sweep in seven subdirectories, four of them migrated role homes no
predicate has ever protected — a scope change argued on a rename.

**`actions.rs` and `imp.rs` were retained as keys**, argued on behaviour rather
than on the rename: `actions.rs` keeps the two toggle action bodies that persist
user intent, and `imp.rs` keeps the `size_allocate` vfunc and its `constructed()`
wiring. Narrowing a key because *some* of a file's content moved is a scope
change.

**Parity assertions on both sides, each proved by a deliberate red.** Three
assertions per side: the role home requires both invariants and the matrix; the
re-key does **not** reach four named sibling role homes; and the retained
literals still protect their files.

- Rust half: `cargo test -p cargo-gtk-proof --lib` failed with
  `left: [] right: ["native-minimap-highlight-anchors"]` before the key,
  and passed after.
- Python half: `scripts/check-visual-proof-policy.py --self-test` failed with
  *"the geometry role home must require the native minimap highlight
  invariant"* before the key, and passed after.

Both halves' self-tests were confirmed to **actually run** — slot 6 found the
Python entry point delegating to the Rust binary before its own tests, and the
CLI now runs the Python self-tests before delegating.

**Task 2.4 — the mutation glob still reaches the moved `policy.rs`, verified
after the move rather than assumed.** `ui/**/policy.rs` reaches
`ui/window/geometry/policy.rs` by convention, and the count was read from the
tool at **both** paths: **81 before** (`make mutants-list` in a clean worktree at
`69f78b09`) and **81 after**. Reported as **relocation parity**, not gain; the
module's gain-from-zero was slot 7a's. The matrix's recorded **80** was stale by
one and is corrected.

### A.8 The disposal lane's surface: shape, readers, visibility (tasks 0.8, 3.2, 3.3)

**The design's §E4 premise is refuted by measurement, and the correction changes
the shape of the consolidation.**

§E4 states *three* parallel typed observation values reached through *five*
accessors, partitioning the row's 8 gated declarations as 5 observation + 3
actuation. Measured at implementation, `aggregate_pressure_evidence_for_test`
(`:1145`) is **not an observation accessor**. It is a 185-line fixture driver: it
spawns worker threads, blocks on a `Condvar`, saturates the lane, drives GTK
heartbeats, and contains ten `assert!`/`assert_eq!` calls, panicking when the
fixture violates an invariant. It *runs a scenario* and returns what it observed.

So the row's 8 declarations partition as **4 observation + 4 actuation**:

| Kind | Declarations |
| --- | --- |
| Observation (retired into the surface) | `limits_for_test` `:1013`, `progress_limits_for_test` `:1020`, `lane_snapshot_for_test` `:1027`, `progress_lane_snapshot_for_test` `:1034` |
| Actuation (unchanged) | `hold_disposal_capacity_for_test` `:1057`, `fill_disposal_capacity_for_test` `:1077`, `hold_progress_disposal_capacity_for_test` `:1096`, **`aggregate_pressure_evidence_for_test` `:1145`** |

Folding the fourth actuator into the surface would have made *reading* the
surface spawn threads, mutate every counter the surface reports, and panic on
failure — breaking both the non-materialization and the reentrancy proofs. It is
excluded, and the exclusion is stated in the surface's own module doc so a later
reader does not re-propose it.

**Shape as implemented.** One surface, `PlainDisposalEvidence`, reached through
one accessor `plain_disposal_evidence()`, with the two lanes as **named
components** (`ordinary`, `progress`) of type `PlainDisposalLaneEvidence` rather
than two top-level accessors — the shape slot 7a used for
`BufferSnapshotEvidence`. `PlainDisposalLimits` and `PlainDisposalSnapshot` are
`model/plain_disposal.rs` **domain** types with production consumers; they are
**called, never moved or duplicated**, so the row's census resolution against
relocation stands and delta 2's no-forked-limit clause is satisfied by
construction.

The surface also introduces `is_quiesced()` on both types. That predicate existed
before only as an open-coded `running_jobs == 0 && queued_jobs == 0` re-derived at
each call site, so every proof carried its own definition of "quiet" and any one
could drift. Naming it once is what makes task 3.4's proof statable.

**Visibility (task 3.3): "already narrowest", recorded with its measurement.**
The reader set is **35 call sites across 5 files, every one in the `lushtext`
crate** — a different crate from `lushtext-core`:

| Accessor | Call sites |
| --- | --- |
| `lane_snapshot_for_test` | 24 |
| `progress_lane_snapshot_for_test` | 8 |
| `limits_for_test` | 1 |
| `progress_limits_for_test` | 1 |
| `aggregate_pressure_evidence_for_test` | 1 (actuation, unchanged) |

`pub` under a `test-utils` gate is the narrowest visibility that compiles for a
cross-crate reader. Slot 7a's instruction to narrow `DisposalPressureEvidence`
*from* `pub` is therefore **unexecutable as written** — executing it would break
the widget lane and call it a narrowing. Confirmed as authored.

**Task 3.1 — cells re-derived, both corrections downward:**

| Cell | Authored | Measured (baseline `69f78b09`) | After this change | After the review pass |
| --- | --- | --- | --- | --- |
| `ui/plain_disposal.rs` | 1,542 / ~1,344 | 1,542 / **1,343** | 1,614 / 1,415 | **1,621 / 1,422** |
| `model/plain_disposal.rs` | 692 / ~465 | 692 / **464** | unchanged | **711 / 483** |
| `*_for_test` declarations | 8 | 8 | **4** | 4 |
| `cfg(feature = "test-utils")` sites (ui) | 17 | 17 | 27 | 27 |
| `cfg(any(test, test-utils))` sites (ui) | 13 | 13 | 6 | **4** |
| either, in `model` half | 0 | 0 | 0 | 0 |

Predicate: physical lines minus the file's trailing `#[cfg(test)] mod tests`
block, counted from the `#[cfg(test)]` attribute line to EOF (`ui:1416`,
`model:465` at baseline). Production **grew** by 98 lines against the 1,807-line
baseline: the surface, its two component types, `is_quiesced()`, and their
documentation. That is the expected direction for a consolidation that adds one
typed surface while removing four accessor bodies, and it is recorded rather than
presented as a reduction.

**The last column is the point, not a formality.** Three of these cells moved
*again* during the review pass, and the `model` half moved after being recorded
as **unchanged**: S13 moved `is_quiesced()`'s definition into the domain
`PlainDisposalSnapshot` (+19 production lines there), and S12 collapsed two
`cfg(any(test, test-utils))` attributes that were stacked *dead* beneath a
narrower `test-utils` attribute (6 -> 4). A cell measured before the last edit is
a stale cell — the same failure mode this change records for three of its own
facades.


### A.9 The three surface proofs, and how the lane was quiesced (tasks 3.4–3.6)

All three discharged and green under the headless no-retry widget lane, with
**zero `FLAKY:` lines**.

**The quiesce mechanism, which did not exist before.** §E4 names this the
change's main proof hazard: the lane's counters are process-wide atomics advanced
by worker threads, so read-to-read identity is not a property of the reader's
control flow — a job admitted by an unrelated test in the same process can move
`running_jobs` between two reads a test believes are adjacent. The proofs share
one helper, `quiesce_lanes(budget)`, which drains to a terminal, pumps the GTK
main context, and confirms **through the surface itself** that both lanes report
zero running and zero queued. It returns whether quiescence was reached, so a
caller degrades rather than asserting something unsound.

| Proof | Discharge |
| --- | --- |
| **Reentrancy** (3.4) | Identity asserted **only** with the lane quiesced. Under an outstanding hold — where a worker can still advance a counter — the assertion is **monotonicity** (`admitted_jobs` never decreases) plus invariance of the fixed ceilings, not equality. This is exactly the distinction slot 7a's no-retry lane caught as an unsound assertion whose panic read like a production defect |
| **Disposal** (3.5) | A lane owns no `TemplateChild`, so the stage is named rather than skipped: **a torn-down owner whose pending reservation must not be stranded**. The proof takes a hold, observes the lane non-quiesced, drops the owner, and requires the surface to return to an honest quiesced answer |
| **Non-materialization** (3.6) | Read in both extremes. Empty: five successive reads must be byte-identical to the baseline. Saturated: both lanes' limits, `admitted_jobs`, all three high-water marks, overweight exclusivity, and replacement-headroom borrowing compared across two adjacent reads. This is the proof that would have failed had the pressure fixture been folded in — one read would have moved nearly every field asserted |

**Task 3.7 — no shared limit moved or forked.** Confirmed: the consolidation
touched accessors only. `MAX_REPLACE_UNDO_RETAINED_BYTES`
(`services/content_search/replace.rs:43`), `MARKDOWN_PLAN_RESERVATION_BYTES`, and
`PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY` (`ui/plain_disposal.rs:48`) are
unchanged in value and location.

**The confirmation surfaced a real defect and it was fixed in-stream.**
`ui/window/session_restore/journal.rs:33` reads
`use ...::PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY as STARTUP_PRELOAD_RESERVATION_BYTES;`
— **a value renamed while crossing a seam**, which is the archetype defect
`.agents/rules/rust.md` names, and it carried **no doc comment**. The rule permits
a one-line delegating alias under a second domain name only when the alias's own
doc comment names the owner and the contract. That doc comment was added, naming
the owner and stating the contract it carries (the startup preload reserves the
*entire* reserved lane, not a share of it). This also explains why the earlier
measurement could not find `STARTUP_PRELOAD_RESERVATION_BYTES` as a constant: it
never was one.

**Task 3.8 — the `DisposalProducer` family, premise confirmed, cause corrected.**
The dead items are real and reproduce, but not in the configuration the task
names. They are invisible to `cargo check` under *either* feature setting and
appear only in the **default-feature `--all-targets` (`cfg(test)`) build**:
6 warnings covering 10 items — `MAX_SMALL_PENDING_DISPOSAL_BYTES`,
`try_own_for_gtk`, `DisposalProducerInner`, `DisposalProducer` with its five
associated items, and `retry_pending`.

The cause is a **gate mismatch, not dead code**: the declarations were gated
`cfg(any(test, feature = "test-utils"))` while every one of their consumers is
gated `cfg(feature = "test-utils")` alone. Under plain `cfg(test)` they compiled
with zero users. Seven gates were narrowed to match their consumers (five
declarations, two `impl` blocks, and two imports the narrowing orphaned). Nothing
was deleted: `try_own_for_gtk` in particular has three live callers in
`ui/search_panel/journal.rs` and `ui/command_palette/mod.rs`, so retiring it —
which a file-scoped grep would have suggested — would have broken the build.
All ten items now resolve, and the lane's 16 unit tests plus every widget module
that reads the surface stay green.


### A.10 Data-safety pass: every candidate with its verdict, including cleared (§7)

Every candidate, **including the cleared ones** — a test that cannot fail is
worse than no test, so a cleared candidate is recorded with why it is clear.

| # | Row | Candidate | Verdict |
| --- | --- | --- | --- |
| 7.1 | `WFR-PLAIN-DISPOSAL` (tier-3) | does every retirement path carry the permit forward or release it? | **Clear, with the mechanism named.** The **panic path**: the worker wraps `job.run()` in `catch_unwind` and still calls `admission.finish(active, panicked)` and bumps `capacity_epoch`, so weight is released on panic. The **terminal destructor**: `Drop for DisposalJob` takes its terminal and wraps it in `catch_unwind` so a destructor panic can neither double-panic nor kill a long-lived worker. The **reserved-but-unsubmitted path**: `Drop for DisposalPermit` cancels the queued reservation and notes the capacity release, so a dropped permit cannot leak budget. The **exact-owner teardown path**: `Drop for DisposalLaneInner` closes the sender *first* and then joins, so envelopes already in the channel are drained and finish their accounting before the join returns rather than being stranded. Every admission lock is taken through `lock_unpoisoned`, so a poisoned mutex cannot strand the lane either |
| 7.2 | `WFR-TAB-STRIP` (tier-3) | does any pin or bulk-close path run teardown before a cancellable `close_page()`? | **Clear.** `close_tab_for_path` requests the close and returns; `authorize_and_close_tab_pages` deposits tokens and requests; the teardown exists **once**, in `handle_tab_detached`. Proved by deliberate revert: reinstating the pre-fix shape made `sidebar::test_close_tab_for_path_defers_teardown_until_the_page_detaches` fail with *"expected the pending close to leave the load uncancelled"*, and restoring it made the whole module pass again |
| 7.2b | `WFR-TAB-STRIP` | can a bulk-close authorization token outlive its batch? The token is keyed by the page's **heap address**, so a survivor would, once that address is recycled, silently authorize closing a *different, modified* tab without its save-changes dialog | **Cleared, and the speculative fix was removed rather than shipped.** A sweep retiring unspent tokens was written; with it removed the test still passes, because eligible targets are collected from the **live page order** and every target's `close_page` reaches the request handler, which consumes its token first. The invariant holds structurally. Shipping the sweep would have recorded a fix for a defect that does not exist — the false-correction error this programme names. The test is retained and **retitled** as the protocol assertion it actually is |
| 7.3 | `WFR-SHELL-GEOMETRY` | did any clamp, notify, or allocation path gain a GSettings write or an `AdwBreakpoint` reparse in the move? | **Clear, and now assertable for the first time.** The evidence surface exposes `persisted_workspace_fraction` / `persisted_properties_fraction` separately from the live fractions, and `test_allocation_ticks_clamp_geometry_without_persisting_or_reparsing` drives five allocations across five widths and requires both persisted values unmoved and the breakpoint still installed. Before this surface the rule was checkable only by reading the code or by watching an animation stutter in the installed Flatpak, which is how it was found the first time |
| 7.4 | `WFR-EDITOR-MEMORY-EVICTION` | can an eviction race a save's buffer snapshot or evict state a draft autosave is about to persist? | Clear (slot 7b's earlier pass, using the two race-injector hooks) |
| 7.5 | `WFR-RECENT-DOCUMENTS` | can a stale completion publish rows for a superseded query? does activation re-check the path? | **Clear on the query path, with one recorded residue.** There is **no worker in the query path**: `refresh_filter` runs synchronously over `source_rows`, so no stale publish is representable. The one worker, the startup load, is generation-guarded — a generation change makes the completion **merge** instead of replace, and paths the user removed while it was in flight are subtracted from what came off disk. Activation goes through the ordinary load path, which handles a vanished file as a failed-load placeholder with its inline alert rather than losing anything. Two residues landed in `docs/next/persistent-format-hardening.md` as **S7B-2** (no re-entrancy guard on `load_recent_documents_async`, cleared today on a single call site, recorded because the guard is *absent* rather than present) and **S7B-3** (in-session entries are not re-checked for existence until the next start) |
| 7.6 | `WFR-STARTUP-PREFLIGHT` | can the preflight admit an unmigrated format on an error path? | **No — and the reasoning is recorded, not just the conclusion.** Every error path in `services/format_upgrade/inventory.rs` yields `FormatClassification::Damaged`, and `UpgradeableOld` / `FutureVersion` are produced only from a *successfully parsed* envelope, so an error cannot present as an unmigrated format. `Damaged` maps to `ReportOnly`, so it does **not** hold the gate — landed as **S7B-1**, with its downstream defence stated: an incompletely read draft inventory yields an untrusted `DraftManifestAuthority` whose `Ineligible` replacement eligibility refuses destructive cleanup and manifest replacement |
| 5.7 | `WFR-STARTUP-PREFLIGHT` | the unbounded startup activation-open queue | **Bounded here, not deferred.** `MAX_PENDING_ACTIVATION_OPENS = 64`, with the overflow **dropped and counted** rather than opened immediately — returning "not queued" would send the path straight down the normal open path and defeat the gate, which is the opposite of the safe direction. The dropped count is reported once when the gate releases. **Its coverage gap is recorded** rather than covered by a new actuation seam: reaching the queued state needs a startup compatibility dialog |

### A.11 Facade projections against measurements, all sixteen-plus rows (tasks 0.6, 11.3)

**Six facades written, all under budget, none needing escalation past step one.**
Each projection is recorded beside its measurement so a falsified projection is
visible as such.

| Row | Projection | Measured | Margin | Verdict |
| --- | --- | --- | --- | --- |
| `WFR-TRANSIENT-DISMISSAL` | ≈120 | **199** | 171 | over projection and over its 170 worst case, and stated as such |
| `WFR-FOCUS-MODE` | ≈150 | **338** | 32 | **badly** over projection — the largest miss of the six |
| `WFR-EDITOR-MEMORY-EVICTION` | ≈200 | **333** | 37 | over |
| `WFR-RECENT-DOCUMENTS` | ≈250 | **252** | 118 | within two lines of its projection, and it was the declared escalation candidate |
| `WFR-TAB-STRIP` | ≈220 | **217** | 153 | within three lines |
| `WFR-SHELL-GEOMETRY` | ≈190 | **240** | 130 | over projection, under budget |

**The projections were unreliable in both directions, and the pattern is worth
carrying**: the two projected smallest (transient dismissal at ≈120, focus mode
at ≈150) came in **highest**, while the declared escalation candidate landed
within two lines of its projection and never needed escalating.
**Three of these figures were themselves stale by the end of the change** and are
the final re-measurement: geometry 164 -> 240, tab strip 223 -> 217, recent
documents 239 -> 252. They were taken before the module docs were finished, which
is the slot-5b failure mode; the table above is re-derived after the last edit. What the projections mispredicted was how much
narration a row's *inversions* demand, not how much code it owns. Focus mode's
facade is 338 because the mode borrows chrome state from four surfaces and must
narrate giving each back; the geometry facade, at 240, is the second-lowest of
the six because its machinery delegates cleanly into one coordination module.

**`WFR-RECENT-DOCUMENTS` needed no escalation**, so escalation step 1 was not
taken and step 2 was never reached. The reason it came in low is recorded: the
row's largest population was **26 gated declarations**, and retiring them into an
evidence surface removed them from the facade rather than redistributing them.

**Task 11.3 is discharged separately**: the matrix's facade table now lists
**all twenty-two** migrated facades re-measured, where it had listed eleven
against sixteen migrated rows. **Four** inherited figures moved — the load facade
**271 -> 277** (low by six), search bar **238 -> 229**, markdown preview **270 ->
244**, and sidebar **292 -> 293**. The verb was **re-measure**, not confirm, and
the middle two were found while fixing a fail-open that had left the budget rule
silently inert for three of sixteen rows. `AGENTS.md` carried the markdown
preview and search bar figures too and was corrected with the matrix; a
re-measurement that reaches one document and not the other has only moved the
drift.

### A.12 Mutation figures, parity and gain separated, and 7a's triage contradiction resolved (§9, task 9.4a)

**Parity and gain are reported separately, and neither is dressed as the other.**
Every figure is path-filtered from `mutants.out/outcomes.json` rather than read
from a run's headline counts, because **`MUTANTS_RE` does not filter `delete
field` mutants** — the floor task 9.1 documents rides along in every scoped run.

**Gains from zero** — modules that did not exist before, so there is no
before-count to claim parity against:

| Module | Generated | Caught | Unviable | **Survivors** |
| --- | --- | --- | --- | --- |
| `ui/window/transient_dismissal/policy.rs` | 10 | 8 | 2 | **0** |
| `ui/window/focus_mode/policy.rs` | 17 | 16 | 1 | **0** |
| `ui/window/editor_memory_eviction/policy.rs` | 22 | — | — | generation count only |
| `ui/open_popover/policy.rs` | **6** | 5 | 1 | **0** |
| `ui/window/tab_strip/policy.rs` | **46** | 46 | 0 | **0** |

**Relocation parity** — `ui/window/policy.rs` -> `ui/window/geometry/policy.rs`:

| Measurement | Value |
| --- | --- |
| generated **before**, `make mutants-list` in a clean worktree at `69f78b09` | **81** |
| generated **after**, same command in this tree | **81** |
| executed after the move | **77 caught, 4 unviable, 0 survivors** |

The file is byte-identical across the move apart from visibility keywords, which
cargo-mutants does not mutate. **The matrix's recorded 80 was stale by one** and
is corrected; slot 7a's separate gain-from-zero claim for the
`adaptive_shell.rs` rename stands on its own and is not restated as parity.

**Task 9.1 — the floor, re-derived from the tool.** `MUTANTS_RE` set to a
non-matching pattern leaves **13** `delete field` mutants in every scoped run,
and they are identical across all three runs: **8** in `services/file_tree.rs`
and **5** in `services/draft_service.rs`. (Slot 7a measured 34 for the whole
default scope; 13 is what survives *this* change's three scoped runs, and the
difference is scope, not drift.)

**Task 9.6 — the two ratchet rows, re-derived from the tool a second time.**

| Ratchet row | Owning row | Survivors measured | Recorded figure |
| --- | --- | --- | --- |
| `DirectoryScan` bounded-scan telemetry (`services/file_tree.rs`) | `WFR-WORKSPACE-TREE` | **8** | 8 — confirmed |
| Orphan-cleanup continuation fields (`services/draft_service.rs`) | the draft row | **5** | 5 — confirmed |

**Per-row decision: both are carried as ratchet rows, not closed here.** Both are
the same operator class — bounded-work counters no test asserts, in two rows
whose whole subject is bounded work — and closing either means adding a test that
asserts the counter. That is worth doing when a defect makes one of them matter;
doing it now would pin a number nobody is reading. Recorded in the deferral
inventory with that reason rather than left as an unexplained residue.

**Task 9.4a — slot 7a's triage contradiction, resolved against the tool.** Its
**B.6** said 160 newly-in-scope mutants were **untriaged** (*"`make mutants-diff`
was not run"*); its **A.12** recorded an actual run of **130 caught / 13 unviable
/ 17 missed** with the 17 since triaged. **A.12 is right and B.6 is stale**: B.6
was written first and not revised when the run landed. The decisive evidence is
this change's own runs — the three scoped runs above reach **every** module slot
7a brought into scope that this change touches, and their only missed mutants are
the 13-mutant pre-existing floor, with **zero** survivors in any `policy.rs`. A
scope of 160 untriaged mutants cannot coexist with that. **Task 9.5's scope,
resolved from this: the actual surviving set in the modules this change owns is
zero**, and budgeting to triage 160 would have reported a fictional
accomplishment.

**Task 9.2 and 9.3, observed rather than assumed.** No run here used `--in-diff`:
this change contains **eight** role-home creations and two renames, and a rename
is a whole-file delete plus add, so `--in-diff` would have measured far more than
the logic that changed — the hazard at its maximum. Every run was scoped by
`MUTANTS_RE` with **path-filtered** outcomes instead, and no diff file was passed
to `run-mutants.sh`, so `ensure_diff_file`'s silent three-dot substitution was
never reachable.

**Tooling note carried forward:** a bare `cargo mutants --list` in this checkout
returns **zero** mutants for every path, including modules known to be in scope.
Only `scripts/run-mutants.sh` / `make mutants-list` produce the real list. A
future slot reaching for the bare tool will otherwise conclude a module has no
mutation coverage when it has full coverage.

### A.13 Lane consequences of this change's moves (§10)

Lanes run against the **final** tree, after the last `ui/**` edit, from a clean
artifact root where the lane owns one. **Each result is recorded with the lane's
lie-mode beside it** (task 10.4), because a green run is not the claim it looks
like.

| Lane | Result | Lie-mode / note |
| --- | --- | --- |
| `cargo fmt --all --check` | **pass** | it formats **nothing** under `tests/widget/` and always passed there; the hole is closed separately below |
| `rustfmt --edition 2024 --check crates/lushtext/tests/widget/*.rs` | **pass** | the new reach added by task 8.1, wired into `make check-fmt` and applied by a new `make fmt` |
| `cargo clippy --workspace --all-targets --all-features -D warnings` | **pass** | **hides** breaks the default-feature build reports |
| default-feature `cargo clippy -p lushtext-core --all-targets` | **pass** | this is the configuration that caught the pre-existing `replace.rs` dead-code gate mismatch, invisible to `--all-features` |
| rustdoc gate, **both** configurations, by hand | **pass** | `make check` does not run it. Six new facades in role homes is the exact shape that trips `private_intra_doc_links` |
| `openspec validate --all --strict` | **pass — 111 / 0** | after both deltas landed; matches slot 7a's count |
| `make check-workflow-boundaries` (+ `--self-test`) | **pass** | validates that a declared role **path exists** and that a `policy.rs` is GTK-free; it does **not** validate that a role *name* fits the module's job, so every "called presentation surface" classification here is a **review** claim, not a gated one. After 8.5 it also depends on a `Slot` header located by name |
| `make check-automation-docs` (+ self-test) | **pass** | cannot see a field nobody registered. This change registered none and added no automation projection; it **removed** a re-derivation instead |
| `make check-filesystem-boundary` | **pass** | |
| `make check-blueprint` | **pass** | Blueprint drift only; it does not prove geometry |
| `make check-agent-docs`, `make check-agent-skills` | **pass** | re-run after the `.agents/rules/*`, `AGENTS.md`, and module-doc edits |
| `make automation-client-self-test` | **pass** | parser and envelope only; not real-process D-Bus |
| `make test` | **pass — 1,794 non-widget tests** plus the full widget lane | the count moved from 1,792 during the review pass, which added S17's two pure-predicate unit tests |
| headless widget lane, `--retries 0`, whole suite | **pass, with `FLAKY:` count = 0** — but only after a blocker the *second* such run found | the no-retry run is the one that makes a flake visible rather than absorbed, and it earned that description here. An earlier no-retry run recorded `FLAKY: 0`; a later one on the same tree reported one `FLAKY:` line, and the "flake" was a **SIGSEGV**. See the S7B-6 block below. A single green no-retry run is not evidence of a stable suite when the failure rate is ~22% per run |
| focused widget lanes during development | **pass** | `open_popover` (incl. 4 new), `tab_strip` (5 new), `shell_geometry` (5 new), `window::` (whole module, twice), `sidebar::test_close_tab_for_path` |

**Two deliberate reds were run as part of verification rather than only as
development steps**, because each is the evidence that a test can fail:

- `sidebar::test_close_tab_for_path_defers_teardown_until_the_page_detaches`
  failed with *"expected the pending close to leave the load uncancelled"* when
  the pre-slot-7a teardown shape was reinstated, and passed again when restored.
- The `tab_strip` bulk-close-authorization test **passed without** the sweep
  written for it, which is why that sweep was **removed** rather than shipped —
  see A.10.

**The exported D-Bus contract is unchanged, proved by measured diff** (task 6.8):
`INTROSPECTION_XML` is byte-identical to `69f78b09`, and
`crates/lushtext-core/src/model/automation.rs`,
`crates/lushtext-core/src/model/action_catalog.rs`, and
`crates/lushtext-core/src/services/action_catalog/` have **empty** diffs against
it. All five tab-context action names are unchanged. The before/after Automation1
capture half is covered by `make automation-smoke` below.

| `make test-prop` | **pass — 44 / 44** | bounded property lane, separate from the default non-widget scope |
| `make fuzz-corpus-replay` | **pass — 3 / 3** | stable replay of committed seeds; read-only, no `cargo-fuzz` |
| `make check-gtk-lush-policy`, `make check-gtk-lush-adoption` | **pass** | no GTK Lush extraction was proposed, and these prove none happened |
| `make test-workspace-row-states` | **pass**, `FLAKY:` count 0 | the focused lane whose string filter task 2.6 checked |

**Smoke lanes**, run from a clean artifact root
(`build/smoke/{accessibility,visual,visual-geometry}` removed first):

| Lane | Result | Content asserted, not exit code |
| --- | --- | --- |
| `make accessibility-smoke` | **pass** | *"verified AT-SPI anchors and focus artifacts"*; re-run from a clean root in the review follow-up after the `ui/window/AGENTS.md` edit voided its fingerprint |
| `make visual-smoke` | **pass** | screenshots captured for every scenario incl. `transparency-readability` and `recovery-startup`; re-run from a clean root with the other two |
| `make visual-geometry-smoke` | **pass** | **80 cases, 80 passed, 0 non-passing**, read from the re-run summary. `pixel_verified_invariant_ids: ["native-minimap-highlight-anchors"]` and `animation_verified_invariant_ids: ["native-minimap-animation-highlight-anchors"]` — **both** invariants the re-keyed predicates now require of `ui/window/geometry/**`, read from `summary.json` rather than from the exit code. **44** cases carry screenshot-derived pixel evidence, and the **workspace-sidebar animation matrix** is present in full: `compact-overlay`, `intermediate-1100sp`, and `wide-desktop`, each in both `--show` and `--hide` |
| `make check-accessibility-policy` | **pass** | requires the accessibility and visual summaries to carry a source fingerprint matching the current tree; both were regenerated after the last `ui/**` edit |
| `make check-visual-proof-policy` | **pass** | requires an unfiltered geometry summary matching the current visual-sensitive diff, pixel-verifying every required invariant. **This is the gate the §E3 re-key armed**: before the re-key it would have passed while requiring none |
| `make check-policy` | **pass** | the umbrella, including all of the above plus blueprint, filesystem boundary, workflow boundaries, and automation docs |

**A second review pass (the independent review of this change) found four record
blockers and eighteen should-fixes; all were applied and re-verified.** The ones
that changed *code* rather than text are recorded here because each is the class
this programme keeps naming:

| # | Finding | Resolution |
| --- | --- | --- |
| B1 | The capability delta this change owns listed terminal statuses as `migrated`/`exempt`/`cross-cutting` while the matrix shipped `superseded`. Only the gate half of task 0.4b-i had landed | The spec text now carries `superseded` **with a counterpart obligation**: a superseded row must name its replacements and the gate must verify each exists, so a terminal label cannot become a silent exemption. Three self-test arms, proved by deliberate red |
| S2 | `editor_memory_eviction/evidence.rs` reached `source_view()` — a panicking `TemplateChild` — **transitively**, through `estimated_live_buffer_bytes` and `eligible_for_memory_eviction`, violating the transitive-disposal rule *this change added* | The walk now skips a disposed page via `LushtextEditorPage::is_disposed`. Proved by deliberate red: replacing the guard with `false` panics with *"Failed to retrieve template child"*. No test is retained, and **why** is recorded in the module doc: the only way to get a disposed page into the walk is `run_dispose()` on a still-parented page, which GTK itself reports as incorrect |
| S3 | Three modules held contradictory positions on whether template children are cleared. `ui/sidebar/evidence.rs` said they are **never** cleared; two others recorded an observed panic | Driven, and the answer was sharper than the question: `run_dispose()` on a section panics **inside the section's own `dispose()`** on a cleared `GtkListView`. The children are cleared. The sidebar doc's *conclusion* was right and its *reason* was false; the reason is corrected and the observation landed as **S7B-5** |
| S13 | `is_quiesced()`'s doc claimed centralization while **12** open-coded copies survived, **2 of them with a different predicate** (also requiring `retained_bytes == 0`) | The definition moved to the domain `PlainDisposalSnapshot`; all 12 copies and both internal re-derivations now delegate. The two stronger call sites read `is_quiesced() && retained_bytes == 0`, so the extra condition is visible as extra. Zero open-coded copies remain |
| S14 | `open_popover/evidence.rs` constructed **eight GTK widgets on every read** to report a row template that is a **class constant** | Removed from the surface and exposed as an associated function; the surface gained `query` instead, retiring two `search_entry_for_test()` reads. The reentrancy proof now drives **all five** mutable-borrow paths, including the startup-merge branch a stale generation guards |
| S6, S7 | The `json_persistence` lane ran three widget filters with **no** assertion; `make check-fmt`'s wildcard failed **open** on an empty match (`rustfmt` on stdin exits 0) | Both closed: the assertion applied, and `$(error …)` on an empty `WIDGET_TEST_SOURCES`, proved by deliberate red |
| S8 | Eight action-catalog `owner` strings named deleted modules; `check-automation-docs` passed only because **both sides shared the stale string** | Re-keyed to `window/palette_shell` and `window/tab_strip`; the gate then failed on the doc side until `automation-reference.md` was regenerated, which is the gate working |
| S17 | The activation-queue ceiling had no test, and its message said `file(s)` | Two **pure** predicates with real unit tests. A first attempt asserted the constant's value — the "test that cannot fail" class — and was replaced after clippy rejected it |

**Task 2.6 found a real one, and it was the lane's own guard that found it.**
`make performance-smoke` **failed** on a healthy tree with
*"widget proof 'editor\_page::test\_large\_unicode\_load\_installs\_in\_exact\_bounded\_slices'
matched nothing; its filter is stale."* The test exists, ran, and passed — the
**assertion** was broken, and had been:

- `smoke_assert_ran`'s default pattern is `test result: ok\. [1-9]`;
- the **custom widget harness** (`crates/gtk-lush/proof-harness/src/lib.rs:423`)
  prints `test result: ok. all tests passed` — **no digit, ever**;
- so **four** call sites asserting widget logs with the default pattern could
  never pass, and the staleness condition they existed for was unchecked in both
  directions: the lane failed on a healthy tree *and* would not have caught a
  genuinely stale filter for a different reason.

Fixed with a shared `smoke_assert_widget_ran` in `scripts/smoke-common.sh` — one
predicate, not four copies — asserting the harness's own header
(`^running [1-9][0-9]* tests`), which is exactly the staleness condition: a
filter matching nothing yields `running 0 tests`. The caller has already failed
the run on a non-zero exit, so a test that ran and failed never reaches the
guard.

**Proved three ways**, because a guard that cannot fail is the defect it was
guarding against: a healthy widget log **passes**; a `running 0 tests` log is
still a **finding**; and the old default pattern against the *healthy* log
**fails**, which is the pre-existing bug demonstrated rather than asserted.

**The remaining smoke lanes**, run directly after the sequence above:

| Lane | Result | Content asserted |
| --- | --- | --- |
| `make performance-smoke` | **pass** (after the guard fix) | **26** summary sections, **21** harness result lines, **zero** `FAIL`/`stale` in the run log. `transient_file_load_headless`, `markdown_retirement_pressure`, and `buffer_replacement_reentrancy` — the three widget-backed sections whose guard was broken — all present with their evidence lines |
| `make crash-recovery-smoke` | **pass** | *"crash recovery smoke completed"* with artifacts |
| `make automation-smoke` | **pass** | *"automation D-Bus smoke completed"*. This is the before/after Automation1 capture half of task 6.8, whose schema half is the measured diff above |
| `make builder-diagnostics-smoke` | **pass** | *"Builder diagnostics passed"* |

**Task 10.17, checked rather than asserted — and it failed once, which is the
point of checking.** The review-fix cycle's follow-up pass edited
`crates/lushtext-core/src/ui/window/AGENTS.md` (a stale sibling-module list and a
lost `## Editing Rules` heading). That path sits under the
`crates/lushtext-core/src/ui/` **prefix**, so it is inside *both* the
accessibility source fingerprint and the visual-sensitive predicate — a
**markdown module doc voids all three smoke proofs**, and both gates said so
rather than passing. All three lanes were re-run from clean artifact roots
afterwards, and the results below are from that re-run. The lesson is narrow and
worth stating: the fingerprint sets are keyed on *directory prefixes*, not on
`*.rs`, so "I only edited a doc" is not a reason to skip the check — ask the
predicate.

After that re-run, every `ui/**` and `crates/lushtext/tests/widget/**` source
file is again **older** than all three smoke summaries, so no edit voided a proof
after it was taken. The gates were re-run after the later `.agents/rules/*`, `AGENTS.md`,
`Makefile`, `scripts/smoke-common.sh`, and docs edits, none of which is
visual-sensitive or accessibility-relevant (verified against both predicates
directly rather than assumed):

| Gate, re-run last | Result |
| --- | --- |
| `make check-accessibility-policy` | **pass** — 9,364 added UI-sensitive lines checked, plus the self-test |
| `make check-visual-proof-policy` | **pass** — *"summary matches current visual-sensitive diff; summary pixel-verified required visual invariants"* |
| `make check-policy` (full umbrella) | **pass** |
| `make check-agent-docs`, `make check-agent-skills` | **pass** — 14 skills, 13 Cargo packages, required release workflow roles |
| `openspec validate close-workflow-readability-programme --strict` | **valid** |
| `openspec validate --all --strict` | **pass — 111 / 0** |

**Final gate sweep, re-run in full after the review follow-up.** Every row below
was executed against the tree as it now stands, after the last edit of any kind:

| Gate | Result |
| --- | --- |
| `make fmt` then `make check` (fmt + all-feature clippy + full `check-policy`) | **pass** |
| default-feature `cargo clippy -p lushtext-core --all-targets -D warnings` | **pass** |
| rustdoc gate, `--all-features` **and** default features | **pass** in both |
| `cargo nextest run --workspace` | **pass — 1,794 / 1,794**, 11 skipped |
| headless widget lane, `--retries 0`, whole suite | **pass — 1,203 tests, `FLAKY:` count 0, zero `CRITICAL` lines** |
| `make accessibility-smoke`, `make visual-smoke`, `make visual-geometry-smoke` | **pass**, all three from clean artifact roots |
| `make check-accessibility-policy`, `make check-visual-proof-policy` | **pass** against the re-run summaries |
| `make check-workflow-boundaries --self-test` | **pass** — 22 pure, mutation-scoped policy modules; matrix and ledger agree |
| `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-doctests`, `make gtk-lush-examples` | **pass** — run because this session changed `gtk-lush-proof-harness` |
| `make check-agent-docs`, `make check-agent-skills` | **pass** — 14 skills, 13 packages |
| `openspec validate --all --strict` | **pass — 111 / 0** |

**Any lane not listed as passing here was not run, and the change does not claim
it.** Not run: `make portal-sandbox-smoke` (host confinement diagnostics, which
this change does not touch), `make visual-geometry-oracle-smoke`, and the
`make mutants-*` sweeps beyond the three scoped runs in A.12.

**A fourth pre-existing blocker, found by the second no-retry widget lane and
recorded here because the shape of the investigation is the transferable part.**

`workspace_section::test_inline_rename_refuses_to_replace_an_existing_sibling`
reported `FLAKY: passed on attempt 2`. `preexisting-blockers.md` says a `FLAKY`
line is a blocker, so it was reproduced in isolation rather than retried: **5
failures in 40 runs** for that one test, **9 in 40** across the four
inline-rename tests. The harness prints no panic for a failed attempt, so the
real failure came from `coredumpctl`:

```
#0 wl_proxy_get_version                 (libwayland-client)
#1 gtk_im_context_wayland_global_get    (libgtk-4)
#2 gtk_im_context_wayland_get_global
#3 notify_im_change                     <- a zwp_text_input_v3 listener callback
...
#16 gtk_lush_proof_harness::flush_events
#17 gtk_lush_proof_harness::wait_until
#18 test_inline_rename_refuses_to_replace_an_existing_sibling
```

**It was a SIGSEGV, not a timing flake**, and the three CRITICALs the lane
reported as "unexpected warning output" were its first three steps, not a
separate problem. `signal: 11` is what the harness had been absorbing as a retry.

**Three application-side fixes were implemented and each was measured.** None is
in the tree; all three are recorded because "we tried the obvious thing" is worth
exactly as much as its measurement:

| Attempt | Rationale | Measured |
| --- | --- | --- |
| grab focus to the owning `GtkListView` before unparenting the entry | `.agents/rules/widget-wiring.md`'s focus-restoration rule names inline rename explicitly | **14/40** — and a probe showed why: the guard was `entry.has_focus()`, which is **`false`** for a `GtkEntry` whose internal `GtkText` owns focus, so it never ran at all |
| same, with the guard corrected to ask the root's focus widget | the predicate now fires (probe: `root_focus=GtkText, focus_inside=true`) | **22/30 — worse** |
| clear the window focus (`set_focus(None)`) before unparenting | fewer focus transitions than handing focus to a list view | **22/30 — worse** |
| keep the entry alive past removal (deliberate leak) | test whether finalization is the trigger at all | **13/25 — no help** |
| `GTK_IM_MODULE=gtk-im-context-simple`, production code untouched | the dangling state is in GTK's private per-display IM global | **0/30, then 0/40 on the final tree** |

**The first row is the lesson worth keeping**: a focus guard written as
`entry.has_focus()` compiles, reads correctly to a reviewer, and is always false.
It was caught by printing the value, not by reading the code again.

The fix landed in `gtk_lush_proof_harness::recommended_pre_gtk_environment()`,
which grew from four settings to five, beside `NO_AT_BRIDGE`,
`GDK_DEBUG=no-portals`, `GTK_USE_PORTAL=0`, and `GSK_RENDERER=cairo`. All five
say one thing: the private compositor advertises desktop services it cannot
provide, so the client half is not started against it. Headless Mutter
advertises `zwp_text_input_manager_v3` with no input method behind it and no user
typing into it.

**What is not claimed:** that the GTK race is fixed. A real Wayland session runs
the same backend and the same user action destroys the same focused entry.
`S7B-6` records that exposure, names the three measured non-fixes so the next
owner does not re-derive them, and leaves the question to the live-display
walkthrough that is still user-gated.

**A tooling hazard hit during verification, recorded because it is the class
`.agents/rules/` already names.** The first attempt to sequence these lanes used
`while pgrep -f run-widget-tests; do sleep; done` as a gate. `pgrep -f` matches
the **full command line**, and the waiter's own command line contains the string
it searches for — so the waiter matched itself and three sibling waiters matched
each other, and the sequence blocked indefinitely on a lane that had already
finished. This is exactly the *"never self-matching waiter loops — use foreground
commands with explicit timeouts"* rule, met in practice rather than read. The
lanes were re-run directly.

### A.14 Cold-read check: can a reader name each new row's stages from its facade alone?

Checked for all six.

| Facade | Stages named | Inversions named with a resumption point | Absences recorded as conclusions |
| --- | --- | --- | --- |
| `transient_dismissal/mod.rs` | 3, in order, with stage 2 stated as touching no widget | 1 — the one-tick idle latch, with its mechanism and the concrete bug it prevents | no coordination module, no `test_policy.rs`, no seam beyond `TransientSurfaceState` |
| `focus_mode/mod.rs` | entry and exit as one borrow/return pair | the affordance-hide `SupersedingTimer` | — |
| `editor_memory_eviction/mod.rs` | the bounded continuation's stages | the idle continuation, with the generation guard that rejects a stale resumption | — |
| `open_popover/mod.rs` | **5**, numbered, with the two that do not run in this module marked as such | 1, and it is the interesting one: the sequence **leaves** the widget at stage 5 through a callback and **resumes** at stage 2 when the window calls `set_recent_rows` again — which is how removing a row updates the list without this module knowing what a recent document is | no coordination role at this home, no seam value object, no `test_policy.rs`, each with its reason |
| `geometry/mod.rs` | the four entry points and the one sequence they share | 2 — the toolkit-owned sidebar animation, with the rule that the settle burst is armed **before** the animated property is set; and the two focus restorers' `idle_add_local_once` | no seam beyond `AdaptiveShellInputs`, no `test_policy.rs` |
| `tab_strip/mod.rs` | the shared 5-step sequence and the five entry points into it | 1 — `close_page` is **cancellable**, so control leaves at stage 5 and resumes in the detach terminal; the facade and the coordination module both say *why* running teardown before that terminal strands a draft | no `test_policy.rs`, no seam beyond `TabLayoutEntry`, zero actuation seams added |

Every facade also carries a **role table** naming its called presentation
surfaces explicitly, so a reader can tell a role from a projection without
opening the file.

### A.15 The coverage proof, re-derived (task 5.10)

**Three figures were in circulation for one denominator** — the matrix's proof
read **198**, slot 7a recorded **266**, and a count at this change's authoring
gave **286**. **None was adopted.** The disagreement is itself the finding, and
its cause is now stated in the matrix beside the proof:

1. the file count **grows with every migration**, because the convention
   replaces one large module with a facade plus several small role files, so
   198 -> 266 -> 286 -> 306 is mostly the convention working;
2. the three figures used **different predicates**, and none said which.

**Predicate, stated:** every `*.rs` under `crates/lushtext-core/src`, minus files
a sibling declares as `#[cfg(test)] mod <name>;`, minus the crate's root-level
infrastructure files.

| Quantity | Count |
| --- | --- |
| `*.rs` under `crates/lushtext-core/src` | **308** |
| separate-file `#[cfg(test)]` modules | 2 |
| production files | **306** |
| crate infrastructure at the crate root | **4** |
| attributable to a matrix row or the tier list | **302** |

**Delta against 198/195, with its cause:** +108 files, of which the great
majority are role files created by eleven slots of migration; and the old
proof's *"the remaining 3 are the crate infrastructure files"* is corrected to
**4**, because `fuzzing.rs` was added after the census and the proof was never
re-derived. That one-file correction is the kind the re-derivation obligation
exists to catch: a completeness claim that was true and quietly stopped being so.

The full per-directory attribution table is in the matrix. **The split changed
it**, which is why re-deriving was required work rather than a confirmation: six
new role-home directories appear that did not exist when the previous proof was
written.

---

## Appendix B — closeout

### B.0 Whether the declared split was taken, and on what trigger (task 0.13)

**No split was taken. All seven replacement rows landed in this change**, plus
`WFR-PLAIN-DISPOSAL`'s lane surface and `WFR-AUTOMATION-SPINE`'s terminal
status, so the declared boundary (*after `WFR-SHELL-GEOMETRY`*) was passed rather
than used.

**This section previously said the opposite, and the correction is the point.**
An earlier revision recorded *"the split was taken, and at an EARLIER boundary
than the declared one — after `WFR-TRANSIENT-DISMISSAL`, one row migrated."* That
was written as a **stopping record** at a real decision point, and the session
then continued through three more rows without revising it; a later session lost
its transcript, re-established state from the **tree**, and found the change's own
appendix describing a boundary it had passed. The lesson is recorded in B.5: a
stopping record must be superseded in place when it is released, not left to be
reconciled by whoever reads it next. The original text is not preserved verbatim
because the IMPLEMENTATION STATE banner already carries the correction and two
contradicting narratives is the defect, not the remedy.

**Delta clause allocation (0.13a): no allocation was needed.** Resolution (i) —
withhold both delta files for a continuation slot — was the earlier revision's
answer under the split. With no split, both deltas' obligations are dischargeable
here: delta 1(b)/(d) need every row terminal and the closeout record, and both
exist; delta 1(a)/(c) need the reassignments and the re-derived coverage proof,
and both were performed; delta 2 needs the lane's surface, which landed. **Both
delta files were landed into their live specs**, and `openspec validate --all
--strict` passes 111/0.

### B.1 Programme and matrix agreement (tasks 11.1, 11.2)

**Every matrix row is terminal**, and `make check-workflow-boundaries` passes
**truthfully** rather than because a claim was weakened — the gate was made
*stricter* in the same change, not looser.

| Status | Rows |
| --- | --- |
| `migrated` | **22** |
| `cross-cutting` (terminal) | `WFR-BUFFER-SNAPSHOT`, `WFR-PLAIN-DISPOSAL`, `WFR-STARTUP-PREFLIGHT`, `WFR-AUTOMATION-SPINE`, `WFR-MIGRATION-LEDGER` |
| `exempt` | `WFR-EDITOR-MEMORY` |
| `superseded` | `WFR-SHELL-LAYOUT` |
| **transitional** (`pending`, `deferred`, `partially-conforming`) | **none** |

The ledger's `slot 7b` line is closed: every row it named is on the **complete**
side, and **no `outstanding` line follows**. That absence is now load-bearing
rather than cosmetic — delta 1's mechanical half reads it as the machine-readable
statement that the programme is closed, and fails any transitional row that
survives it.

Nothing was deleted from the ledger's grammar section: the `(partial)` rules and
the four failure conditions stay, joined by the parsing-path audit's additions
(A.6a) and this change's transitional-at-close rule.

`11.2a` is discharged: the remaining-scope row recorded this change's artifacts
as *"proposal + tasks + 2 spec deltas"* and it also ships a **`design.md`**; the
cell now lists all four. `11.2b` is discharged by construction — the retired
row's disposition was written into the ledger line and the matrix row in **one**
edit, so no intermediate state exists where one names a row the other has
removed.

### B.2 Programme completion record: measured outcomes against the baseline (task 11.4)

Written in `docs/next/workflow-readability.md` as **"Baseline after slot 7b — the
programme's measured outcome"**, in the same delta-table shape every prior slot
used, so the programme's last row reads against its predecessors rather than
standing alone. Summarised here:

| Quantity | After slot 7a | After slot 7b |
| --- | --- | --- |
| Workflows migrated | 16 | **22** |
| Rows terminal | **20 of 22 at HEAD** (22 rows, 2 `pending`) | **29 of 29** |
| Policy modules in the convention | 17 | **22** |
| Facades measured against 370 | 16 | **22, all re-measured** |
| Automation projections | 7 | 7, and one **re-derivation** removed |
| Capability deltas landed | 3 | **5** |

The record also carries the refreshed `Measurement Definitions` denominators —
the programme's actual ratchet — the **single deferral inventory**, and an
explicit statement of what is **not** claimed.

### B.3 The single deferral inventory (task 11.4)

Written in full in `docs/next/workflow-readability.md`'s
**"The single deferral inventory"** table, so it lives where a future reader
looks rather than inside an archived change. It carries, each with its gating
condition and owner: the live-display walkthrough and the manual Orca check
(both **user-gated**, and neither written as "accepted" on this change's own
authority); the **ten** open `[~]` markers those classes span — eight user-gated
instances of the two rows above plus two items that are **machine-gated rather
than user-gated**, reclassified here (review item S5); `scan_execution.rs`'s
~1,982 production lines and `ui/plain_disposal.rs`'s **1,422** as the **only two** items
recorded as accepted refactor debt; slot 6's conditionally-cleared
`minimap_work_pending` as a **standing condition** rather than a closed item; the
two ratchet rows of `delete field` survivors; the bounded startup queue's
recorded coverage gap; and the `ui/automation.rs` ratchet row, which stays open
with its **8** persisting reading-expression sites and its new owning row.

**The `[~]` reconciliation, re-grepped in slot 7b and corrected — the inherited
numbers were wrong in every position.** The record said *23 markers, 16 of them
slot 5a's, nine genuinely open*. Re-derived by grepping `openspec/changes/**`
with slot 7a archived:

| Count | Population |
| --- | --- |
| **35** | `[~]` markers across the whole programme (33 archived + 2 in this change) |
| **18** | slot 5a's, closed by slot 5b — not 16 |
| **7** | slot 7a's deferrals *to this slot* (its 1.1, 1.4, 2.2, 2.3, 2.5, 8.7, 9.10), **all discharged here**: both deltas landed, both mechanical halves landed, the disarm observed, the re-key made in both implementations with parity assertions and deliberate reds, slot 6's candidate recorded as a standing condition, and the completion section written |
| **8** | genuinely open and **user-gated**: live-display proof in slots 4, 5b, 6, 7a, and 7b, and the manual Orca check in slots 6, 7a, and 7b |
| **2** | genuinely open and **machine-gated, not user-gated** — see below |

18 + 7 + 8 + 2 = **35**. The arithmetic closes, which none of the three inherited
routes did.

**Two items were misfiled as user-gated and are reclassified** (review item S5).
Neither needs the user; both need a machine condition this change could not
supply:

- **slot 5b's 7.6** — the two-tree automation capture-and-diff. It is
  machine-runnable: it needs a second worktree at a baseline ref plus a capture
  run on each. This change ran the *single-tree* visual-geometry lane (80/80,
  both invariants), which is not the same proof. Gating condition: a baseline
  worktree and two capture runs. Not run here because its subject is the
  workspace tree, not this change's rows.
- **slot 4's 10.7** — the benchmark **baseline comparison**. The lane itself
  passes (it passed again here); what is deferred is a `bench-baseline` /
  `bench-compare` pair, which is only meaningful on a **quiet machine**. Gating
  condition: an otherwise-idle host, not user availability.

A reader who greps 35 markers and finds 25 of them already closed will otherwise
conclude twenty-five items were abandoned.

### B.4 Findings landed in `docs/next/` (tasks 5.7, 7.7, 8.1)

**"Handed on" has no recipient after this change**, so every finding reached a
durable home rather than an appendix.

| Finding | Home | Severity |
| --- | --- | --- |
| A startup-critical metadata file that cannot be *read* opens the preflight gate (`Damaged` -> `ReportOnly`), with the reasoning that it still cannot admit an **unmigrated** format and that the draft manifest's untrusted authority is the downstream defence | `docs/next/persistent-format-hardening.md` **S7B-1** | LOW |
| `load_recent_documents_async` has **no re-entrancy guard**; cleared today on a single call site, recorded because the guard is absent rather than present | same, **S7B-2** | LOW |
| In-session recent entries are never re-checked for existence until the next start | same, **S7B-3** | INFORMATIONAL |
| A smoke-lane staleness guard that **could never pass**: four `make performance-smoke` call sites asserted widget-harness logs with `smoke_assert_ran`'s default `test result: ok\. [1-9]` pattern, which the custom harness never emits. Found by the review pass (S6), fixed with one shared `smoke_assert_widget_ran` and proved three ways | same, **S7B-4** | INFORMATIONAL |
| `run_dispose()` on a workspace section panics **inside the section's own `dispose()`** on a cleared `GtkListView`. Not reachable through refcount-driven teardown, which every window-closing test exercises — recorded because it settled a three-way documentation contradiction about whether template children are cleared (S3) | same, **S7B-5** | INFORMATIONAL |
| Destroying a focused `GtkEntry` under headless Mutter **segfaults** in GTK's Wayland input-method backend. Mitigated in the harness environment (`GTK_IM_MODULE=gtk-im-context-simple`, 9/40 -> 0/40); the GTK race is **not** fixed and a real Wayland session runs the same backend | same, **S7B-6** | MEDIUM |
| The **rustfmt reach hole** — `cargo fmt --all --check` passed while formatting **nothing** under `tests/widget/` | **fixed, not recorded.** `make check-fmt` now also runs `rustfmt --edition 2024 --check` over the widget test sources, and a `make fmt` target applies both. The only admissible gating condition for deferring was a measured conflict between the reach mechanism and the registry generation; there is none, because the registry keys on file *names*, which formatting does not change | — |
| The **unbounded startup activation-open queue** | **fixed, not recorded** — see A.10's last row | — |

### B.4a Inherited claims falsified, with what each was measured against (§8, task 11.8)

| Inherited claim | Source | Measured | Verdict |
| --- | --- | --- | --- |
| `focus_indexing.rs` is "three stories"; eviction is ~590 lines; its middle story is "the geometry story" | §D1 via slot 7a | four stories; eviction **407**; the middle story contains **no geometry code** and is itself three fragments with three different owners | **falsified in four ways** (A.2a) |
| Eviction spans `:56`–`:190` and `:380`–`:667` | proposal Finding 1b | `:56`–`:189` and `:380`–`:666`; the cited upper bounds name the next story's opening line | off by one at both bounds; the **~406** figure is right to ±1 |
| Palette file-index build is ~171 production lines | proposal Finding 1b | **145** | -26 |
| Focus restoration is ~129 lines / palette overlay ~104 | proposal Finding 1b | **125** / **108** | -4 / +4 |
| The dead tuple ladders are "~70 lines" | slot 7a review pass | **8 sites, 20 placeholders, 68 lines** (6 seven-element, 2 five-element) | the proposal's re-derivation to 8/20 is **confirmed exact**; the original "~70" was untraceable but landed within 2 lines of the truth |
| Three ledger-check holes exist ("S12") | slot 7a review pass | all three reproduce, **plus a fourth found by the sweep and a fifth surfaced by fixing the first two** | framing was wrong (parsing path, not conditions); count was **low** (A.6a) |
| `git_lines` has zero callers | slot 7a review pass | confirmed: one definition, zero call sites repo-wide | confirmed; removed |
| The print proof verifies behaviour after `close()` | comment in `tests/widget/window.rs:14571` | the test never called `close()`; the following assert duplicated the one above it | confirmed; fixed by **adding the step the comment claimed**, so the two asserts are now distinct and the claim is earned |
| The facade-budget rule protects all migrated rows | implied by the gate passing | **3 of 16** rows were silently unchecked; 2 of those 3 carried stale figures | falsified; fixed and both figures corrected |

> **SUPERSEDED — both were reached.**

~~Two claims this change was asked to test and did not reach…~~

Both were tested and are recorded with their measurements: slot 7a's
`DisposalPressureEvidence` narrowing instruction is **unexecutable as written**
(A.8's reader measurement — 35 call sites across 5 files, all in a different
crate, so `pub` under a `test-utils` gate is already narrowest), and its
B.6/A.12 mutation-triage contradiction is **resolved against the tool in favour
of A.12** (A.12's closing paragraphs).


### B.5 Convention and tooling friction this change hit (task 11.6)

Each hit while *using* the thing, not while reading it.

- **A markdown file voids three screenshot proofs.** The accessibility source
  fingerprint and the visual-sensitive predicate are both keyed on the
  `crates/lushtext-core/src/ui/` **directory prefix**, not on `*.rs`. Editing
  `ui/window/AGENTS.md` — a module-layout doc — therefore invalidated the
  accessibility, visual, and visual-geometry summaries and failed two gates.
  Both gates were right and the instinct ("it is only a doc") was wrong. Ask the
  predicate before assuming a file is outside a proof's scope.
- **A `FLAKY:` line can be a segfault, and the harness will not tell you.** The
  per-test retry prints `ok (FLAKY: passed on attempt 2)` and nothing from the
  failed attempt, so a crash reads exactly like a tight wait budget. The real
  failure came out of `coredumpctl`. Reproduce in isolation and **count** before
  theorising: 9 failures in 40 is invisible to any single rerun, and the first
  hypothesis here — a 2-second realization wait — was wrong.
- **A guard can be always-false and still read as correct.** `entry.has_focus()`
  is `false` for a `GtkEntry` whose internal `GtkText` owns focus. The first fix
  compiled, cited the right rule, and did nothing; a one-line probe printing the
  value settled in seconds what re-reading the code had not.
- **Measure a toolkit-race fix rather than reasoning about it.** Three
  application-side orderings, each measured over 25–40 runs, were neutral or
  **worse** than doing nothing. Only the environment-level change reached zero,
  because the dangling state was in GTK's private per-display IM global.
- **A path-keyed gate is disarmed by the migration that moves its file, silently
  and greenly.** Known in the abstract; met concretely here, and the only thing
  that made it visible was *observing the disarm first*. Reviewing the edit
  would not have shown it, because the gate exits 0 either way.
- **`rustdoc`'s `private_intra_doc_links` and the facade convention pull in
  opposite directions.** A narrative facade naturally wants to link its own
  coordination modules and `pub(crate)` seam values; every one of those is an
  error. Six new facades were written with the names in backticks and no links,
  and the gate passed first time — the first slot for which that is true.
- **A gate keyed on a naming convention survives a rename that a literal-keyed
  gate does not.** `scripts/accessibility_source_fingerprint.py` keys on
  `crates/lushtext-core/src/ui/` and `crates/lushtext/tests/widget/` **prefixes**,
  so six role-home creations and two renames needed no edit at all. The
  visual-proof predicates key on literals and needed both. That contrast is
  worth carrying: prefer a prefix when re-keying.
- **The mutation scope's `ui/**/policy.rs` convention paid off the first time a
  `policy.rs` moved between directories.** Relocating `ui/window/policy.rs` into
  a nested role home needed **no** configuration change, and the verification
  was a measurement rather than an edit. The rule's instruction to *verify after
  the move rather than assume* is still what made it evidence.
- **A review pass whose findings reach no artifact is a worse handoff than one
  that reaches an archived directory.** Finding 6's six items were re-verified
  here and **two of their figures were wrong** — 411 hunks not 171, and a
  "near-duplicate pair" that is a one-expression default-argument wrapper. The
  fix is a durable home, not a better memory, which is why every finding in B.4
  landed in `docs/next/`.
- **An appendix can outrun its own change.** This change's B.0 and B.7 were
  written as a stopping record and the session then continued past them; a later
  session re-established state from the **tree** and found the artifact
  describing a boundary it had passed. Recorded because the honest response is to
  supersede the section in place, not to delete it.

### B.6 Corrections to earlier handoff text (task 11.8)

Each corrected so a later reader does not re-plan a non-item or inherit a wrong
pointer.

| Earlier text | Correction |
| --- | --- |
| slot 7a's proposal says *"five self-test keys"* in one place and six in another | **six** is correct; verified intact in this change |
| slot 7a's narrowing instruction for `DisposalPressureEvidence` | **unexecutable as written** — the type is already `test-utils`-gated and its only reader is in another crate, so `pub` is the narrowest visibility that compiles. "Already narrowest" is the recorded outcome |
| slot 7a's A.11/A.14/A.15 read as though no facade was written, while its header reports five | noted; those sections are in an archived change and are not edited here, but the discrepancy is recorded so a reader does not conclude the facades are missing |
| slot 7a's A.6 / A.13a contradiction about the accessibility summary-absence fail-open | **A.6 is right and A.13a is stale.** The script fixes it at `scripts/check-accessibility-policy.py:345` with an explicit comment naming the fail-open, and the self-test carries the absent-summary red arm (`N1`). Recorded here so neither is trusted blind again |
| slot 7a's B.6 / A.12 contradiction about mutation triage | resolved against the tool; see A.12 |
| §D1's *"three stories"* verdict for `focus_indexing.rs` | **four**, with four owners; and the middle one contains **no geometry code**. See A.2a |
| the automation ratchet row read as retired | **occurrence-retired, expression persisting at 8 sites.** It must not be struck retired; its new owning row is `WFR-TAB-STRIP` |
| slot 7a's `design.md:110` citing `ui/window/mod.rs` at **257 physical with 113 lines of headroom** | it measured **269/101** at this change's authoring; the file now measures differently again after this change's moves, which is the point — a line pointer into a live file is a pointer with a shelf life |
| slot 7a's A.4 citing the shell trace at `:157`–`:256` | it sat at `:169`–`:268` at authoring |
| the matrix's finding **G8**: *twelve dangling `mutation parity` pointers across four archived changes*, and *"the gate cannot catch this"* | **both halves falsified.** Re-derived in this change: **zero** real dangling pointers remain — the only non-existent path in the matrix is a documentation placeholder of the form `openspec/changes/<change>/evidence/<file>.md`. And the gate **does** catch it: `check-workflow-boundaries.py` validates backticked repo paths, and it caught a mistyped archive pointer introduced while writing this very change. The mechanical half G8 asked for already exists |
| the proposal's rustfmt figure of **411 hunks across 18 files** (itself a correction of an inherited 171) | re-measured under a stated predicate (`rustfmt --emit stdout \| diff -u`, counting `@@` hunks): **339 hunks across 18 files**. The predicate is the difference, and none of the three earlier figures stated one. The reformat was taken regardless, so the number is history rather than a budget |
| the proposal's *"near-duplicate pair"* in `encoding/dialogs.rs` | **does not exist.** `append_action_row` is a one-expression default-argument wrapper, and the file has **five** row builders, not four. The six `present_*` shapes were probed for real duplication (task 8.4a) and share only a four-line preamble already factored into `build_dialog` and `standard_dialog_content`; extracting further would add a closure indirection to a called presentation surface of a row migrated one change ago. **"No near-duplicate exists" is the recorded outcome** |

### B.7 What is terminal, on what grounds, and what is not claimed (task 11.9)

**Terminal: every row.** 22 `migrated`, 5 `cross-cutting`, 1 `exempt`, 1
`superseded`. No `pending`, `deferred`, or `partially-conforming` row remains,
and the gate now fails any that appears once the ledger declares no outstanding
slot.

**Per row, on what grounds:**

| Row | Grounds |
| --- | --- |
| `WFR-TRANSIENT-DISMISSAL`, `WFR-FOCUS-MODE`, `WFR-EDITOR-MEMORY-EVICTION`, `WFR-RECENT-DOCUMENTS`, `WFR-SHELL-GEOMETRY`, `WFR-TAB-STRIP` | `migrated`: facade under budget, pure policy with unit tests in the mutation scope, evidence surface with its three **driven** proofs, state-extreme coverage, a green headless no-retry widget lane with zero `FLAKY:` lines, and mutation figures reported by kind |
| `WFR-SHELL-LAYOUT` | `superseded`: replaced by the seven rows above plus `WFR-STARTUP-PREFLIGHT`, with a terminal label added to the vocabulary for it and the row **retained** so its history stays reachable |
| `WFR-STARTUP-PREFLIGHT` | `cross-cutting`: fails criterion 1 **by design** — it orders five *other* workflows' entry points and has no user-initiated operation of its own. Probe evidence recorded: separable pure decisions exist, and their sole consumer is a gate serving five workflows, which is verbatim the cross-cutting criterion |
| `WFR-PLAIN-DISPOSAL` | `cross-cutting`, surface obligations **discharged**: one surface, one accessor, named lane components, three driven proofs with a quiesce mechanism that did not exist before, no shared limit moved or forked |
| `WFR-AUTOMATION-SPINE` | `cross-cutting`: no facade, no role home, no ordered stage sequence of its own — a read-only projection every workflow feeds, advanced incrementally by **eight** slots (2a, 2b, 3a, 3b, 4, 5a, 5b, 6) before 7b gave it its terminal status — **nine** ledger rows name it. `exempt` was rejected in advance and rightly: a row eight slots kept advancing is not what `exempt` means |
| `WFR-BUFFER-SNAPSHOT`, `WFR-MIGRATION-LEDGER` | `cross-cutting`, unchanged; the ledger row's probe evidence is recorded here for the first time |
| `WFR-EDITOR-MEMORY` | `exempt`, unchanged; its probe evidence is recorded here for the first time |

**What is explicitly not claimed:**

- **The live-display walkthrough and the manual Orca check were not performed**,
  and this change does **not** write "accepted" against either, in the matrix, in
  the programme record, or here. They are user-gated, and the geometry row is
  the one that most needs the first of them. Eight consecutive slots have now
  shipped without it; that is recorded as the programme's standing gap, not as a
  discharge.
- The **ten open `[~]` items** are inventoried, not discharged: **8** user-gated and **2** machine-gated (B.3's table).
- The two ratchet rows' `delete field` residue is **carried**, with the per-row
  decision recorded, not closed.
- `scan_execution.rs` and `ui/plain_disposal.rs` are recorded as **accepted
  refactor debt** — the only two items in the programme so recorded, and both
  named with the reason a line-count split would be the error the convention
  forbids.
- Everything else outstanding is in the single deferral inventory with a gating
  condition and an owner. **Nothing else is recorded as accepted debt.**

