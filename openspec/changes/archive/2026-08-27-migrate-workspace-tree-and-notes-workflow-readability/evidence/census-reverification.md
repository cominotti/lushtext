# Census re-verification, two rows (task 0.3)

Method: production lines only. `#[cfg(test)]` inline modules are excluded by a
brace-tracking counter; a `#[cfg(test)] mod tests;` **declaration** whose body
lives in its own file is excluded along with that file. Row-scoped: only files
the workflow owns, never a shared service, a cross-cutting module, or a
neighbour it merely calls.

## `WFR-WORKSPACE-TREE`

### Size

Census cell (pre-change): `28 files, 16,947 lines (ui 11,682 / model 1,368 /
services 3,897)`.

Re-derived, row-scoped: **20 files, 11,214 production lines**, all in
`ui/sidebar/**`.

| File | Production lines |
| --- | --- |
| `ui/sidebar/mod.rs` | 406 |
| `ui/sidebar/workspaces.rs` | 843 |
| `ui/sidebar/imp.rs` | 246 |
| `ui/sidebar/callbacks.rs` | 219 |
| `ui/sidebar/dialogs.rs` | 205 |
| `workspace_section/tree_loading.rs` | 1,269 |
| `workspace_section/tree_index.rs` | 844 |
| `workspace_section/folders.rs` | 835 |
| `workspace_section/context_menus.rs` | 809 |
| `workspace_section/dnd.rs` | 769 |
| `workspace_section/mod.rs` | 744 |
| `workspace_section/peek.rs` | 728 |
| `workspace_section/refresh.rs` | 666 |
| `workspace_section/watch.rs` | 583 |
| `workspace_section/actions.rs` | 534 |
| `workspace_section/imp.rs` | 508 |
| `workspace_section/row_factory.rs` | 463 |
| `workspace_section/watch_targets.rs` | 264 |
| `workspace_section/row_accessibility.rs` | 199 |
| `workspace_section/icon_presentation.rs` | 80 |

Authoring's upper bound of **11,364 across 21 files** is confirmed exactly, and
the correction is the exclusion of `ui/sidebar/file_tree_item.rs` (150 lines):
the matrix lists it under "Surfaces With No Coordination Tier", so it is **not
this row's to count**. Confirmed by reading that matrix section rather than
assuming.

**Pooled populations the old cell had shared**, named with their sharing rows:

- `services/file_tree.rs` (603 production of 1,050 raw) — shared with the
  command palette's file index and with `WFR-SHELL-LAYOUT`'s recent-documents
  surface. Not owned.
- `services/workspace_manager.rs`, `services/workspace_watch.rs`,
  `services/file_peek.rs` — GTK-free services this row calls. Counted whole,
  including their co-located tests, by the old `services 3,897` subtotal.
- `model/` 1,368 pooled `model/workspace.rs` (434 production of 799 raw, domain,
  28 referencing files) with the two genuinely single-workflow policy modules.

### Seams

Census tuple: `24/7/29/5 = 65 fns, 116 sites`. Re-derived row-scoped:
**58 `*_for_test` functions across 106 `#[cfg(feature = "test-utils")]` gate
sites**, in 10 files.

| File | fns | gate sites |
| --- | --- | --- |
| `workspace_section/mod.rs` | 15 | 23 |
| `workspace_section/watch.rs` | 15 | 24 |
| `workspace_section/refresh.rs` | 9 | 10 |
| `sidebar/dialogs.rs` | 6 | 6 |
| `workspace_section/dnd.rs` | 6 | 12 |
| `workspace_section/tree_loading.rs` | 4 | 10 |
| `workspace_section/tree_index.rs` | 3 | 3 |
| `workspace_section/imp.rs` | 0 | 8 |
| `workspace_section/watch_targets.rs` | 0 | 7 |
| `workspace_section/folders.rs` | 0 | 3 |

The census figure of 65/116 was **not** row-scoped: it pooled the service-side
seams in `services/workspace_manager.rs` (3 fns / 9 sites) and
`services/workspace_watch.rs` (4/4), which the services own.

This is the **largest seam population in the programme**; slot 4's largest
single row held 28 functions across 55 sites.

### Test-only override storage

The census counted 5 override statics for this row. Re-derived, the tree side has
**no module statics at all**: its configuration overrides are **test-only fields
on production state structs**, which no `static` grep finds —
`RefreshRuntimeState::{test_reconcile_batch_delay, test_scan_delay,
test_empty_probe_reads}` and `WatchRuntimeState::{test_start_delay,
test_drop_delay, test_worker_starts, test_last_poll_notices, test_disabled}`
in `workspace_section/imp.rs`, plus a `tree_loading.rs` thread-local counter and
`watch_targets.rs`'s `touched_rows`. Recorded as configuration seams.

## `WFR-NOTES-BOOKMARKS`

### Size

Census cell: `22 files, 12,521 lines (ui 4,977 / model 770 / services 6,774)`.

Re-derived, row-scoped: **4 files, 4,365 production lines** in
`ui/window/notes/**` — `browser.rs` 1,749, `editors.rs` 929, `mod.rs` 892,
`bookmarks.rs` 795. None carries a `#[cfg(test)]` module.

`ui/window/startup_data.rs` (435) is **not** added: task 2.2 decides it
cross-cutting, owned by neither this row nor a restore row (see
`shared-ownership-decisions.md` §2.2). That is a census correction **about
ownership** rather than about a count.

**Pooled populations**, named with their sharing rows:

- `services/palette/notes.rs` — **2,163 production lines of a 3,428-line file**,
  shared with migrated `WFR-COMMAND-PALETTE`. Split re-derived by item span
  rather than taken from authoring: **~180 production lines browser-only** —
  `NotesBrowserMode` and its label/description surface (109–125),
  `NotesBrowserQueryRequest`/`Result`/`Coordinator` (126–145), the browser query
  delay override and its hook (147–166), `query_notes_browser_source`
  (1456–1524), and `search_note_entries_in_category` (1542–1594); **~140
  palette-only** — `PALETTE_NOTE_SOURCE_LIMITS` (97–108),
  `load_palette_note_entries_for_scope` (480–512),
  `admit_synthetic_note_bodies_for_benchmark` (1346–1375), and
  `search_note_entries` (1525–1541); and **~1,840 genuinely shared** — the
  bounded sidecar scan and its admission ledger, `NoteSourceAdmission`,
  `NoteSourceRefreshCoordinator`, entry construction, dedup, scoring, and the
  property-test equivalence surface. Authoring's first read said ~130 / ~300 /
  ~1,700; the browser-only figure is **higher** and the palette-only figure
  **lower** than that guess. Slot 7 should use these numbers rather than
  re-derive them.
- `services/palette/tests.rs` (1,223) is that module's separate-file
  `#[cfg(test)] mod tests;`. A naive per-file production scan counts it as
  production; it is excluded.
- `services/note_storage.rs` (337), `bookmark_service.rs` (694),
  `bookmark_excerpt.rs` (888), `folder_note_service.rs` (948),
  `document_note_service.rs` (676), `services/format_upgrade/**` (3,013) — GTK-free
  services this row calls, counted whole by the old `services 6,774` subtotal.
- `services/migration_ledger.rs` (476) is cross-cutting (`WFR-MIGRATION-LEDGER`),
  also consumed by `WFR-LOCAL-HISTORY`.
- `model/` 770 pooled the five notes domain modules, all of which stay (task 3.7).

### Seams

Census tuple: `2/4/4/0 = 10 fns, 16 sites, 2 override statics`. Re-derived
row-scoped: **7 fns across 15 gate sites** — `mod.rs` 1/7, `browser.rs` 3/3,
`bookmarks.rs` 2/4, `editors.rs` 1/1. `startup_data.rs` has **zero**.

The census tuple pooled the service-side seams in `services/palette/notes.rs`
(4 fns / 6 sites, shared with the palette row) and in
`services/format_upgrade/**`.

### Test-only override storage

Module statics, unlike the tree side: `NOTES_BROWSER_SOURCE_ENTRY_LIMIT_FOR_TEST`
(`notes/mod.rs`), `BOOKMARK_EXCERPT_PREVIEW_DELAY_MS` (`notes/bookmarks.rs`),
plus `NOTES_BROWSER_QUERY_DELAY_MS` and `NOTE_SOURCE_DELAY_MS` re-exported from
`services/palette`, and the `format_upgrade` legacy registry override. The last
three are **service-owned** and stay there on the `editor_io.rs` precedent.

## Pure-policy consumer counts, as owning workflows

| Module | Owning workflows | Verdict |
| --- | --- | --- |
| `model/workspace_scan.rs` | 1 (`WFR-WORKSPACE-TREE`) | relocate (task 5.2) |
| `model/workspace_persistence.rs` | 1 (`WFR-WORKSPACE-TREE`) | relocate (task 5.1) |
| `model/workspace.rs` | many (workspace tree, notes, palette, shell layout, search scope, automation) | domain, stays |
| `model/note.rs` | 1 owning, but consumed by GTK-free `services/note_storage.rs` | stays; relocation would invert dependency direction |
| `model/bookmark.rs` | 1 owning, consumed by `services/bookmark_service.rs`, `bookmark_excerpt.rs` | stays |
| `model/sidecar_identity.rs` | **cross-workflow kernel** — consumed by note, bookmark, document-note, draft, local-history, search-backup, and format-upgrade code | stays |
| `model/folder_note.rs` | 1 owning, consumed by `services/folder_note_service.rs` | stays |
| `model/document_note.rs` | 1 owning, consumed by `services/document_note_service.rs` | stays |

Substring false positives named and excluded: `note` matches
`connect_notify_local`, `notes_menu_button`, `noted`; `bookmark` matches
`connect_bookmarks_changed` and `bookmark_change_generation` on the editor page;
`workspace` matches `workspace_split_view`, `workspace-sidebar-*` GSettings keys,
and `workspace_folder_paths_for_notes`. Consumer sets were taken from `use`
statements and `crate::model::` paths, not from substring hits.
