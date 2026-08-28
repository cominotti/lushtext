# Workspace-tree evidence surface: the no-materialization hazards (task 0.5)

Re-verification of the code facts this change's `WorkspaceTreeEvidence` surface
must prove inert. Every line number below was read from the current tree; the
archive figures it supersedes are named where they moved.

## The count is SIX, not five

The archived predecessor
(`openspec/changes/archive/2026-08-27-migrate-workspace-tree-and-notes-workflow-readability/evidence/evidence-surface-materialization.md`)
says in prose "The **five** code facts the delta was written against", while the
table immediately below it lists **six** rows. The prose sentence is the error:
the six table rows are six distinct sites with six distinct hazards, and none is
a duplicate of another. **This change works against six facts.** The prose count
is not repeated here.

## The six facts, re-verified against current code

All paths relative to `crates/lushtext-core/src/ui/sidebar/`.

### 1. `find_store_for_dir` — a nominal read materializes, scans, and caches

`workspace_section/tree_index.rs:389` (unchanged from archive).

```rust
389:    pub(super) fn find_store_for_dir(&self, dir_path: &Path) -> Option<gio::ListStore> {
...
400:        let store = self
401:            .find_dir_row(dir_path)?
402:            .children()
403:            .and_then(|m| m.downcast::<gio::ListStore>().ok())?;
404:        self.imp()
405:            .dir_stores
406:            .borrow_mut()
407:            .insert(dir_path.to_path_buf(), store.downgrade());
```

`row.children()` at `:402` runs the `GtkTreeListModel` create function (fact 2b
below), which populates a child store and starts a background scan. The
`insert` at `:404-407` then mutates the `dir_stores` cache. Archive cited
`:389`, `:402`, `:405-408`; the cache mutation is now `:404-407` (`Some(store)`
occupies `:408`).

Callers: `tree_index.rs:314`, `tree_index.rs:433`, `actions.rs:84`,
`actions.rs:93`. Note it also reaches fact 6 (`find_dir_row`) on the way, so one
call can both evict from `dir_rows` and insert into `dir_stores`.

### 2. `visible_child_stores` — `row.children()` with no `is_expanded()` filter

`workspace_section/tree_index.rs:483` and `:499` (both unchanged).

```rust
483:    fn visible_child_stores(&self) -> Vec<(PathBuf, gio::ListStore)> {
...
498:            let Some(store) = row
499:                .children()
```

The loop at `:488` walks *every* flattened row; there is no `is_expanded()`
test anywhere in the body, so each iteration materializes that row's children.
Sole caller: `tree_index.rs:471`, inside `remove_from_model`.

### 3. `expanded_store_index` — safe *only* because of a guard

`workspace_section/refresh.rs:442` (function; archive cited `:452`, which is the
guard line and is still exactly `:452`).

```rust
442:    fn expanded_store_index(&self) -> HashMap<PathBuf, Vec<gtk4::gio::ListStore>> {
...
452:            if !row.is_expanded() {
453:                continue;
454:            }
...
464:            let Some(store) = row
465:                .children()
```

Only the `:452` guard keeps the `:465` `children()` call from materializing a
collapsed row. Callers: `refresh.rs:309` (`refresh_materialized_view`) and
`refresh.rs:395` (`plan_refresh`). This is precisely the clause in
`.agents/rules/widget-wiring.md`: *"Where the workflow's own code reaches such an
accessor safely only because of a guard, derive the field from the workflow's
authoritative state instead of repeating the guarded walk."* The evidence surface
must derive from `expanded_paths`, not repeat this walk.

### 4. `set_expanded(true)` at four sites — materializes *and* queues a watcher restart

| Site | Context | Archive |
| --- | --- | --- |
| `workspace_section/folders.rs:376` | `expand_folders()` — bulk expand of top-level rows | `:376`, unchanged |
| `workspace_section/folders.rs:484` | `restore_folder_model_state()` idle callback, reading `expanded_paths` at apply time | `:484`, unchanged |
| `workspace_section/actions.rs:71` | new-file/new-dir completion, expanding a collapsed target dir | archive said `:65` — **moved +6** |
| `workspace_section/tree_loading.rs:1260` | `schedule_child_state_restore()` timeout callback | `:1260`, unchanged |

(Related non-`true` sites for orientation, not part of the fact:
`folders.rs:368` and `tree_loading.rs:149` set `false`; `folders.rs:386` and
`mod.rs:731` set a computed bool.)

The `notify::expanded` hook is installed once per row in
`workspace_section/watch.rs:151-182` (`install_expanded_watch_hook`), and it
queues two things:

```rust
163:        row.connect_notify_local(Some("expanded"), move |row, _| {
...
167:            if let Some(section) = section_weak.upgrade() {
168:                section.record_row_expansion_transition(row);
169:            }
170:            if super::dnd::expanded_watch_should_be_suppressed(row) {
171:                return;
172:            }
...
176:            glib::idle_add_local_once(move || {
...
181:                section.refresh_workspace_watch_row(&row);
182:            });
```

So one `set_expanded(true)` (a) synchronously mutates the authoritative
`expanded_paths` set via `record_row_expansion_transition`
(`tree_index.rs:65-109`), and (b) schedules an idle
`refresh_workspace_watch_row` (`watch.rs:185-200`), which on a changed
contribution calls `queue_workspace_watch_restart` (`watch.rs:212-231`). That
function's queued work is explicitly a **debounced watcher restart**:

```rust
226:        self.imp().watch_runtime.restart_debounce.schedule(
227:            self,
228:            Duration::from_millis(WATCH_RESTART_SETTLE_MS),
229:            |section, _| section.start_current_workspace_watch(),
230:        );
```

An evidence-surface read that reaches `children()` therefore does not merely
allocate a store: it can restart the filesystem watch lifecycle.

### 5. `derive_expanded_paths_from_model` — increments counters the surface reports

`workspace_section/tree_index.rs:28`; counters at `:31-39` (archive cited
`:28-35`, before the current formatting).

```rust
 28:    pub(super) fn derive_expanded_paths_from_model(&self) -> Option<HashSet<PathBuf>> {
...
 31:        runtime
 32:            .expansion_capture_scans
 33:            .set(runtime.expansion_capture_scans.get().saturating_add(1));
 34:        runtime.expansion_capture_rows.set(
...
 38:                .saturating_add(u64::from(tree_model.n_items())),
```

Both counters live on `RefreshRuntimeState` (`workspace_section/imp.rs:99`) at
`imp.rs:159` (`expansion_capture_scans: Cell<u64>`) and `imp.rs:161`
(`expansion_capture_rows: Cell<u64>`), and both are already asserted as evidence
through `expansion_capture_metrics_for_test` (`tree_index.rs:178-186`). An
observer that calls this derivation corrupts the metric it reports — the
"observer that changes the metric it observes is not an observation" case.

### 6. `find_dir_row` — a lookup that evicts

`workspace_section/tree_index.rs:354` (function; archive cited `:370`, which is
the eviction line and is still exactly `:370`).

```rust
354:    pub(super) fn find_dir_row(&self, dir_path: &Path) -> Option<gtk4::TreeListRow> {
...
370:            self.imp().dir_rows.borrow_mut().remove(dir_path);
```

A stale-but-upgradeable weak row whose item no longer matches is **removed from
the `dir_rows` cache** on what reads as a pure lookup. The scan that follows
(`:375-388`) then re-inserts.

## (a) The no-rewalk clause and every call site

`.agents/rules/ui.md` ("Live expansion state"):

> `expanded_paths` is authoritative live state ... A targeted in-place refresh
> must not rewalk the flattened `GtkTreeListModel` to rediscover expansion; the
> full derivation (`derive_expanded_paths_from_model`) is reserved for
> bootstrap, pre-replacement capture, and the test oracle.

The module doc at `tree_index.rs:22-27` repeats the same reservation. Exhaustive
search of `crates/lushtext-core/src/` and `crates/lushtext/tests/`:

| Call site | Classification |
| --- | --- |
| `tree_index.rs:60`, inside `save_expanded_paths` (`:59`) | **bootstrap + pre-replacement capture.** Its only caller is `folders.rs:77`, inside `load_folder_model`, immediately before the model is rebuilt and a new `GtkTreeListModel` installed. That one path serves both the initial load (bootstrap) and every later folder-set replacement/drill-down (pre-replacement capture). |
| `tree_index.rs:173`, inside `derived_expanded_paths_for_test` (`:172`, `#[cfg(feature = "test-utils")]`) | **test oracle.** Consumed only by `crates/lushtext/tests/widget/workspace_section.rs:3819`, `:5341`, `:5422`, `:5535`, each comparing the oracle against `expanded_paths_for_test()`. |

**No OTHER call site exists.** Nothing in `refresh.rs`, `tree_loading.rs`, or
`watch.rs` reaches the derivation; the targeted in-place refresh path reads
`expanded_paths` directly (`tree_loading.rs:1245`, `:1257`) exactly as the rule
requires. Note the derivation is `pub(super)`, and the only surface that widens
it beyond the module tree is the `test-utils`-gated oracle.

**Why this matters to task 5.3.** 5.3 dissolves `tree_index.rs`, so the
derivation acquires a new home — and that home will sit beside the refresh and
watch code that must *not* call it. Two obligations follow: (i) the reservation
comment must move with the function, not be left behind in a deleted file's
header, and (ii) the `test-utils` gate on `derived_expanded_paths_for_test` must
survive the move. If the oracle lands in a module where the gate is dropped or
the function is widened to `pub(crate)`, the test oracle silently becomes an
available production caller, and the very next in-place-refresh edit can rewalk
the flattened model without any gate objecting —
`make check-workflow-boundaries` does not check this.

## (b) `build_children_model` — the materialization entry point

`workspace_section/tree_loading.rs:115`:

```rust
115: pub(super) fn build_children_model(
116:     section: &LushtextWorkspaceSection,
117:     dir_path: &Path,
118: ) -> gio::ListStore {
119:     if super::dnd::folder_reorder_drag_is_active() {
120:         return empty_children_model_for_drag_hover(section, dir_path);
121:     }
...
125:     let store = gio::ListStore::new::<FileTreeItem>();
126:     populate_child_store(section, dir_path, &store);
```

Its doc line reads: *"Build the child model for one expanded directory and kick
off its background scan."* `populate_child_store` (`:169`) is what registers the
store identity and dispatches the scan.

**Callers: exactly one.** `workspace_section/folders.rs:105`, inside the
`gtk4::TreeListModel::new(...)` create closure installed by `load_folder_model`
(`folders.rs:96-107`). That confirms it is THE materialization entry point: every
`row.children()` in facts 1, 2, and 3 lands here, and nothing else does. **This
is the function the evidence surface must never reach**, transitively included.

The drag-hover branch at `:119-121` is a defensive fallback only
(`empty_children_model_for_drag_hover`, `:131`): it returns an empty store and
collapses the row back on idle, and it increments a `test-utils` counter
(`DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT`) that widget tests assert stays at zero.
It is not a licence for an observer to call `children()` — it exists because GTK
can request children during a reorder drag.

## (c) `expanded_paths` — the authoritative live set

Field: `workspace_section/imp.rs:263`, on the imp struct
`LushtextWorkspaceSection` (`imp.rs:218`, `#[derive(Default, CompositeTemplate)]`):

```rust
262:    /// Remember expanded paths across drill-downs to restore tree state.
263:    pub expanded_paths: RefCell<std::collections::HashSet<PathBuf>>,
```

Every mutation site:

| Site | Function | Kind of mutation |
| --- | --- | --- |
| `tree_index.rs:61` | `save_expanded_paths` | Whole-set **replace** from the derivation, pre-replacement only (`folders.rs:77`) |
| `tree_index.rs:81-108` | `record_row_expansion_transition` | The `notify::expanded` transition path: insert on expand (`:83`); on collapse, prune the descendant subtree (`:104`), or defer ambiguous paths to reconciliation |
| `tree_index.rs:120-131` | `reconcile_expanded_subtree_from_model` | Duplicate-aware fallback for the collapse case: `retain` away one prefix (`:121`) and re-derive only under it |
| `tree_index.rs:140-158` | `rename_expanded_subtree` | **Rename prefix rewrite**: remove old-prefix paths, reinsert under the new prefix |
| `tree_loading.rs:493-497` | `clear_dir_states` | **Accepted reconciliation retirement**: `retain(|path| !under_removed_root(path))` |

Reads: `folders.rs:482` (restore predicate), `tree_loading.rs:1245` and `:1257`
(targeted-refresh restore, read at apply time), `tree_index.rs:166`
(`expanded_paths_for_test`).

**`.agents/rules/ui.md`'s claim is confirmed.** The rule's four named
maintainers — row `notify::expanded` transitions, accepted reconciliation
retirement (`clear_dir_states`), rename prefix rewrites, and the reserved full
derivation — are exactly the five code sites above (the reconciliation fallback
being a sub-case of the collapse transition). The set is kept current
independently of the flattened model, both restore paths read it at apply time
rather than cloning a schedule-time snapshot (`tree_loading.rs:1251-1257`
comments this explicitly), and nothing derives expansion from `children()` for
ordinary refresh. It is therefore a sound and sufficient source for the evidence
surface's expansion fields.

## The six hazards the evidence surface must prove inert

One testable assertion each — what must be **identical before and after** a
surface read, with rows collapsed and with rows expanded:

1. **`find_store_for_dir`** — `dir_stores` map length and key set are identical
   before and after the read, and the workspace scan-task counters
   (`active_workspace_scan_tasks_for_test`,
   `workspace_scan_task_high_water_for_test`) are unchanged.
2. **`visible_child_stores`** — no child store exists after the read that did not
   exist before: `child_store_paths` length and the set of realized child stores
   are identical, for a section whose rows are all collapsed.
3. **`expanded_store_index`** — the surface's expansion fields equal
   `expanded_paths_for_test()` without any `children()` walk occurring: the
   refresh runtime's scan/batch counters and `child_store_paths` are unchanged
   across the read.
4. **`set_expanded` / watcher restart** — the watch runtime is untouched: the
   watch-target count, the pending `restart_debounce` state, and the watch
   generation/restart counter are identical before and after, and no
   `start_current_workspace_watch` runs on a main-loop drain following a read.
5. **`derive_expanded_paths_from_model`** — `expansion_capture_metrics_for_test()`
   returns the *same* `(scans, rows)` pair before and after N consecutive reads
   (N ≥ 25), proving the surface does not call the derivation it reports.
6. **`find_dir_row`** — `dir_rows` key set and length are identical before and
   after the read, including for a path whose cached weak row has gone stale, so
   no observation evicts.

Plus the two cross-cutting proofs the notes surface already established:
repeated reads of unchanged state are byte-identical after each operation that
takes a `borrow_mut()` of state the accessor reads; and every `TemplateChild`
read goes through `try_get()` so a `run_dispose()`d section answers honestly
instead of panicking.

---

# Discharge: how the surface proves inertness (tasks 6.2, 6.3)

`crates/lushtext-core/src/ui/sidebar/evidence.rs` — **544 production lines** (re-derived
in the fix cycle; an earlier revision of this file said 269, measured before the module
and field documentation was written), one `WorkspaceTreeEvidence` value of **26** fields,
built from **one** derivation.

That derivation is `workspace_snapshot_evidence`, the ten scalars the exported
`window.workspace` snapshot serializes. The full surface calls it and moves the result
into its own struct, so a polled snapshot never allocates the per-section collections and
the two values cannot drift. An earlier revision hand-copied the derivation into both
accessors — the scattered-getter regression this surface exists to end — and that
duplication was removed in the fix cycle.

## The six hazards, and how each is avoided

The surface reaches **none** of them, directly or transitively. Every expansion figure
derives from **`expanded_paths`**, which `.agents/rules/ui.md` already names the
authoritative live set — so avoiding the hazards is a *consequence* of using the
correct source, not a workaround bolted on top.

| Hazard | Avoided by |
| --- | --- |
| `build_children_model` | never called; it stays the materialization entry point in `scan_execution.rs` and is named as such in that module's doc |
| `find_store_for_dir` | never called; the surface needs no store lookup because it counts `expanded_paths`, not rows |
| `find_dir_row` | never called |
| `visible_child_stores` | never called |
| `derive_expanded_paths_from_model` | never called; the surface **reports** the capture counters, so calling it would advance the metric it observes |
| `set_expanded(true)` | never called; the surface writes nothing |

## Proved, not asserted

`crates/lushtext/tests/widget/sidebar.rs`,
`test_workspace_tree_evidence_reads_are_inert_collapsed_and_expanded`.

An `inertness_probe` captures every quantity a read must not disturb — the two
expansion-capture counters, the two process-global scan-admission counters, the
expanded-path count, the section count, the summed per-section scan pressure
(active + pending + admission-waiting), the summed watch target generations, **and the
sorted key sets of the `dir_stores` and `dir_rows` registries themselves** — then five
consecutive surface reads are sandwiched between two identical captures.

**The registries are captured by key set, not by count, and that is load-bearing.**
Hazards 1 and 6 are an *insert* and an *evict* respectively, and a count alone is
identical across an insert-plus-evict pair — so counting would let exactly the pair this
proof exists for through. An earlier revision of the probe had dropped the registry reads
and kept only the counters; they were restored in the fix cycle, together with an
assertion that **both registries are non-empty in the expanded case**, so neither half
can pass vacuously against two empty maps.

Run **twice**, which is the part that matters:

1. **with rows collapsed**;
2. **with rows expanded** (`section.expand_folders()`, a real production drive), because
   the hazardous accessors behave differently once the `GtkTreeListModel` has children
   to hand back. This is the case where `row.children()` would materialize and a cache
   lookup would evict.

Both assert equality of the whole probe. No worker starts and no watcher restart is
queued, which the scan-pressure and watch-generation terms in the probe are what detect.

## The three standing proofs

| Proof | Test | How |
| --- | --- | --- |
| tight-borrow discipline | `test_workspace_tree_evidence_reads_stay_side_effect_free_across_mutation` | drives the workflow through each operation that takes a mutable borrow the accessor reads — load, scope change, persistence request, settle — and reads the surface **after** each one, asserting repeated reads are identical. Deliberately **not** a read while a borrow is held: that is the panic the constraint prevents, not a demonstration of it |
| zero-workspace honesty | `test_workspace_tree_evidence_is_honest_with_zero_workspaces` | every aggregate is 0 and `no_workspaces` is true, while `process_scan_task_limit` is non-zero — which is exactly why that field is named `process_*` |
| disposal / child collection | `test_workspace_tree_evidence_answers_honestly_across_a_real_section_teardown` | unlists a workspace, which really destroys its section, then asserts the aggregate tracks the live set and keeps answering |

## A correction this change made to its own design

An earlier draft carried a `disposed_sections_skipped` field fed by a
`header_box.try_get()` predicate. Driving it failed, and reading the code explained
why: **`LushtextWorkspaceSection::dispose()` does not call `dispose_template()`**, so
its template children are never cleared and the predicate could never fire. The
attempt to force the state with `run_dispose()` also emitted
`Gtk-CRITICAL ... gtk_box_remove: assertion 'gtk_widget_get_parent (child) == box'`,
which is a warning this project treats as a bug rather than as test noise.

Both the field and the predicate were **removed**. The surface is disposal-safe for a
**stronger** reason than a guarded read: **it reads no `TemplateChild` at all.** Every
per-section field comes from a `Cell` or `RefCell` on the imp struct, which outlives
`dispose()`, and the one widget call it makes — `is_visible()` — is valid on a
disposed-but-alive widget.

**A guard that cannot fire is worse than no guard**, because it implies a hazard has
been handled. Both `evidence.rs`'s module doc and `ui/sidebar/AGENTS.md` now record the
finding and the condition under which the guard must come back: if a future change makes
the section clear its template children, add it **together with** a test that drives the
state.

## Scope honesty for the scan counters (task 6.1's decision)

**Decision: name the scope honestly rather than fake per-section accounting.**
`ACTIVE_WORKSPACE_SCAN_TASKS` and `WORKSPACE_SCAN_TASK_HIGH_WATER` guard a
process-wide ceiling of 4 across **all** sections in **all** windows. The fields are
therefore `process_active_scan_tasks`, `process_scan_task_high_water`, and
`process_scan_task_limit`, and both the field docs and `scan_admission.rs`'s module doc
state the scope. Per-section accounting was rejected as a behavior change to an
admission gate this change is not otherwise touching.

**None of the three reaches the exported snapshot**, so the honesty question is
contained to the internal surface.
