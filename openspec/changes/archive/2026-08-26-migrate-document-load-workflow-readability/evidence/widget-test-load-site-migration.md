# Widget-test load site migration

Two populations, kept separate because they need different answers: the
**feature-gated inspection seams** the evidence surface retires, and the
**ungated `.imp().` reach-through** that appears in no seam census at all and
shapes production field layout.

## 1. Retired inspection seams — 10 surfaces, 51 call sites migrated

Counted from the diff rather than from the current tree, so the figures are the
sites that actually moved and not sites the change's own new tests added:

```
$ for f in load_generation_for_test load_installation_active_for_test \
    load_installation_slice_count_for_test load_installation_weight_for_test \
    load_projection_suspended_for_test transient_load_admission_snapshot_for_test \
    transient_load_disposal_wakeup_armed_for_test load_cancel_token_for_test; do
    printf "%-46s %s\n" "$f" \
      "$(git diff crates/lushtext/tests/widget/ | grep '^-' | grep -c "$f")"
  done
```

Every one now reads a field on `LoadEvidence`. No assertion was dropped and no
test was deleted.

| Retired seam | Replaced by | Sites |
| --- | --- | --- |
| `load_generation_for_test()` | `load_evidence().generation` | 7 |
| `load_installation_active_for_test()` | `load_evidence().installation_active` | 14 |
| `load_installation_slice_count_for_test()` | `load_evidence().installation_slice_count` | 4 |
| `load_installation_weight_for_test()` | `load_evidence().installation_weight` | 2 |
| `load_projection_suspended_for_test()` | `load_evidence().projection_suspended` | 5 |
| `transient_load_admission_snapshot_for_test()` | `load_evidence()` — the admission fields are flattened onto the surface under the same names, so `.active_count`, `.queued_count`, `.active_weight`, `.high_water_weight`, and `.exclusive_active` read unchanged | 15 |
| `transient_load_disposal_wakeup_armed_for_test()` | `load_evidence().disposal_wakeup_armed` | 2 |
| `load_cancel_token_for_test()` | `load_evidence().cancel_token_identity` plus the new `previous_request_cancelled` — see the note below | 2 |
| `load_runtime::snapshot_for_test()` (crate-internal) | `admission::admission_snapshot()`, read only by `evidence.rs` | — |
| `load_runtime::disposal_wakeup_armed_for_test()` (crate-internal) | `admission::disposal_wakeup_armed()`, read only by `evidence.rs` | — |

**One retirement needed the surface extended rather than mapped.**
`load_cancel_token_for_test` returned the live `Arc<AtomicBool>`, and
`test_new_load_cancels_previous_token_without_reusing_identity` held the *first*
token across a second load to assert "starting a newer load must permanently
cancel the previous token". After rotation the old `Arc` is unreachable from the
editor, so no read-only field could express that. Rather than keep an
`Arc`-returning getter — which would have made `LoadEvidence` neither `Clone` nor
`PartialEq` in any useful way — the surface gained
`previous_request_cancelled: Option<bool>`, recorded by `rotate_load_identity`
from the outgoing token.

**What that field does and does not prove**, stated plainly because the first
wording overclaimed it: rotation is its only writer and the entry stage always
cancels before it rotates, so on every reachable path it reads `Some(true)` once
a second request has started. It is therefore not a detector for an uncancelled
token — it **pins the cancel-before-rotate ordering**. Swap those two steps and
it reads `Some(false)`, and the migrated assertion fails. The retired seam could
observe the old token directly; this observes the ordering that retired it, which
is the part a reader of the workflow needs. This is the convention working as
intended — a test needing a fact the surface lacks extends the surface.

## 2. Ungated `.imp().` reach-through — 17 sites catalogued, 7 migrated

Found by grepping `.imp().` against the load-relevant field groups rather than by
grepping `_for_test`, because these sites are invisible to the seam census.

### 2a. Load-workflow state: 7 sites, all writes, all migrated

```
$ git diff crates/lushtext/tests/widget/ | grep '^-' | grep -E 'imp\(\)'
        page.imp()                                        # file_path.replace, wrapped
        page.imp().load_state.set(EditorLoadState::Loading);
        page.imp().load_state.set(EditorLoadState::Loaded);
        page.imp()                                        # file_path.replace, wrapped
        page.imp().load_state.set(EditorLoadState::Failed);
        page.imp().load_state.set(EditorLoadState::Loaded);
        page.imp().load_state.set(EditorLoadState::Loading);
```

Five `load_state` writes and two `file_path` replacements, across three tests.

These set production load state from the test side — actuation reach-through
masquerading as setup. Every one became a **real drive of the workflow**; none
became a new seam.

| Site (pre-change) | Was | Now | Category |
| --- | --- | --- | --- |
| `editor_page.rs` `test_source_view_accessibility_tracks_loading_state` | `file_path.replace(...)` + `load_state.set(Loading)` | `page.load_file_async(&path)` against a real tempdir fixture, then `assert_eq!(load_evidence().load_state, Loading)` | real drive |
| same test, second half | `load_state.set(Loaded)` | `page.set_file_path(&path)`, the named `document_identity` operation that publishes `Loaded` | real drive |
| `editor_page.rs` `test_source_view_accessibility_tracks_failed_load_state` | `file_path.replace(...)` + `load_state.set(Failed)` | `load_file_async` then `apply_load_result_for_test(generation, Err(..))` — the production publish path, through the stale-generation gate | real drive, existing seam |
| same test, second half | `load_state.set(Loaded)` | `page.set_file_path(&path)` | real drive |
| `editor_page.rs` `test_failed_reload_restores_file_monitor_for_preserved_buffer` | `load_state.set(Loading)` | `page.load_file_async(&path)`, so the failure below is accepted against the generation the workflow actually published rather than a forged one | real drive |

The accessibility rewrites are deterministic despite starting a real background
load: `spawn_blocking_then` completes through a low-priority idle source, and the
assertions that follow iterate no main loop, so the completion cannot land
mid-test.

### 2b. Cross-cutting document metadata: 10 sites, recorded and left

`page.imp().file_size.set(...)` (7 sites in `editor_page.rs`, 1 in `window.rs`)
and `editor.imp().size_check.set(...)` (1 in `window.rs`) are **not
load-workflow state**. They are shared editor-page document metadata, now in
`ui/editor_page/document_identity.rs`, written by both document workflows and
read by the minimap, encoding, accessibility, eviction, and local-history paths.
Their tests are arranging *metadata* to reach a size-policy or eviction state,
not arranging a load.

Migrating them would mean either inventing a metadata seam this row does not own,
or forcing a multi-megabyte real load into tests about minimap availability and
memory residency. Both are worse than recording the boundary. **They are handed
to whichever slot migrates the shared identity group**, with the same discipline
slot 3a used when it handed 40 `drafts.*` / `session.*` sites to slot 4.

### 2c. The recent-documents popover: 1 site, reassigned

`open_popover.rs:1107` reads `window.imp().recent_documents.loading`. This is
popover list state, not load state — see the matrix's
[recent-documents surface census gap](../../../docs/workflow-readability-matrix.md#the-recent-documents-surface-census-gap).
Assigned to `WFR-SHELL-LAYOUT` (slot 7) together with `ui/open_popover/**` and
`ui/window/recent_open.rs`, which appear in no matrix row's file set.

## 3. Actuation seams: 8 → 7, and none added

| Seam | Disposition |
| --- | --- |
| `apply_load_result_for_test` | preserved — drives the stale-generation gate, otherwise reachable only by winning a race with a background read |
| `apply_reload_error_for_test` | preserved — same, for the reload-over-loaded-content path |
| `apply_loaded_content_for_test` | preserved — applies post-load size policy without loading tens of megabytes through `GtkTextBuffer` to cross a threshold |
| `reset_transient_load_admission_for_test` | preserved — the coordinator is process-wide, so a case cannot otherwise start from a known lane state |
| `load_runtime::reset_for_test` | **removed as a separate surface.** Its body moved into the editor-page seam above, so one mechanism now has one surface instead of two |
| `select_open_file_for_test`, `select_open_file_uri_for_test`, `cancel_open_file_for_test` (`ui/window/dialogs.rs`) | preserved — chooser-bound; `cancel_open_file_for_test` is the programme record's own named example of the deferred category |

**This is the first time the programme reduced the actuation count.** The
reduction is real consolidation, not reclassification: the process-wide reset has
exactly one mechanism and now exactly one caller-visible name.

## 4. Configuration seams: 2 → 1 value

`NEXT_LOAD_BODY_DISPOSAL_PROBE` and `NEXT_LOAD_DISPOSAL_RESERVATION_WEIGHT` were
two module-level statics in the retired `load_runtime.rs`. They are now one
`LoadTestPolicy` value in `ui/editor_page/load/test_policy.rs`, with the whole
module behind `#[cfg(feature = "test-utils")]` so a production build compiles no
override storage. Both public setter names are unchanged, so no test call site
moved.

The 6 load-side overrides in `services/editor_io.rs`
(`set_load_delay_for_test`, `set_payload_load_delay_for_test`,
`delay_load_for_test`, `delay_payload_load_for_test`,
`take_load_processing_chunks_for_test`,
`cancel_load_after_processing_chunks_for_test`) **stay in the service**, because
the service owns the behavior they change. A second policy value in `ui/` would
shadow them and would have to be kept in sync across two workflows — the same
answer slot 3a recorded for the save side.
