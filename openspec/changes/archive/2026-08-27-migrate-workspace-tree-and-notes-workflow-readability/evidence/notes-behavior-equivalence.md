# Behavior equivalence, `WFR-NOTES-BOOKMARKS` (task 3.9)

Acceptance is that note and bookmark records, migration reconcile, browser
contents, error surfaces, timing characteristics, and the exported D-Bus contract
behave identically to the pre-migration workflow — **except** where a confirmed
data-safety defect was fixed, each of which is listed separately below with the
behavior that intentionally changed.

## What carries the equivalence

The migration was performed as **moves, not rewrites**. Every stage body was
relocated with its statements in order; the only bodies edited are the ones named
under "Intentional behavior changes". That is why the pre-existing widget and
integration coverage is the primary equivalence evidence rather than a
re-derivation of it:

| Suite | Result |
| --- | --- |
| `cargo nextest run --workspace --all-features -E 'not binary(widget)'` | **1,713 passed, 0 failed, 11 skipped** |
| `crates/lushtext/tests/integration/notes.rs` (sidecar migration, interrupted-retry, attempt-cap) | passing, unmodified |
| Widget notes coverage in `crates/lushtext/tests/widget/window.rs` | passing; its reads of the retired inspection seams now read the evidence surface, with the assertions unchanged |
| `make command-palette-notes-smoke` | see the gate matrix in the change report |

State extremes exercised by the retained suite, mapped to the task's list:

| Case | Covered by |
| --- | --- |
| Browser with no notes / one / many / a query with no matches / a truncated source | existing `window.rs` browser tests, plus `set_notes_browser_source_entry_limit_for_test` (now routed through `notes/test_policy.rs` with the same name and semantics) |
| **Rapid repeated mode switching that must not let a stale completion publish** | `test_notes_browser_keeps_one_active_one_latest_query_and_disposes_work`, plus the `seams.rs` unit tests that now make a *cross-coordinator* comparison a compile error rather than a runtime risk |
| Bookmark toggle and label edit with no editor / an unsaved editor / one / many | existing bookmark widget tests; `policy.rs` additionally unit-tests the parse and both message families exhaustively |
| A debounced save superseded by a newer one, and a save that fails | existing tests, **plus** the new failure-retry behavior below |
| Document-note and folder-note open / save / discard, and the folder chooser's zero/one/many | existing tests; `policy::folder_note_target_for_workspace` is now unit-tested across all three arities plus the two non-workspace cases, including the regression that `SingleFolder` takes the **first** stored folder |
| First Edit → Render activation for an existing non-empty note and an initially empty one | existing `editor_execution` tests, unmodified; the pre-render path was not touched |
| A rename that migrates sidecars inline, one deferred to the ledger and reconciled next launch, and attempt-cap exhaustion | `tests/integration/notes.rs`, unmodified, **plus** the new serialization proof below |
| The startup format gate for equal / older-upgradeable / newer app data, including a partial apply | unmodified — task 2.2 decided `startup_data.rs` cross-cutting and this change did not touch it |

## Intentional behavior changes, each a confirmed-defect fix

These are the only user-observable differences, and each has a regression test
**proved to fail without its fix** (see `data-safety.md`):

| Change | Before | After |
| --- | --- | --- |
| Bookmark write failure | The dirty flag was cleared before the write and the error arm restored nothing, so the bookmarks stayed in memory until the next toggle | The failure re-arms the dirty flag and reschedules through the debounce, so a transient failure retries instead of being dropped |
| Tab or window close with a pending bookmark write | The debounce holds its target weakly and nothing flushed it, so a bookmark added inside the 200 ms quiet window was silently lost | `flush_bookmarks_for_editor` runs synchronously on tab detach, and `flush_all_pending_bookmarks` runs at the head of the close-safety chain |
| A bookmark write whose sidecar has not been read back | An empty live set overwrote — and, because `save_document` deletes on empty, **deleted** — a sidecar the editor had never loaded | The write is deferred while `sidecar_resolved` is false and the live set is empty; the flag is set by note resolution (either arm) and by any successful write |
| A startup migration completing under restored tabs | Nothing re-resolved, so a tab whose sidecar had just been moved kept showing zero bookmarks | `resolve_notes_for_open_editors` re-reads every open saved editor when the reconcile reports completed kinds |
| One ambiguous or unwritable sidecar during bulk migration | A `?` aborted the whole loop at the first bad item, and filename-sorted scan order meant every retry stopped there, so after three attempts every *later* sidecar was abandoned forever | Per-item isolation through the shared `note_storage::SidecarMigrationTally`: the failure is logged and counted, the loop continues, and the aggregate error still reports the kind incomplete so the ledger retries |
| Two overlapping renames | `operation()` ran outside every lock, so A→B and B→C could interleave and strand a sidecar while **both** ledger entries were retired | A process-wide migration-operation mutex serializes whole tracked operations — serialization, not supersession, because a superseding coordinator would drop the first hop |

**Two error-message texts changed**, as a direct consequence of per-item
isolation: `"ambiguous document note sidecar conflict"` and its folder-note
sibling are now the per-item *cause* in a `tracing::warn!`, and the returned error
is the aggregate `"N document-note sidecar(s) could not be migrated; M
succeeded"`. The two existing tests that asserted the old text were updated to
assert the new aggregate while keeping their real contract — that neither sidecar
is guessed at and the kind reports incomplete. No **user-visible** string changed:
the status message the user sees on a failed rename migration is still
`"Rename succeeded, but note sidecars could not be moved"`.

## Timing characteristics

Unchanged. Every debounce window, delay override, and coordinator generation
policy is the same value at the same point:
`NOTES_SAVE_DEBOUNCE_MS` 200, `NOTE_SAVE_RESPONSE_REFRESH_DEBOUNCE_MS` 80, and
`COMMAND_PALETTE_NOTES_REFRESH_DEBOUNCE_MS` 150 moved from three files into
`policy.rs` with their literals pinned and unit-asserted. The
`NoteSourceRefreshCoordinator` retirement is a **type substitution** onto a
coordinator with identical submit/finish/invalidate semantics; the only
observable difference is that its snapshot gained two high-water fields, which are
internal and reach no exported schema.

## The exported D-Bus contract

Unchanged; see `automation-no-widening.md`.
