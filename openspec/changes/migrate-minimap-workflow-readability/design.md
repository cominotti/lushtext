## Context

This document exists because `docs/next/workflow-readability.md` and
`docs/workflow-readability-matrix.md` have both said since slot 1 that **slot 6
is the one migration slot expected to need a design document**, for "pixel-verified
geometry under animation frames". Authoring treated that as a hypothesis to test,
not an instruction to obey. It is confirmed, and the confirming evidence is
recorded here rather than asserted:

| Question | Why the convention does not answer it |
| --- | --- |
| **D1** Where the role home goes, and what that does to two gates keyed on the old path | The convention fixes the *role home* rules. It says nothing about mechanical gates that name a file by literal path, and two of them protect this row |
| **D2** Where the pure/GTK line falls in the projection math | About twenty free functions in `minimap.rs` are pure arithmetic in spirit; a subset take `gtk4::Border`, `gdk::Rectangle`, `TextIter`, or `sourceview5::Map`. `policy.rs` forbids those imports, so the line is a design choice with mutation-coverage consequences |
| **D3** How many coordination roles the freeze/settle choreography is | It is one state machine with two entry actors, two timers, and an early-exit path. It could be one `execution`, or `execution` + `retirement`. The bounded set does not decide it |
| **D4** Who owns `MinimapState` | It is this row's state, in another workflow-family's `imp.rs`. The taxonomy has a name for that shape; applying it needs a stated reason |
| **D5** What "behavior preservation" means when the contract is pixels | Every prior slot proved preservation with widget tests plus a smoke lane. This row's oracle is a screenshot detector across animation frames |

Nothing here re-opens a settled programme decision. The facade budget, the bounded
role set, the two permitted role homes, the evidence-surface rules, and the
mutation-parity requirement are all consumed as given.

## Goals / Non-Goals

**Goals**

- Make `WFR-MINIMAP` readable end to end under the convention, with one facade,
  one `policy.rs`, one `evidence.rs`, and bounded coordination roles.
- Retire the row's hand-listed mutation configuration — one `examine_globs` entry
  and 14 `exclude_re` entries naming 66 methods — by moving pure logic into the
  scope's naming convention, not by widening any glob.
- Keep every rendered pixel identical, proved by the screenshot detector rather
  than by app-computed rectangles.
- Leave both path-keyed gates *armed against the new paths*, proved by running
  them.

**Non-Goals**

- Changing minimap behavior, geometry, thresholds, timings, or CSS. Every constant
  in `minimap.rs:30`–`:122` moves verbatim or does not move.
- Redesigning the freeze choreography. Its shape is a June 2026 bug fix with
  stream-frame proof behind it; this change relocates it, it does not improve it.
- Touching `gtk-lush-widgets`' `RenderHoldOverlay` or `gtk-lush-viewport`. GTK Lush
  crates are leaf crates that must not depend on LushText, so policy extracted from
  the minimap adapter lands in **this workflow's** `policy.rs`, never in a
  GTK Lush crate. Stated because the row's arithmetic looks generic enough to
  tempt the opposite.
- Changing any **string-keyed** automation or styling identifier. The surface
  names `minimap-shell`, `minimap-marker-strip`, `minimap-source-map`,
  `minimap-native-viewport`, the four `minimap-*` pixel-anchor names, the
  `minimap-refresh` blocker and workflow ids, and the CSS classes
  `minimap-reflow-freeze` and `minimap-wide-editor-slider-offset` are matched as
  literal strings by the `cargo-gtk-proof` runner, by widget tests, and by
  `resources/style/style.css`. They are the same class of hazard as §D1's
  path-keyed gates, and the response here is simply not to touch them.
- Promoting any deferred item from slots 4, 5a, or 5b.

## Decisions

### D1: Per-workflow subdirectory `ui/editor_page/minimap/`, and both path-keyed gates are re-keyed in the same change

**The role home is not a free choice.** `ui/editor_page/` hosts eight workflows.
`save/`, `load/`, and `buffer_replacement/` already took per-workflow
subdirectories, and the fixed names `policy.rs` and `evidence.rs` cannot be shared.
A flat `ui/editor_page/minimap_policy.rs` would leave the `ui/**/policy.rs`
mutation scope, which the mutation-testing capability classifies as a coverage
regression that blocks the relocation. So: `ui/editor_page/minimap/`, `mod.rs` as
the facade, unqualified role file names inside. This is the fourth adopter of a
settled pattern and carries no new convention weight.

**What it costs is the finding this design document exists to make explicit.**
Renaming `ui/editor_page/minimap.rs` to `ui/editor_page/minimap/mod.rs` disarms two
gates, silently and greenly:

1. `.cargo/mutants.toml:35` lists the old path in `examine_globs`. After the move,
   only `minimap/policy.rs` is in scope, through the `ui/**/policy.rs` convention.
   Everything else in the directory leaves the mutation lane. `make mutants-full`
   still exits 0.
2. The native-minimap invariant predicate matches the old path with `==`, in **two
   implementations that must agree**: `scripts/check-visual-proof-policy.py:142`
   (highlight invariant) and `:168` (animation invariant), and
   `crates/cargo-gtk-proof/src/policy.rs:814` and `:842`. After the move, a diff
   touching only `minimap/` no longer *requires*
   `native-minimap-highlight-anchors` or
   `native-minimap-animation-highlight-anchors`. `make check-visual-proof-policy`
   still passes. Note the directory prefix `crates/lushtext-core/src/ui/` keeps the
   files *visual-sensitive*, so proof is still demanded — but the two invariants
   that are the whole point of the row's proof are no longer **required by name**.

**Decision:** re-key both, in this change, and prove the re-keying by running each
gate against the **final staged** tree rather than by reading the patch. The
`examine_globs` entry is not merely re-pointed: it **retires**, because the whole
purpose of extracting `policy.rs` is that the convention reaches the pure logic and
the adapter is legitimately out of scope. The invariant predicates become
prefix-matched on `crates/lushtext-core/src/ui/editor_page/minimap/`, so a future
split inside the directory cannot disarm them again.

**Why this becomes a spec statement rather than a task.** The convention already
says documentation naming a relocated path is updated in the same change. The
generalization — *a mechanical gate keyed on a literal path is re-keyed by the
migration that moves that file, in every implementation of the predicate, proved
by running it* — is the same idea with sharper consequences, and slot 7 inherits
`ui/window/actions.rs` and `ui/window/imp.rs`, which **both** appear in the same
predicate. Leaving it as this change's task list would hand slot 7 a trap.

**Rejected:** keeping `minimap.rs` as a flat facade with sibling role files in
`ui/editor_page/` (`minimap_execution.rs`, and so on). It preserves both gate keys
for free, which is genuinely attractive, and it is still rejected: it cannot own a
`policy.rs`, so the row's pure math would have to stay in `model/` or take a
prefixed name outside the mutation scope. That trades the slot's primary
deliverable for a gate-maintenance convenience, and it leaves the gates' fragility
undiscovered for slot 7.

### D2: `policy.rs` takes the scalar-domain math; the GTK-typed shells stay in coordination

`policy.rs` MUST contain no `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`
import, and `make check-workflow-boundaries` fails naming the file and the import.
The row's ~20 free functions split cleanly along that line once the boundary types
are scalar, and the split is drawn by **what the function decides**, not by whether
it currently compiles without GTK:

**Moves to `policy.rs`** (pure today or pure after its parameters are made
scalar):

- `fit_marker_bounds`, `fit_projected_bounds`, `fit_native_slider_to_source_map_bounds`
  — the clamping and minimum-height arithmetic, and the row's four densest
  mutation-exclusion clusters;
- `native_slider_estimate_from_inputs` with its `NativeSliderEstimateInput`;
- `minimap_availability_for_policy` with `MinimapAvailabilityPolicy`, and
  `wrapped_layout_analysis_required_for_bytes` — the O(1) live-byte eligibility
  rule, whose 2 MiB exactness is a stated contract;
- `source_map_editor_height_ratio_from_heights`, `wide_editor_slider_offset_class`,
  `fitting_source_map_page_size`, `finite_adjustment_distance_from_lower`;
- `normalize_line_runs`, `markers_from_lines`, `modified_line_mark_samples`;
- `marker_lane_width`, `marker_lane_x`, `marker_rgba`;
- `line_top_in_target`, `line_bottom_in_target`, `target_y_from_widget_y`,
  `gtk_f64_to_milli`, and `document_height_from_iter_rect`, which needs **both** a
  re-signature and a **rename**: once it takes `(y, height)` scalars instead of a
  `gdk::Rectangle`, its name describes a mechanism it no longer touches, and
  intent-first naming governs a `pub(crate)` cross-module operation. The new name
  states the decision — the document height derived from a line's vertical span —
  not the toolkit type it used to unpack;
- the relocated `MinimapAnalysisPolicy` / `MinimapAnalysisAccumulator` /
  `MinimapAnalysisResult` from `model/minimap_analysis.rs`;
- every threshold constant whose value is a policy decision rather than a widget
  property.

**Stays in coordination** — these read live GTK state and are the *gatherers* the
mutation-testing capability already contemplates: `source_map_border`,
`source_map_bounds_relative_to`, `adjustment_diagnostics`, `text_view_rect`,
`visible_editor_line_iters`, `collect_search_match_lines`, `collect_modified_lines`,
`draw_marker_strip`, `sync_source_map_geometry`, `force_source_map_top_layout`.

**The seam types are what make this possible, and they are D2's real content.**
Scalar value objects carry the boundary, so the pure functions never see a GTK
type and the call sites cannot swap two same-typed rectangles.
`MinimapProjectionSpace` and `MarkerProjectionSpace` already exist as private
structs and become the reified pure seams — these two are **established**, each
crossing two or more function boundaries today.

A third, an adjustment-facts bundle over `{value, lower, upper, page_size}`, is a
**candidate only, not a finding**, and it is recorded that way deliberately.
Authoring asserted it crossed two or more boundaries; review could not reproduce
that. `fitting_source_map_page_size` takes two pairs drawn from **two different**
adjustments, and `finite_adjustment_distance_from_lower` takes a pair, not the
quadruple — so on current evidence the bundle qualifies under neither limb of the
seam rule. There is also a **duplicate-type hazard**: `MinimapAdjustmentDiagnostics`
(`minimap.rs:293`–`:305`) already reifies `{at_lower, value_milli, lower_milli,
upper_milli, page_size_milli}` for the automation projection, so a second type
over the same five facts differing only in scaling would be a parallel shape
rather than a seam. Task 3.3's drop-if-unqualified rule bounds this: the candidate
is introduced only if implementation *demonstrates* two crossings that are not
better served by extending or reusing the existing diagnostics type.

Per the seam rule, reification is warranted by seams, not by long signatures. A
bundle used by exactly one private helper and reconstructed nowhere else does not
get a type.

**Rejected:** hoisting the math into `model/`. The census already classified
`minimap_analysis.rs` as single-consumer and slated it *out* of `model/`;
`.agents/rules/rust.md` forbids placing a module in `model/` solely to obtain
tooling reach. **Also rejected:** pushing the geometry arithmetic into
`gtk-lush-viewport` or `gtk-lush-widgets`. Those are leaf crates that must not
depend on LushText, and this arithmetic encodes LushText's minimap product
decisions (the 8px strip, the four lanes, the 13px CSS outset, the 0.20 wide-editor
ratio). It is not generic geometry wearing a product hat.

### D3: Six coordination roles, three of them stage-order-qualified `execution`

Five stage orders, mapped to the bounded set:

| Stage order | Role module | Why this name |
| --- | --- | --- |
| Availability classification and analysis eligibility | `admission.rs` | It is a gate deciding whether expensive work may start against a bounded budget (the 2 MiB live-byte estimate, the per-slice character budget, the marker cap), reserving the analysis generation and lifetime up front |
| Sliced cancellable content analysis | `analysis_execution.rs` | The forward phases of the bounded GTK-iterator scan and its `idle_add_local` resumption |
| Marker collection, strip draw, native geometry sync | `projection_execution.rs` | The workflow's primary rendering work: debounced marker refresh, `queue_minimap_draw`, `sync_minimap_view_geometry`, diagnostics projection |
| Width-reflow freeze, settle, and reveal | `reflow_execution.rs` | The `SettleBurst` choreography, both entry actors, the follow-up reveal, and the early user-scroll reveal |
| Buffer, adjustment, and search observation | `watch.rs` | Maintaining observation of external sources: the buffer `SignalBag`, the adjustment `changed` wiring, the in-tab search session. **Corrected during implementation:** this row originally read "settings, and style observation … the GSettings bindings including `minimap-width`", which was wrong. Those bindings live in `ui/editor_page/imp.rs` beside the editor page's other preference bindings and were **not** moved — `minimap-width` is a declarative `gio::Settings::bind` with a clamping mapping, which `.agents/rules/ui.md` requires to stay a settings-to-widget projection rather than becoming imperative coordination. `watch.rs` observes the buffer, both vertical adjustments, and the search session; it binds no setting |
| Cancellation and payload release | `retirement.rs` | `cancel_minimap_analysis`, `dispose_minimap_analysis`, cache and marker release, `clear_modified_line_marks` — destroying payloads the workflow is finished with |

**Three `execution` modules is permitted, not a workaround.** The convention lets a
workflow that owns several ordered stage orders qualify a bounded role name with
the stage order it serves, in the workflow's own domain vocabulary — the precedent
is `query_execution.rs` / `index_execution.rs` and `replace_execution.rs`. Analysis,
projection, and reflow are three distinct ordered stage orders of one workflow, and
the alternative is taking an ill-fitting bounded name for two of them, which the
convention explicitly forbids.

**The freeze/reveal path is one role, not two, and this is the decision D3 exists
to make.** It is tempting to give the cover's removal to `retirement`: it releases
something. It is rejected on the test slot 3b used — *is the job cohesive enough
that a reader would look for it under its own name* — plus a second test this row
supplies. The freeze, the settled repair, the quiet-window reveal, and the
early-reveal-on-user-scroll are **one state machine with a shared invariant**:
frozen pixels must be revealed exactly once, after repair, and never while the
burst is pending. `reflow_reveal_pending` exists precisely because the reveal and
the burst are separately live. Splitting the machine across a role boundary would
put `minimap.rs:938`'s
`if minimap.reflow_settle.pending() || !minimap.reflow_reveal_pending.get()` guard
on one side and its setter on the other — the class of defect the seam rules exist
to make unrepresentable. `retirement` gets analysis payloads, which really are
finished with; it does not get a live overlay.

**`journal` does not apply.** The row keeps no durable record and reads nothing
back at startup. Named because slot 2b, 3a, and 5b each recorded a near miss and
the test is worth applying rather than skipping.

**The two freeze entry actors survive as two named operations.**
`schedule_minimap_reflow_settle_with_freeze` (user action, from
`ui/window/actions.rs:596`) and `schedule_minimap_reflow_settle` (passive
`ViewportObserver`, from `overscroll.rs:115`) differ by one boolean into a shared
implementation. `.agents/rules/ui.md` states that distinction as a behavior
contract — capture the freeze from the user action; passive observers only schedule
the settled repair. Both stay `pub(crate)` on the facade with the contract in their
doc comments, and the facade narrates *why* there are two.

### D4: `MinimapState` in `ui/editor_page/imp.rs` is a called presentation surface

The row's state — `source_map`, `marker_strip`, `markers`, `modified_marks`,
`availability`, the analysis generation/lifetime/session/cache, the `Debounce`, the
`SettleBurst`, the `RenderHoldOverlay` owner, the `SignalBag` — lives in a struct
defined in the **editor page's** `imp.rs`, alongside `FocusModeEditorState` and the
template children.

It **stays there**, classified as a called presentation surface. The taxonomy names
exactly this shape — "GTK subclass state and template children" — and the
classification is a real decision with three supports:

1. `dispose()` at `imp.rs:660`–`:680` tears the row's state down in a documented
   order interleaved with the page's other subsystems. Moving the struct without
   moving `dispose()` splits a lifecycle; moving `dispose()` is out of scope.
2. `minimap_overlay` is a `TemplateChild` on `LushtextEditorPage`, bound by the
   Blueprint template. It cannot leave `imp.rs`, and separating the overlay from
   the state that drives it buys nothing.
3. The migrated `WFR-DOCUMENT-SAVE` and `WFR-DOCUMENT-LOAD` rows already recorded
   `ui/editor_page/imp.rs` as the editor page's own shared state rather than
   theirs. Reversing that for this row would contradict two settled rows.

A called presentation surface is **not a role**, owns no `policy.rs` and no
`evidence.rs`, and is recorded in **both** its own module doc and the matrix row.
Slot 5a's re-check found three of eight rows meeting only half of that two-place
requirement; this row meets both, deliberately.

**Consequence for the evidence surface**: `evidence.rs` reads `MinimapState`
through the page's `imp()` from inside its own workflow family, which is ordinary
private access, not a cross-boundary reach-through. The four *cross-boundary*
reaches this change retires are `ui/automation.rs`'s, from a different workflow.

### D5: Pixel preservation is the acceptance oracle, and it is captured against the staged tree

The row's contract is written in rendered pixels, so "behavior-neutral" is proved
by the screenshot detector, not by widget tests alone. Four commitments:

1. **Both named invariants are required and pixel-verified**, not merely present:
   `native-minimap-highlight-anchors` and
   `native-minimap-animation-highlight-anchors` appear in the root summary's
   `pixel_verified_invariant_ids`. A rectangle-only or name-only pass is not
   proof for these files, which the visual-geometry capability already states.
2. **Stream-frame capture, not final-settle only.** The six
   `minimap-sidebar-workspace-animation` case ids named in
   `check-visual-proof-policy.py:44`–`:49` cover compact-overlay, intermediate-1100sp,
   and wide-desktop at both show and hide. Minimap drift is an animation-frame
   invariant; a final-settle-only pass would not detect a freeze that reveals one
   frame early.
3. **The lane runs against the final staged state.** Slot 5b's ship lesson is
   binding: the gate fingerprints the visual-sensitive diff, and staging a rename
   changes that digest. A lane run before `git add -N` proves a tree that is not
   the one shipping. New files are `git add -N`'d before the first diff-aware gate,
   and the lane is re-run if the digest moves.
4. **A clean artifact root each time.** Stale case directories under
   `build/smoke/visual-geometry` can make the root summary report evidence the
   current binary did not produce.

**Rejected:** accepting widget-level allocation assertions as sufficient because
"no rendering code changed". The row's rendering is native `GtkSourceMap` output
plus CSS plus app-drawn markers, and the June 2026 slider-drift history is exactly
a case where every app-computed rectangle was correct and the pixels were not.

### D6: The benchmark keeps reaching the analysis policy through a narrow re-export, and `mod minimap;` stays private

`model/minimap_analysis.rs` has **two** consumers, not the one the census cell
records. One is in-crate (`ui/editor_page/minimap.rs`); the other is the external
benchmark target, `crates/lushtext-core/benches/benchmarks.rs:44`–`:46`, which
imports `MinimapAnalysisAccumulator`, `MinimapAnalysisPolicy`, and
`MinimapAnalysisResult` by their `lushtext_core::model::minimap_analysis::` path.

Today that path is public all the way down (`model/mod.rs:23`). After relocation
the path would be `lushtext_core::ui::editor_page::minimap::policy::…`, and
`editor_page/mod.rs:21` declares **`mod minimap;`** privately — unlike its
`pub mod load;` and `pub mod save;` siblings. The bench would stop compiling.

Three options, and the reason each of the rejected two is rejected:

1. **Widen to `pub mod minimap;` and make `policy.rs` public.** Rejected. It
   inverts the posture of the three existing per-workflow role homes in the same
   directory, and it exports a whole role module to obtain reach for three types.
   The convention already refuses to place a module somewhere merely to obtain
   tooling reach; exporting one for the same reason is the same mistake pointed
   outward.
2. **Move the benchmark's use to an in-crate path** — for example by having the
   bench measure through an existing public façade. Rejected for this change: it
   changes what the benchmark measures, and a structural migration is the wrong
   place to renegotiate a performance baseline.
3. **A narrow, precisely scoped re-export.** *Chosen.* `editor_page/mod.rs`
   already re-exports this row's public types in three groups at `:67`–`:72`;
   the three analysis types join the `pub use` group, so the bench's import path
   changes and nothing else about the module's visibility does. `mod minimap;`
   stays private, matching the directory's posture. Slot 3a set the precedent
   explicitly: relocating `save_admission.rs` kept "a precisely scoped `pub`
   subset" alive because the benchmark and the widget tests consumed it.

**The obligation this creates for §10 is the real point.** The break is a *hard
compile error* in a target that neither `make check`'s Clippy pass nor the
nextest lane builds by default, and CI's `Bench Compile` job is a separate job.
§10 therefore gains an explicit bench-compile step; discovering this class in CI
after a green local battery is exactly the failure mode §D1 exists to prevent,
arriving through a different door.

**What this does not change:** the relocation itself. Eligibility is counted in
**owning workflows**, and a benchmark target is not one. The census's conclusion —
single owning workflow, relocates — survives intact; only the word "consumer" was
being used for two different quantities.

## Risks / Trade-offs

| Risk | Mitigation |
| --- | --- |
| **The facade exceeds 370.** This is the slot the record names as most likely to break the budget: five stage orders and ~18 external entry points | Escalation path declared in the proposal *before* writing. Delegate harder first (5b took twelve stage orders to 291); amend the number with the retroactive re-check only if that fails; never hide narration in a coordination module |
| **A path-keyed gate is re-keyed in one implementation and not the other**, leaving Python and Rust disagreeing | Both implementations are named in the tasks with file and line, and the acceptance step *runs* both against the staged diff rather than reading them |
| **Mutation parity is lost in the relocation and hidden behind the extraction gain** | The mutation-testing capability already requires the two figures reported separately, each naming its invocation and file-level anchors. Tasks follow that split |
| **The pure/GTK line is drawn wrongly and `policy.rs` needs a GTK type after all** | `make check-workflow-boundaries` fails naming the file and the import, early and cheaply. The scalar seam types in D2 are the designed answer, not a hope |
| **Extraction arrives with survivors because its only assertions were in widget tests** — 5b's `workspace_scope_kind_name` lesson | The row's 1,241-line co-located test module already unit-tests most of the extracted math. Tasks budget a unit test per extracted function that lacks one |
| **The freeze `debug_assert!` is weakened or moved** past a mutation, tripping the denied `debug_assert_with_mut_call` | It moves verbatim with its guard; the all-targets all-features Clippy gate and the default-feature build both run |
| **A stale exclusion is re-derived into a *new* stale exclusion** | Every surviving exclusion is proved against a real generated mutant from `mutants-list`, not against the source text |

## Migration Plan

1. Gates, orientation, row-scoped re-derivation with units named, and the
   `data-safety` pass — before any code moves.
2. Apply the two amendments and pay the retroactive re-check.
3. Create `minimap/policy.rs`: relocate `model/minimap_analysis.rs` with §D6's
   narrow re-export, then extract the scalar math behind the seam types. Report
   parity and gain separately.
4. Create the six coordination roles and the facade; `MinimapState` stays and is
   classified in both required places.
5. `evidence.rs`: extend `MinimapAnalysisSnapshot`, absorb the **seven**
   inspection accessors, decide the one actuation seam's disposition, discharge
   the three mandated proofs and no-materialization.
6. Retire the four `ui/automation.rs` reach-throughs; project from evidence with a
   byte-identical no-widening proof; register the drift gate.
7. Re-key both path-keyed gates; retire the `examine_globs` entry and the 14
   `exclude_re` entries; delete the four stale line-anchored ones and the seven
   dead method names.
8. Matrix row, slot ledger, and programme record advanced in-change; measured cells
   re-derived **last**, after the final test and mutation runs.
9. Full verification battery, with the visual-geometry lane last and against the
   final staged tree.

## Resolved Questions

**Does this row need a `design.md`?** Yes — see Context. The record's expectation
is honoured with the confirming evidence recorded, not merely obeyed.

**Does the row own a `policy.rs` at all, or is its pure logic cross-cutting?**
It owns one. `model/minimap_analysis.rs` has exactly **one owning workflow**, and
the extracted projection math has no consumer outside the row. The negative
finding that would have made the row policy-less was probed for and not found.
Stated in owning workflows rather than in consumers on purpose: the module has
**two** consumers — this row and the benchmark target (see §D6) — and an earlier
draft of this document used the two words interchangeably. Eligibility is counted
in owning workflows, so the conclusion is unaffected, but the distinction is the
one the convention actually makes.

**Is `MinimapAnalysisSession` still an adequate seam?** Yes, and it is not
re-derived away. It already carries `{generation, lifetime}` and the convention
treats a coordinator that owns its generation and exposes a currency predicate as
*being* the seam value object. D2 promotes **two** existing private structs to
reified pure seams and records **one candidate** that implementation must qualify
or drop; none of that replaces this one.

**Does the row need a new test-only actuation seam?** Planned: **zero**. Slot 5b's
unspent budget of one remains unspent. If implementation finds one genuinely
unavoidable, it is justified individually at its definition, per the deferred-seam
rule.
