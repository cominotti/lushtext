# Facade budget: re-measurement and re-projection (tasks 2.1, 9.2)

**The budget line is 370 physical lines and is not edited by this change.**

## 1. Why a re-projection was required

Slot 5a projected **≈351 of 370** by arithmetic over
`crates/lushtext-core/src/ui/sidebar/mod.rs` **at 406 lines**. The file is now
**415** production lines (415 raw; the file has no `#[cfg(test)]` module, so the two
units coincide). Three of the four subtrahends were recorded as **line ranges**, and
a nine-line growth makes every recorded range wrong by construction. This change
therefore re-derives each subtrahend by **reading the current file** rather than
carrying the ranges forward.

## 2. The three recorded ranges: stale in position, exactly right in size

| Subtrahend | 5a's recorded range | Current range | 5a's size | Current size | Shift |
| --- | --- | --- | --- | --- | --- |
| `WorkspaceSidebarWidthPreset` | 171–273 | **180–282** | 103 | **103** | +9 |
| `SidebarFileRowStateSnapshot` | 56–91 | **65–100** | 36 | **36** | +9 |
| `WorkspacePersistenceFlushError` | 34–54 | **43–63** | 21 | **21** | +9 |

**Every range is stale by exactly +9, and every size is unchanged.** This is a
precision finding worth recording in both directions: 5a's *ranges* had gone stale,
and 5a's *arithmetic* was right. The +9 of growth is entirely above line 171 —
module declarations (`:9-19`) and one `#[cfg(feature = "test-utils")]` re-export pair
(`:32-35`) — which is exactly what the census attributed it to, so the growth
attribution and the range shift corroborate each other.

## 3. The fourth subtrahend, re-derived rather than inherited

The fourth is **not** a whole-block extraction, which is why it has no recorded
range: it is the `impl LushtextSidebar` inspection and focus block at **`:102-178`
(77 lines)**. It contains two different things:

- three state reads — `workspace_persistence_pending` (`:105`),
  `workspace_persistence_inflight` (`:111`), `workspace_refresh_blocks_readiness`
  (`:120`) — which become **evidence projections** plus a cheap facade accessor
  (task 7.2 requires the readiness blocker to read a bool "identical by
  construction" rather than building a whole surface per poll);
- four focus and context-menu helpers — `focus_first_visible_file_tree` (`:133`),
  `focus_first_visible_header_controls` (`:146`),
  `show_first_visible_file_tree_context_menu` (`:160`),
  `show_first_visible_header_context_menu` (`:170`) — each currently spending 8–10
  lines on the same `.imp().sections.borrow().iter().find(..).is_some_and(..)`
  shape, which is coordination the facade must **delegate**, not own.

Both groups stay reachable from the facade as one-line delegations. The block
therefore shrinks from **77 to ~24**: a subtrahend of **~53**.

## 4. Re-projection

Lines leaving `mod.rs`:

| Content | Lines | Destination |
| --- | --- | --- |
| `PERSIST_DEBOUNCE_MS` + doc (`:40-41`) | 2 | `policy.rs` — a cap this row owns, pinned as a literal with its user-facing reason (task 3.1) |
| `WorkspacePersistenceFlushError` (`:43-63`) | 21 | `persist_execution.rs` — the coordination module owns its own typed failure |
| `SidebarFileRowStateSnapshot` (`:65-100`) | 36 | `seams.rs` — a window→sidebar intent bundle crossing two boundaries |
| inspection/focus block net reduction (`:102-178`) | ~53 | `evidence.rs` + `folder_execution.rs` / `context_menus.rs` delegation |
| `WorkspaceSidebarWidthPreset` (`:180-282`) | 103 | `ui/sidebar/width_preset.rs` — **cross-cutting, `WFR-SHELL-LAYOUT`'s** |
| separating blank lines | ~4 | — |
| **total leaving** | **~219** | |

Retained non-narration core: **415 − 219 = ~196**, comprising the `mod`
declarations (`:9-19`), imports (`:21-30`), the `test-utils` re-export pair
(`:32-35`), the two `pub use` lines (`:37-38`), the `glib::wrapper!` block
(`:284-294`), the public operation surface (`:296-409`), and `impl Default`
(`:411-415`).

Added: **~6** lines of new `mod` declarations for `list_execution`,
`persist_execution`, `filter_execution`, `evidence`, and `pub mod width_preset`.

**Narration budget = 370 − 196 − 6 = ~168 physical lines** for the module doc.

## 5. Does the narration fit in ~168 lines?

Costed against the twelve-stage-order case, which task 0.4 has since **confirmed** is
the real one (the more expensive of its two possible verdicts, so this projection was
conservative before the verdict and remains correct after it):

| Narration element | Est. lines |
| --- | --- |
| preamble: what the workflow is, the two-directory nested role home, the canonical role home | 15 |
| twelve stage orders, each with intent named, stages delegated, and its inversion's resume point compressed to one line | ~8 × 12 = 96 |
| role table (facade, policy, evidence, seams, 11 coordination modules, 9 called presentation surfaces) | 24 |
| "State this workflow shares with others" table (the form the load facade established) | 12 |
| **total** | **~147** |

**~147 against a ~168 budget**, giving a projected facade of **≈349 of 370** with
~21 lines of headroom.

### Reconciling the two projections

Task 0.4 confirmed the twelfth stage order exists (workspace folder add/remove — see
`evidence/stage-traces.md`), so two projections are now on the table and they must be
reconciled rather than averaged:

| Projection | Basis | Stage orders costed | Result | Margin |
| --- | --- | --- | --- | --- |
| slot 5a's, carried forward | 406 lines, four subtrahends, **11** stage orders | 11 | ≈351 | 19 |
| slot 5a's + the twelfth | as above plus the twelfth order's narration | 12 | **≈358–360** | ~10 |
| **this change's independent re-derivation** | 415 lines, fourth subtrahend re-measured, **12** stage orders costed from the start | 12 | **≈349** | ~21 |

The two twelve-order figures differ by ~10 lines because they are built from
different retained cores: 5a's projected forward from 406 without re-measuring the
fourth subtrahend, while this re-derivation measured the `:102-178` inspection block's
actual reduction (~53). **Both fit under 370**, so the decision is the same either
way, and the conservative figure (~360, margin ~10) is the one this change plans
against. The independent route landing within ~11 lines of the inherited one after
both were re-based on twelve stage orders is a meaningful cross-check rather than a
restatement.

**The margin is real but thin at the conservative end.** Task 9.2 measures the
written facade and is the figure of record; if it lands above 370 the escalation
path in section 6 applies in order, starting with delegating harder — not with
editing the budget line.

## 6. Escalation path outcome

The path was fixed in advance, in order:

1. **delegate harder** — slot 2b's exact sequence: delegate every stage body,
   compress each inversion to one line, fold module-ownership detail into the role
   table and the shared-state table. **This is what the projection above assumes,
   and it is sufficient.**
2. **escalate in-change with the measured count** — a convention amendment now
   costing a **nine-row** retroactive re-check. **Not needed; not proposed.**
3. **split the census row** — available only on new evidence. **Not taken, and no
   new evidence was found.** Slot 5a's rejection stands and was re-confirmed by
   reading the current code: the workspace list's add/unlist creates and destroys
   the very sections the file tree lives in, `load_workspaces` is the single entry
   point for both, and both share `current_scope`, `workspaces_file`, and the
   persistence debounce. A budget problem is not evidence, and there is no budget
   problem.

**No line is added to `ui/search_panel/mod.rs` (369/370), and the notes (178), save
(223), load (271), palette (335), and four slot-4 facades are untouched by this
change.** Task 9.3 re-measures all of them rather than trusting these recorded
figures, because slot 5a found three of eight previously recorded facade sizes stale.

## 7. What this row's eleven-or-twelve stage orders actually cost

Slot 5a's own finding sharpened the programme's model and is confirmed here: 5a's
**five**-stage-order facade fits in **178**, so **stage-order count alone is not the
pressure**. The exemplar's 369 comes from twelve prose inversions plus a large value-type
surface in one file.

This row is the test of that model, and it holds: this row has **more than twice**
5a's stage orders, yet its projected facade is **≈349**, not ≈2×178. The reason is
that the pressure is **value-type surface and prose inversions**, not stage count —
and this row's largest value-type block (`WorkspaceSidebarWidthPreset`, 103 lines,
25% of the current file) **leaves as cross-cutting**, paying for most of the
narration on its own. A row with eleven stage orders and no departing value type
would be the genuinely hard case; this row is not it.

**Final measurement against 370 is recorded in section 8 when the facade text is
written (task 9.2).**

## 8. Final measured facade — **292 of 370** (task 9.2)

**The narration landed, and the facade fits with 78 lines of headroom.**

| Milestone | Facade size |
| --- | --- |
| before this change | **415** |
| after `WorkspaceSidebarWidthPreset` left | 316 |
| after `PERSIST_DEBOUNCE_MS`, `WorkspacePersistenceFlushError`, and `SidebarFileRowStateSnapshot` left | 264 |
| after the four repeated focus walks were delegated | **229** |
| after the narration was written | 291 |
| **final, re-derived in the fix cycle** | **292** |

**The projection was beaten, not merely met.** Section 5 projected ≈349 and the
conservative re-base of slot 5a's figure said ≈358–360. The actual is **292**, and the
difference is attributable rather than lucky:

- the fourth subtrahend reduced further than the ~53 estimated, because the four
  focus/context-menu helpers were not merely compressed but **delegated** to one named
  `with_first_visible_section` operation in `list_execution.rs`. Each had spelled out
  the same `sections.borrow().iter().find(is_visible)` walk in the facade;
- the narration came in at **~70 lines rather than ~147**, because every stage order is
  one **table row** (stage order | ordered stages | where control resumes) instead of a
  prose paragraph. Twelve rows plus two supporting tables cost far less than twelve
  paragraphs, and read better: a reader comparing two inversions can scan one column.

**Escalation was not needed, and is not proposed.** Step 1 of the recorded path
(delegate harder) was sufficient on its own. No amendment, no nine-row retroactive
re-check, no census-row split — and slot 5a's rejection of the split stands unchanged,
now with the additional evidence that one facade comfortably narrates both halves.

### What the narration actually contains

Because "fits in the budget" is not the same as "narrates":

- all **twelve** stage orders, each with its ordered stages and, for each inversion,
  **the point where control resumes** — the part a reader cannot guess;
- a role table for the canonical home, naming the two modules that are **not** this
  workflow's (`width_preset.rs`, `file_tree_item.rs`);
- a dedicated section on the inversion most easily read wrong — the deferred expansion
  restore's apply-time read — plus the no-rewalk clause beside it;
- a **"State this workflow shares with others"** table, the form the load facade
  established, naming six neighbours and **which direction state flows** in each.

## 9. Other facades' headroom protected (task 9.3)

Re-measured rather than read from the recorded figures, because slot 5a found three of
eight previously recorded facade sizes stale:

| Facade | Measured | Budget | Status |
| --- | --- | --- | --- |
| `ui/search_panel/mod.rs` | **369** | 370 | untouched — **not one line added**, as required |
| `ui/window/notes/mod.rs` | **178** | 370 | untouched |
| `ui/editor_page/save/mod.rs` | **223** | 370 | untouched |
| `ui/editor_page/load/mod.rs` | **271** | 370 | untouched |
| `ui/command_palette/mod.rs` | **335** | 370 | untouched |

All five reproduce their recorded figures exactly, and **none is pushed over**: this
change adds no physical line to any facade other than the sidebar's, which shrank.
