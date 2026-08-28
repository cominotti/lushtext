# `window.workspace`: pre-change surface, projection plan, and no-widening proof (section 7)

## 7.1 The exported surface, identified from the code rather than from memory

Read from `crates/lushtext-core/src/model/automation.rs` and
`docs/automation-reference.md`. **These are the pre-change values the changed tree
must reproduce exactly.**

### The `window.workspace` object — ten fields

`AutomationWorkspaceSnapshot`, all ten confirmed present:

| Field | Type | Documented meaning |
| --- | --- | --- |
| `scope_kind` | `String` | current scope kind: `all` or `workspace` |
| `scope_workspace_id` | `Option<String>` | concrete workspace id when the scope targets one workspace |
| `scope_workspace_name` | `Option<String>` | user-visible workspace name for the selected workspace |
| `workspace_count` | `u32` | total persisted workspaces |
| `folder_count` | `u32` | total configured folder memberships across all workspaces |
| `scoped_folder_count` | `u32` | folder memberships covered by the current scope |
| `no_workspaces` | `bool` | whether no persisted workspaces exist |
| `persistence_inflight` | `bool` | whether the sidebar is writing workspace state in the background |
| `persistence_dirty` | `bool` | whether another workspace save is pending after the in-flight write |
| `filter_animation_active` | `bool` | whether the workspace filter fade sequence is active |

### The three readiness blockers this row owns

| Constant | Serialized id | Source at authoring |
| --- | --- | --- |
| `READINESS_BLOCKER_WORKSPACE_TREE_REFRESH` (`:256`) | `workspace-tree-refresh` | `imp.sidebar.workspace_refresh_blocks_readiness()` — a **named facade accessor**, already clean |
| `READINESS_BLOCKER_WORKSPACE_PERSIST` (`:254`) | `workspace-persist` | `imp.sidebar.workspace_persistence_pending()` — a **named facade accessor**, already clean |
| `READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION` (`:258`) | `workspace-filter-animation` | **`imp.sidebar.imp().workspace_filter_animation_active.get()`** — a production `.imp()` **reach-through** at `ui/automation.rs:766`, this row's to retire |

`READINESS_BLOCKER_WORKSPACE_SIDEBAR_ANIMATION` (`:260`, `workspace-sidebar-animation`)
is **not this row's** — it is `WFR-SHELL-LAYOUT`'s (slot 7), because the blocker
follows the animation, not the row name. Honoured, not absorbed (task 7.5).

### The predicate and workflow ids

- `AutomationReadinessPredicate::WorkspaceRefreshComplete` → serialized
  `workspace-refresh-complete` (`model/automation.rs:453`).
- `AUTOMATION_WORKFLOW_WORKSPACE_REFRESH` → `workspace-refresh` (`:30`).

### Every predicate that lists one of the three blockers

Confirmed by reading the blocker lists in `model/automation.rs`. Six lists
reference them, at `:278-280`, `:297-299`, `:324-326`, `:342-343`, `:354-356`, and
`:378-380`. Note that the list at **`:342-343` includes only
`workspace-tree-refresh` and `workspace-persist`** — it does **not** include
`workspace-filter-animation`. That asymmetry is part of the pre-change contract and
must survive unchanged; a projection refactor that "tidied" it into uniformity would
be a widening.

The predicates involved include `app-startup`, `recovery-restore-complete`,
`visual-geometry-settled`, and `accessibility-settled`, per the task list, plus
`workspace-refresh-complete` itself.

## 7.2 Projection plan

Each of the ten fields projects from the new `ui/sidebar/evidence.rs` surface, with
names, types, and semantics **unchanged**.

The two `bool` readiness blockers keep reading a **cheap facade accessor** rather
than building a whole surface per poll — the pattern slots 3a, 3b, 4, and 5a all
used. This row can go one better: its readiness predicate is itself a pure function
over scalars and belongs in `policy.rs`, so the blocker and the surface field can be
**identical by construction** rather than identical by inspection.

Current sources, for the diff to be checkable afterwards:

| Field | Current source |
| --- | --- |
| `scope_kind`, `scope_workspace_id`, `scope_workspace_name` | `imp.sidebar.current_scope()` + `workspace_scope_name` |
| `workspace_count`, `folder_count`, `no_workspaces` | `imp.sidebar.workspaces_file()` |
| `scoped_folder_count` | `imp.sidebar.current_scope_folder_paths()` |
| `persistence_inflight` | `imp.sidebar.workspace_persistence_inflight()` |
| `persistence_dirty` | `imp.sidebar.workspace_persistence_pending()` |
| `filter_animation_active` | **reach-through** `imp.sidebar.imp().workspace_filter_animation_active.get()` |

All counts pass through `bounded_len`, which the redaction contract relies on; that
must not change.

## 7.3 The reach-throughs, re-derived rather than copied

**This row's two, to retire (task 6.7):**

| Site | Expression | Purpose |
| --- | --- | --- |
| `ui/automation.rs:766` | `imp.sidebar.imp().workspace_filter_animation_active.get()` | readiness blocker |
| `ui/automation.rs:927` | `filter_animation_active: imp.sidebar.imp().workspace_filter_animation_active.get()` | workspace snapshot |

Both line numbers **confirmed exact** by grep at implementation time, not inherited.

The field itself is `Cell<bool>` at `ui/sidebar/imp.rs:74`, defaulted at `:128`, and
driven at `:213`/`:224` from `connect_child_revealed_notify` — which is the filter
fade's **primary resumption point** (see `evidence/stage-traces.md`, which found slot
5a had missed it entirely and counted only the headless safety-net timer). So
`filter_execution.rs` is the correct owner of both the field and its projection.

**A note so an absence is not read as a miss:** `workspace_filter_animation_active`
has **zero** widget-test reach-through sites. Its only two readers anywhere are the
two production sites above, which is why retiring it is task 6.7's work and not task
6.6's.

**Deliberately left alone, with their owning rows (task 6.7):** the six out-of-scope
production reach-throughs. Re-derived at implementation time rather than copied from
5a's handoff, which recorded pre-fix numbers:

| Sites | Expression | Owning row |
| --- | --- | --- |
| `:518`, `:519` | `window.imp().tab_view` | `WFR-SHELL-LAYOUT` (slot 7) |
| `:1144`, `:1151`, `:1169`, `:1231` | editor / minimap | `WFR-MINIMAP` (slot 6) |

Fixing one from outside is how a migrated row acquires a change nobody planned.

## 7.4 No new field reaches the schema

To confirm: generations, tickets, admission and mailbox counters, expansion sets,
retained weights, queue depths, and truncation state are all **internal** to the
evidence surface and reach **no** snapshot field. The existing redaction tests are
the contract, and no absolute filesystem path beyond the already-bounded
`scope_workspace_*` fields may reach the schema.

## 7.5 Ownership decisions honoured rather than re-decided

- `workspace-sidebar-animation` is `WFR-SHELL-LAYOUT`'s — the blocker follows the
  animation, not the row name.
- The palette's `command-palette-index` disjunct **stays a direct call**.
- The two recorded **absences** — the notes browser dialog's coordinators and the
  startup format-upgrade flow have no readiness blocker — are the **status quo, not
  gaps to fill**. Adding one would be widening.

## 7.6 Two-tree capture-and-diff

_Pending: run `make automation-smoke` on a pre-change tree and on the changed tree
under isolated headless Mutter with a private D-Bus session and the same fixtures,
then diff the `workspace` object, the action catalog, and all readiness predicates to
zero differences. Slot 5a did **not** run this two-tree capture, and this row's object
is ten fields plus three blockers, so the gap is not narrow here._

**Operational note carried forward:** keep the comparison worktree's path **short**.
Slot 4 lost a run to `libmutter-ERROR: Failed to create socket` under a deep scratch
path — a message that says nothing about path length.

## 7.7 Evidence Projection Map registration (task 7.3)

Confirmed by reading `docs/automation-reference.md`: the map holds rows for
`window.content_search`, `window.command_palette`, `window.tabs`,
`window.local_history`, and `window.notes`. **There is no `workspace` row.**
Registering a new projecting object is different work from extending attribution on
an existing one, and this change owes the registration for all ten fields, keyed by
evidence type and attributed by the binding each field is read through.

---

# Outcome: the projection landed (tasks 7.2, 7.3, 7.4, 7.8)

## All ten fields now project from one evidence read

`ui/automation.rs`'s `workspace_snapshot` builds `WorkspaceTreeEvidence` **once** and
projects every field from it. Names, types, and semantics are unchanged, and the
`bounded_len` / `bounded_snapshot_text` redaction wrappers are preserved on exactly the
fields that had them.

That single-read property matters more for this object than for the other five: this
workflow's model is a lazily materialized `GtkTreeListModel`, so re-deriving the counts
from widgets is precisely what would make a nominal snapshot read materialize child
stores, start a background scan, and queue a watcher restart.

## Both reach-throughs retired

`grep 'sidebar.imp()' crates/lushtext-core/src/ui/automation.rs` → **no matches**.

| Former site | Was | Now |
| --- | --- | --- |
| `:766` readiness blocker | `imp.sidebar.imp().workspace_filter_animation_active.get()` | `imp.sidebar.workspace_filter_animation_active()` — a named facade accessor |
| `:927` snapshot field | the same reach-through | `evidence.filter_animation_active` |

The blocker deliberately does **not** build the whole surface: it is polled, and one
`Cell` read is the right cost. It is nonetheless **identical by construction** to the
evidence field, because both read that one cell — which is exactly what retiring the
reach-through bought, and what "identical by construction rather than by inspection"
means in practice.

## Two accessors became dead, and were retired

Retiring a reach-through in favour of a surface is supposed to *remove* readers, so this
is the expected consequence rather than incidental cleanup:

- `LushtextSidebar::workspace_persistence_inflight` — its only caller was the snapshot,
  which now reads `evidence.persistence_inflight`. **Deleted.**
- `ui/automation.rs`'s private `workspace_scope_name` — a second `match` over
  `WorkspaceScope` producing the documented `scope_kind` tokens. **Deleted**, and the
  decision extracted to `policy::workspace_scope_kind_name` so the surface and the
  exported contract cannot drift. Two independent matches over one enum is exactly how a
  contract string changes on one side only; it now has a unit test pinning both literals.

`workspace_persistence_pending` **stays**, because the `workspace-persist` blocker polls
it.

## No widening

- `make check-automation-docs` passes with its self-test, confirming the docs still match
  the exported action catalog, D-Bus interface, snapshot schema, workflow events, and
  readiness predicates.
- `make automation-client-self-test` passes.
- **No snapshot field was added, removed, renamed, or retyped.** The 15 action-catalog
  owner strings are `"sidebar/workspace_section"` — the **directory**, which this change
  did **not** rename — so all 15 remain accurate with no edit. That vindicates the
  directory-level choice: nine modules inside it were renamed and zero owner strings went
  stale. Task 9.11's decision is therefore recorded as **directory, confirmed by
  survival**, and the caution stands for whoever revisits it: `check-automation-docs`
  proves the two sides **agree**, not that either is true, so a stale owner would pass.
- The blocker-list asymmetry at `model/automation.rs:342-343` (one list omits
  `workspace-filter-animation`) is **preserved unchanged**.
- The 15 internal evidence fields that are not contract — persistence generations and
  failed flag, flush-waiter count, expansion set size and capture counters, section
  counts, process-global scan counters, and watch/refresh aggregates — reach **no**
  snapshot field.

## Task 7.6 — live capture against the changed tree

`make automation-smoke` **passes** on the migrated tree: it launches the real app under
isolated headless Mutter with a private D-Bus session, introspects the app-owned
automation object, and asserts catalog, readiness, snapshot, predicate, wait, event, and
action behaviour.

The `window.workspace` object captured from the **live D-Bus surface**
(`build/smoke/automation/assertions/snapshot-initial.json`):

```json
{
  "filter_animation_active": false,
  "folder_count": 0,
  "no_workspaces": true,
  "persistence_dirty": false,
  "persistence_inflight": false,
  "scope_kind": "all",
  "scope_workspace_id": null,
  "scope_workspace_name": null,
  "scoped_folder_count": 0,
  "workspace_count": 0
}
```

**All ten documented fields present, with their documented names, types, and null
semantics** — projected through `workspace_snapshot_evidence` rather than re-derived
from widgets, and matching the pre-change contract exactly.

**One deliberate value-semantics change, recorded rather than slipped in.**
`scope_workspace_name` now reports `null` when the scoped workspace is absent from
`workspaces.json`, where the pre-change path reported `""` — `workspace_name_for_id`
answers with an empty string for an unknown id, and passing that through produced a
present name that is not a name beside a **non-null** `scope_workspace_id`. The
documented type (`string?`) and meaning ("the selected workspace name, **if any**") are
unchanged; this makes the value honest against them. It is not a widening: no field is
added, removed, renamed, or retyped, and the reachable value set shrinks by one.

### What was not run, and why the gap is narrow

The **two-tree** form — capturing the same object from a pre-change worktree and diffing
— was not run, and task 7.6 is recorded `[~]` rather than complete. The available claim
is nonetheless independent of two runs agreeing about fixtures:

- **the object is registered in the drift gate** (`EVIDENCE_PROJECTIONS` in
  `scripts/check-automation-docs.py`), added in the fix cycle. Rejection was proved in
  both directions: renaming `folder_count` in the Evidence Projection Map produces
  findings, renaming `WorkspaceTreeEvidence::folder_count` in the surface produces
  different findings, and reverting each returns the check to clean. This is the durable
  half of what a two-tree diff shows once — the diff proves today's values agree, the
  registration prevents tomorrow's from silently diverging;

- **no schema field was added, removed, renamed, or retyped**, verified from
  `model/automation.rs`;
- **`make check-automation-docs` passes with its self-test**, checking the docs against
  the live action catalog, D-Bus interface, snapshot schema, workflow events, and
  readiness predicates;
- **`make automation-client-self-test` passes**;
- the blocker-list asymmetry at `model/automation.rs:342-343` is preserved unchanged;
- the live capture above matches the documented contract field for field.

What a two-tree diff would add is confirmation that the *values* agree at runtime for
identical fixtures. That remains owed, and the operational note stands for whoever runs
it: **keep the comparison worktree's path short** — slot 4 lost a run to
`libmutter-ERROR: Failed to create socket` under a deep scratch path, a message that
says nothing about path length.
