## Context

The workspace sidebar's file tree is a `GtkListView` over a `GtkTreeListModel` (`passthrough = false`, `autoexpand = false`), rendered through a `GtkTreeExpander` per row (`ui/sidebar/workspace_section/row_factory.rs`) and hosted in `gtk_lush_widgets::ViewportSliceBin`. Its create function is the closure in `workspace_section/folder_execution.rs`:

```rust
if !file_item.is_dir() || file_item.is_empty() == Some(true) { return None; }
file_item.path().map(|path| scan_execution::build_children_model(&section, &path).upcast())
```

`build_children_model` builds a `gio::ListStore` and calls `populate_child_store`. That function registers the store in `child_store_paths`, `child_store_refs` (through `ensure_child_store_identity`), `dir_stores` and `child_scan_flights`, reads two GSettings keys, and submits a scan. When a process-wide scan admission permit is free, `start_child_scan` admits it synchronously and hands `(store, path, cancel, permit)` to `gtk_lush_tasks::spawn_blocking_then`. That tuple keeps a **strong** reference to the store until the GTK completion runs. The worker calls `services::file_tree::scan_directory_bounded`, which reads the directory and peeks one level into up to `workspace-empty-folder-lookahead-cap` subdirectories.

### What GTK does (GTK 4.22.5 source, verified for this proposal)

| Fact | Source |
| --- | --- |
| `gtk_tree_expander_update_for_list_row` evaluates `gtk_tree_list_row_is_expandable (row) && !self->hide_expander`. `is_expandable` comes first, so `hide-expander` does not prevent the query. | `gtk/gtktreeexpander.c:189` |
| The update runs from `set_list_row` (every bind, and every rebind as rows recycle during scroll) and from the row-notify callback for every property except `expanded` and `item`. It also runs from the `indent-for-depth` / `indent-for-icon` setters. | `:904`, `:277-310`, `:950`, `:993` |
| `is_expandable` returns `FALSE` when `node->empty` is set, returns `TRUE` when `node->model` is set, and otherwise **calls the create function and unrefs the result**. Only `None` is remembered (`node->empty`). A positive answer is not cached, so every query on a collapsed, non-empty node calls the create function again. | `gtk/gtktreelistmodel.c:1255-1283`, `:280-291` |
| `set_expanded(false)` clears `node->model` (`g_clear_object`) and then emits `notify::expanded` and `notify::children`. The `children` notify reaches the bound expander's update, which queries `is_expandable` on the now-collapsed node and calls the create function again. | `:515-530`, `:399-410`, `:1207-1218` |
| `set_expanded(true)` calls the create function, installs the model in `node->model`, emits `items-changed` and then `notify::expanded` and `notify::children`, all before it returns (A22). | `:1198-1218`, `:493-512` |
| `GtkTreeListRow` objects are cached weakly per node: dispose clears `node->row`, and a later `row()` call creates a new object. | `:1029-1038` |

A22 (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`) already pins, as a clause "recorded beside it", that `is_expandable()` runs the create function and discards the model, and that `None` marks the row empty. It does **not** pin that `GtkTreeExpander` makes that query on every bind, that `hide-expander` does not prevent it, that a positive answer is not cached, that collapsing leads to a fresh probe, or that a discarded probe model is finalized when nothing else holds it. This design depends on all five.

### Hypotheses the measurement must confirm or refute (task group 1)

- **H1:** each bind of a collapsed directory row whose parent-scan hint is `Some(false)` or `None` (lookahead cap exhausted) submits one child scan, and each recycled rebind submits another.
- **H2:** collapsing an expanded, bound directory submits one scan of that directory.
- **H3:** while a probe's scan is in flight, `dir_stores[path]` upgrades to an orphan store that no row shows.
- **H4:** probe registrations (`child_store_paths`, `child_store_refs`, `child_scan_flights`) outlive their stores until address reuse or section teardown.
- **H5:** probe scans compete with real expansions and folder-emptiness probes for the process-wide scan admission.

If H1 and H2 are refuted (no scan is submitted on bind or collapse), the change stops after task group 2 (the axiom) and records the refutation in `docs/next/formal-verification.md`. The fix is not built for a cost that does not exist.

## Goals / Non-Goals

**Goals:**

- Binding, rebinding or collapsing a directory row causes no directory I/O, no scan admission, no `spawn_blocking_then` slot and no child-store registration.
- A directory GTK actually expands is scanned exactly as before: same admission, same active-plus-latest flight, same generation guards, same bounded reconciliation.
- Readiness, the reveal's scan wait and every existing sidebar contract stay correct.
- The GTK behaviour this relies on is a pinned ledger axiom with a probe and a sample.
- Failing-first evidence: every new assertion fails on the current tree before the fix lands, and the recorded numbers come from the run.

**Non-Goals:**

- Changing the empty-folder lookahead, the `(Empty)` semantics, the attention-moment revalidation, or GTK's permanent "empty" marking after a `None`.
- Replacing `GtkTreeExpander` or `GtkTreeListModel` (for example with `passthrough = true` and custom expansion logic).
- Reducing `scan_execution.rs`'s recorded size debt beyond not growing it.
- Any change to the long-line stream's budgets or code.

## Decisions

### D1. Deferred materialization: the create function only creates a store; a drain decides afterwards

The create function cannot tell whether GTK is probing (`is_expandable`) or installing (`set_expanded(true)`): both reach the same call. GTK reveals the difference only after the call returns. A probe's model loses its last reference inside `is_expandable`, while an installed model is held by `node->model`. So:

1. `build_children_model` (drag-hover path unchanged and first) creates an empty `gio::ListStore<FileTreeItem>`. It pushes `PendingMaterialization { store: WeakRef, path, lifetime }` onto a per-section queue, increments `child_model_creations`, and arms the section's drain if it is not already armed. It registers nothing else, reads no settings and submits nothing.
2. The drain (`glib::idle_add_local_full(glib::Priority::HIGH, …)`) takes at most `MATERIALIZATION_DRAIN_BATCH` entries per dispatch and re-arms if more remain. For each entry it asks the pure policy for a `MaterializationDisposition`:
   - `DiscardProbe`: the weak reference no longer upgrades. Increment `child_model_probes_discarded`.
   - `Retired`: the section lifetime changed, the section is disposed, or the path was retired by `clear_dir_states`.
   - `AlreadyMaterialized`: `child_store_identity_matches` already holds, for example because a refresh reached the installed store through `find_store_for_dir` → `populate_child_store` first. Skip, so the store is never scanned twice.
   - `Materialize`: increment `child_materializations` and call the existing `populate_child_store(section, path, &store)` unchanged.
3. The new code lives in a new coordination module `workspace_section/materialization_execution.rs` (a bounded `execution` name qualified by the stage order it serves), so the over-target `scan_execution.rs` does not grow. The pure decision `materialization_disposition(facts) -> MaterializationDisposition` and the drain-batch constant live in `ui/sidebar/policy.rs`, inside the mutation scope.

Why `Priority::HIGH`: GLib dispatches only the highest-priority ready sources in an iteration. A high-priority idle therefore runs before the reveal's 16 ms `DEFAULT` poll, before default-priority idles such as the watch-row refresh, and before GDK's redraw (`GDK_PRIORITY_REDRAW`). A user's expansion gets its scan submitted in the same main-loop iteration, earlier than the frame that draws the expanded (still empty) row, so latency does not visibly change. The drain is bounded per dispatch, so it cannot starve the loop.

**Alternatives considered:**

- *A cheap expandability hint from the parent scan.* LushText already has one: the lookahead `is_empty` marks `Some(true)` rows, whose create call returns `None`, after which GTK never asks again. A hint cannot make a non-empty row expandable without the create function returning a model, so it cannot remove the probe for `Some(false)` and `None` rows. That leaves the hint necessary but not sufficient. It stays as it is.
- *`hide-expander` for hinted rows.* Ineffective: `is_expandable` is evaluated before `!hide_expander` (`gtktreeexpander.c:189`).
- *Return `None` when the hint is unknown.* GTK would mark the node empty for good and the user could never expand it. Rejected.
- *Start the scan from the row's `notify::expanded` hook* (`watch.rs::install_expanded_watch_hook`). The hook is attached to a `GtkTreeListRow` object, and GTK caches those weakly and recreates them, so a recreated row object has no hook and its expansion would never be scanned. The drain depends only on the store's liveness, which GTK's own ownership decides. Rejected as fragile.
- *Cache one store per path and hand it back to repeated probes.* Keeps a store alive for every bound directory, and GTK still discards the model on each probe. Rejected.
- *Probe-aware passthrough model with a custom expander.* A rewrite of the tree, its DnD, its accessibility and its gesture handling for one cost. Rejected.
- *Default-priority idle without readiness coverage.* The reveal's poll could run first and read "no scan pending" for a directory that was just expanded. Rejected. D2 adds readiness coverage on top of the priority choice.

### D2. A pending materialization of a live store is a pending child scan

`child_scan_blocks_readiness` additionally reports any pending-materialization entry **whose weak store still upgrades**. A probe's store is finalized inside `is_expandable` (new axiom), before any readiness reader can run, so probe entries never count; only an installed store waiting for the drain does. `child_scans_pending()`, `workspace-refresh-complete`, the reveal's scan wait and `workspace_refresh_blocks_readiness` therefore never read an installed-but-unscanned directory as "scanned, and empty". No readiness predicate or blocker name changes, so the automation contract does not change.

`child_scan_blocks_readiness` also drives the file tree's accessible `busy` state through `sync_child_scan_busy_state`, and the manual-refresh "Workspace folders refreshed" announcement. So the enqueue inside the create function MUST NOT call `sync_child_scan_busy_state` (the store is still alive at that moment and a probe would set `busy`), and the drain calls it once per dispatch only after it applied a `Materialize` (which `populate_child_store` already does). Today every probe scan toggles the tree's accessible `busy` state while the user merely scrolls; after this change binding and scrolling leave it untouched, which is an accessibility behaviour change recorded in task 5.3.

### D3. The evidence surface carries the measurement

`WorkspaceScanPressureEvidence` gains `child_model_creations`, `child_model_probes_discarded`, `child_materializations`, `pending_materializations`, `scan_submissions` (`populate_child_store` calls, which today happen inside the create function) and `registered_child_stores` (`child_store_paths.len()`). Each is read from a `Cell` or `RefCell` and calls no hazard accessor, so the surface's no-materialization proof still holds and is extended to read before and after a scroll. Widget tests read these fields. No `*_for_test` getter is added, and the matrix ratchet is unchanged.

### D4. Failing-first, deterministic measurement

The measurement is a widget test in an existing sidebar widget module (`crates/lushtext/tests/widget/workspace_section.rs`, already in a shard). Its fixture is synthetic: one workspace folder holding 300 sibling directories, each holding one file, built through `services::filesystem::fixture`. `workspace-empty-folder-lookahead-cap` is set to its minimum (100) through the memory GSettings backend, so the rows mix `Some(false)` hints (the first 100 directories in the bounded scan's traversal order, which need not be display order, so the test reads each row's hint rather than assuming positions) and `None` hints. The test presents the window at a fixed size, waits for `workspace-refresh-complete`, and records the evidence counters after each of these steps: first bind; scroll to the end through the outer scroller; scroll back; expand one directory, wait, then collapse it; expand one directory. Assertions are counts, which are deterministic. Per-dispatch timing is recorded in the test output and in the change's evidence file, not asserted, because the widget harness's frame clock is not a reliable timer.

Target assertions, which must fail on the current tree:

- After binds and scrolls with no subdirectory expanded, `scan_submissions` and `registered_child_stores` equal the number of expanded directories, which is the workspace folder root alone (its own expansion legitimately creates, registers and scans one store). Equivalently, the per-path projection shows no submission and no registration for any of the 300 subdirectories (H1, H4).
- The file tree's accessible `busy` state never becomes true during the bind and scroll steps (D2).
- Collapsing adds no scan submission (H2).
- For every never-expanded directory path, no `dir_stores` entry upgrades. This is read through a section-evidence projection of path-keyed liveness, never through `find_store_for_dir` (H3).
- One expansion causes exactly one `child_materializations` and one admitted scan, its children reach the flattened model, and `workspace-refresh-complete` returns.

H5 follows from H1 and is reported, not asserted separately. The measured pre-fix numbers are written to `openspec/changes/stop-expander-bind-time-child-scans/evidence/baseline.md` before any production edit.

### D5. The new axiom: probe and sample

The probe follows A22's pattern: a `TreeListModel` over root items `a` (create returns a one-item store), `b` (create returns `None`) and `c`. The create function counts calls and keeps a `WeakRef` to every store it returns. A `GtkTreeExpander` is constructed but not placed in any window. The probe measures:

1. `set_list_row(a)` → one create call, and that store's weak reference is dead on return.
2. `set_list_row(None)`, then `set_list_row(a)` again → one more call (no positive cache).
3. With `hide-expander = true`, a rebind still makes one call.
4. `a.set_expanded(true)` → one call, the store is alive and equals `children()`, and the `children` notify does not call again.
5. `a.set_expanded(false)` with the expander bound → the installed store is dead after the call, and one more call happens whose store is dead on return.
6. Control: `b` is probed once, and any number of rebinds make zero further calls.

The sample shows the same fixture in a `GtkListView` with a live "create calls" counter for a person to watch while scrolling and toggling, and `--check` prints the verdict headless. The ledger row records the dependent design (`materialization_execution.rs`) and the consumer tests from D4. A22's "recorded beside it" clause gains a pointer to the new id. A22's own statement does not change.

### D6. Formal verification fit

The disposition is a pure function over a small finite domain: weak liveness, lifetime currency, path retirement, already materialized. Its whole input space is enumerated in an exhaustive unit test (every combination, asserted against a reference table). That gives the same guarantee a Kani harness would, at no shard cost, so no Kani harness is added and none is needed to keep `make check-kani-shards` consistent. The function is in `ui/sidebar/policy.rs`, so its mutants are a gain from zero in the mutation scope. The drain batch arithmetic (take ≤ N, re-arm if more remain) is integer-only and covered by the same unit test. `ui/sidebar/policy.rs` is not a whole-pixel geometry module, so rule 10 is unaffected.

## Risks / Trade-offs

- **[Risk] GTK holds a probe model past `is_expandable` (a future change, or a hidden extra reference).** → The drain would then scan a store nobody installed: the old waste, but no incorrect state, because `populate_child_store` is unchanged. The new axiom's probe fails first and flags it on any toolkit update (`make gtk-axioms` runs first under the toolkit-update procedure), and D4's assertions fail too.
- **[Risk] GTK starts caching positive answers or stops querying on bind.** → The design stays correct, because only installed stores are scanned. The probe fails, and the ledger row and this design are revisited under the ledger rule.
- **[Risk] Code that ran synchronously after `set_expanded(true)` expected the store to be registered already** (restore, refresh, reveal). → `find_store_for_dir` falls back to `row.children()` and registers the store. A refresh that materializes it first is handled as `AlreadyMaterialized`. Readiness covers the gap. The existing reveal, restore and refresh widget tests stay green, and task 4 audits every `set_expanded(true)` call site.
- **[Risk] An entry retired for a store GTK still shows.** `clear_dir_states` runs on rename and removal. A directory renamed in the same main-loop iteration as its expansion would keep an installed, unscanned store. → This matches today, where the same `clear_dir_state` cancels the store's in-flight scan and drops its registration; the next refresh reaches the store through `find_store_for_dir`'s `row.children()` fallback. Inline rename is asynchronous, so the window is in practice unreachable. No new hazard.
- **[Risk] Queue growth within one turn.** → Entries are a weak reference plus a path. Binds per turn are bounded by the realized rows (A1: about 205 per list), and expansions per turn by user or restore batches. Each dispatch handles at most `MATERIALIZATION_DRAIN_BATCH` entries.
- **[Trade-off] A ListStore is still allocated per probe.** GTK requires a model to answer "expandable". The cost is one GObject allocation and a `Vec` push. Bind-dispatch time is recorded against the working interaction budget B_interact = 8 ms per dispatch.
- **[Risk] Store-address ABA after a discarded probe.** → Probes are no longer registered by address, so the existing `ensure_child_store_identity` guard only ever sees installed stores.
- **[Risk] Visual or accessibility gates.** → The touched files sit under `crates/lushtext-core/src/ui/`, so the accessibility and visual fingerprints are dirty. The smoke lanes are rerun headless. The expander's rendered affordance and accessible expanded state still come from GTK's own `is_expandable` and `is_expanded`, which do not change.

## Migration Plan

No persisted state, setting, action or D-Bus surface changes. The change lands in one series: measurement (failing), axiom, fix, docs. Rollback is a revert. The axiom row stays valid after a revert, because it describes GTK, not LushText.

## Open Questions

- **Final axiom id.** This change names its axiom **T1** provisionally. `gtk-lush-axioms`' catalogue test requires contiguous ids, so the final id is the next free one when this change is applied: A23 if it lands before `probe-long-line-toolkit-costs` (which then renumbers its A23–A36 range, its task 1.2), otherwise the id after the last applied long-line axiom.
- **`MATERIALIZATION_DRAIN_BATCH` value.** Proposed 256 (the calibrated `CHILD_APPEND_BATCH_SIZE`, which is per-row work, while a drain entry is cheaper). Keep 256 unless the measured drain dispatch exceeds the working B_interact.
- **Whether the new counters belong in the `window.workspace` automation snapshot.** This proposal keeps them evidence-only (no automation docs change). Exporting them would let a live-session check confirm the fix on a real workspace.
