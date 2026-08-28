## Why

This is **slot 7, the final slot** of the workflow readability programme
(`docs/next/workflow-readability.md`). Eleven workflows are migrated across slots
1 through 6. Seven matrix rows still carry a **non-terminal status** — six
`pending` and one `deferred` — and two more carry a terminal `cross-cutting`
label while still holding live obligations. The programme's whole value rests on
the matrix being the completion source of truth, so a programme that stops here
leaves the one document a cold session is told to trust reporting work as
outstanding that nobody intends to do, and work as settled that nobody has done.

Slot 7's contract is therefore stronger than "migrate the remaining rows": it must
leave the programme with **nothing outstanding**. Every row terminal, every
inherited handoff item disposed, every remaining deferral inventoried in one place
with its gating condition, and the programme record closed out against its own
baseline.

Five findings, measured at authoring rather than inherited, reshape the slot.

### Finding 1 — `WFR-SHELL-LAYOUT` is not one workflow, and its own row says so

The row's stage trace already states it: *"This row is a residual grouping of 19
shell surfaces that share the window adapter and have no coordination seam; slot 7
may split it if the facade work shows it holds more than one story."*

Measured at authoring, the row is **19 files / 9,214 physical / 8,999 production
lines** (production = physical minus `#[cfg(test)]` modules; the row has no
separate-file test module, so total and production differ only by in-file test
blocks). Its files narrate, at minimum: tab-strip context/pin/bulk-close/reorder
(`tabs.rs`), adaptive split-view geometry and breakpoints (`imp.rs` +
`adaptive_shell.rs` + `sidebar/width_preset.rs`), Focus Mode chrome suppression
(`focus_mode.rs`), file-chooser and close-confirmation dialogs (`dialogs.rs`),
the recent-documents popover surface (`open_popover/**` + `recent_open.rs`),
window-level transient dismissal (`transient_surfaces.rs`), zoom (`zoom.rs`),
startup format preflight (`startup_data.rs`), document lifecycle and chrome
(`documents.rs`), and action/shortcut wiring (`actions.rs`).

A single narrative facade cannot narrate those in 370 physical lines, and the
programme's own evidence says why: the two data points that stress the budget are
**stage-order count** (5b: twelve stage orders at 291) and **external entry
surface** (slot 6: 24 operations from 16 files, first escalation in the
programme). This row has more of both than any migrated row. So the question is
not "can the facade fit" but **"is this one workflow"**, and answering it honestly
is decision work that must precede any structural edit. `design.md` §D1 carries the
criteria and the bounded set of permitted outcomes; task 0.5 resolves it from a
derived stage trace, not from the line count.

**Splitting to fit a budget would be the forbidden response.** The matrix's budget
section names exactly that: "splitting the census row to make two smaller facades"
is not available. The split this row may take is licensed by its *own* provisional
grouping clause and must be justified by stage-order evidence, with the census
coverage proof re-derived so no file loses attribution.

### Finding 2 — the residual rows' census cells are wrong, and the worst error is a seam count low by 29 functions

Every measured cell was re-derived at authoring. Corrections in **both**
directions, per the re-derivation rule, with the unit stated on each figure:

| Row | Cell | Census says | Measured at authoring | Direction |
| --- | --- | --- | --- | --- |
| `WFR-SHELL-LAYOUT` | Seams | `1/2/8/0 = 11 fns, 47 sites` | **40 `*_for_test` fns, 71 gate sites** | **low by 29 fns / 24 sites** |
| `WFR-SHELL-LAYOUT` | Size | `19 files, 8,449 lines (ui)` — unit unstated | **19 files, 9,214 physical / 8,999 production** | low, and the cell was unlabelled |
| `WFR-ENCODING` | Size | `1 file, 907 lines (ui)` | **2 files, 952 production** (`ui/editor_page/invisibles.rs`, 45, was uncounted) | low; a file was missing, not just a number |
| `WFR-STATUS-NOTIFICATIONS` | Seams | `1/0/0/0 = 1 fn, 1 site` | **1 fn, 1 site — exact** | **no correction.** Authoring first recorded this cell as wrong and was itself wrong: `ui/info_bar/mod.rs:166`–`:167` is a genuine gated inspection seam (`inline_alert_announcement_key_for_test`), and the single gate site gates that function rather than a struct or import. Corrected here before implementation, because a false correction in a re-derivation table is worse than an uncorrected census figure — it spends the row's credibility to move a number that was already right |
| `WFR-PLAIN-DISPOSAL` | Seams | `8 fns, 18 sites` | **8 fns, 17 sites** | high by 1 site |
| `WFR-MARKDOWN-PREVIEW` | Seams | `21 fns, 56 sites` | **21 fns, 56 sites** | **exact** |
| `WFR-EDITOR-FIND` | Size / seams | `3 files, 824 lines`; `0/0/0/0` | **824 production; 0 fns, 0 sites** | **exact** |

The shell-layout seam error has a traceable cause worth recording rather than just
a number: slot 3b assigned `ui/open_popover/**` and `ui/window/recent_open.rs` to
this row when it found they *appeared in no row's file set at all*, and the row's
**seam cell was never re-derived after the assignment**. `ui/open_popover/**`
alone carries **33 gate sites and 24 `*_for_test` functions** — more seam
functions than any migrated row retired except the exemplar. Slot 5a's lesson was
that a size cell can be wrong about *ownership*; this is the mirror image, a
correctly-resolved ownership decision that never propagated into the cell it
changed.

**One file is attributed twice, and it changes both places.**
`ui/properties_panel/**` (333 lines) appears in `WFR-SHELL-LAYOUT`'s 19-file set
*and* in the matrix's `Surfaces With No Coordination Tier` list, which the census
introduces as an enumeration proving that "none is a workflow". Both cannot be
true: either the files are part of the shell row (and the tier list overstates
what it has cleared) or they are not (and the row's size and seam cells include
lines it does not own). Task 0.3c resolves it explicitly, because §D1's outcome
and the coverage proof in task 9.8 both read from these two lists.

Two cells reproducing exactly is itself a finding: the census is not uniformly
unreliable, and slot 5b's warning that "a handed-on number is a hypothesis" cuts
both ways.

### Finding 2b — `ui/window/search.rs` (955 lines) is attributed to nothing

The same class as slot 3b's recent-documents gap, found in the change whose job is
to prove full coverage. `crates/lushtext-core/src/ui/window/search.rs` is **955
physical lines** and appears in **no** matrix row's file set, **not** in
`WFR-SHELL-LAYOUT`'s nineteen files, **not** in `WFR-SEARCH-REPLACE`'s cell (which
reads "all under `ui/search_panel/**`"), **not** in the no-coordination-tier list,
and **not** in any called-presentation-surface table.

The expected verdict is that it is `WFR-SEARCH-REPLACE`'s **window-side called
presentation surface**: slot 2b worked in this exact file when it fixed the
window-side undo asymmetry, giving it `journal::begin_undo_restore` (returning
`UndoRestoreClaim`) and `journal::finish_undo_restore` — which is precisely the
coordination/presentation split, with the called surface importing its claim type
from the canonical role home. That is an expectation, not a conclusion: task 0.5d
carries it as a fourth contested file, and whichever verdict lands, the file must
appear in a row's file set **and** in the re-derived coverage proof. A migrated
row whose cell says "all under `ui/search_panel/**`" while 955 lines of its
window-side surface sit outside that glob is a cell this change must correct.

### Finding 3 — 248 production lines of pure policy sit in `ui/` under a name no gate can see

`crates/lushtext-core/src/ui/window/adaptive_shell.rs` is **416 physical / 248
production lines**, contains **zero** `gtk4`/`glib`/`gio`/`libadwaita`/
`sourceview5` imports, and its own module doc says *"Pure policy for the window's
adaptive secondary surfaces… never reads GSettings or mutates widgets."* It is
this row's pure policy, correctly separated, and it is **outside** the
`ui/**/policy.rs` mutation glob because it is not named `policy.rs`.

No gate can find this class. `make check-workflow-boundaries` enforces two halves
— that every `policy.rs` is GTK-free, and that every `policy.rs` is reachable from
`examine_globs` — and neither asks the converse question: *is there pure policy in
`ui/` that is not named `policy.rs`?* The programme's mutation-scope convention is
a naming convention, so a pure module under any other name is silently uncovered,
and the configuration reports success. This is the same defect class slot 6 named
(protection that vanishes while every command exits 0) applied to the *inclusion*
side rather than the exclusion side. Capability delta 3 closes it with a
mechanical discovery check, and the retroactive re-check sweeps the whole crate for
other instances rather than assuming this is the only one.

### Finding 4 — two rows hold a terminal label and live obligations at the same time

`WFR-BUFFER-SNAPSHOT` and `WFR-PLAIN-DISPOSAL` are both labelled `cross-cutting`,
which the Status Labels section defines as terminal, and both appear in slot 7's
scope list in the Migration Order table. Measured at authoring, both carry real
obligations that no migration will ever trigger, because neither will ever migrate:

- **`ui/buffer_snapshot.rs`** — 1,149 physical / **1,084 production** lines, **40
  gate sites / 9 `*_for_test` functions** (the densest gate-to-line ratio of any
  single `ui/` file), and **three parallel typed observation types**:
  `BufferSnapshotMetrics`, `BufferSnapshotStateForTest`, and
  `BufferSnapshotCountersForTest`. Three typed observation paths over one lane's
  state is exactly the duplication `workflow-evidence-surfaces` forbids — and its
  "folded in rather than duplicated" scenario is written to fire "when a workflow
  migrates", which this lane never does. It also owns
  `char_count_requires_chunked_snapshot`, a **shared limit** that slot 3a
  deliberately did not fork into save policy.
- **`ui/plain_disposal.rs` + `model/plain_disposal.rs`** — 2,234 physical /
  **1,805 production**, 8 `*_for_test` functions, and — stating the predicate,
  because two predicates give two answers — **17** `cfg(feature = "test-utils")`
  sites plus **13** `cfg(any(test, feature = "test-utils"))` sites = **30**
  attribute sites in `ui/plain_disposal.rs`, with **0** in
  `model/plain_disposal.rs`. The 13 dual-gated sites are where task 5.7's
  `DisposalProducer` family lives, which is why the census's single figure of 18
  matched neither. And
  `DisposalPressureEvidence` is declared **`pub`** while the Evidence Surface
  Baseline records the settled rule as "the narrowest visibility an evidence
  surface's readers require, with a pre-existing wider type narrowed to it". The
  narrowing trigger, again, is a migration that will not happen. This is the
  slot's **tier-3** row.

The honest resolution is not to force either into the convention — the census
resolved both against relocation on grounds that still hold — but to state that a
cross-cutting *lane* owes the surface rules even though it owes no facade, and to
discharge those obligations in the programme's closing change. That is capability
delta 2.

### Finding 5 — `WFR-AUTOMATION-SPINE` has no terminal status and two sources disagree about its slot

The row is `pending`. Its matrix `Slot` cell reads `2a onward, incrementally per
migrated workflow`, and the Migration Order table's slot-7 row **does not list
it**, while the programme record's machine-readable ledger **does**:
`- slot 7 (outstanding): … WFR-AUTOMATION-SPINE`. Both cannot be right after the
last workflow migrates, and `pending` cannot survive a programme with no further
slots.

Related, and not assumed: `MinimapEvidence` is **not** registered in
`check-automation-docs.py`'s `EVIDENCE_PROJECTIONS` (seven projections are, from
slots 1, 2a, 3a, 3b, 4, 5a, 5b). Slot 6 recorded that as "a result rather than an
omission" — the minimap's ≥18 `visual_geometry.native_minimap` fields are derived
from live widget geometry rather than workflow state, so they read through named
widget operations. The Completion Rule's last bullet says "**any** automation
snapshot field for this workflow projects from the evidence surface", so slot 7
must **verify** that reading and record the verdict, not inherit it. Task 8.6.

### Finding 6 — a confirmed data-safety defect is already this row's, and five handed-on findings never reached a durable home

Two independent passes found the **same** defect from two directions and both named
it slot 7's. Slot 5a recorded it as **M-3** (*"premature teardown survives a
refused close"*, owner *"tabs/close workflow — `WFR-SHELL-LAYOUT`, slot 7"*) and
slot 5b recorded it as finding 4 on the delete path (`close_tab_for_path`). It is
one fix, and it is still present, at `ui/window/documents.rs:1127`–`:1130`:

```rust
editor.cancel_load();
editor.stop_file_monitor();
self.untrack_editor_memory(editor);
tab_view.close_page(&page);
```

The teardown runs **before** `close_page`, which for a modified tab routes to a
save-changes dialog the user may **cancel** — leaving a live tab whose load is
cancelled and whose file monitor is stopped. Worse, a cancelled in-flight load
sets `has_incomplete_load_installation`, which makes autosave **skip that tab's
draft**, so the user's unsaved work loses its recovery record after an action they
declined. `.agents/rules/preexisting-blockers.md` has no exceptions: this is fixed
in this change, with a regression test proved to fail without the fix, and the
teardown moved to the confirmed-close terminal.

Separately, and a programme-record defect rather than a code one: **slot 5b's five
handed-on data-safety findings never reached the durable homes their handoff
named.** All five were re-verified as still present at authoring, and a grep for
`close_tab_for_path`, `republish_document_identity`, `merge_bookmark_target`,
`handle_add_folder_to_workspace`, and `rename_durable_no_replace` across `docs/`
and `.agents/` returns **zero hits**. Finding 5's stated home was *"this change's
own Appendix B.2"* — a directory that is now archived, which is precisely the
failure mode `docs/next/persistent-format-hardening.md` opens by warning against
(*"recorded here rather than in that change's directory, because a change
directory is archived and these outlive it"*). Slot 5a's nine findings **did**
land; slot 5b's five did not.

Because slot 7 is the **last** slot, "handed on" has no recipient. Landing all
five in `docs/next/` records, with severity, site, owning row, and current
line-verified location, is this change's obligation — one of them
(`handle_add_folder_to_workspace`) has already **moved**, to
`ui/sidebar/membership_execution.rs:41` under slot 5b's own dissolution, so the
handoff's site pointer is already stale. That is exactly what a year of
archived-only handoffs produces.

### Inheritances this slot is the named recipient of

Verified against the code at authoring, not copied from the handoff notes:

| Inherited | From | Status at authoring |
| --- | --- | --- |
| Two `ui/automation.rs` `window.imp().tab_view` reach-throughs | slot 6 B.2, matrix reach-through table | **confirmed, lines moved by one**: both are at `:517`/`:518` inside `current_readiness_failure`, not `:518`/`:519`. Match on the expression. The adjacent comment at `:522` already documents a *deliberate* non-evidence read of `load_state()`, so the fix is the **tab enumeration**, not the predicate |
| `ui/window/actions.rs` and `ui/window/imp.rs` appear in the native-minimap path predicates | slot 6 B.2 | **confirmed and larger than reported**: both files are literal keys in **three** predicates in **each** of the two implementations (highlight, animation, workspace-sidebar-animation matrix) — six hard-coded pairs — plus **six** further literal `ui/window/imp.rs` keys inside the two implementations' own self-tests (Python `:594`/`:786`/`:808`, Rust `:69`/`:228`/`:254`). The minimap's files are prefix-keyed and safe; these are not |
| The last hand-listed `examine_globs` UI entry, `ui/markdown_preview/inline_footnotes.rs` | slot 6, `.cargo/mutants.toml` comment | confirmed; the file is 1,066 physical / 621 production and carries **6** symbol-anchored `exclude_re` entries, the largest UI block in the configuration |
| `ui/open_popover/**`, `ui/window/recent_open.rs`, `OpenPopoverRowLayoutSnapshot`, and the ungated `window.imp().recent_documents.loading` read in `tests/widget/open_popover.rs` | slot 3b's recent-documents census gap | confirmed. The gap was closed as an *assignment*; its consequences for the receiving row's cells were not, which is Finding 2 |
| `ui/sidebar/width_preset.rs` (125 lines) is shell-layout's, not the tree row's | slot 5b | confirmed; three consumers, already re-pointed by 5b |
| Facade budget 370, not to be edited without escalation | slot 2a, re-confirmed 3a/3b/5b/6 | **the repo-wide tightest margin is still 1** — `ui/search_panel/mod.rs` is still exactly 369, and slot 2b's warning to plan against 1 line stands unchanged. What is new is that the programme's **most recent** facade landed at 366, a margin of 4, after its first escalation step. Do not read 4 as headroom. Eleven facades measured at authoring; see below |
| One unspent actuation-seam budget | slot 5b, unspent by slot 6 | this change plans to spend **zero** and says so in task 4.6 |
| `mutants-diff` proves nothing on an uncommitted worktree | slot 5b | task 10.9 generates the diff and passes it explicitly |
| Run the rustdoc gate by hand; `make check` does not | 3a shipped the failure, 3b fixed it, 5b and 6 re-warned | task 10.3, before shipping any facade. This slot ships **more new facades than any other**, which is the exact shape that trips it |
| Argument-count suppressions: obligation already discharged | matrix, slot 3a | **confirmed by measurement**: exactly **1** `#[expect(clippy::too_many_arguments)]` in the workspace, at `model/action_catalog.rs:177`, the exempt domain catalog constructor, and **zero** `#[allow(...)]` of that lint. None of slot 7's rows carries one. The sweep inherits a discharged obligation |
| An accessibility-policy **false positive** lesson | **slot 5b, not slot 6** | **corrected at authoring.** No `pgrep` appears anywhere in slot 6's archive; the real lesson is slot 5b's `evidence/test-counts.md`: `make check-accessibility-policy` flagged a module whose doc said "drag-hover" while describing what had moved *away* to another module. The gate was right to ask; the fix was to name the owning module. Task 10.6 carries it as a **wording** hazard for this slot's many module-doc rewrites, not as a process probe |
| A deferred dead `.max(1)` in a migrated `policy.rs` | **does not exist** | **corrected at authoring.** Slot 6 found a dead `.min(upper - lower)` in `minimap/policy.rs` and **removed it**; it was not deferred. The only `.max(1)` discussion is one deliberately *not* added, with the reason recorded at `minimap/projection_execution.rs:499` (doc block `:495`–`:500`). Task 0.12 re-confirms rather than re-planning, so the non-item is not resurrected by a later reader |
| The `evidence.rs` test-utils gating gap at `minimap/mod.rs:43` | **slot 6, already fixed, and at `:47`** | **corrected at authoring.** It was a cold-read documentation defect, fixed in slot 6: `mod.rs:47` now states that `evidence` and `test_policy` are `test-utils`-gated so production reads live state directly. Task 6.9 treats it as the **required shape** for each of this slot's new surfaces, not as an open item |

**Explicitly not inherited**, confirmed by path in task 0.11 rather than assumed:
slot 4's two `[~]` items, slot 5a's `[~]` live and manual proofs, slot 5b's `[~]`
items 7.6 and 10.13, and slot 6's `[~]` items 10.19 and 10.20. Those are
**user-gated**, they stay user-gated, and this change's contribution is to
**inventory them in one place** rather than leave the programme's only outstanding
work distributed across four archived directories. See task 11.4.

### Inherited open items this slot must dispose, each verified against the code at authoring

Not a summary of the handoffs — the items whose *disposition* is this change's
responsibility. Every line number was re-checked; three had drifted.

| Item | Source | Verified state | Disposition here |
| --- | --- | --- | --- |
| Teardown before `close_page` (M-3 = 5b finding 4, one defect) | 5a + 5b | present, `documents.rs:1127`–`:1130` (5a recorded `:1107`) | **fix**, task 7.4. This row's own confirmed defect |
| Slot 5b's five handed-on findings have no `docs/next/` home | 5b | zero hits repo-wide; one site already moved | **land all five**, task 11.5 |
| Unbounded startup activation-open queue | 5a `evidence/data-safety.md:57` | owner cross-cutting `startup_data.rs`, *"handed to slot 7 rather than absorbed here"* | bounded-work assessment plus a budget, or a recorded `docs/next/` home with its gating condition — task 7.6 |
| `flush_dirty_drafts` has no production caller | slot 4 B.3 | present, `drafts/journal.rs:129`, ~100 production lines, `pub`, three widget-test callers only; the async path drifted `:706` → `dialogs.rs:711` | task 10.20: retire or keep, preserving the close-discard and manifest-failure coverage those three tests carry either way |
| `publish_projection` is `false` at every call site | slot 4 B.3 | present; definition `session_restore/admission.rs:259`, unreachable arm `:271`, three call sites all `false` | task 10.20 |
| `current_window_width` duplicated | slot 4 B.3 | present, `window/imp.rs:1353` and `window/local_history/preview_execution.rs:38`; a third **test-side** copy at `tests/widget/window.rs:2480` was outside the recorded candidate | task 10.20, with the ownership decision the candidate said it needed |
| `DisposalProducer` family reports `never used` under default features | slot 6 `tasks.md:1379` | present: `MAX_SMALL_PENDING_DISPOSAL_BYTES`, `try_own_for_gtk`, `DisposalProducerInner`, `DisposalProducer` and its five associated items, `retry_pending` — `ui/plain_disposal.rs:53`, `:848`, `:862`, `:942`. 8 warnings / 12 items; not blockers under `--all-features` | **this slot's row owns it**; task 5.7 decides, and slot 5b's lesson that `--all-features` hides default-feature breaks applies directly |
| `workspace-sidebar-animation` readiness blocker and `WorkspaceSidebarWidthPreset` | 5a, restated by 5b | confirmed shell-layout's; `ui/sidebar/width_preset.rs` (125 lines), three consumers already re-pointed | honour, absorb nothing; §D1 places both |
| Should a constrained width collapse the sidebar while side-by-side preview is open? | 5a `evidence/live-run.md` | **the lane race is already fixed in-stream by 5a** — `--wait-predicate visual-geometry-settled` added to six adaptive-collapse scenarios and proved over four clean runs. Only the **product** question remains | task 0.13 records it as a `WFR-SHELL-LAYOUT` product question with no dependency in this change, destined for `docs/next/adaptive-sidebar.md` rather than decided here |
| Slot 6's landed data-safety fix has **no regression test** | slot 6 `evidence/data-safety.md:130` | confirmed; reaching either arm needs a new actuation seam | task 0.12: state whether this slot finds a real trigger. If not it stays recorded — the honest outcome slot 6 named |
| `minimap_work_pending` under-reports during a slice | slot 6, **unresolved** candidate | needs evidence on whether any readiness or visual-geometry snapshot can be taken from a `mark-set` handler | task 8.7 — this slot owns `WFR-AUTOMATION-SPINE`, so it is the change that can answer it |
| String-keyed lane filters unprotected | slot 6 B.2 | `scripts/run-performance-smoke.sh` carries 17 Criterion group names, 20 widget test names, and 3 module-qualified test paths; `smoke_assert_ran` guards several call sites but not by construction | task 2.7: verify every filter this slot's renames touch **by running the lane**, per slot 6's fail-open lesson |
| `scan_execution.rs` is ~2,000 production lines | 5b | confirmed unchanged; `WFR-WORKSPACE-TREE`'s | not absorbed; named in the closeout inventory as a recorded size follow-up |
| Evidence pointers must be rewritten to archive form at archive time | 5b `tasks.md:1782`, *"the step four prior changes missed"* | this change's pointers stay live-form until archiving | task 11.7 states it as an archiving step rather than a body edit |
| `services/palette/notes.rs` census split (~180 / ~140 / ~1,840) | 5a | *"Slot 7 should use these numbers rather than re-derive them"* | used, not re-derived; the palette row is migrated and out of scope |
| Slot 6's task 10.11 wording names a field that will never hold the id | slot 6 | the runner splits `pixel_verified_invariant_ids` from `animation_verified_invariant_ids` | task 10.13 uses the correct field names, and the closeout records the correction so a future reader is not misled |
| Slot 6's non-goal excluding the promotion of deferred items from slots 4, 5a, and 5b | slot 6 `design.md:55` (paraphrased, not quoted) | that constraint was slot 6's own scope fence, and **slot 7 is where it lifts** | this table *is* that lift — exercised item by item with a stated disposition, not wholesale |

### Facade budget: per-row projections and the escalation path, declared before writing

**No amendment is proposed and the budget line is not to be edited by default.**
All eleven migrated facades were re-measured at authoring, because task 9.3's verb
is *re-measure*, not *confirm*, and slot 5a found three of eight stale:

| Facade | Physical | Margin |
| --- | --- | --- |
| `ui/search_panel/mod.rs` | 369 | 1 |
| `ui/editor_page/minimap/mod.rs` | 366 | 4 |
| `ui/command_palette/mod.rs` | 335 | 35 |
| `ui/sidebar/mod.rs` | 292 | 78 |
| `ui/window/drafts/mod.rs` | 289 | 81 |
| `ui/editor_page/load/mod.rs` | 271 | 99 |
| `ui/editor_page/save/mod.rs` | 223 | 147 |
| `ui/window/local_history/mod.rs` | 215 | 155 |
| `ui/window/notes/mod.rs` | 178 | 192 |
| `ui/editor_page/buffer_replacement/mod.rs` | 168 | 202 |
| `ui/window/session_restore/mod.rs` | 165 | 205 |

The matrix's own facade table is **stale**: it is headed "after slot 3b", lists
four facades, and records load at 253 where it now measures 271. Task 9.3 replaces
it with the eleven-row table plus this slot's own facades.

Projections for this slot's new facades, stated as predictions that task 9.2
measures and may falsify:

| Row | Current entry file | Projection | Worst case | Basis |
| --- | --- | --- | --- | --- |
| `WFR-PRINT` | `ui/window/print.rs` (172) | **≈95** | 130 | one stage order, no inversion, one entry point |
| `WFR-STATUS-NOTIFICATIONS` | `ui/window/notifications.rs` (153 prod) | **≈150** | 200 | one stage order, one timer that re-arms a visual effect rather than resuming a stage; two called presentation surfaces (`status_bar/`, `info_bar/`) |
| `WFR-EDITOR-FIND` | `ui/search_bar/mod.rs` (395) | **≈230** | 300 | fully synchronous; `imp.rs` already absorbs the widget half, and `editor_page/search.rs` becomes a called presentation surface |
| `WFR-ENCODING` | `ui/window/encoding.rs` (907) | **≈210** | 280 | three dialog stage orders, each handing its write to an already-migrated row |
| `WFR-MARKDOWN-PREVIEW` | `ui/markdown_preview/mod.rs` (1,985) | **≈330** | **>370** | **the row most likely to need escalation.** Five recorded inversions is an unverified floor (five consecutive slots found theirs low; 5b's was low by 8.8x), and slot 6's datum says the stressor is the external entry surface, which task 0.4 bounds before any code moves |
| `WFR-SHELL-LAYOUT` | depends on §D1 | **not projected as one facade** | — | projecting a number for a row that may not be one workflow would prejudge §D1. Each row the split produces is projected in task 0.6, after the stage trace exists |

**Escalation path, declared now so it is not invented under pressure**, unchanged
from slot 6 because slot 6 proved step one sufficient:

1. Exceeding 370 is answered first by **delegating harder** — extracting called
   presentation surfaces and pushing stage bodies into coordination roles. Five
   proofs support this (2b, 4, 5b, 6, and the palette).
2. If delegating harder cannot reach 370 without moving *narration* into a
   coordination module, the change **amends the budget number** in the matrix and
   pays the retroactive re-check across every migrated row in the same change. At
   eleven migrated rows this is the most expensive it has ever been, which is an
   argument for step 1, not a prohibition on step 2.
3. **Not available**: splitting a census row to make two smaller facades, or
   hiding stage narration behind a helper module whose only job is to shorten
   `mod.rs`. Both were rejected in slot 1. A `WFR-SHELL-LAYOUT` split decided by
   §D1's stage-order evidence is a *different* act and must be argued on that
   evidence alone — if the split's only support is the line count, it is this
   forbidden response wearing the row's provisional-grouping clause as cover.

### Data safety: the slot's highest tier is tier-3, and the pass is a first-class work item

Slot 5a's lesson is binding: *"Slots 5b, 6, and 7 should plan the data-safety pass
as a first-class work item with its own budget, not as a gate to pass."* Six
consecutive slots found at least one confirmed defect; slot 5a found eleven and
lost its second row to them; slot 6 found two, one of them in an already-migrated
row.

**This slot does not start at zero.** Finding 6's teardown-before-close defect is
already confirmed by two independent passes and already owned by one of this
slot's rows, so the pass begins with one fix committed rather than with a hope of
finding something. Six concrete places to look:

- **`WFR-PLAIN-DISPOSAL` is tier-3** and is the retirement lane for *every*
  workflow's document-sized payloads. A dropped terminal strands whoever waited on
  it — slot 3b fixed exactly that shape in the load row — and a mis-accounted
  permit lets the next admission overshoot the budget.
- **`WFR-BUFFER-SNAPSHOT` reads the live GTK buffer** under the chunked-install
  paragraph-boundary rules in `.agents/rules/ui.md`, and it is the capture path
  for save, draft autosave, encoding analysis, preview, and local history — five
  workflows, four of them migrated and tier-3.
- **`ui/window/dialogs.rs`** (861 production lines) holds *save-changes
  confirmation on close*, whose contract in `ui/window/AGENTS.md` is dense and
  data-safety-critical: reject input across the selected-save pipeline, fingerprint
  discarded editor identity/content generation/modified/path state at
  confirmation, recheck active saves and freshness before destruction, and restore
  retryable drafts on every aborted close. Whether that belongs to this row at all
  is itself a §D1 ownership question.
- **`ui/window/documents.rs`** (1,158 production lines) owns tab close and
  delete-driven close, which is where Finding 6's defect lives. Two passes reached
  it from two directions and neither reached it from the *tab-pin* or *bulk-close*
  paths in `tabs.rs`, so those are the unexamined neighbours.
- **`ui/window/startup_data.rs`** is the startup **format-upgrade preflight**
  gate — and slot 5b handed on a confirmed fail-open finding (M-5) in the format
  gate. Task 0.10 establishes whether M-5's site is inside this row's files.
- **`WFR-ENCODING` reloads and re-encodes the user's document bytes** and hands
  the write to the migrated save row; a perturbed hand-off is a data-safety defect
  even though the row is tier-1.

**Disposition rule, stated before the pass runs** (slot 5b's lesson that a plan
needs one in advance): a confirmed finding whose fix is inside this slot's files is
fixed here with a regression test **proved to fail without the fix** by deliberate
revert-and-rerun; a confirmed finding in an already-migrated or out-of-scope row is
fixed here if its fix is independent of that row's structure, and otherwise handed
on with severity, site, owning row, and a named durable home. Because this is the
**final** slot, "handed on" has no later slot to receive it: every handed-on
finding must land in a `docs/next/*.md` record and be named in the closeout
inventory. Nothing may be recorded as accepted debt
(`.agents/rules/preexisting-blockers.md` has no exceptions).

### One change, and the split path if it is needed

**This is authored as one change**, matching the programme record's slot line and
the Migration Order table, with per-row sections ordered by increasing risk and
proof cost:

`WFR-PRINT` → `WFR-EDITOR-FIND` → `WFR-STATUS-NOTIFICATIONS` → `WFR-ENCODING` →
`WFR-BUFFER-SNAPSHOT` → `WFR-MARKDOWN-PREVIEW` → `WFR-PLAIN-DISPOSAL` →
`WFR-SHELL-LAYOUT` → `WFR-AUTOMATION-SPINE` + programme closeout.

The four tier-1 rows first because they are small and prove the shape of a
no-inversion facade; the two cross-cutting lanes and the pixel-visible preview row
next; tier-3 disposal and the highest-proof-cost shell row last; the closeout
strictly last, because it asserts that nothing is outstanding and can only be
written truthfully once everything else has landed.

**Split path, named in advance** (the 5a/5b precedent, where the split was forced
by a data-safety pass the proposal had argued against needing): if the data-safety
pass or §D1's shell decomposition consumes the change's capacity, the split
boundary is **after `WFR-MARKDOWN-PREVIEW`**:

- **7a** — the four tier-1 rows, `WFR-BUFFER-SNAPSHOT`, `WFR-MARKDOWN-PREVIEW`,
  all three capability deltas with their retroactive re-checks, and every matrix
  cell correction.
- **7b** — `WFR-PLAIN-DISPOSAL`, `WFR-SHELL-LAYOUT`, `WFR-AUTOMATION-SPINE`'s
  terminal status, and the programme closeout.

The boundary is chosen so the closeout stays in the last change and the tier-3 row
travels with the highest-proof-cost row. Taking the split means replacing the
ledger's `slot 7` line with `slot 7a` and `slot 7b` lines and splitting the
remaining-scope row; it does **not** renumber anything, and a partially migrated
row is never an acceptable outcome (slot 5a's Completion Rule reasoning). Task 0.14
records the decision point and its trigger.

## What Changes

- **Re-derive every residual row's measured cells** row-scoped, production units
  named on every figure, and correct `docs/workflow-readability-matrix.md` —
  including the shell-layout seam count (low by 29 functions), the two unlabelled
  size cells, the missing `WFR-ENCODING` file, and the `WFR-STATUS-NOTIFICATIONS`
  seam cell that counted a gate site as a function.
- **Decide, from a derived stage trace, whether `WFR-SHELL-LAYOUT` is one
  workflow**, and implement whichever bounded outcome §D1 selects: one row with
  one facade, or replacement rows each naming one workflow plus no-coordination-tier
  entries for surfaces with no ordered stages — with the census coverage proof
  re-derived so no file loses attribution.
- **Migrate the four tier-1 rows** (`WFR-PRINT`, `WFR-EDITOR-FIND`,
  `WFR-ENCODING`, `WFR-STATUS-NOTIFICATIONS`): a narrative facade each, role homes
  chosen and recorded, called presentation surfaces classified, configuration seams
  collapsed into one test policy per row, and — for each row the matrix says needs
  no seam value object and no evidence surface — **probe first and record the
  negative finding**, because "the census said none" is not evidence.
- **Migrate `WFR-MARKDOWN-PREVIEW` by adding the facade and one evidence surface
  only**, consolidating its 21 seam functions and three override statics, without
  redoing the module decomposition two earlier changes already paid for.
- **Resolve `inline_footnotes.rs` by making it the preview row's `policy.rs`**,
  which is the one outcome that satisfies all three constraints at once. The module
  is GTK-free by design with **214 production lines of real decision logic**, and it
  is in the mutation scope *only* through the hand-listed `examine_globs` entry.
  Deleting that entry without renaming the file would (i) drop the row's pure logic
  out of the scope, which `mutation-testing` classifies as a coverage regression,
  and (ii) leave behind exactly the unclassified pure `ui/` module that this
  change's own third delta is written to catch — while the delta forbids using its
  recorded-reason escape for convenience. Renaming it to `policy.rs` is a **role
  assignment**, not a re-decomposition, so it does not touch the preview non-goal:
  no responsibility moves between modules and no file is split. The six
  symbol-anchored `exclude_re` entries are re-keyed to the new path and re-verified
  against real generated mutants, and the entry retires by convention rather than
  by exception.
- **Rename `ui/window/adaptive_shell.rs` to the shell row's `policy.rs`**, bringing
  248 production lines of already-pure policy inside the mutation convention, with
  the gain-from-zero measured from the tool and reported separately from any parity
  claim.
- **Discharge the two cross-cutting lanes' surface obligations**: consolidate
  `WFR-BUFFER-SNAPSHOT`'s three parallel typed observation types into one evidence
  surface, narrow `WFR-PLAIN-DISPOSAL`'s `pub` `DisposalPressureEvidence` to its
  readers' visibility, and discharge the three mandated proofs plus the
  no-materialization statement on each — while keeping both rows `cross-cutting`
  and keeping `model/plain_disposal.rs` out of GTK Lush scope.
- **Retire the two remaining `ui/automation.rs` reach-throughs** with a named
  window-level tab enumeration, and **resolve `WFR-AUTOMATION-SPINE` to a terminal
  status**, including verifying rather than inheriting slot 6's verdict that the
  minimap's snapshot fields need no evidence registration.
- **Re-key or retire the path-keyed gates this slot's moves touch** — six literal
  `actions.rs`/`imp.rs` pairs across three predicates in each of two
  implementations, plus five self-test keys — proved by running each gate against
  the shipped tree, with the explicit constraint that a re-key must not broaden the
  predicate to files it never protected.
- **Retire the stale `ui/window/tabs.rs` calibration comment** in
  `.cargo/mutants.toml`, which records an exclusion that the current
  `examine_globs` never applies.
- **Advance every matrix row to a terminal status, close the slot ledger, and write
  the programme completion record** — measured outcomes against the section 2
  baseline, the eleven-facade table, and a single inventory of every remaining
  user-gated deferral with its gating condition.

## Capabilities

### New Capabilities

None. Phase 0 holds the contract; this change consumes and closes it.

### Modified Capabilities

**Deltas 1 and 2 (`workflow-readability-boundaries`, `workflow-evidence-surfaces`)
were relocated to slot 7b (`close-workflow-readability-programme`) at split time**,
because both state obligations whose discharge lands there: terminal status on every
row, the residual-grouping split, the closeout record, and the cross-cutting lane's
surface. This change carries delta 3 (`mutation-testing`) only, discharged here. The
delta text below is retained unedited as the authoring record of all three.

- **`workflow-readability-boundaries`** — three statements on the census
  requirement, all obligations rather than restatements. (a) **Terminal status at
  programme close**: when the final slot lands, every row carries `migrated`,
  `exempt`, or `cross-cutting`; `pending`, `deferred`, and `partially-conforming`
  do not survive, and a row resolved as non-migrating records the probe evidence
  for that conclusion rather than asserting it. (b) **Residual grouping rows**: a
  census row that groups several surfaces sharing one adapter is provisional; its
  migration derives its stage orders first and, where the grouping is not one
  workflow, is replaced by rows that each name one workflow, with the coverage
  proof re-derived — and a split justified by line count rather than stage-order
  evidence is the forbidden budget response. (c) **Closeout record**: the
  programme record carries a completion section stating measured outcomes against
  its baseline and listing every remaining deferral in one place with its gating
  condition.
- **`workflow-evidence-surfaces`** — one statement extending the evidence-surface
  requirement from *migrated workflows* to **cross-cutting coordination lanes**. A
  lane that is not a workflow but exposes observable state through test-only
  inspection seams holds one typed evidence surface under the same visibility,
  reentrancy, non-materialization, bounded-child, and single-observation-path
  rules; parallel typed observation types over one lane's state are the duplication
  the requirement already forbids; and because no migration event will ever fire
  for such a lane, the obligation is discharged by the programme's closing change
  rather than deferred to it.
- **`mutation-testing`** — one statement on the configuration requirement:
  because the default scope identifies pure decision logic **by naming
  convention**, pure policy in `ui/` under any other file name is silently outside
  the scope while every command exits 0. The project's mechanical check therefore
  discovers pure `ui/` modules that are not named `policy.rs` and fails until each
  is either renamed into the convention or **classified by its declared workflow
  role**; the check's existing reachability half is not sufficient, because it can
  only inspect files the convention already selects.

  **The classification is role-based rather than a short content escape**, because
  the tree already contains many legitimately GTK-free non-policy modules and a
  narrow escape would make the new check fail on the shipped tree. Measured at
  authoring by the reviewer: **25** GTK-free modules under `ui/`, of which **11**
  are `policy.rs`; the other 14 include GTK-free **narrative facades**
  (`window/drafts/mod.rs` 290, `window/notes/mod.rs` 179), **seam value-object
  modules** (`sidebar/seams.rs` 294, `window/notes/seams.rs` 151), **bounded
  coordination roles** (`command_palette/retirement.rs` 100,
  `window/drafts/retirement.rs` 57), `sidebar/workspace_section/watch_targets.rs`
  (270), `sidebar/width_preset.rs` (126), and six `test_policy.rs`. Every one of
  those is correctly named under the convention already: a facade is a facade, a
  `retirement.rs` is a bounded role. So the check asks "does this module carry a
  declared role?" and only an **unclassified** pure module — carrying no role and
  holding decision logic — is a finding. Authoring's own looser scan returned 30
  rather than 25, which is itself the lesson: the check must **state its purity
  predicate precisely** rather than leave it to whoever runs a grep, and task 0.9
  re-derives the inventory under the predicate the check actually implements.

All three add obligations, so all three carry the retroactive-amendment cost
across **eleven** migrated rows — the most expensive re-check the programme has
paid. That cost is accepted deliberately: the closing change's own sweep *is* the
re-check, and the not-a-confirmation streak now stands at six, so the tasks treat
it as real work rather than paperwork.

## Impact

**Affected code**, physical/production lines where the row's size is described and
physical where a whole file moves:

- `crates/lushtext-core/src/ui/markdown_preview/**` — 9 files, 7,773 / 6,478,
  with `mod.rs` at 1,985 physical becoming the facade; `inline_footnotes.rs`
  (1,066 / 621) leaves the hand-listed mutation entry.
- `crates/lushtext-core/src/ui/window/preview.rs` — 572 / 556, the preview row's
  window-side presentation half.
- `crates/lushtext-core/src/ui/window/**` — `actions.rs` 863, `adaptive_shell.rs`
  416/248 (renaming to `policy.rs`), `dialogs.rs` 861, `documents.rs` 1,158,
  `focus_mode.rs` 344, `imp.rs` 1,678, `mod.rs` 257, `startup_data.rs` 435,
  `tabs.rs` 634/587, `transient_surfaces.rs` 202, `workspace_scope.rs` 48,
  `zoom.rs` 156, `recent_open.rs` 282, `encoding.rs` 907, `print.rs` 172,
  `notifications.rs` 183/153.
- `crates/lushtext-core/src/ui/open_popover/**` — 3 files, 1,422, carrying 33 gate
  sites and 24 `*_for_test` functions.
- `crates/lushtext-core/src/ui/search_bar/**` — 2 files, 712, plus
  `ui/editor_page/search.rs` (112).
- `crates/lushtext-core/src/ui/status_bar/**` (334), `ui/info_bar/**` (370),
  `ui/properties_panel/**` (333).
- `crates/lushtext-core/src/ui/buffer_snapshot.rs` — 1,149 / 1,084; over the
  1,000-line production target, recorded rather than split blind.
- `crates/lushtext-core/src/ui/plain_disposal.rs` (1,542 / 1,341) and
  `crates/lushtext-core/src/model/plain_disposal.rs` (692 / 464).
- `crates/lushtext-core/src/ui/editor_page/invisibles.rs` (45) — the file the
  `WFR-ENCODING` size cell omitted.
- `crates/lushtext-core/src/ui/sidebar/width_preset.rs` (125) — shell-layout's,
  already re-pointed by slot 5b.
- `crates/lushtext-core/src/ui/automation.rs` (2,208 / 2,077) — the two
  `window.imp().tab_view` reads at `:517`/`:518`.
- `crates/lushtext-core/src/ui/window/focus_indexing.rs` (856) — **contested
  ownership**: its module doc claims focus restoration *and* palette indexing, and
  the palette row is migrated. §D1 resolves it.

**Affected configuration and gates:** `.cargo/mutants.toml`,
`scripts/check-workflow-boundaries.py`, `scripts/check-automation-docs.py`,
`scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs`,
`scripts/run-performance-smoke.sh` (string-keyed filters this slot's renames
touch).

**Affected documentation:** `docs/workflow-readability-matrix.md`,
`docs/next/workflow-readability.md`, `docs/automation.md`,
`docs/automation-reference.md`, `docs/mutation-testing.md`,
`docs/accessibility-matrix.md`, `docs/end-user-coverage.md`, `AGENTS.md`,
`README.md`, `crates/lushtext-core/src/ui/window/AGENTS.md`,
`.agents/rules/rust.md`, `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`,
`.agents/rules/build.md`, `.agents/rules/documentation.md`, and any
`.agents/skills/*/references/*.md` naming a moved path.

**Affected tests:** `crates/lushtext/tests/widget/{window,editor_page,open_popover,preferences,status_bar,markdown_preview}.rs`
and the co-located test modules that follow the production code they cover.

**Not affected, and deliberately so:** the exported D-Bus automation contract, the
rendered Markdown preview output, every visual geometry invariant id, and the
behavior of the eleven already-migrated workflows. Where a task's proof is that
something did *not* change, it is a measured diff rather than an assertion.
