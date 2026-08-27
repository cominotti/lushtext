# Shared-ownership and structural decisions (tasks 2.1–2.10)

Each decision is made **before** the workflow that would otherwise absorb it is
touched.

## 2.1 The rename join — `ui/window/documents.rs` → notes

**Decision.** `migrate_note_sidecars_after_rename(&Path, &Path)` stays a
**named operation on the notes facade**, invoked from the tree side through the
window. The name is already intent-first, so it is kept rather than renamed; the
facade delegates the body to the notes `journal` coordination module.

**Ordering guarantee: owned by `WFR-WORKSPACE-TREE`.** The tree's rename stage
completes its own cache, watch-row, expansion-set, and item-cache updates and
only then fires the rename callback that reaches
`ui/window/documents.rs`, which updates open tabs and *then* calls the notes
operation. The notes row owns nothing about that ordering; it owns only the
guarantee that once called, pending ledger state is recorded before any sidecar
move (contract 1 in `durability-contracts.md`).

The notes row migrates **first** precisely so this entry point is a settled call
into a migrated facade by the time the tree row is restructured.

## 2.2 `ui/window/startup_data.rs` — **option (3), cross-cutting**

**Decision: cross-cutting, owned by neither row.** Recorded in the matrix's
Cross-Cutting Coordination section with its owning callers named, and the file
stays at `ui/window/startup_data.rs` with the ownership sentence added to its own
module doc.

**Decided by owning workflows, not by call count.** `continue_startup_data_flow`
calls, in order:

| Call | Owning workflow |
| --- | --- |
| `reconcile_pending_migrations_on_startup` | `WFR-NOTES-BOOKMARKS` (this change) |
| `sidebar.load_workspaces()` | `WFR-WORKSPACE-TREE` (this change) |
| `refresh_workspace_scope_consumers` | `WFR-WORKSPACE-TREE` + `WFR-SHELL-LAYOUT` |
| `flush_pending_activation_opens` | `WFR-DOCUMENT-LOAD` (migrated) |
| `load_session_and_drafts` | `WFR-SESSION-RESTORE` (migrated) |
| `start_autosave_timer` | `WFR-DRAFT-RECOVERY` (migrated) |

That is **five owning workflows**, not the four the handoff anticipated. The
gate's other half — the format-upgrade preflight, dialog, and apply — is
governed by its own capability spec (`format-upgrade-workflow`) and rewrites app
data for *every* consumer, not for notes: the dialog body itself says
"workspaces, sessions, drafts, notes, or undo state".

Options (1) and (2) were both rejected on the same ground: naming a home for a
module whose job is *release ordering across five workflows* would make one row's
matrix entry claim a file that four other rows equally drive. Slot 4 flagged this
class — an ownership correction where a name invites trust — as more dangerous
than a wrong count.

**Consequences, recorded so nothing is absorbed silently:**

- The notes row's file set **loses this file**. `census-reverification.md` records
  the row at 4 files / 4,365 production lines, not 5 / 4,800.
- `load_session_and_drafts` and `start_autosave_timer` remain **calls** into
  migrated facades. Untouched.
- The one-shot `completed` latch keeps its current semantics exactly.
- The 21 ungated `imp().startup_data_flow.*` reads in
  `crates/lushtext/tests/widget/window.rs` do **not** become notes-evidence
  reads, because a cross-cutting module owns no evidence surface. They migrate to
  one **named window operation**,
  `LushtextWindow::startup_data_flow_completed() -> bool`, which is the shape
  slot 4 used for `document_identity.rs`. That is not a "second narrow getter"
  under the evidence-surface rule, which governs a migrated workflow's surface;
  a cross-cutting module has none.
- `pending_activation_paths` boundedness is routed to task 7.1, not decided here.

## 2.3 `StartupDataFlowState` and its unbounded queue

Recorded, not decided here: `pending_activation_paths: RefCell<Vec<PathBuf>>` has
no cap and is fed by the desktop/CLI activation interface while the gate is
pending. Verdict routed to task 7.1.

## 2.4 `NoteSourceRefreshCoordinator` — **retire it**

**Decision: retire the type into
`services::single_flight::SingleFlightCoordinator`.** Authoring's comparison is
confirmed from the code, and one claim was **stronger** than stated: the
cancellation type does not merely differ by name — `NoteSourceRefreshStart`
already carries `PaletteSearchCancellation`, which
`services/palette/runtime.rs:14` already aliases to
`single_flight::FlightCancellation`. The two coordinators therefore already share
their cancellation type.

Verified differences, all in the shared type's favour:

| | `NoteSourceRefreshCoordinator` | `SingleFlightCoordinator<R>` |
| --- | --- | --- |
| `submit`/`finish`/`invalidate`/`is_current`/`has_work`/`snapshot` | present | present, identical semantics |
| generic over the request | no | yes |
| `clear_pending()` | absent | present |
| `active_generation()` | absent | present |
| snapshot fields | 4 | 6 (adds `active_high_water`, `pending_high_water`) |

All state that genuinely differs between the window's
`command_palette_note_refreshes` instance and the browser's `source_refreshes`
instance lives in the **surrounding structs**, not in the coordinator. Two
sibling types in the same service — `NotesBrowserQueryCoordinator` and
`BookmarkExcerptPreviewCoordinator` — are **already aliases** over the shared
coordinator, so this is the last unaliased one in the workflow.

Implementation: three type aliases replace ~100 lines of duplicated
submit/finish/supersede logic.

```rust
pub type NoteSourceRefreshStart = SingleFlightStart<NoteSourceRefreshRequest>;
pub type NoteSourceRefreshCoordinator = SingleFlightCoordinator<NoteSourceRefreshRequest>;
pub type NoteSourceRefreshCoordinatorSnapshot = SingleFlightSnapshot;
```

**The migrated row this touches, and the neutrality proof.** The palette's
instance changes type. Permitted **only** as a type-level substitution:

- the palette's `command-palette-index` readiness blocker reads
  `command_palette_note_refreshes.has_work()` — unchanged method, unchanged
  semantics;
- the palette's evidence surface does not project this coordinator's snapshot at
  all, so no palette snapshot field changes shape;
- no exported D-Bus field is added, removed, or retyped.

Proved by capture-and-diff in `automation-no-widening.md`, not by assertion.

**Call sites that move with it**, named: `services/palette/notes.rs`
(definition + its own unit test at `:2872`), `services/palette/mod.rs` re-export
list, `ui/window/imp.rs` type of two fields, `ui/window/notes/browser.rs`
destructuring (unchanged, `SingleFlightStart` has the same public field names),
`ui/window/notes/mod.rs` snapshot field type, and
`crates/lushtext-core/benches/benchmarks.rs:1294`.

**The blocker slot 2a recorded is dissolved by this slot's own work**, not
deferred a third time: the shape change to `NotesBrowserRuntimeSnapshot` is
absorbed because that snapshot is folded into this row's `evidence.rs` in the
same change.

## 2.5 `WFR-WORKSPACE-TREE` role home — **option (2), nested**

**Decision: canonical role home `ui/sidebar/`, with bounded coordination role
modules nested inside `ui/sidebar/workspace_section/`.** Exactly one `policy.rs`
and one `evidence.rs`, both at `ui/sidebar/`.

Option (1) — all roles flat with `workspace_section/` recorded wholly as
presentation — was rejected as **dishonest**: `watch.rs`, `refresh.rs`,
`tree_loading.rs`, and `folders.rs` coordinate ordered stages and are not widget
projection. Option (3) — a per-workflow subdirectory — buys nothing, because
`ui/sidebar/` hosts exactly one workflow and there is no `policy.rs`/`evidence.rs`
name collision to resolve.

`ui/**/policy.rs` in `.cargo/mutants.toml` reaches `ui/sidebar/policy.rs`;
re-verified after the move by task 5.5.

### Every module classified

`ui/sidebar/` — canonical role home:

| Module | Classification |
| --- | --- |
| `mod.rs` | **narrative facade** |
| `policy.rs` | **pure policy** (the row's one) |
| `evidence.rs` | **evidence** (the row's one) |
| `seams.rs` | **seam value objects** |
| `test_policy.rs` | test-only configuration, entirely `#[cfg(feature = "test-utils")]` |
| `list_execution.rs` | coordination — `execution`, qualified: workspace-list load and add/rename/unlist |
| `persist_execution.rs` | coordination — `execution`, qualified: the persistence pipeline |
| `filter_execution.rs` | coordination — `execution`, qualified: the workspace scope filter fade and its settle timer |
| `callbacks.rs` | **called presentation surface** — projects the workflow onto the window's callback slots |
| `dialogs.rs` | **called presentation surface** — workspace/folder dialog construction |
| `imp.rs` | **called presentation surface** — subclass state, template children, construction, disposal |
| `width_preset.rs` | **cross-cutting**, not this row's: `WorkspaceSidebarWidthPreset` is the `workspace-sidebar-width-policy` capability's value, consumed by Preferences and the window shell (`WFR-SHELL-LAYOUT`, slot 7) |
| `file_tree_item.rs` | outside the row — matrix "Surfaces With No Coordination Tier" |

`ui/sidebar/workspace_section/` — nested:

| Module | Classification |
| --- | --- |
| `watch.rs` | coordination — `watch`. **Already correctly named; not renamed for symmetry.** |
| `scan_admission.rs` | coordination — `admission`, qualified: the per-child-store scan flight and its admission retry |
| `scan_execution.rs` | coordination — `execution`, qualified: the child scan worker, batched reconciliation, and child-store materialization |
| `refresh_execution.rs` | coordination — `execution`, qualified: refresh coalescing and planning (was `refresh.rs`, which is not a bounded name) |
| `folder_execution.rs` | coordination — `execution`, qualified: top-level folder rows, the empty probe, and focused-folder drilldown (was `folders.rs`) |
| `file_execution.rs` | coordination — `execution`, qualified: create, inline rename, delete (was `actions.rs`) |
| `peek_execution.rs` | coordination — `execution`, qualified: `Space` peek (was `peek.rs`) |
| `reorder_execution.rs` | coordination — `execution`, qualified: folder-reorder DnD (was `dnd.rs`) |
| `imp.rs` | **called presentation surface** |
| `row_factory.rs` | **called presentation surface** |
| `context_menus.rs` | **called presentation surface** |
| `row_accessibility.rs` | **called presentation surface** |
| `icon_presentation.rs` | **called presentation surface** |
| `mod.rs` | **called presentation surface** — the section GObject's public wrapper |

Two pre-convention modules are **dissolved rather than renamed**, because
neither is one coordination job:

- `tree_index.rs` (844) — its pure index arithmetic (splice windows,
  changed-path→owning-directory, common prefix/suffix, desired-versus-current
  diff) moves to `policy.rs`; its child-store lookup and cache maintenance move
  to `scan_execution.rs`, which is where child stores are materialized. No new
  role name is needed, and the bounded set is not widened. This was the one
  module with a real risk of forcing an escalation, and dissolving it is the
  honest alternative to inventing an `index` role.
- `watch_targets.rs` (264) — already pure and GTK-free: its two generation
  newtypes move to `seams.rs`, its mirror arithmetic to `policy.rs`, and its
  snapshot into `evidence.rs`.

The presentation modules keep every behavior obligation the
"Workspace-section wiring has focused owners" requirement places on them, with
ownership recorded in their own module docs and named in the matrix row.

## 2.6 One row or two, and the facade budget — **option (1), one facade, delegated hard**

**Decision: one row, one facade, no escalation, no split.** Measured before any
facade text was written.

The reconciled trace (task 0.4) is **11 stage orders, 27 primitives, 11
non-primitive callback resumptions**. Against that:

| Facade component | Measured / budgeted lines |
| --- | --- |
| `ui/sidebar/mod.rs` today | 406 |
| less `WorkspaceSidebarWidthPreset` (lines 171–273), moved to `width_preset.rs` as cross-cutting | −103 |
| less `SidebarFileRowStateSnapshot` (56–91), moved to `seams.rs` | −36 |
| less `WorkspacePersistenceFlushError` (34–54), moved to `seams.rs` | −21 |
| less the five readiness/focus/context-menu section fans (93–169), delegated to `evidence.rs` and the section presentation surface | −60 |
| remaining public API, wrapper, imports | ≈186 |
| plus narration: header, 11 stage orders compressed, inversions one line each, role table, shared-state table | ≈120 |
| plus module declarations and re-exports | ≈45 |
| **projected facade** | **≈351 of 370** |

Option (2), escalating the budget, is **not** taken: it would cost a ten-row
retroactive re-check and the measurement says it is unnecessary. Option (3),
splitting the census row, is **not** taken: although a prima facie case exists
(the sidebar imp versus the section imp; `dialogs.rs`'s six dialog seams versus
the section's fifty-two), the two halves are **not** independent — the workspace
list's add/unlist creates and destroys the very sections the file tree lives in,
`load_workspaces` is the single entry point for both, and both halves share
`current_scope`, `workspaces_file`, and the persistence debounce. Splitting would
have been budget avoidance for one workflow that simply narrates a lot, which the
task forbids.

The budget line is not edited. Final measurement is recorded in
`facade-measurements.md`.

## 2.7 The `journal` role name, checked per stage order

Test applied: *does a later stage of the same workflow read the record back* —
not *does it touch the disk*.

| Record | Verdict | Reason |
| --- | --- | --- |
| **Notes migration ledger** | **`journal`** | Pending entries for three kinds are written by the rename stage and read back by the startup-reconcile stage **in a later process run**, under a bounded `MAX_MIGRATION_ATTEMPTS` attempt count, with stale-entry completion. This is the strongest `journal` fit in the programme. The mutual-exclusion gate serializing ledger writes lives **inside** the journal per slot 2b, not in a separate `admission`. |
| **Note and bookmark sidecars** | **not a journal** — ordinary durable service persistence | Written on save by `bookmark_service` / `document_note_service` / `folder_note_service`, read back on the next open, browser build, or palette source refresh. They are the workflow's *authoritative user content*, not a generation-guarded recovery record: no generation is stamped in the file, there is no stale-record cleanup, and no later stage restores *from a failure* using them. Reading stage named: the browser source build (`browser.rs:990`) and `resolve_notes_for_editor` (`mod.rs:379`). |
| **Format-upgrade backup** | **not this row's, and not a journal** | Written before apply and read back only by manual recovery, never by a later stage of the workflow. Its owner is cross-cutting `startup_data.rs` / the `format-upgrade-workflow` capability (task 2.2). Decided explicitly rather than by analogy to the ledger. |
| **Workspace persistence (`workspaces.json`)** | **not a journal** — `execution` with latest-generation supersession | Written on debounce, read back at the next launch by `load_workspaces`. That read-back is *load*, the way any settings file is loaded, not recovery from a failure: the file carries no generation, there is no stale-record cleanup, and a failed write leaves the previous file intact and awaits **explicit** user retry. Slot 4 established that a `journal` owns the gate serializing its own writes; here the one-active-write gate exists to keep the *worker* single-flighted, which is `execution`'s own bookkeeping. Named `persist_execution.rs`. |

No genuinely novel coordination job appeared, so
`gtk-adapter-module-boundaries`' role set is **not** amended.

Stage-order qualification: applied to the **new** modules only. `watch.rs` is
already a correct bounded role name and is **not** renamed for symmetry, per slot
2b's narrow reading.

## 2.8 Buried pure policy in the shared services — decided explicitly

A `services -> ui` relocation is **forbidden outright**, and the answer for a
service with a `model/` or second-service consumer is **no**.

| Service | Verdict |
| --- | --- |
| `services/file_tree.rs` | **stays.** Consumed by the palette's file index and the recent-documents surface as well as this row. Its `classify_entry` decision is genuinely shared. |
| `services/workspace_manager.rs` | **stays.** Consumes `model/workspace.rs` and is consumed by automation and the window; its pure parts are workspace-file schema decisions the model owns. |
| `services/workspace_watch.rs` | **stays.** The mailbox cap and coalescing are the *service's* contract, shared with the external-file-monitor capability. |
| `services/file_peek.rs` | **stays.** Peek metadata formatting is presented by this row but computed from service-owned file facts; the formatters this row owns (size/time/metadata strings) are extracted from the **GTK adapter**, not from the service. |
| `note_storage.rs`, `bookmark_service.rs`, `bookmark_excerpt.rs`, `folder_note_service.rs`, `document_note_service.rs` | **stay.** Each consumes a `model/` module and is consumed by the palette service too. |
| `services/migration_ledger.rs` | **stays, cross-cutting** (`WFR-MIGRATION-LEDGER`); also consumed by `WFR-LOCAL-HISTORY`. |
| `services/format_upgrade/**` | **stays**, owned by the `format-upgrade-workflow` capability and reached only through cross-cutting `startup_data.rs`. |

Behavior in all of them is unchanged by this change.

## 2.9 Closed boundaries, confirmed not re-opened

`model/workspace_search.rs` (closed by 2b), `model/file_load.rs` (closed by 3b),
`model/buffer_replacement.rs`, `model/editor_memory.rs`,
`model/migration_ledger.rs`, `ui/plain_disposal.rs`, `ui/buffer_snapshot.rs`,
`services/single_flight.rs`, and `services/sync.rs` are cross-cutting, exempt, or
already decided. **None is re-opened here.** `services/single_flight.rs` is
*consumed* more widely by task 2.4 but is not modified.

## 2.10 Excluded by scope, recorded where a reader hits the adjacency

- `WFR-SHELL-LAYOUT` (slot 7) keeps the workspace sidebar show/hide animation and
  the recent-documents surface. `WorkspaceSidebarWidthPreset` moves to
  `ui/sidebar/width_preset.rs` and is recorded as **that row's**, not this one's.
- `ui/sidebar/file_tree_item.rs` is a surface with no coordination tier.
- `WFR-MINIMAP` is slot 6; its four `ui/automation.rs` reach-throughs are left
  alone (task 6.1).
- `ui/window/local_history/restore_execution.rs`'s two `resolve_notes_for_editor`
  calls and `local_history/journal.rs`'s `MigrationKind` record **stay calls**
  into a migrated row.
- The command palette's note source is migrated: this change substitutes its
  coordinator type under 2.4 and does not otherwise restructure it.
