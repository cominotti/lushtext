# Automation: no widening (tasks 8.1–8.8)

## The exported surface this row owns, recorded pre-change

Read from `model/automation.rs` and `docs/automation-reference.md`, not from
memory. `window.notes` has **six** fields:

`notes_menu_open`, `active_document_file_backed`,
`active_document_bookmark_count`, `active_line_has_bookmark`,
`document_note_available`, `folder_note_available`.

The row owns **no** readiness blocker and **no** readiness predicate of its own.
Two absences are recorded so a later slot does not read them as gaps to fill
silently: **the notes browser dialog's three coordinators have no readiness
blocker, and the startup format-upgrade flow has none either.** Adding one would
be widening; leaving them is the status quo this change preserves.

The `window.workspace` object (10 fields), the `workspace-persist`,
`workspace-tree-refresh`, and `workspace-filter-animation` blockers, the
`workspace-refresh-complete` predicate, and the `workspace-refresh` workflow id
all belong to `WFR-WORKSPACE-TREE` and are **untouched**; they move to slot 5b.

## What changed, and what did not

| Field | Before | After |
| --- | --- | --- |
| `notes.notes_menu_open` | `imp.notes_menu_button.is_active()` inline | projected from `NotesEvidence.notes_menu_open`, which reads the same property through `try_get()` |
| `notes.active_document_bookmark_count` | `bounded_len(editor.bookmark_records().len())` inline | projected from `NotesEvidence.active_document_bookmark_count`, still passed through `bounded_len` |
| `notes.active_line_has_bookmark` | `editor.current_bookmark().is_some()` inline | projected from `NotesEvidence.active_line_has_bookmark` |
| `notes.folder_note_available` | `!imp.sidebar.current_scope_folder_paths().is_empty()` inline | projected from `NotesEvidence.folder_note_available`, **byte-identical derivation** |
| `notes.active_document_file_backed` | inline in `notes_snapshot` | **one shared helper** `active_document_is_file_backed` |
| `notes.document_note_available` | `active_document_file_backed` | same, from that helper |
| `local_history.active_document_file_backed` | inline in `local_history_snapshot` | **the same shared helper** |

**No field was added, removed, retyped, or re-semanticized.** `folder_note_available`
deserves a specific note: the notes workflow's *own* availability decision is
`policy::folder_note_action_available` over a concrete workspace scope, which is
**stricter** than the exported field's "any folder in the current shared scope".
The evidence surface deliberately carries the **exported** derivation, with a
comment saying so, because changing it would alter the contract.

## Task 8.4's three pieces of gate work

1. **`window.notes` is a new projecting object** in the Evidence Projection Map,
   which previously held rows only for `window.content_search`,
   `window.command_palette`, `tabs[]`, and `window.local_history`. Four rows added.
   `window.workspace` remains absent because it remains unprojected — slot 5b's.
2. **The dual-binding case is resolved by making it not a binding case.**
   `snapshot-field-active-document-file-backed` is bound to **two** snapshot
   objects, so one documented field id would have mapped to two evidence types.
   Rather than extend the gate's per-binding attribution, the change removed the
   ambiguity at its source: **neither object projects it.** The field is the active
   document's *identity*, not either workflow's state, and both objects now derive
   it from one helper. The gate needed no extension, and `make
   check-automation-docs` passes.
3. **The decision task 8.4(3) demanded, stated as a rule for slots 6 and 7:**
   *a fact about the active document's identity is projected once, by the
   identity's owner, not by every workflow that consumes it.* Recorded in
   `docs/automation-reference.md` immediately above the map, where the next slot
   will hit it. The alternative — re-sourcing `local_history.active_document_file_backed`
   from the migrated local-history surface — was rejected because it touches a
   migrated row to solve a problem that disappears when the fact is derived once.

## Internal fields confirmed to reach no snapshot (task 8.5)

Every other `NotesEvidence` field is internal and appears in no exported schema:
`browser` (and its three nested coordinator snapshots, including the two new
high-water counters the coordinator retirement added), `active_note_save_captures`,
`palette_note_source_busy`, `palette_note_source_awaiting_admission`.
`OpenEditorNoteCaptureEvidence` is `#[cfg(feature = "test-utils")]` and cannot
reach a production snapshot at all.

**No note body, bookmark id, sidecar identity key, note id, or file path** can
reach the schema: the surface carries only booleans and counts, and the one
`String`-ish thing anywhere near it — the note text — never leaves
`policy.rs`/`editor_execution.rs`. `docs/automation-reference.md`'s privacy
boundary already excludes note and bookmark identifiers, and the existing
redaction tests are unmodified and passing.

## How no-widening was proved, and what was **not** run

**Proved by:**

- `make check-automation-docs` passing, which validates the exported action
  catalog, the read-only D-Bus interface, the snapshot schema, the workflow event
  schema, readiness predicates and blockers, and the Evidence Projection Map
  against the Rust source. It is the gate that fails on a renamed or added field.
- `crates/lushtext/tests/widget/window.rs::test_automation_snapshot_reports_bounded_live_window_state`,
  which asserts `notes.active_document_file_backed`,
  `notes.active_document_bookmark_count`, and `notes.active_line_has_bookmark`
  against a live window and is **unmodified** by this change — so it is comparing
  the new projection against expectations written for the old inline derivation.
- The full non-widget suite (1,713 passing), which includes the automation
  schema and redaction tests.

**Not run, and recorded as such rather than implied:** task 8.6's **two-tree
`make automation-smoke` capture-and-diff**. It requires building and launching a
baseline tree under isolated headless Mutter and a private D-Bus session with the
same fixtures, then diffing the `workspace` and `notes` objects, the action
catalog, and all readiness predicates to zero differences. That is the stronger
proof and it was not performed here. The gap is narrower than it would normally be
— `window.workspace` is untouched by this change, so only `notes` could have
moved, and its four projected fields are covered by the unmodified live-window
assertion above — but it is a gap, and it belongs with slot 5b's run when
`window.workspace` actually starts projecting.

If it is run: **keep the comparison worktree's path short.** Slot 4 lost a run to
`libmutter-ERROR: Failed to create socket`, a message that says nothing about path
length.
