# Evidence-surface no-materialization and disposal proofs (tasks 3.5, 4.6)

The `workflow-evidence-surfaces` delta this change lands requires the
no-materialization property to be **proved rather than asserted**, by reading the
surface in both the unmaterialized and materialized states and showing the
workflow's admission counters, registries, generations, and derivation metrics
identical before and after each read.

## `NotesEvidence` — proved, and one proof found a real panic

Three headless widget tests in `crates/lushtext/tests/widget/window.rs`, all
passing at `--retries 0` with zero `FLAKY:` lines:

| Test | Proves |
| --- | --- |
| `test_notes_evidence_reads_stay_side_effect_free_across_mutation` | The reentrancy constraint. Drives the workflow through opening a saved document, toggling a bookmark, presenting the browser, and disposing the dialog — each of which takes a `borrow_mut()` of state the accessor reads — and reads the surface **after** each one, never while a borrow is held. Then asserts three consecutive reads of unchanged state are identical. |
| `test_notes_evidence_read_materializes_nothing_and_advances_no_generation` | The no-materialization statement. Over a live browser session with a published source, reads the surface **25 times** and asserts the whole `NotesBrowserRuntimeSnapshot` is byte-identical before and after, field by field for the ones that would move: `source.started`, `query.started`, `preview.started` (no excerpt worker may start from an observation), `query.cancellation_requests`, and `active_note_save_captures` (**counting captures must not prune them**). |
| `test_notes_evidence_answers_honestly_for_a_disposed_window` | The disposed-widget rule, on a `run_dispose()`d window. |

**The disposal proof caught a real panic before it shipped.** The surface's first
draft called `self.active_editor()`, which reads `imp.tab_view` — a
`TemplateChild<AdwTabView>` — through the **panicking** accessor. GTK4 clears
template children in `dispose()`, before Rust's `Drop`, so the first run failed
with

```
Failed to retrieve template child. Please check that all fields of type
`AdwTabView` have been bound and have a #[template_child] attribute.
```

Every `TemplateChild` read in `notes/evidence.rs` now goes through `try_get()`:
`tab_view` (for the active editor, the bookmark count, and the cursor-bookmark
flag) and `sidebar` (for folder-note availability). This is the third time the
disposed-widget rule has caught something by panicking rather than by review, and
it is the first time the panic came from a *transitively* reached template child
rather than from a direct read — `active_editor()` looks like an ordinary window
operation at the call site.

### Why the notes surface is not the hard case

Recorded so the delta's motivation is not misread. `NotesEvidence` walks
`AdwTabView`, which is **fully materialized** — pages exist because tabs exist.
The statement exists for `GtkTreeListModel`, which creates children on demand, and
that model belongs to `WFR-WORKSPACE-TREE`. The notes surface proves the
*discipline* (no pruning, no generation advance, no worker start, `try_get()`
everywhere); the tree surface will prove the *hazard*.

## `WorkspaceTreeEvidence` — not built; the hazard is documented and unchanged

Task 4.6's surface is **not implemented in this change** (the tree row's
structural migration moved to slot 5b). The five code facts the delta was written
against were confirmed from the code during task 1.1 and are recorded here so 5b
inherits them verified rather than re-derived:

| Fact | Site | Why it matters to an evidence surface |
| --- | --- | --- |
| `find_store_for_dir` calls `row.children()` | `workspace_section/tree_index.rs:389`, `:402` | Runs the `GtkTreeListModel` create function, populates a child store, and **starts a background scan** — and then **inserts into the `dir_stores` cache** at `:405-408`, mutating on a nominal read |
| `visible_child_stores` calls `row.children()` with **no `is_expanded()` filter** | `tree_index.rs:483`, `:499` | Walks *every* flattened row and materializes each one's children |
| `expanded_store_index` is safe **only** because of a guard | `workspace_section/refresh.rs:452` (`if !row.is_expanded() { continue }`) | The delta's "where the workflow's own code reaches such an accessor safely only because of a guard, derive the field from authoritative state instead of repeating the guarded walk" clause exists for exactly this |
| `set_expanded(true)` at four sites | `folders.rs:376`, `:484`, `actions.rs:65`, `tree_loading.rs:1260` | Materializes children **and** fires the `notify::expanded` hook that queues a **watcher restart** |
| `derive_expanded_paths_from_model` increments the capture counters | `tree_index.rs:28-35` (`expansion_capture_scans`, `expansion_capture_rows`) | Those counters are themselves **asserted as evidence**, so an observer calling this derivation corrupts the metric it observes |
| `find_dir_row` mutates its cache on a nominal read | `tree_index.rs:370` (`dir_rows.borrow_mut().remove(...)`) | A "lookup" that evicts |

The surface 5b builds MUST derive from `expanded_paths` — the authoritative live
set — rather than from any of these, and MUST prove it with reads taken with rows
collapsed and with rows expanded.
