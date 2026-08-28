## Why

This is **slot 6** of the workflow-readability programme recorded in
`docs/next/workflow-readability.md`: **the minimap**, and nothing else. It
migrates `WFR-MINIMAP` and carries `WFR-AUTOMATION-SPINE` forward incrementally
as every slot since 2a has.

The row has been deferred since the census, and the matrix's
[Outlier Resolutions](../../../docs/workflow-readability-matrix.md) states the
reason in terms this proposal does not restate as opinion: the deferral is about
**proof cost, not fit**. `model/minimap_analysis.rs` is already pure and
single-consumer, `MinimapAnalysisSession` is already a reified `{generation,
lifetime}` seam, and `MinimapAnalysisSnapshot` is already a partial evidence
surface. What made the row expensive is that its rendered output carries
**pixel-verified visual geometry invariants with animation-frame stability
requirements**, and that retiring its hand-listed mutation configuration is only
safe once the pure projection math sits in a `policy.rs` the scope reaches by
convention.

Ten workflow rows are migrated across eight completed slots, so the tier
prerequisite is satisfied many times over — but it is confirmed by reading the
matrix and the ledger, not this proposal. See task 0.1.

**This slot honours the record's design-document expectation.** Both
`docs/next/workflow-readability.md` and the matrix have said since slot 1 that
slot 6 is the one slot expected to need a `design.md`, for "pixel-verified
geometry under animation frames". Reconnaissance confirmed the expectation rather
than discharging it, and the evidence is recorded in the design document itself:
the row has three genuinely competing decompositions (§D2's pure/GTK line, §D3's
freeze choreography, §D4's subclass-state ownership) and **two path-keyed gates
that silently stop protecting the row the moment its file is renamed** (§D1).
None of those is answerable from the convention alone. `design.md` is authored.

### The two path-keyed gates are the finding that reshapes this slot

Every prior slot's structural risk was *coverage that could be lost and measured*.
This row has a sharper one, found during authoring and not named by any prior
handoff: **two mechanical gates protect this row by literal file path, and this
migration renames that path.**

| Gate | Where the path is written | What silently stops happening |
| --- | --- | --- |
| Mutation scope | `.cargo/mutants.toml:35`, `examine_globs` entry `crates/lushtext-core/src/ui/editor_page/minimap.rs` | The **entire** minimap file leaves the mutation scope. Only the new `minimap/policy.rs` stays, through the `ui/**/policy.rs` convention. The run still exits 0 |
| Native minimap pixel invariants | `scripts/check-visual-proof-policy.py:142`/`:168` **and** `crates/cargo-gtk-proof/src/policy.rs:814`/`:842` — the same predicate implemented twice, matched by `==` against the literal path | `native-minimap-highlight-anchors` and `native-minimap-animation-highlight-anchors` stop being **required** invariants for minimap diffs. `make check-visual-proof-policy` still passes |

Both failures are green. Neither is visible in a diff of the row's own files.
This is the same class as slot 3a's `ui/**/policy.rs` mutation-scope reasoning and
slot 5b's `mutants-diff`-proves-nothing lesson, generalized: **a gate keyed on a
literal path is a gate that a migration disarms.** The convention already requires
skills and rules that reference a relocated path to be updated in the same change;
it does not yet say the same about mechanical gates. This change states it, and
pays the retroactive re-check. See "Capabilities".

### The row's measured cells are wrong, in both directions, and the size cell is wrong by 34%

Six consecutive slots found their cells wrong; slot 4 made re-derivation a stated
obligation and slot 5b added the units rider. Both apply here, and the units rider
is the one that bites: **the matrix's size cell for this row is a raw count.**

Re-derived at authoring, row-scoped, `#[cfg(test)]` excluded by brace tracking,
with the unit named on every figure:

| Figure | Matrix cell | Re-derived | Direction |
| --- | --- | --- | --- |
| `ui/editor_page/minimap.rs` | 3,779 (**raw**) | **2,510 production** / 3,779 raw | cell is raw; production is **34% smaller** |
| `model/minimap_analysis.rs` | 186 (**raw**) | **121 production** / 186 raw | same |
| Row size | 3,965 (raw, 2 files) | **2,631 production** across 2 files | restate with the unit named |
| Seam functions | 11 | **11** — reproduces exactly | unchanged |
| Seam gate sites | 16 | **21** | **under-counted by 5** |
| `minimap_analysis.rs` consumers | 1 | **2** — see below | **under-counted by 1** |
| Inversions | 3 | **floor; re-derive from the code** | see below |

The seam-site correction is the interesting one and it is a **row-scoping** error
of the kind slot 3a and slot 5b each hit from the other side. Sixteen
`#[cfg(feature = "test-utils")]` gate sites are in `minimap.rs`; the census
stopped there. **Five more are `MinimapState` fields in
`ui/editor_page/imp.rs:351`–`:363`** — `analysis_slices`,
`analysis_chars_per_slice_high_water`, `analysis_cancellations`,
`analysis_terminals`, and `analysis_after_slice_hook`. They are unambiguously this
row's seams: every one is read by `minimap_analysis_snapshot_for_test`. A row's
seam census must follow the row's state, not the row's filename.

**The consumer correction is the one with a compile break behind it.** The census
cell reads `1 consumer`, and one *in-crate* consumer is correct:
`ui/editor_page/minimap.rs`. The second is an **external target**:
`crates/lushtext-core/benches/benchmarks.rs:44`–`:46` imports
`lushtext_core::model::minimap_analysis::{MinimapAnalysisAccumulator,
MinimapAnalysisPolicy, MinimapAnalysisResult}` and uses them at `:3213`, `:3433`,
and `:3569`–`:3570`. The visibility chain today runs `model/mod.rs:23`
`pub mod minimap_analysis;` — reachable from the bench. After relocation it would
run `ui/mod.rs:14` `pub mod editor_page;` → `editor_page/mod.rs:21`
**`mod minimap;`**, which is private, unlike its `pub mod load;` and
`pub mod save;` siblings. **The bench stops compiling**, and §10 as first drafted
had no bench-compile step, so CI's `Bench Compile` job would have been the first
thing to notice. The eligibility conclusion is unchanged — a benchmark target is
not an owning workflow, so the module still relocates — but "one consumer" and
"one owning workflow" were used interchangeably in the first draft and only the
second is true. The visibility decision is design §D6's; the accounting is task
0.3's, which now counts external-target consumers explicitly.

**The inversion count is a floor, for the fifth consecutive time.** The census
records three. Authoring identified at least **six** resumption points across
**five** stage orders, and the trace is task 0.4's, not this proposal's:
`glib::idle_add_local` analysis-slice resumption (`minimap.rs:1123`), the marker
`Debounce`, the `reflow_settle` `SettleBurst`, that burst's
`schedule_follow_up(MINIMAP_REFLOW_REVEAL_DELAY)` reveal, the early-reveal path
`reveal_minimap_reflow_freeze_for_user_scroll` re-entering the same choreography
from a *different* actor, and the passive `ViewportObserver` re-entry from
`overscroll.rs`. Narrate from the code.

### Four stale mutation-configuration entries, found by reading rather than by any gate

The row's 14 `exclude_re` entries name 66 methods. Authoring checked them against
the tree, which nothing else does:

- **Seven named methods do not exist.** `apply_minimap_width_from_settings`,
  `wrapped_minimap_layout_exceeds_budget`, `buffer_has_line_exceeding_char_budget`,
  `collect_long_line_warnings`, `line_top_in_strip`, `line_bottom_in_strip`, and
  `buffer_y_to_strip_y` all return zero definitions. They excluded something that
  was renamed or deleted, and the exclusion outlived it.
- **Four entries are anchored to a literal `line:column` that has moved.** All
  four `fit_projected_bounds` entries name `minimap.rs:2046:55`, `:2047:21`,
  `:2054:16`, and `:2058:19`. `fit_projected_bounds` now begins at **line 2435**;
  line 2046 is a `markers.push` inside `normalize_line_runs`. Those four
  exclusions match nothing and have not for some time.

A stale exclusion is not harmless: it is a documented equivalence claim that no
longer protects the mutant it names, so the mutant it *was* protecting either
survives unexplained or was renamed into a different exclusion's reach without
anyone deciding that. This change deletes what is dead and re-derives what
remains, and the `mutation-testing` delta states the obligation so the next
line-anchored exclusion is re-verified rather than inherited.

### Behavior preservation here means pixel preservation

This is the distinguishing constraint of the slot and it is not a formality. The
row's contract is written in rendered pixels: `.agents/rules/ui.md` requires that
native `GtkSourceMap` minimap drift be treated as an **animation-frame invariant,
not only a final-settle invariant**, and `openspec/specs/editor-minimap/spec.md`
carries four requirements whose oracle is the screenshot detector rather than any
app-computed rectangle.

Quoted verbatim from `.agents/rules/ui.md`, because these are the anchors the
tasks assert against and paraphrase would lose them:

> Native `GtkSourceMap` minimap drift is an animation-frame invariant, not only a
> final-settle invariant. If sidebar/properties/editor-width work can reflow the
> active editor while the minimap is visible, capture stream frames with native
> viewport pixel anchors. A product fix may temporarily freeze already-rendered
> native minimap pixels with `gtk_lush_widgets::RenderHoldOverlay` during a
> detected width burst, but it must not draw, recolor, or restyle a replacement
> highlight. The freeze cover must be opaque if the live source map is allowed to
> repaint underneath, or transparent snapshot pixels can leak a stale native
> slider frame. It must reveal the live native map after the settle repair and
> quiet repaint window. Capture the freeze from the user action that is about to
> start the shell transition; passive scroll-adjustment or allocation observers
> should only schedule the settled repair, because they can fire after GTK has
> already invalidated or partially realized the native map.

And from `.agents/rules/widget-wiring.md`:

> If a freeze or snapshot overlay sits above a live native widget while that
> widget repaints underneath, give the cover an opaque background matching the
> captured surface; otherwise transparent pixels can leak the live widget's stale
> rendered state through the snapshot.

Three consequences the task list is built around, rather than a closing checklist:

1. **`make visual-geometry-smoke` is a primary acceptance lane**, run against the
   *final staged state*, including the six `minimap-sidebar-workspace-animation`
   case ids and both required invariant ids. Slot 5b's ship lesson applies with
   force here: staging a rename changes the diff digest the gate fingerprints, so
   a lane run before `git add` is not proof of the shipped tree.
2. **The freeze-capture actor distinction is a behavior anchor, not an
   implementation detail.** `ui/window/actions.rs:596`
   `protect_active_minimap_for_shell_width_transition` captures with freeze;
   `overscroll.rs:115` `schedule_minimap_reflow_settle()` schedules without. The
   two entry points differ only in one boolean into
   `schedule_minimap_reflow_settle_impl`, which is exactly the shape a
   role split can silently collapse. Task 5 preserves both and proves both.
3. **The `debug_assert!(render_hold.live_child().opacity() >= 0.99)` at
   `minimap.rs:926` is a load-bearing opacity contract**, not a scratch assertion,
   and it interacts with the workspace lint denying `debug_assert_with_mut_call`.
   It survives the move verbatim or the move is wrong.

### Inheritances this slot is the named recipient of

Verified against the code at authoring, not copied:

| Inherited | From | Status at authoring |
| --- | --- | --- |
| Four production `ui/automation.rs` `.imp()` reach-throughs | 5b B.2, matrix reach-through table | **confirmed, and the line numbers have moved**: `:1152` `editor.imp().scrolled_window`, `:1159` `editor.imp().minimap_overlay`, `:1177` `editor.imp().minimap.source_map`, `:1239` `editor.imp().minimap.marker_strip`. 5b recorded `:1144`/`:1151`/`:1169`/`:1231`. Match on the expression |
| `model/minimap_analysis.rs` relocates to `ui/editor_page/minimap/policy.rs` | matrix Policy Module Census | confirmed: 1 consumer, pure, 121 production lines |
| Facade budget 370, not to be edited without escalation | slot 2a, re-confirmed 3a/3b/5b | this row is the one the record names as most likely to break it. Escalation path declared **before** writing; see below |
| One unspent actuation-seam budget | 5b B.2 | this change plans to spend **zero** and says so in task 4.4 |
| `mutants-diff` proves nothing on an uncommitted worktree | 5b B.2 | task 10.9 generates the diff and passes it explicitly |
| Run the rustdoc gate by hand; it is CI-only | 3a shipped the failure, 3b fixed it, 5b re-warned | task 10.3, before shipping the facade |
| Three widget-test `.imp()` reach-throughs into `minimap_overlay` | this authoring | `window.rs:2643`, `editor_page.rs:3827`/`:3838`/`:3845`. In scope; task 6.6 |

**Explicitly not inherited**: slot 4's two `[~]` items, slot 5a's `[~]` live and
manual proof, slot 5b's `[~]` live walkthrough, its unrun task 7.6 two-tree
capture, its `scan_execution.rs` size follow-up, and its five handed-on
non-tree data-safety findings. Task 0.11 confirms by path that none has moved
into this row's files, rather than assuming it.

### Facade budget: the projection, and the escalation path, declared before writing

**No amendment is proposed and the budget line is not to be edited by default.**
The programme's four data points agree that what stresses the number is the count
of **stage orders**, not inversions, entry points, or risk tier — the exemplar's
two sit at 369, the palette's two at 335, save's one at 223, load's one at 253,
and 5b's **twelve** at 291 by delegating harder.

This row narrates **five** stage orders and, unusually, a wide external entry
surface: roughly eighteen `pub`/`pub(crate)` operations are called from fifteen
files outside the row. Entry-point count did not stress the load facade, but load
had seven; this has more than double that, and each costs a narration line plus a
delegation.

**Projection: ≈300 of 370, with a credible worst case near 340.** The projection
is a prediction that task 9.2 measures and may falsify.

**Escalation path, declared now so it is not invented under pressure:**

1. If the facade exceeds 370, the first response is **delegate harder** — 5b took
   twelve stage orders to 291 that way, and it is the response with four proofs.
2. If delegating harder cannot reach 370 without moving narration into a
   coordination module (which would defeat the facade's purpose), the change
   **amends the budget number** in `docs/workflow-readability-matrix.md` and pays
   the retroactive re-check the amendment rule requires across all migrated rows.
3. What is **not** available: splitting the census row to make two smaller
   facades, or hiding stage narration behind a helper module whose only job is to
   shorten `mod.rs`. Both were rejected in slot 1 and are not re-opened.

### Data safety: tier-2 logic, but not zero

The row holds no user bytes and writes no file, so it is **not** a tier-3 restore
family. It is not zero either, and the mandatory `data-safety` pass in task 0.9
has three concrete places to look rather than a hope of finding something:

- the analysis path reads the **live GTK buffer** through a bounded cursor, under
  the `ui/buffer_snapshot` and O(1)-byte-estimate rules;
- `record_modified_lines` / `mark_entire_buffer_modified` /
  `clear_modified_line_marks` are driven by load, save, local-history restore, and
  draft restore — four user-content workflows, all migrated, whose contracts this
  row must not perturb;
- `set_minimap_tracking_suspended` is the guard those workflows use so a
  programmatic buffer replacement is not recorded as a user edit, and
  `.agents/rules/rust.md` documents that the guard "suspends and exactly restores
  `imp().minimap.tracking_suspended`". A role split that makes suspension and
  restoration land in different modules is exactly how that exactness is lost.

Five consecutive slots found at least one confirmed finding. Task 0.9 budgets for
findings; `.agents/rules/preexisting-blockers.md` has no exceptions.

## What Changes

- **Re-derive the row's measured cells** row-scoped, production units named on
  every figure, and correct `docs/workflow-readability-matrix.md` — including the
  size cell's unit, the five missed seam gate sites, and the inversion floor.
- **Choose and implement the role home**: a per-workflow subdirectory
  `crates/lushtext-core/src/ui/editor_page/minimap/`, matching the `save/`,
  `load/`, and `buffer_replacement/` precedent in the same directory. Flat role
  names are mechanically unavailable: `ui/editor_page/` hosts eight workflows and
  the fixed `policy.rs` / `evidence.rs` names are already spent.
- **Re-key both path-keyed gates in the same change**: the `.cargo/mutants.toml`
  `examine_globs` entry, and the native-minimap invariant path predicate in **both**
  `scripts/check-visual-proof-policy.py` and `crates/cargo-gtk-proof/src/policy.rs`.
  Prove the re-keying by running each gate against the final staged diff and
  showing the invariants are still demanded and the scope still reached.
- **Relocate `model/minimap_analysis.rs`** into `minimap/policy.rs` with
  before/after mutation parity evidence, reported separately from the extraction
  gain.
- **Extract the row's pure projection math** out of the GTK adapter into that same
  `policy.rs` — the marker-bounds and projected-bounds fitting, native-slider
  estimation, lane geometry and palette, availability policy, height-ratio and
  page-size arithmetic, wide-editor slider classification, and the wrapped-layout
  byte threshold — behind pure scalar seam types, so the 14 hand-listed
  `exclude_re` entries and the hand-listed `examine_globs` entry can retire.
- **Assign bounded coordination roles** to the remainder, with the three
  `execution` modules stage-order-qualified as the convention permits, and record
  `ui/editor_page/imp.rs`'s `MinimapState` as a **called presentation surface** in
  both its module doc and the matrix row.
- **Consolidate the evidence surface**: extend the existing gated
  `MinimapAnalysisSnapshot` into `minimap/evidence.rs`, absorbing the **seven**
  inspection `*_for_test` accessors, state the disposition of the row's one
  **actuation** seam, and discharge the three mandated proofs plus the
  no-materialization statement.
- **Retire the four production `ui/automation.rs` reach-throughs** and project the
  minimap snapshot fields from evidence, with a byte-identical no-widening proof
  and drift-gate registration.
- **Delete the four stale line-anchored exclusions and the seven dead method
  names**, and re-derive every surviving exclusion against a real mutant.
- **Advance the matrix row to `migrated`, the slot ledger to
  `slot 6 (complete)`, and the programme record's status, baseline, and
  remaining-scope table** in the same change.

## Capabilities

### New Capabilities

None. Phase 0 holds the contract; this change consumes it.

### Modified Capabilities

- **`workflow-readability-boundaries`** — one statement extending the existing
  "Standing guidance stays consistent with the convention" requirement from
  *documentation* that names a relocated path to **mechanical gates** that do. A
  gate keyed on a literal file path SHALL be re-keyed by the migration that moves
  that file, in the same change, and the re-keying SHALL be proved by running the
  gate against the final staged state rather than asserted. Where one predicate is
  implemented more than once, every implementation is re-keyed. This is a new
  obligation, so the retroactive-amendment rule applies: task 1.2 re-checks all
  migrated rows, and the re-check is grep-cheap because the question is "did this
  row's migration move a file any path-keyed gate names".
- **`mutation-testing`** — two statements on the "Mutation Testing Configuration"
  requirement. First, a hand-listed `examine_globs` entry naming a pre-convention
  adapter file is a path-keyed scope entry that retires with its workflow's
  migration, and a migration that renames such a file SHALL prove the scope did
  not narrow. Second, an `exclude_re` entry anchored to a literal `line:column` or
  to a symbol name SHALL be re-verified against a real generated mutant whenever
  the file it names is touched, and an entry matching nothing SHALL be deleted
  rather than carried.

Both are obligations rather than restatements, and both are named as amendments
with their re-check cost accepted, per the record's rule that a delta adding an
obligation signals real convention work rather than spec hygiene.

## Impact

**Affected code, raw line counts where whole files move and production counts
where the row's size is described:**

- `crates/lushtext-core/src/ui/editor_page/minimap.rs` — 3,779 raw / 2,510
  production, becoming the `minimap/` role home.
- `crates/lushtext-core/src/model/minimap_analysis.rs` — 186 raw / 121
  production, relocating into `minimap/policy.rs`.
- `crates/lushtext-core/src/model/mod.rs:23` — the `pub mod minimap_analysis;`
  declaration is removed with the module.
- `crates/lushtext-core/benches/benchmarks.rs:44`–`:46`, `:3213`, `:3433`,
  `:3569`–`:3570` — the second consumer of the relocated module. Its import path
  changes and its reachability depends on design §D6's visibility decision.
- `crates/lushtext-core/src/ui/editor_page/imp.rs` — `MinimapState` (lines
  328–388) classified as a called presentation surface; five gated fields join the
  row's seam census.
- `crates/lushtext-core/src/ui/editor_page/mod.rs` — module declaration and the
  three re-export groups at `:67`–`:72`.
- `crates/lushtext-core/src/ui/automation.rs` — four reach-throughs retired;
  minimap snapshot fields projected from evidence.
- Callers whose `use`/path lines move: `overscroll.rs`, `focus_mode.rs`,
  `search.rs`, `style_scheme.rs`, `bookmarks.rs`, `document_identity.rs`,
  `local_history.rs`, `load/execution.rs`, `save/execution.rs`,
  `buffer_replacement/execution.rs`, `ui/window/actions.rs`,
  `ui/window/documents.rs`, `ui/window/drafts/restore_execution.rs`,
  `ui/theme.rs`, `ui/search_bar/`.

**Affected configuration and gates:** `.cargo/mutants.toml`,
`scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs`.

**Affected documentation:** `docs/workflow-readability-matrix.md`,
`docs/next/workflow-readability.md`, `docs/automation-reference.md`,
`docs/automation.md`, `.agents/rules/rust.md`, `.agents/rules/ui.md`,
`.agents/rules/widget-wiring.md`, `.agents/rules/build.md`,
`docs/accessibility-matrix.md` (row `A11Y-EDITOR-MINIMAP` already names the owner
path-agnostically as `ui/editor_page/minimap`; task 9.9 verifies rather than
edits).

**Maintained documents that name a moved path and must be updated with it** —
enumerated here because a path named in prose is exactly what
`.agents/rules/documentation.md` and the amended convention require moving in the
same change:

- `AGENTS.md:70` (the `model/minimap_analysis.rs` module-layout line) and `:118`
  (the `editor_page/` line, which lists `load/`, `save/`, and
  `buffer_replacement/` as the per-workflow role homes and names minimap as a
  loose "helper alongside" — that sentence becomes wrong);
- `README.md:438`–`:439`, which names `model/{workspace_search,plain_disposal,minimap_analysis}.rs`;
- `crates/lushtext-core/src/ui/editor_page/AGENTS.md:24`–`:25` ("minimap behavior
  in `minimap.rs`") and `:43` ("must come from the current accepted
  `model::minimap_analysis` cache");
- `docs/mutation-testing.md:161` (a table row keyed on the live
  `ui/editor_page/minimap.rs` path) and `:177`–`:194` (the June 2026 minimap
  mutant baseline of 215 → 86 and the "narrow documented exclusions" sentence,
  both of which §7's retirement makes stale).

`crates/gtk-lush/viewport/README.md:24` mentions minimap refreshes as adoption
evidence only, names no path, and needs no edit; recorded so a later reader does
not treat the omission as a miss.

**Affected tests:** `crates/lushtext/tests/widget/editor_page.rs`,
`window.rs`, `preferences.rs`; `minimap.rs`'s 1,241-line co-located test module,
which follows the production code it covers.

**Not affected, and deliberately so:** the rendered output. Every visual
requirement in `openspec/specs/editor-minimap/spec.md` and
`openspec/specs/visual-geometry-invariants/spec.md` is unchanged, and the change
is accepted only when both required invariant ids are pixel-verified against the
final staged tree.
