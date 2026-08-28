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
> **Capability deltas 1 and 2 live in this change**, relocated from slot 7a's
> directory at authoring under its own task 0.14a: a delta must not ship in a change
> that cannot discharge its obligation. Delta 1 needs every row terminal; delta 2
> needs the lane's surface. Both were re-based against the live specs and needed no
> edit — each is a strict superset of the live requirement it modifies, with zero
> modified or removed lines.
>
> **Authoring inputs consumed from slot 7a, not re-derived** (its B.0): §D1's
> resolution with its ≥12 stage orders and 15-of-18 co-located state groups; the four
> contested-file verdicts; §D6's constraint re-proved intact; the corrected cells
> including `WFR-SHELL-LAYOUT` as **tier-3** and the coverage proof stale by 68 files;
> and the `[~]` reconciliation — **23** markers, **16** of them slot 5a's and closed by
> 5b, **seven** genuinely open, **plus 7a's own two** makes **nine**.
>
> **Two inherited figures were falsified at authoring**, which is why this change
> re-derives rather than reports: the rustfmt reach gap is **411 hunks across 18
> files**, not 171; and slot 7a's `DisposalPressureEvidence` narrowing instruction is
> **unexecutable as written** — the type is already `test-utils`-gated and its only
> reader is another crate, for which `pub` is the narrowest visibility that compiles.
>
> **The recurring lesson, now seven slots deep:** a handed-on number is a hypothesis,
> a census cell can be wrong in its *kind* and not only its magnitude, and a step that
> quietly succeeds against the wrong input is the defect class this programme keeps
> rediscovering. This change's own verification cannot be read off exit codes.
>
> **What this change may not do:** re-open §D1; split a row on line-count evidence;
> create a `pending` row; write "accepted" against a user-gated gate; or claim the
> programme complete while any row, delta, or inventory item is outstanding.

---

## 0. Gates, orientation, decisions, and premise re-verification

Every decision task precedes every structural task, because §E1's ownership verdicts
determine which files move and therefore which gates need re-keying. Do not start
section 4 before section 0 closes.

- [ ] 0.1 `git add -N` every new file **as soon as it exists**, before running any
  diff-aware gate. `make check-visual-proof-policy`, the diff-aware half of
  `make check-accessibility-policy`, and `make mutants-diff` build their changed-file
  set from `git diff <base>`, which does not list untracked paths at all. This change
  creates **six new role-home directories**; a green diff-aware gate computed over a
  file set that omits all of them is not evidence. Re-run the lane after the files are
  visible; if adding them changes a digest and the gate starts failing, re-run the
  lane rather than unstaging.
- [ ] 0.2 **Confirm the relocated deltas still match the live specs** before planning
  around them. `diff` each delta's requirement body against the live requirement of
  the same title and confirm the delta remains a strict superset. Slot 7a's delta 3
  was synced into `openspec/specs/mutation-testing/spec.md`; confirm nothing that
  sync touched is quoted by delta 1 or 2. Record the two diffs' shapes in A.1.
- [ ] 0.3 **DECISION (§E1): confirm each replacement row satisfies criterion 1**, from
  the stage trace §D1 already derived — one user-initiated operation, or a family
  sharing one ordered stage sequence — recorded **per surface with its evidence**.
  - [ ] 0.3a Show that **no two of the seven share one ordered stage sequence**.
    *"Where two candidates share one stage sequence they are one row, not two."* The
    pairs to test explicitly: geometry versus transient dismissal (both react to
    window-level state), tab strip versus recent documents (both open documents), and
    Focus Mode versus geometry (both suppress chrome).
  - [ ] 0.3b Show that `WFR-TAB-STRIP`'s pin, bulk-close, and reorder paths **share**
    the close stage order rather than being three stage orders. If they do not, the row
    is wrong and the trace must be re-read before a row is added.
  - [ ] 0.3c Argue `WFR-RECENT-DOCUMENTS`'s **two** stage orders as one workflow's. Two
    stage orders in one row is permitted — the exemplar has two — but it is the
    measured budget stressor, so the argument and the facade projection are the same
    decision.
  - [ ] 0.3d Record `WFR-STARTUP-PREFLIGHT` as **failing** criterion 1 by design, with
    the five workflows it orders named, and take `cross-cutting` with the probe
    evidence delta 1 requires of a non-migrating resolution.
- [ ] 0.4 **DECISION (§E1): the row count against the declared maximum.** The candidate
  table listed five; §D1 removed one; §D1's findings created three. State the four
  inside the maximum and the three forced by findings 2 and 4 and by the coverage
  proof, each with the finding that forced it. **Do not present the departure as a
  consequence of file sizes** — if any row's only support is its line count, it is the
  forbidden budget response and must be withdrawn.
- [ ] 0.5 **DECISION (§E1): the four reassignments**, each on the code rather than on
  the handoff.
  - [ ] 0.5a `dialogs.rs` — which of `WFR-DOCUMENT-SAVE` and `WFR-DRAFT-RECOVERY` owns
    which of its five stage orders, and whether the file is a coordination module of
    one and a called presentation surface of the other. Name its three unrecorded
    freshness/identity values and decide whether each becomes a seam value object of
    the receiving row.
  - [ ] 0.5b `ui/window/search.rs` (955/928) — confirm it holds two of
    `WFR-SEARCH-REPLACE`'s coordination stages plus one coordination job of its own,
    assign **bounded coordination role names** accordingly, and correct that row's
    cell, which reads "all under `ui/search_panel/**`" and is now false by 928
    production lines.
  - [ ] 0.5c `focus_indexing.rs`'s palette story — `WFR-COMMAND-PALETTE`'s cell says
    the file *"stays window code"*. Decide which of the cell and the reassignment is
    wrong, on the code.
  - [ ] 0.5d `mod.rs`'s `setup_theme_selector` (~100 lines), the tenth story §D1 found
    in neither list — tier list, or a stage of the geometry sequence. Decide and record.
  - [ ] 0.5e **Re-derive every receiving row's staled measured cells** in this change.
    Delta 1's cross-row staling statement is this change's own delta, so the "record
    that they are stale" escape it grants is not available here.
- [ ] 0.6 **Project each new facade before writing it**, against the projections in the
  proposal, and record the projection beside the measurement in A.11 so a falsified
  projection is visible as such. **The tightest repo margin is 1 line**
  (`ui/search_panel/mod.rs` at 369). Plan against 1, not against slot 7a's 105–270
  landings. `WFR-RECENT-DOCUMENTS` is the declared escalation candidate; if it exceeds
  370 after honest delegation, take escalation step 1 and record the attempt before
  considering step 2.
- [ ] 0.7 **DECISION (§E2): role homes.** Confirm that `ui/window/policy.rs` and
  `ui/window/mod.rs` are both unavailable as flat role names, and take a per-workflow
  subdirectory for every new row in `ui/window/`. Confirm `ui/open_popover/` as
  `WFR-RECENT-DOCUMENTS`'s canonical role home with `window/recent_open.rs` in the
  nested position, and decide whether that module takes a bounded coordination role
  name or is recorded as a called presentation surface.
- [ ] 0.8 **DECISION (§E4): the disposal surface's shape and visibility**, before any
  edit. Enumerate the four typed observation values and six accessors; state that
  `DisposalCapacityHold` / `ProgressDisposalCapacityHold` are actuation and stay out;
  state that `DisposalOwned<T>` and `DisposalPermit` are seam values in ten workflows'
  signatures and stay unchanged; and **measure the reader set of every type before
  choosing a visibility**. "Already narrowest" is a legitimate recorded outcome.
- [ ] 0.9 **Confirm the nine open `[~]` items are not inherited as work.** Verify by
  path that slot 4's two, slot 5a's, slot 5b's 7.6 and 10.13, slot 6's 10.19 and 10.20,
  and slot 7a's 10.22 and 10.23 are user-gated and stay user-gated. This change's
  contribution is to **inventory** them in one place (task 11.4), not to discharge
  them. State **23 / 16 / 9** together — a reader who greps 23 markers and finds 16
  reassigned will otherwise conclude sixteen items were abandoned.
- [ ] 0.10 **Re-verify Finding 6's six items against the code**, not against the brief
  that named them. Two inherited figures are already falsified (411 hunks not 171; 8
  destructuring sites binding 15 placeholders, not "~70 lines"). For the "S12"
  ledger-check holes, whose only evidence is a label that appears nowhere in the
  repository, re-derive from `check-workflow-boundaries.py`'s four documented ledger
  failure conditions and record what the re-derivation finds, including "no hole
  exists".
- [ ] 0.11 **Re-verify slot 7a's own inheritances** rather than trusting them: that the
  two `ui/automation.rs` reach-throughs are gone (they are — `current_readiness_failure`
  now iterates `window.open_editors()`), that `ui/window/policy.rs` is present at
  813 physical, that §D6's six predicate pairs and six self-test keys are intact, and
  that `MinimapEvidence` is still absent from `EVIDENCE_PROJECTIONS`.
- [ ] 0.12 **Resolve slot 7a's internal appendix contradiction** about
  `check-accessibility-policy`'s summary-absence fail-open: A.6 finding 1 records it
  **fixed and proved by deliberate red**, A.13a records it *"Not fixed here"*. Read the
  script. Record which is true, and if it is unfixed, fix it with its own self-test arm.
- [ ] 0.13 **Record the split decision point and its trigger.** Six new facades exceeds
  slot 7a's five, and 7a split. The boundary is **after `WFR-SHELL-GEOMETRY`**; the
  trigger is the data-safety pass, the recent-documents seam retirement, or the §E3
  re-key consuming the change's capacity. **Taking the split moves both deltas again**
  into 7c, by the same rule that moved them here: a 7b that leaves the spine `pending`
  cannot carry delta 1. Taking it also means replacing the ledger's `slot 7b` line with
  `slot 7b` and `slot 7c` lines and splitting the remaining-scope row. It never
  renumbers, and a partially migrated row is never an acceptable outcome.
- [ ] 0.14 **Quote the behavior anchors this change must preserve verbatim in
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

- [ ] 1.1 **Land delta 1's statements** in
  `openspec/specs/workflow-readability-boundaries/spec.md`: cross-row cell staling;
  terminal status at programme close with probe evidence; matrix/ledger reconciliation;
  provisional grouping rows and the forbidden line-count split; and the completion
  record with its deferral inventory and its no-self-acceptance rule.
- [ ] 1.2 **Land delta 2's statement** in
  `openspec/specs/workflow-evidence-surfaces/spec.md`: a cross-cutting lane owes the
  surface but not the facade, under the same visibility, reentrancy,
  non-materialization, and bounded-child rules with the same three proofs; no forked
  shared limit; the surface's file may keep the lane's name; discharge by the closing
  change.
- [ ] 1.3 **Retroactive re-check for delta 1(a) — cross-row staling.** For every
  migrated row, ask whether any earlier change assigned it files without re-deriving its
  cells. `WFR-SHELL-LAYOUT` was the known instance and is being retired; the question
  here is whether a **migrated** row inherited the same shape. Slot 3b's assignment of
  `ui/open_popover/**` is the template to look for.
- [ ] 1.4 **Retroactive re-check for delta 1(b) — terminal status.** Sweep every row:
  does any status label carry a trailing narrative that contradicts the label? Does any
  non-migrating terminal row lack probe evidence? `WFR-EDITOR-MEMORY` (`exempt`) and
  `WFR-MIGRATION-LEDGER` (`cross-cutting`) predate the probe rule; establish whether
  each records a probe and record the finding either way.
- [ ] 1.5 **Retroactive re-check for delta 1(c) — provisional groupings.** Is
  `WFR-SHELL-LAYOUT` the only residual grouping row? Test the criterion against every
  row that names more than one surface family, and record the negative findings.
- [ ] 1.6 **Retroactive re-check for delta 2.** Does any migrated row expose a second
  typed observation path alongside its surface, or an evidence type wider than its
  readers need? Slot 7a named three `pub` candidates from the Evidence Surface
  Baseline: `DisposalPressureEvidence` (this change's, §E4),
  `WorkspaceScanPressureEvidence`, and `NoteScoringEquivalenceEvidence`. **Establish
  each one's reader set before concluding anything about its visibility** — §E4's
  measurement shows why: a cross-crate widget-test reader makes `pub` the narrowest
  compiling visibility, and "narrow it" would be a regression dressed as compliance.
- [ ] 1.7 **Implement the mechanical half delta 1 still owes**: fail when a matrix row
  carries a transitional status while the ledger has no `outstanding` slot naming it.
  Slot 7a implemented the slot-agreement half and left this one, *"which travels with
  7b, the change that can produce a transitional status."* Prove it by **deliberate
  red** — produce a transitional status with no outstanding slot, see the gate fail,
  then close it.
- [ ] 1.8 **Decide whether `pending`, `deferred`, and `partially-conforming` stay in
  `KNOWN_STATUS_LABELS`.** Delta 1 says they must not survive the closing change. If the
  labels remain accepted by the gate, the rule is enforced by review only, which is the
  class the programme keeps fixing. If they are removed, the gate must still fail
  *informatively* on an unknown label rather than silently exempting the row. Decide,
  implement, and prove both arms.
- [ ] 1.9 `openspec validate --all --strict` after landing both deltas, and record the
  pass/fail counts. Slot 7a recorded 111 passed / 0 failed after delta 3.

---

## 2. Path-keyed gates, mutation scope, and drift-gate registrations

- [ ] 2.1 **Observe the disarm before fixing it (§E3).** With the geometry code moved
  out of `imp.rs` and `actions.rs` and **no key added**, run
  `make check-visual-proof-policy` and the `cargo-gtk-proof` half and show each
  **passing while protecting nothing**. Record the observation. This is the property
  that makes reviewing the edit insufficient, and it is the only step that proves the
  re-key was necessary rather than decorative.
- [ ] 2.2 **Re-key to the narrowest key that still selects exactly the protected code**,
  in **both** implementations: add
  `crates/lushtext-core/src/ui/window/geometry/` as a role-home prefix constant
  alongside the retained `actions.rs` and `imp.rs` literals. A
  `crates/lushtext-core/src/ui/window/` prefix is **forbidden** — it would demand two
  pixel invariants and the sidebar animation matrix of seven subdirectories, four of
  them migrated role homes no predicate has ever protected.
- [ ] 2.3 **Do not remove `actions.rs` or `imp.rs` as keys** without arguing the
  behavior. Both retain protected code (§E1: `actions.rs` is not demotable; `imp.rs`
  keeps its template-child and non-geometry halves). Narrowing a key because *some* of
  a file's content moved is a scope change and must be argued on the behavior, not the
  rename.
- [ ] 2.4 **Verify the mutation glob still reaches the moved `policy.rs`.** Moving
  `ui/window/policy.rs` into `ui/window/geometry/policy.rs` keeps it inside
  `ui/**/policy.rs` by convention — **verify it after the move** rather than assume, per
  the nested-home rule, and re-derive the module's mutant count from the tool. The
  sources disagree: the matrix records **80** mutants with 15 survivors triaged to zero
  for `adaptive_shell.rs` → `policy.rs`; report what the tool says now, and state the
  figure as **relocation parity** (before → after) rather than as gain, because this
  module's gain-from-zero was already paid by slot 7a.
- [ ] 2.5 **Add a parity assertion to each implementation and prove each by a deliberate
  red.** One assertion on one side is the half that passes while the other side is
  wrong. Confirm the Python half's self-tests actually run — slot 6 found them
  unreachable.
- [ ] 2.6 **Re-key or retire every other path-keyed or string-keyed reference this
  change's moves touch**, and verify each **by running the lane** rather than by
  reading it: `.cargo/mutants.toml` `exclude_re` entries anchored on moved symbols,
  `scripts/run-performance-smoke.sh`'s 17 Criterion group names / 20 widget test names
  / 3 module-qualified test paths, and `scripts/check-automation-docs.py`'s
  `EVIDENCE_PROJECTIONS` paths. Slot 6's fail-open lesson applies: a filter that
  matches nothing exits 0.
- [ ] 2.7 **Retire the stale `ui/window/tabs.rs` calibration comment** in
  `.cargo/mutants.toml`, which records an exclusion the current `examine_globs` never
  applies. Slot 7a's proposal named it and did not reach it.
- [ ] 2.8 **Register each new evidence surface that projects to automation**, or record
  that none does. Slot 7a's projection count stayed at seven because its new surfaces
  are `test-utils`-gated and report no automation field; establish the same for each of
  this change's six, per surface, and prove any new registration **rejects a real
  rename** rather than asserting the drift gate works.

---

## 3. `WFR-PLAIN-DISPOSAL` — the lane's surface (§E4)

No facade, no coordination role names, no `policy.rs`. The row stays `cross-cutting`
and advances to `cross-cutting — surface obligations discharged`, matching
`WFR-BUFFER-SNAPSHOT`'s resolved form.

- [ ] 3.1 **Re-derive the row's measured cells** row-scoped, with the predicate stated
  on every figure because this is the census's only dual-gate user: `ui/plain_disposal.rs`
  at **1,542 physical / ~1,344 production** and `model/plain_disposal.rs` at
  **692 / ~465**; **8** `*_for_test` declarations; **17** `cfg(feature = "test-utils")`
  plus **13** `cfg(any(test, feature = "test-utils"))` sites in the `ui` half and
  **0** of either in the `model` half. State the direction of every correction, and
  record that an unchanged cell is a legitimate outcome.
- [ ] 3.2 **Consolidate the four parallel typed observation values into one surface**,
  reached through one accessor, with the ordinary and progress lanes as **named
  components** rather than two top-level accessors — the shape slot 7a used for
  `BufferSnapshotEvidence`. Retire all six existing accessors and update every reader
  in `crates/lushtext/tests/widget/{plain_disposal,window,command_palette,editor_page,
  markdown_preview,search_panel}.rs`.
- [ ] 3.3 **Record the visibility conclusion with its reader measurement.** Slot 7a's
  instruction to narrow `DisposalPressureEvidence` from `pub` is **unexecutable as
  written**: the type is already `test-utils`-gated and its only reader is
  `crates/lushtext/tests/widget/plain_disposal.rs:16`, in a different crate. State the
  measured reader set, state the narrowest visibility that compiles for it, and if that
  is `pub`, record "already narrowest" as the finding. Do not execute a narrowing that
  breaks the widget lane and call it compliance.
- [ ] 3.4 **Discharge the reentrancy proof with the lane quiesced.** This lane's state
  includes process-wide atomics and high-water marks mutated by **worker threads**, so
  read-to-read identity is not a property of the reader's control flow. Drain to a
  terminal, confirm zero running and zero queued **through the surface itself**, then
  assert identity; assert **monotonicity** rather than equality anywhere a worker can
  still advance a counter. Slot 7a's no-retry widget lane caught exactly this class —
  an unsound assertion in its own evidence proof whose panic read like a production
  defect, which a single retry would have hidden.
- [ ] 3.5 **Discharge the disposal proof.** The lane's surface is not derived from a
  `TemplateChild`, so state which stage plays the disposed-widget role for a lane —
  a torn-down owner whose pending job is cancelled — and prove the surface answers
  honestly rather than panicking.
- [ ] 3.6 **Discharge the non-materialization proof.** Read the surface with the lane
  empty and with it saturated, and show admission counters, retained-byte accounting,
  high-water marks, retry-source counts, and producer terminals **identical before and
  after each read**. Prove it, do not assert it.
- [ ] 3.7 **Confirm no shared limit moved or forked.** The lane's constants are consumed
  by ten workflows (`MAX_REPLACE_UNDO_RETAINED_BYTES`, `MARKDOWN_PLAN_RESERVATION_BYTES`,
  `STARTUP_PRELOAD_RESERVATION_BYTES`, `PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY`).
  Consolidating the surface must not relocate or duplicate any of them — delta 2 states
  it, and slot 3a's decision not to fork
  `char_count_requires_chunked_snapshot` is the precedent.
- [ ] 3.8 **Resolve the `DisposalProducer` family's 12 default-feature `never used`
  items**: `MAX_SMALL_PENDING_DISPOSAL_BYTES`, `try_own_for_gtk`,
  `DisposalProducerInner`, `DisposalProducer` and its five associated items, and
  `retry_pending` — all inside the 13 dual-gated sites. Retire what is dead; gate or
  justify what is live. They are invisible to `clippy --all-features`, which is slot 5b's
  lesson about which configuration hides what, so **check under default features**. Do
  not leave 12 warned items in the row the closing change declares settled.
- [ ] 3.9 **Record `ui/plain_disposal.rs`'s 1,344 production lines against the
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

- [ ] 4.1 **`WFR-TRANSIENT-DISMISSAL`** — role home `ui/window/transient_dismissal/`.
  Facade against the ≈120 projection. Preserve the dismissal contract exactly: Bubble
  phase so focused children, dialogs, popovers, dropdowns, and entries get first chance;
  exactly **one** topmost visible dismissible surface closed per Escape; Focus Mode
  after transient dismissal; palette click-away through `close_command_palette()` so
  saved-focus restoration runs; and the pointer sequence claimed so the same press does
  not activate an underlying control. Name the one-tick idle latch as an inversion with
  its resumption point.
- [ ] 4.2 **`WFR-FOCUS-MODE`** — role home `ui/window/focus_mode/`. Facade against ≈150.
  Preserve fullscreen ownership, preview compatibility, the affordance-hide
  `SupersedingTimer`, and the readable-column margin path that queues the preview
  layout-settle. Probe for a seam value object and for an evidence surface **before**
  concluding either is unnecessary, and record the negative finding — slot 7a's four
  `policy: none` rows all turned out to own a `policy.rs`.
- [ ] 4.3 **`WFR-EDITOR-MEMORY-EVICTION`** — role home
  `ui/window/editor_memory_eviction/`, extracted from `focus_indexing.rs`. Facade
  against ≈200. Carry its generation counter, its bounded idle continuation, its 8 test
  seams, and its **two race-injector hooks** across the move without changing timing.
  **Do not widen `WFR-EDITOR-MEMORY`'s `exempt` resolution** to cover this code
  (design non-goal); `model/editor_memory.rs` stays where it is and keeps its
  resolution.
- [ ] 4.4 **`WFR-RECENT-DOCUMENTS`** — canonical role home `ui/open_popover/`, nested
  with `window/recent_open.rs`. Facade against ≈250, **the escalation candidate**.
  - [ ] 4.4a Retire the row's **26 gated declarations across 37 sites** — more than
    every other shell surface combined — into one evidence surface plus one
    `test_policy.rs`. Fold `OpenPopoverRowLayoutSnapshot` in.
  - [ ] 4.4b Fix the **ungated** `window.imp().recent_documents.loading` read in
    `crates/lushtext/tests/widget/open_popover.rs`, inherited from slot 3b's census gap
    and never closed.
  - [ ] 4.4c Discharge the three surface proofs, with the **disposed-widget** proof read
    through `try_get()` — the popover's fields derive from template children, and slot
    5a's trap was that a transitive window accessor derefs one and panics.
  - [ ] 4.4d If the facade exceeds 370, take escalation **step 1** (extract called
    presentation surfaces, push stage bodies into coordination roles) and record the
    attempt and its measurement before considering step 2. Do not split the row.
- [ ] 4.5 **`WFR-SHELL-GEOMETRY`** — role home `ui/window/geometry/`, with
  `ui/window/policy.rs` moving in. Facade against ≈190.
  - [ ] 4.5a Preserve every anchor quoted in 0.14 **verbatim in behavior**. The
    allocation-path rule is the one a role move is most likely to break: allocation and
    programmatic notify paths clamp runtime geometry and cache derived thresholds, and
    **never** persist fractions to GSettings or reparse an `AdwBreakpoint` condition.
    Persistence stays tied to explicit user intent, restore, or animation completion.
  - [ ] 4.5b Keep the `workspace-sidebar-animation` readiness blocker with the
    **animation**, not with the row name, per slot 5a. Keep
    `ui/sidebar/width_preset.rs`'s `WorkspaceSidebarWidthPreset` in this row with its
    three consumers, already re-pointed by slot 5b.
  - [ ] 4.5c Preserve the `ClipBin` zero-minimum-height contract so the status bar can
    still be allocated inside the visible height, and the compact `AdwBottomSheet`
    bounded-natural-height contract.
  - [ ] 4.5d This row's files are **visual-sensitive**. Its changes require two named
    pixel invariants and the workspace-sidebar animation matrix; §E3's re-key is what
    keeps that requirement armed after the move, and task 10.13 is what proves it.
- [ ] 4.6 **`WFR-TAB-STRIP`** — role home `ui/window/tab_strip/`. Facade against ≈220.
  - [ ] 4.6a Take the close/delete half of `documents.rs` and leave the rest with its
    owner; state which row owns the remainder rather than leaving `documents.rs`
    unattributed.
  - [ ] 4.6b Carry slot 7a's teardown-before-close fix across the move **without
    duplicating the teardown**, and confirm its regression test still fails without the
    fix by deliberate revert-and-rerun. Slot 7a's B.4 records that the handoff's word
    "move" would have duplicated it.
  - [ ] 4.6c Pair every structural tab operation with explicit refresh of all
    tab-dependent UI, per `.agents/rules/widget-wiring.md`: signal ordering during
    `close_page()` is not guaranteed and `selected-page` may not fire when closing a
    non-selected tab. Also reset window-level projections that can outlive the previous
    tab (preview-only mode) before scheduling editor focus restoration.
  - [ ] 4.6d **Spend zero actuation seams.** Slot 5b's budgeted one remains unspent by
    6 and 7a, and this change plans to leave it unspent. If a stage is unreachable
    without one, say so and leave the coverage gap recorded rather than spending it
    quietly.
- [ ] 4.7 **One test policy per row.** Test-only timing and limit overrides belong in
  the row's single `test_policy.rs`, not in several module-level statics, and no
  override storage may compile without the test feature. Confirm each new
  `evidence.rs` and `test_policy.rs` is `test-utils`-gated with the module doc stating
  that production reads live state directly — the shape slot 6 fixed at
  `minimap/mod.rs:47`.
- [ ] 4.8 **For every row, probe for a seam value object and record the finding.** A
  bundle crossing two or more function boundaries or reconstructed at two or more call
  sites is reified; a bundle used by one private helper is not. Reuse the shapes the
  codebase already uses (Ticket + Facts + predicate; coordinator generation identity)
  rather than inventing a parallel one. **A value must not be renamed while crossing a
  seam.**
- [ ] 4.9 **Treat every `#[expect(clippy::too_many_arguments)]` this change would add as
  an unreified seam.** The workspace has exactly **1**, at
  `model/action_catalog.rs:177`, the exempt domain catalog constructor. Do not add a
  second.

---

## 5. Reassignments, cross-cutting rows, and the coverage proof

- [ ] 5.1 **Implement 0.5a's `dialogs.rs` verdict**, and re-verify the close-coordination
  contract in `ui/window/AGENTS.md` holds **exactly** after the reassignment: input
  rejected across the selected-save pipeline and later draft/session yields; discarded
  editor identity, content generation, modified state, and path fingerprinted at
  confirmation; active saves and freshness rechecked before cleanup and destruction; and
  retryable drafts plus sensitivity restored on every aborted close.
- [ ] 5.2 **Implement 0.5b's `ui/window/search.rs` verdict** with bounded coordination
  role names, and correct `WFR-SEARCH-REPLACE`'s size cell, which excludes 928
  production lines it owns.
- [ ] 5.3 **Implement 0.5c's `focus_indexing.rs` verdict** and reconcile
  `WFR-COMMAND-PALETTE`'s cell.
- [ ] 5.4 **Implement 0.5d's theme-selector verdict.**
- [ ] 5.5 **Populate the no-coordination-tier list** with each surface's evidence for
  all three properties the list's preamble claims — no ordered stages, no coordination
  role, no seam value-object obligation — recorded **per surface**. Expected entries:
  `zoom.rs`, `workspace_scope.rs`, `ui/open_popover/item.rs`, `ui/properties_panel/**`.
  **`transient_surfaces.rs` must not be re-added** and **`actions.rs` must not be
  demoted**.
- [ ] 5.6 **Resolve `WFR-STARTUP-PREFLIGHT`** as a `cross-cutting` row with no facade,
  naming the five workflows `startup_data.rs` orders and recording the probe evidence
  delta 1 requires — including a probe of the module for separable pure decisions.
- [ ] 5.7 **Dispose the unbounded startup activation-open queue** (slot 5a, un-homed by
  slot 7a): a bounded-work assessment with a named budget, or a `docs/next/` record with
  its gating condition and owner. It belongs to `startup_data.rs`, which is
  cross-cutting and owned by none, so *"it belongs to another row"* is not a
  disposition here.
- [ ] 5.8 **Establish whether slot 5b's M-5 format-gate fail-open site is inside this
  change's files** — the question slot 7a's task 0.10 left open — and dispose it under
  the proposal's disposition rule.
- [ ] 5.9 **Retire `WFR-SHELL-LAYOUT`** from the Product Matrix, replaced by the seven
  rows, each with a stable `WFR-*` id, its own measured cells, its slot cell naming this
  change, and its terminal status. Record the retirement rather than deleting the row's
  history.
- [ ] 5.10 **Re-derive the census coverage proof.** The matrix's proof reads *"198 files
  exist under `crates/lushtext-core/src`; 195 are attributed"*; slot 7a measured
  **266**. Re-derive both numerator and denominator, attribute every file, and show the
  delta against 198/195 with the cause. A split changes the attribution table, and an
  un-re-derived proof would be a false completeness claim at exactly the moment the
  programme claims completeness.

---

## 6. `WFR-AUTOMATION-SPINE` (§E5)

- [ ] 6.1 **Probe `ui/automation.rs` (2,214 / ~2,084) for separable pure decisions**
  before concluding the row owns no `policy.rs`, and record the finding either way.
  Slot 7a found five decisions in editor-find, six in notifications, and a whole dialog
  vocabulary in encoding, in **four** rows the census recorded as `policy: none`.
- [ ] 6.2 **DECISION (§E5): the row's terminal status.** Select `cross-cutting`
  (expected) or `migrated` on the evidence; `exempt` is rejected in advance because
  seven slots have advanced this row incrementally.
- [ ] 6.3 **Re-derive the row's evidence cell** rather than inheriting slot 7a's
  correction. Count the registered projections in `EVIDENCE_PROJECTIONS` and state the
  figure with its source; a correction is a measurement, and this change re-derives
  measurements.
- [ ] 6.4 **Strike the two retired reach-throughs from the ratchet table.** The matrix's
  `Production cross-widget reach-throughs still open, by owning row` table records two
  open entries at `ui/automation.rs:517`/`:518` reading `window.imp().tab_view`; both
  are **gone**, replaced by `window.open_editors()`. Mark them
  `~~...~~ RETIRED by slot 7a` per the table's own convention, and separately establish
  how many of the file's **15** current `.imp()` reads cross a workflow boundary under
  the table's predicate — the table's count and a raw grep are not the same measurement.
- [ ] 6.5 **Verify, do not inherit, the `MinimapEvidence` non-registration verdict.**
  Slot 6 called it *"a result rather than an omission"* — the minimap's ≥18
  `visual_geometry.native_minimap` fields derive from live widget geometry rather than
  workflow state. The Completion Rule says *"any automation snapshot field for this
  workflow projects from the evidence surface"*, so read the fields and record the
  verdict. This is the second slot asked to verify it.
- [ ] 6.6 **Re-check slot 6's conditionally-cleared `minimap_work_pending` candidate.**
  The clearance holds only while no `mark-set` handler reads readiness; both handlers in
  the tree reach only scrolling and menu-model refresh. Record it as a **standing
  condition** in the closeout inventory rather than a closed item, so a future
  `mark-set` handler that reads readiness re-opens it visibly.
- [ ] 6.7 **Reconcile all three sources** (Finding 4): the matrix `Slot` cell, the
  Migration Order table, and the programme ledger. Include the slot-label mismatch
  (`2a` versus `2`), the dropped terminal-status clause, and slot 7a's omission of
  `WFR-AUTOMATION-SPINE (partial)` from its own `complete` line — correct it or state it
  as deliberate with its reason. Leaving a defensible omission undefended is how the
  next reader re-opens it.
- [ ] 6.8 **Prove the exported D-Bus contract is unchanged** by a measured diff of the
  exported schema and a before/after Automation1 capture of the same app state, not by
  assertion. Slot 2b's no-widening proof is the shape.

---

## 7. Data safety

Seven consecutive slots found at least one confirmed defect. This change carries two
tier-3 rows. Apply the proposal's disposition rule, and record the verdict, severity,
site, and owning row for every candidate **including the ones cleared** — slot 5a found
two tests that passed against broken code, and a test that cannot fail is worse than no
test.

- [ ] 7.1 **`WFR-PLAIN-DISPOSAL` (tier-3)** — audit the retirement lane's terminal
  ownership: does every path either carry the permit forward or release it? A dropped
  terminal strands whoever waited on it — slot 3b fixed exactly that shape in the load
  row — and a mis-accounted permit lets the next admission overshoot the budget. Cover
  the panic path (`catch_unwind`), the retry path, and exact-owner teardown
  cancellation.
- [ ] 7.2 **`WFR-TAB-STRIP` (tier-3)** — audit the **tab-pin and bulk-close paths in
  `tabs.rs`**, the neighbours two independent passes reached the teardown defect
  *without* examining. Confirm no path runs teardown before a cancellable
  `close_page()`, and that a cancelled bulk close leaves every surviving tab's load,
  monitor, and draft record intact.
- [ ] 7.3 **`WFR-SHELL-GEOMETRY`** — confirm no clamp, notify, or allocation path
  gained a GSettings write or an `AdwBreakpoint` reparse in the move, and that
  persistence still runs only from explicit intent, restore, or animation completion.
  A persistence write moved into an allocation path is a live-warning and
  monitor-refresh regression the widget lane cannot see.
- [ ] 7.4 **`WFR-EDITOR-MEMORY-EVICTION`** — audit eviction against in-flight saves and
  draft writes: can an eviction race a save's buffer snapshot, or evict state a draft
  autosave is about to persist? The two race-injector hooks are the instrument; use
  them.
- [ ] 7.5 **`WFR-RECENT-DOCUMENTS`** — audit the lazy projection gate: can a stale
  completion publish rows for a superseded query, and does the row's activation path
  re-check the path still exists before opening?
- [ ] 7.6 **`startup_data.rs`'s format-upgrade preflight** — the gate slot 5b found
  fail-open (M-5), and the surface `WFR-STARTUP-PREFLIGHT` makes terminal. Confirm the
  preflight cannot admit an unmigrated format on an error path.
- [ ] 7.7 **Land every handed-on finding in a `docs/next/*.md` record** with severity,
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

- [ ] 8.1 **DECISION: the rustfmt gate hole.** `crates/lushtext/tests/widget.rs` reaches
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
- [ ] 8.2 **Fix the proof that misdescribes its own premise.**
  `crates/lushtext/tests/widget/window.rs` asserts
  `print_evidence(&window).document.is_some()` at `:14566` and again at `:14575`–`:14578`
  under a comment claiming *"Verified: the surface still answered `Some` after
  `close()`, so a close-only test would have proved nothing"* — **the test never calls
  `close()`**. Either make the comment describe what the test does, or add the
  `close()` step the comment claims was verified, and remove the duplicate assert. A
  proof that reports a verification it did not perform is the honesty class this
  programme keeps naming, not a comment nit.
- [ ] 8.3 **Remove the dead tuple ladders** in
  `crates/lushtext/tests/widget/markdown_preview.rs`: **8** destructuring sites binding
  **15** `_` placeholders out of seven-element tuple literals left behind when slot 7a
  retired 11 tuple-returning seams into `MarkdownPreviewEvidence` — `:1208`, `:1303`,
  `:1322`, `:1385`, `:1415`, `:2223`, `:2413`, `:2454`. **Re-derive the line count under
  a stated predicate**; the inherited "~70 lines" is untraceable to any measurement.
  Read the fields from the surface directly.
- [ ] 8.4 **De-duplicate `ui/window/encoding/dialogs.rs`'s near-duplicate builders** —
  `append_action_row` (`:342`) and `append_action_row_with_sensitivity` (`:354`) — and
  only where the grouped-row contract in `.agents/rules/ui.md` is preserved **exactly**:
  one `AdwPreferencesGroup` per conceptual set, short row titles with clear subtitles,
  activatable rows for choices and non-activatable for facts, and the widget coverage
  that asserts the grouped section labels and representative row titles/subtitles. This
  file is a called presentation surface of a row migrated one change ago; a tidiness
  edit that changes dialog geometry is a regression.
- [ ] 8.5 **Re-derive the "three residual ledger-check holes".** The inherited label
  "S12" appears nowhere in the repository, so the finding arrives as a label with no
  evidence. Derive from `check-workflow-boundaries.py`'s four documented ledger failure
  conditions — a `complete` slot naming a non-`migrated` row; an `outstanding` slot
  naming a `migrated` row without `(partial)`; a named row id absent from the matrix; a
  matrix row that is neither `migrated` nor `exempt` with a slot assigned and no
  `outstanding` entry — plus the states this change can produce, and fix or record what
  the re-derivation finds. **"No hole exists" is a legitimate outcome** and must be
  recorded as one.
- [ ] 8.6 **Remove `git_lines`** at `scripts/accessibility_source_fingerprint.py:142`–
  `:143`: a one-line wrapper over `git_lines_checked` with **zero** callers, confirmed
  at authoring. Re-run the accessibility fingerprint lane afterwards — the module's own
  bytes are part of the relevant set, so removing dead code voids the proof.
- [ ] 8.7 **Resolve the A.6 / A.13a contradiction** from task 0.12 and record which
  appendix was right, so a later reader does not re-plan a fixed item or trust an
  unfixed one.

---

## 9. Mutation coverage

Report **relocation parity** and **extraction gain** as separate figures, each naming
the exact invocation and its file-level anchors. A rename of an already-pure module that
was never in the scope has no before-count: its result is a gain from zero and must not
be dressed as parity (slot 5b's finding G7 found exactly that conflation in the live
matrix).

- [ ] 9.1 **Re-derive the mutation floor from the tool**, not from recall:
  `MUTANTS_RE='ZZZ_NO_SUCH_MUTANT_ZZZ' make mutants-list`. Slot 7a measured **34**, all
  `delete field`, of which 12 in `services/file_tree.rs`. The floor is a property of the
  **name filter** only; `--in-diff` genuinely bounds a run.
- [ ] 9.2 **Do not scope a focused run with `--in-diff` over a diff containing a
  rename.** A rename is a whole-file delete plus add, so it measures far more than the
  logic that changed — slot 7a saw 347 mutants where the answer was 160. This change
  contains **six** role-home creations, so the hazard is at its maximum.
- [ ] 9.3 **`test -s` any diff file passed to `run-mutants.sh`.** `ensure_diff_file`
  silently substitutes a `git diff origin/main...` three-dot diff when the path is
  missing, and slot 7a got a plausible-looking `54 mutants: 1 missed` against the
  previous slot's committed diff. Check every survivor's path against the files you
  scoped.
- [ ] 9.4 **Report each new `policy.rs`'s mutant count** with parity and gain separated,
  and triage every survivor to zero or to a narrow documented equivalence whose
  invariant is pinned by its own test, so the exclusion cannot outlive its
  justification.
- [ ] 9.5 **Triage slot 7a's 160 newly-in-scope mutants.** Slot 7a states plainly that
  they are untriaged and that *"the figures reported are generation counts, not kill
  counts"*. A programme cannot be closed over an untriaged scope expansion its own last
  change created. Run against a **committed or explicitly passed** diff —
  `make mutants-diff` proves nothing on an uncommitted worktree and exits 0 doing it.
- [ ] 9.6 **Re-derive the two ratchet rows' current survivor counts** from the tool:
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

- [ ] 10.1 `cargo fmt --all --check`, plus whatever reach task 8.1 selects. Record both.
- [ ] 10.2 **Both feature configurations.** The documented blocking command uses
  `--all-features`, which **hides** breaks the default-feature build reports — slot 5b
  found `origin/main` not compiling under default features while `make check` was green,
  and slot 7a's orphaned `cfg` attribute was caught only by the default-feature rustdoc
  build. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`
  **and** the default-feature build, and record both. Task 3.8's 12 items live in
  exactly the configuration `--all-features` cannot see.
- [ ] 10.3 **The rustdoc gate, by hand** — `make check` does not run it:
  `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo doc --workspace --no-deps`.
  This change ships **six** new facades in new `pub` role homes, which is the exact
  shape that trips `private_intra_doc_links`: a facade naturally wants to link its own
  coordination modules and `pub(crate)` seam values. The fix is **always** to drop the
  link and keep the name in backticks; never widen visibility to satisfy
  documentation. This class has shipped three times because the local gates were green.
- [ ] 10.4 `make check-policy`, and separately `make check-workflow-boundaries`,
  `make check-automation-docs`, `make check-filesystem-boundary`, and
  `make check-blueprint`. Record each.
- [ ] 10.5 `make check-accessibility-policy`, after 8.6's edit and after every `ui/**`
  edit. It requires the accessibility and visual smoke summaries to carry a source
  fingerprint matching the current tree, digested from relevant-file **contents**.
- [ ] 10.6 **Accessibility wiring for six new role homes.** Update
  `docs/accessibility-matrix.md` rows for the shell surfaces, route all new metadata
  through `crate::ui::accessibility`, refresh and clear row metadata in
  `connect_bind`/`connect_unbind` for the recent-documents factory, and confirm no
  hover-only affordance lost its keyboard or context-menu alternative. Slot 5b's lesson
  applies to this change's many module-doc rewrites: a module doc describing an
  affordance that has **moved away** is a real gate finding, and the fix is to name the
  owning module.
- [ ] 10.7 **State-extreme coverage for every collection surface this change touches**:
  no tabs / no recent documents / no workspaces, one or a few, and many-or-awkward
  (long paths, capped results, deep nesting). Assert the user-visible contract — right
  empty copy, header and close controls visible, only the item region scrolling, no
  unintended scrollbar in an empty status-only surface — not only model state.
- [ ] 10.8 `make test`, then `make test-widget-headless` with **no retries** and **zero
  `FLAKY:` lines**. A `FLAKY:` line is a blocker, not accepted noise. Task 3.4's
  quiesced-lane proof is the assertion most likely to produce one; if it does, fix the
  assertion's soundness rather than the budget.
- [ ] 10.9 `make test-prop` and `make fuzz-corpus-replay`.
- [ ] 10.10 `make test-workspace-row-states`, and every focused lane whose string filter
  task 2.6 touched — **content-asserted**, not exit-code-accepted.
- [ ] 10.11 `make performance-smoke`, **content-asserted**: grep the lane summary for the
  asserted lines and confirm every filter this change's renames touch still matches a
  non-zero number of tests.
- [ ] 10.12 `make crash-recovery-smoke` and `make automation-smoke`. Both exercise paths
  this change's rows own — draft and session recovery through the shell, and the spine.
- [ ] 10.13 `make visual-geometry-smoke` from a **clean artifact root**. The geometry row
  is visual-sensitive: the run must pixel-verify both named invariant ids, include
  per-case pixel rows, final-frame rendered-anchor stability, and final
  sidebar/editor/minimap geometry. Use `pixel_verified_invariant_ids` and
  `animation_verified_invariant_ids` by their correct names — slot 6's task wording
  named a field that will never hold the animation id.
- [ ] 10.14 `make visual-smoke`, `make accessibility-smoke`, and
  `make builder-diagnostics-smoke`, each from a clean root, after the last source edit.
- [ ] 10.15 `make check-gtk-lush-policy` and `make check-gtk-lush-adoption` — no GTK
  Lush extraction is proposed, and these prove none happened.
- [ ] 10.16 `openspec validate --all --strict`.
- [ ] 10.17 **Any `ui/**` edit made during verification voids the accessibility, visual,
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

- [ ] 11.1 **Advance every matrix row to a terminal status** with probe evidence for
  every non-migrating resolution, and confirm `make check-workflow-boundaries` passes
  **truthfully** rather than because a claim was weakened.
- [ ] 11.2 **Update the slot ledger and the remaining-scope table together**, and close
  the `slot 7b` line. Delete nothing from the ledger's grammar section: the `(partial)`
  rules and the four failure conditions stay, because they are what makes "complete"
  checkable.
- [ ] 11.3 **Replace the matrix's facade table** with every migrated facade re-measured
  in this change — the current table lists **eleven** where **sixteen** rows are
  `migrated`, omitting all five of slot 7a's — and **re-base its prose**, which still
  reads *"only two workflows are migrated today"* and *"slot 3 must plan against 1
  line"*. Verb: **re-measure**, not confirm.
- [ ] 11.4 **Write the programme completion section** in
  `docs/next/workflow-readability.md` per §E6: measured outcomes against the section 2
  baseline in the same delta-table shape prior slots used; the refreshed
  `Measurement Definitions` denominators, which are the programme's actual ratchet; the
  **single deferral inventory**; and the explicit statement of what is **not** claimed.
  The inventory must carry, each with its gating condition and owner: the **nine** open
  `[~]` items stated together with the **23 / 16** reconciliation; the two
  programme-level deferrals with their justification bars; `scan_execution.rs`'s ~2,000
  production lines; `ui/plain_disposal.rs`'s 1,344; slot 6's conditionally-cleared
  `minimap_work_pending` **standing condition**; slot 5b's unresolved candidates; the
  two ratchet rows' residue after task 9.6; and everything tasks 5.7, 7.7, and 8.1 hand
  to a `docs/next/` record.
- [ ] 11.5 **Correct every stale pointer in both documents** (§E6 component 5): the
  record's status-line count (**"eleven"** against its own baseline's **16**), its
  slot-6-terminated migrated-row list, *"Slots 5 through 7 remain authorable"*, the
  `| 5–7 | not yet authored |` change-name row, §3's *"Three are complete"* preamble,
  §7's parenthetical calling `WFR-MINIMAP` and `WFR-MARKDOWN-PREVIEW` deferred, and the
  coverage proof's 198. A closeout is the last moment anyone reads these with the whole
  programme in view.
- [ ] 11.6 **Record the convention and tooling friction this change hit**, each entry hit
  while *using* the thing rather than reading it, in the programme record's friction
  section — including the class Finding 6 belongs to: a review pass whose findings reach
  no artifact is a worse handoff than one that reaches an archived directory, and the
  fix is a durable home, not a better memory.
- [ ] 11.7 **Re-derive the dangling evidence-pointer set.** The matrix's finding G8
  records **twelve** dangling `mutation parity` pointers across four archived changes and
  that *"the gate cannot catch this"*. Re-derive the current set rather than inheriting
  twelve or the inherited "seven", fix each, and decide whether the gate can be made to
  catch it — if it can, that is a mechanical half worth landing in the change that
  closes the programme.
- [ ] 11.8 **Correct earlier handoff text** so a later reader does not re-plan a non-item
  or inherit a wrong pointer: slot 7a's proposal says *"five self-test keys"* in one
  place and six in another (**six** is correct); its narrowing instruction for
  `DisposalPressureEvidence` is unexecutable as written (§E4); its A.11/A.14/A.15
  appendix sections read as though no facade was written while its own header reports
  five; and its A.6/A.13a pair contradicts itself about the accessibility gate (task
  8.7).
- [ ] 11.9 **State what is terminal, on what grounds, and what is not claimed**, per row,
  in B.6's shape — and end with whether anything is recorded as accepted debt. Nothing
  may be.
- [ ] 11.10 **At archive time**, rewrite this change's evidence pointers from live form
  to archive form — the step five prior changes missed. Until then they stay in live
  form, which is the only form that passes the gate while the change is live.
- [ ] 11.11 **Run the repository-learning review** and land whatever durable guidance
  this change earned in `.agents/rules/*`, `AGENTS.md`, `README.md`, and the affected
  skills' references. `.agents/rules/documentation.md` requires the matrix and the
  programme record to advance in the same change as the code; this task covers the rest.

---

## Appendix A — orientation record

Each section is filled by the task named in its heading. An appendix section left empty
means its task did not run.

### A.1 Delta re-basing against the live specs (task 0.2)

### A.2 Criterion-1 evidence, per replacement row (tasks 0.3, 0.4)

### A.3 The four reassignments and the cells they staled (tasks 0.5, 5.1–5.4)

### A.4 Role-home selections and their collision analysis (task 0.7)

### A.5 Behavior anchors quoted, with the rules file each is in (task 0.14)

### A.6 Retroactive re-check results for both deltas (tasks 1.3–1.6)

### A.7 The gate disarm observed, and the re-key that closed it (tasks 2.1–2.5)

### A.8 The disposal lane's surface: shape, readers, visibility (tasks 0.8, 3.2, 3.3)

### A.9 The three surface proofs, and how the lane was quiesced (tasks 3.4–3.6)

### A.10 Data-safety pass: every candidate with its verdict, including cleared (§7)

### A.11 Facade projections against measurements, all sixteen-plus rows (tasks 0.6, 11.3)

### A.12 Mutation figures, parity and gain separated (§9)

### A.13 Lane consequences of this change's moves (§10)

### A.14 Cold-read check: can a reader name each new row's stages from its facade alone?

### A.15 The coverage proof, re-derived (task 5.10)

---

## Appendix B — closeout

### B.0 Whether the declared split was taken, and on what trigger (task 0.13)

### B.1 Programme and matrix agreement (tasks 11.1, 11.2)

### B.2 Programme completion record: measured outcomes against the baseline (task 11.4)

### B.3 The single deferral inventory (task 11.4)

### B.4 Findings landed in `docs/next/` (tasks 5.7, 7.7, 8.1)

### B.5 Convention and tooling friction this change hit (task 11.6)

### B.6 Corrections to earlier handoff text (task 11.8)

### B.7 What is terminal, on what grounds, and what is not claimed (task 11.9)
