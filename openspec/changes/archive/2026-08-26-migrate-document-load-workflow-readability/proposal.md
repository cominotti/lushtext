## Why

This is **slot 3b** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`, and the change that finishes slot 3. Slot 3
as the record scopes it is "save and load"; it was split into 3a
(`migrate-document-save-workflow-readability`, `WFR-DOCUMENT-SAVE`) and this
change (`WFR-DOCUMENT-LOAD`), under the ledger grammar the record already
sanctions for `2a`/`2b`. 3a's proposal carries the split rationale; the short
version is that slot 3 holds **two independently tier-3 workflows** rather than
one workflow with a tier-3 half, they jointly occupy a 1,795-line file the record
names as the programme's third measured symptom, and each needs its own
verification matrix.

**Prerequisite, non-negotiable: 3a must be archived first.** Not a formality —
three concrete dependencies:

1. **They share `ui/editor_page/load_save.rs`.** 3a lifts the save half out and
   leaves a load-only residual with a module-doc pointer here. This change
   dissolves the file. Running them concurrently means two changes rewriting the
   same 1,795 lines.
2. **3a's spec delta establishes the role home this change reuses.**
   `ui/editor_page/` hosts eight workflows and the convention fixes the role file
   names `policy.rs` and `evidence.rs` at one per workflow, so both cannot be flat.
   3a amends `gtk-adapter-module-boundaries` to permit a per-workflow subdirectory;
   this change consumes that permission rather than re-litigating it.
3. **3a hands over shared-field ownership decisions.** Its task 3.2 records who
   owns the fields that straddle the save/load cut — the save path's cancellation
   of an in-flight load, `size_check` (documented as "size classification from the
   last file load" but read by the save path), and the restore-position group that
   sits inside the save write path while serving load restore. This change owns
   the load side of every one of those.

Load is the second half of the file the record describes as "1,795 lines holding
two workflows", and it is the **larger** half: authoring estimated it at roughly
1,046 lines from this side and roughly 1,088 from 3a's, counting the 314 lines of
chunked-install slicing free functions and a long run of the `impl`. The two
estimates differ because the halves interleave rather than occupying clean ranges
(3a's rationale enumerates the interleavings), so neither is authoritative;
**3a's task 12.2 hands over the measured residual line count, and task 0.3 uses
that rather than either estimate.** The matrix's stage trace records four inversions — an
admission drain, a worker read-and-decode completion, bounded install slices
resuming per slice, and finalization — and names the freshness check as the
unreified seam. That seam is `{load_generation, cancel_token}`: already grouped
inside `load_runtime`'s request type, then **exploded back into loose parameters
at both call sites** and compared clause-by-clause at the completion. The matrix
requires `LoadRequestTicket` with an `is_current(&editor)` predicate matching
`SaveCompletionTicket`'s shape.

Load is `tier-3` because it installs decoded bytes into a live buffer under
cancellation, and because it is the path a user reaches a file through: a wrong
freshness verdict shows the previous document's content, or a cancelled load's
content, in a tab that claims to be a different file. It is also the workflow
whose bounded install slices carry the paragraph-boundary contract
`.agents/rules/rust.md` records — a slice that stops mid-paragraph re-lays-out
everything already installed in that paragraph on every later slice, which is the
quadratic behavior that once froze crash recovery of a 33 MB single-line draft for
minutes. **That contract is behavior this change must preserve exactly**, and it
is the reason the install slicing is read before it is moved.

The tier-3 proof gate is satisfied several times over: slots 1, 2a, 2b, and 3a are
complete when this change starts. Confirm it mechanically — see task 0.1.

### The `model/file_load.rs` decision the census deferred to this slot

The `Policy Module Census` records `model/file_load.rs` as "4 consumers,
domain-shaped but sits close to the boundary" and says explicitly that
`WFR-DOCUMENT-LOAD`'s migration in slot 3 "must decide it explicitly rather than
inheriting this row". The verified reference set is **6 production files** plus 3
test/bench files: `model/mod.rs` (the module declaration),
`model/save_admission.rs` (so the two admission models are already coupled),
**`services/editor_io.rs`**, `ui/plain_disposal.rs`,
`ui/editor_page/load_save.rs`, and `ui/editor_page/load_runtime.rs`.

The `services/` consumer settles it the same way slot 2b settled
`model/workspace_search.rs`: a service depends on it, so relocating it under
`ui/` would invert dependency direction (`services -> ui`), which the convention
forbids outright. It is not single-workflow policy either — its three `ui/`
consumers span **two** owning workflows, `WFR-PLAIN-DISPOSAL` (cross-cutting,
slot 7) and `WFR-DOCUMENT-LOAD` — and cross-cutting eligibility counts owning
workflows rather than consuming files, so two owners clears the bar without the
service argument. **The decision this change records is that it stays in
`model/`**: it is already pure, already mutation-scoped through `model/**`, and
already carries co-located unit tests, so the move would trade a
dependency-direction violation for nothing. The change corrects the census cell
in the same breath so slot 4 or 7 does not re-open it.

An earlier authoring pass reported nine `ui/` consumers here; six were grep
false positives on the substring `file_load` rather than references to the module
(`file_load_active` in `ui/automation.rs`; `connect_file_loaded` in
`ui/window/notes/mod.rs` and `ui/window/focus_indexing.rs`;
`file_loaded_callbacks` in `ui/editor_page/imp.rs` and `ui/editor_page/mod.rs`;
and a test function name in `ui/window/drafts.rs`). Task 0.3 names those families
so the overcount is not re-derived. The census cell is still wrong in the other
direction — 4 against a real 6 — which is why premise re-verification is task 0.3
rather than a footnote: this is the third census consumer count to need
correcting, and the correction can go either way.

### Facade budget: the position this change takes

**No amendment is proposed, and the budget line is not to be edited.** The
reasoning is 3a's, restated because it is this change's constraint too:

- **The 1 line of headroom the record warns about belongs to
  `ui/search_panel/mod.rs`, not to a global allowance.** The gate measures each
  migrated row's declared facade separately. This change's concrete exposure is
  that it must not add a physical line to the exemplar's facade at 369/370, and
  must not push 3a's freshly measured save facade over either.
- **The load facade is measured independently and gets the whole 370.** It
  narrates **one** stage order with four inversions. The exemplar's 369 narrates
  twelve inversions across two stage orders; the palette's 335 narrates eight
  across two. Slot 2a's finding — what makes a facade long is stage *bodies*, not
  stage narration — applies directly: the 314 lines of install slicing are stage
  body and belong in coordination.
- **The one honest risk is stated rather than hidden.** Load has more distinct
  *entry points* than save (`win.open-file`, `win.open-recent`, `Ctrl+O`,
  `Ctrl+K`, sidebar row activation, session restore, reopen-with-encoding) and its
  cancellation and abort paths are real narration, not decoration. If the honest
  narration does not fit after delegating stage bodies, compressing inversion
  bullets, and folding module-ownership detail into the role table — the exact
  sequence that brought slot 2b back from 379 to 369 — then **escalate in-change
  with the measured count**. The record says a budget correction is cheaper now
  than at slot 6 and that the window is closing; it also says raising the number
  requires re-checking every migrated row in the same change, which by now is
  three rows plus save. Make the case explicitly or make the narration fit. Do
  neither by editing the line quietly.

## What Changes

- **Dissolve `ui/editor_page/load_save.rs`** by extracting `WFR-DOCUMENT-LOAD`
  into a per-workflow role home, `ui/editor_page/load/`, using the role home 3a's
  delta permits: a narrative facade (`mod.rs`), coordination modules named from
  the bounded set, pure policy (`policy.rs`), and an evidence surface
  (`evidence.rs`). `load_runtime.rs` is retired — `runtime` is the name the
  convention rejects, and the census found it naming three different jobs across
  four files. After this change the file the programme cites as "1,795 lines
  holding two workflows" no longer exists.

- **Reify the freshness seam as `LoadRequestTicket`**, carrying
  `{load_generation, cancel_token}` with an `is_current(&editor)` predicate
  matching `SaveCompletionTicket`'s shape, per the matrix's Seam Value Objects
  section. Constructed once at the workflow entry point and validated as a unit,
  so the pair stops being exploded into loose parameters at the two dispatch sites
  and compared clause-by-clause at the completion.

- **Decide `model/file_load.rs` explicitly, as the census requires: it stays in
  `model/`.** Correct the undercounted consumer cell, move the row into the
  matrix's "Modules confirmed as domain and staying in `model/`" list with the
  dependency-direction reason, and leave a pointer at the old location so a reader
  following the census snapshot does not think the decision is still open.

- **Extract the load workflow's pure decisions into
  `ui/editor_page/load/policy.rs`**: the chunked-versus-direct install threshold,
  the slice-size and **paragraph-boundary** rules that keep bounded installation
  linear rather than quadratic, the install-phase and abort-disposition
  classification, and the load-freshness predicate. None of that logic is under
  mutation today, so this is a coverage **gain from zero** rather than a
  relocation, and it must be reported as such rather than mixed with parity
  numbers. Where a rule already belongs to a cross-cutting owner —
  `model/buffer_replacement.rs` (`WFR-BUFFER-REPLACEMENT`, slot 4) or
  `ui/buffer_snapshot` (`WFR-BUFFER-SNAPSHOT`, slot 7) — it stays there and the
  change records the boundary instead of poaching it.

- **Build the load evidence surface** and fold the existing typed observation into
  it rather than leaving a second path: `FileLoadAdmissionSnapshot` is already
  typed, and `load_runtime::snapshot_for_test` plus the eleven-ish load hooks in
  `load_save.rs` are the scattered getters the convention retires. The surface
  must make the install path observable — the load generation, the request
  ticket's identity, the admission and disposal-wakeup state, install slice count
  and active/weight state, projection-suspension state, and the terminal outcome
  including a publish-refused-as-stale verdict. Then migrate the widget tests that
  reach around the widget: `crates/lushtext/tests/widget/editor_page.rs` writes
  `page.imp().load_state` directly at several sites, which is ungated, appears in
  no seam census, and shapes production field layout. The per-site categorization
  is recorded as evidence.

- **Decide `OpenPopoverRowLayoutSnapshot`'s ownership.** The matrix lists it
  alongside `FileLoadAdmissionSnapshot` in this row's `Evidence surface` cell, but
  it lives in `ui/open_popover/` and describes recent-document row layout, not
  load state. Decide explicitly among **three** outcomes and record the reason:
  it folds into this workflow's evidence; it belongs to the recent-Open popover
  surface and therefore to slot 7's sweep; or the census has a gap and the hosting
  files must be assigned to a row. The third is not hypothetical —
  `ui/open_popover/` and `ui/window/recent_open.rs` appear in **no matrix row's
  file set**, so "it belongs to `WFR-SHELL-LAYOUT`" would be an assumption rather
  than a recorded fact. Do not leave it ambiguous for a third slot to trip over.

- **Project automation from evidence without widening the contract.** The
  exported fields this workflow owns are `tabs[].load_state` and the `file-load`
  readiness blocker, which feeds the `file-open-complete`, `app-startup`,
  `session-restore-complete`, `recovery-restore-complete`,
  `visual-geometry-settled`, and `accessibility-settled` predicates. Those keep
  their names, types, and semantics — including that a failed load reports
  `workflow-failure` rather than readiness — and start projecting from the
  evidence surface, with new `Evidence Projection Map` rows in
  `docs/automation-reference.md` so the live drift gate covers them. Every other
  new evidence field is internal and must **not** reach the snapshot. Because
  `file-load` gates six documented predicates, this change's no-widening proof has
  to cover readiness, not just the snapshot object.

- **Amend `workflow-evidence-surfaces` to promote the reentrancy constraint into
  stated convention** — the promotion slot 2b explicitly handed forward. See the
  Capabilities section.

- **Advance the programme record and the matrix.** Flip slot 3b's ledger line to
  complete, mark `WFR-DOCUMENT-LOAD` migrated with its `Migrated Workflow Roles`
  subsection, carry `WFR-AUTOMATION-SPINE (partial)` onto the complete line and
  onto slot 4's outstanding line, and record slot 3's completion in the
  remaining-scope table.

**Explicit non-goals.** No change to decoding behavior, encoding detection or
fallback, the reopen-with-encoding workflow's semantics beyond delegation, the
bounded install slice budgets or their paragraph-boundary contract, load
cancellation timing, error copy, any user-visible string, or the exported D-Bus
contract. No save-side work: 3a owns `WFR-DOCUMENT-SAVE`, and this change must not
restructure what 3a migrated. No draft, session, or local-history migration (slot
4), no `WFR-BUFFER-REPLACEMENT` or `WFR-BUFFER-SNAPSHOT` restructuring
(cross-cutting), and no `model/editor_memory.rs` change (exempt, no slot).
**No actuation seam is retired**: `cancel_open_file_for_test`,
`select_open_file_for_test`, `select_open_file_uri_for_test`, and
`apply_load_result_for_test` and its siblings drive steps reachable only through
a `GtkFileChooser` or a worker completion — `cancel_open_file_for_test` is the
record's own named example of the deferred category — and they stay: counted and
preserved, not grown. No workflow is reified as an explicit state machine.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `workflow-evidence-surfaces`: the evidence-surface requirement says one accessor
  reads the whole surface, and every surface built so far reads its fields through
  shared `RefCell` borrows. Those two facts together mean **no evidence field may
  be read from inside a `borrow_mut()`**, or the accessor panics. That constraint
  is currently recorded as a per-workflow module note on the exemplar's
  `evidence.rs`, and slot 2b — which added ten fields to that surface and had to
  obey it — recorded explicitly that it "should become a stated convention, not a
  per-workflow module note", because "it follows from 'one accessor reads the whole
  surface' plus `RefCell`, and every later slot will re-derive it". This change is
  a later slot, building the third and fourth such surfaces (3a's save surface and
  this one), so it pays that promotion: the delta states the constraint normatively
  and requires the read-inside-mutation test slot 2b wrote as the proof pattern.

  **What the delta is not.** It adds no new obligation in substance — the
  constraint is already binding on every surface in the tree, and every migrated
  workflow already satisfies it — and it adds no capability. It converts a
  rediscovered-every-slot derivation into a written one, with a named proof
  pattern. That is the hygiene category the record sanctions: "closes a small
  adjacency the convention already sanctions". The programme record predicted this
  exact delta.

  **Flag for the reviewer.** Under the retroactive-amendment rule this triggers a
  per-row re-check of every `migrated` row for the stated constraint and for the
  proof pattern. `WFR-SEARCH-REPLACE` already has the test (slot 2b wrote it);
  `WFR-COMMAND-PALETTE` and `WFR-DOCUMENT-SAVE` must be checked and, if either
  lacks the proof, given it **in this change** rather than left to a later slot —
  that is what "two generations of the convention must not coexist" means for a
  requirement whose whole content is a proof obligation. Expect this to be the
  most substantive part of the amendment work, and do not record it as a
  formality. Anything beyond this single promotion must be escalated, not
  absorbed. The facade line budget is **not** amended by this change.

Note that `openspec validate --strict` fails any change with no `specs/` delta
("Change must have at least one delta"), which is why every migration slot carries
one; slot 2a corrected the record text that said otherwise.

## Impact

**Prerequisites**

Slots 1, 2a, and 2b are archived under `openspec/changes/archive/2026-08-25-*`.
The blocking gate is **3a archived with `WFR-DOCUMENT-SAVE` marked migrated, its
`Migrated Workflow Roles` subsection complete, the slot ledger marking slot 3a
complete, and `make check-workflow-boundaries` passing** — which is this change's
first task, for the three reasons in the Why section. The slot-2a deliverables
this change depends on are the declared facade budget, the stage-order
qualification rule, and the evidence-to-snapshot drift check; the 3a deliverable
it depends on is the per-workflow role home.

**Code touched** (line counts measured at authoring; task 0.3 re-verifies)

- `crates/lushtext-core/src/ui/editor_page/load_save.rs` — the load half
  extracted; the file is removed. It measured 1,795 lines with **zero**
  `#[cfg(test)]` lines before 3a, so all coverage is external and no in-file tests
  move.
- `crates/lushtext-core/src/ui/editor_page/load_runtime.rs` (423) — retired into
  the load workflow's coordination role, including its `thread_local` coordinator
  and its two probe statics.
- `crates/lushtext-core/src/model/file_load.rs` (462) — **unchanged**; the
  decision is recorded, not applied.
- `crates/lushtext-core/src/ui/editor_page/mod.rs` and `imp.rs` — module
  declarations and any load state the extraction re-homes.
- `crates/lushtext-core/src/services/editor_io.rs` (3,035) — **behavior
  unchanged**; in scope only for load-side seam classification and for any buried
  pure policy the change decides about explicitly, per slot 2b's
  `services/search_backup.rs` precedent. Note it holds seven test override
  statics, some load-side, some shared with save.
- `crates/lushtext-core/src/ui/window/documents.rs` (1,138),
  `dialogs.rs` (836), `encoding.rs` (907), `recent_open.rs` (282), and
  `session_restore.rs` — the open, reopen-with-encoding, recent-document, and
  session-restore invocations delegate to named load operations instead of
  re-reading and re-mutating editor load state inline, following slot 2b's
  window-side fix.
- `crates/lushtext-core/src/ui/open_popover/mod.rs` — only if task 5's
  `OpenPopoverRowLayoutSnapshot` decision places it here.
- `crates/lushtext-core/src/ui/automation.rs` — `tabs[].load_state` and the
  `file-load` readiness blocker project from evidence.
- `crates/lushtext/tests/widget/editor_page.rs`, `window.rs`, `open_popover.rs` —
  evidence reads replacing load-related `.imp().` reach-through and retired
  inspection seams.
- `crates/lushtext-core/tests/properties/file_load.rs` — the load property target;
  in scope for confirming it still exercises the same pure logic after extraction.
- `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`,
  `docs/next/workflow-readability.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `AGENTS.md`, `README.md`, and any
  `.agents/rules/*.md` or `.agents/skills/**` reference naming a moved path.

**Verification**

Everything 3a ran, re-aimed at the install path: load behavior equivalence across
a small direct-install file, a file large enough to require chunked installation,
a file whose largest paragraph exceeds the slice budget (which must install in one
turn per the paragraph-boundary contract), an empty file, a binary or
undecodable file, a missing or permission-denied file, a reopen with a different
encoding, a load cancelled by the user mid-install, a load superseded by a newer
load of a different path, and a load whose editor is closed before the worker
returns; confirmation that install and clear slices still end on paragraph
boundaries and that the linear-not-quadratic behavior is preserved, measured
rather than asserted; `make crash-recovery-smoke`, since bounded install is the
path crash recovery uses to restore a large draft; `make performance-smoke` and
the relevant benchmark comparison, because this is the one workflow in slot 3
whose contract is a performance contract; **`make test-prop`**, because the
change extends `crates/lushtext-core/tests/properties/file_load.rs` and that
target is gated behind `required-features = ["property-tests"]` so no default
lane runs it; a `data-safety` pass before and after
the diff; `make mutants-diff` with the gain-from-zero evidence; and a live run
opening real files, reopening with a different encoding, and cancelling a load of
a large file, with clean stderr. Load reads rather than writes, so the live-run
risk is lower than 3a's — but it still runs against fixture files in isolated XDG
directories, and it still checks for a running instance first. Acceptance is that
loaded content, cancellation behavior, error surfaces, install timing
characteristics, and the exported D-Bus contract behave identically to the
pre-migration workflow.
