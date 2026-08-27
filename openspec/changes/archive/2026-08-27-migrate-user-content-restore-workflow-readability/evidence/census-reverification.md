# Premise re-verification, four rows (task 0.3)

Method, per the amendment this change applies: **row-scoped**, production lines
only (`#[cfg(test)] mod tests` excluded), counting only files the workflow owns.
Shared services, cross-cutting modules, and neighbour files the workflow merely
calls are named as *pooled populations* rather than counted.

Measurement command (production line count = index of the file's `#[cfg(test)]
mod tests` marker at column 0):

```
python3 scratchpad/measure.py <files...>   # printed prod=/total= per file
```

## Measured production sizes

| File | Owning row | prod | total | co-located tests |
| --- | --- | --- | --- | --- |
| `ui/editor_page/buffer_replacement.rs` | WFR-BUFFER-REPLACEMENT | **976** | 1,029 | 53 |
| `ui/window/session_persistence.rs` | WFR-SESSION-RESTORE | **973** | 1,110 | 137 |
| `ui/window/session_restore.rs` | WFR-SESSION-RESTORE | **324** | 417 | 93 |
| `ui/window/local_history.rs` | WFR-LOCAL-HISTORY | **1,633** | 1,633 | 0 |
| `ui/editor_page/local_history.rs` | WFR-LOCAL-HISTORY | **828** | 953 | 125 |
| `ui/window/drafts.rs` | WFR-DRAFT-RECOVERY | **2,247** | 2,460 | 213 |
| `ui/window/draft_ordering.rs` | WFR-DRAFT-RECOVERY (see 6.3) | **50** | 119 | 69 |

Row-scoped totals:

| Row | Census `Current size` | Re-derived row-scoped production size | Direction |
| --- | --- | --- | --- |
| `WFR-BUFFER-REPLACEMENT` | (see matrix) | **976** in 1 file | — |
| `WFR-SESSION-RESTORE` | (see matrix) | **1,297** in 2 files | down |
| `WFR-LOCAL-HISTORY` | 6 files, 5,363 lines (services 2,777) | **2,461** in 2 files | **down**, by the whole of `services/local_history_service.rs` |
| `WFR-DRAFT-RECOVERY` | 6 files, 8,930 lines (ui 2,578 / model 442 / services 5,910) | **2,297** in 2 files | **down**, by the whole of `services/draft_service*` |

## Pooled populations the old cells had shared

Named here so a later slot reading from the other side does not re-derive them
as its own share.

| Pooled population | Rows that share it | Disposition |
| --- | --- | --- |
| `services/draft_service.rs` + `services/draft_service/` | `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE` (startup restore state) | stays in services; behavior unchanged |
| `services/session_service.rs` | `WFR-SESSION-RESTORE` only, but it is a service | stays in services |
| `services/local_history_service.rs` | `WFR-LOCAL-HISTORY`, `WFR-DOCUMENT-SAVE` (save-origin capture) | stays in services |
| `services/recovery_metadata.rs` (1,162 prod / 1,636 total) | `WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`, `WFR-LOCAL-HISTORY` (all three publish `RecoveryDiagnostic`) | stays in services; see task 2.3 |
| the six load-side `test-utils` overrides in `services/editor_io.rs` | `WFR-DOCUMENT-SAVE` (3a), `WFR-DOCUMENT-LOAD` (3b), `WFR-DRAFT-RECOVERY` | **stay in the service** — already recorded by 3b, confirmed unchanged here |
| `ui/window/startup_data.rs` (435) | neither row owns it — see task 2.2 | stays; it is the startup format-upgrade gate that *calls* both rows |
| `model/buffer_replacement.rs`, `ui/plain_disposal.rs`, `ui/buffer_snapshot.rs`, `ui/editor_page/restore_position.rs` | cross-cutting, many rows | stay |

## Seam populations, per kind

Counted by `grep -c 'fn [a-z_]*_for_test'` and
`grep -c 'cfg(feature = "test-utils")'` per file, then each function classified.

| File | fns | gate sites | inspection | configuration | actuation | probe/reset |
| --- | --- | --- | --- | --- | --- | --- |
| `ui/editor_page/buffer_replacement.rs` | 8 | 26 | 4 | 0 | 4 | 0 |
| `ui/window/session_persistence.rs` | 4 | 5 | 2 | 0 | 2 | 0 |
| `ui/window/session_restore.rs` | 0 | 0 | 0 | 0 | 0 | 0 |
| `ui/window/local_history.rs` | 4 (3 distinct; `local_history_preview_install_delay_for_test` is defined twice under opposite `cfg`) | 12 | 1 | 1 | 0 | 0 |
| `ui/editor_page/local_history.rs` | 13 | 17 | 5 | 3 | 2 | 0 (+3 private worker delay/fail helpers) |
| `ui/window/drafts.rs` | 28 | 55 | 6 | 8 | 3 | 1 (`set_next_draft_body_disposal_probe_for_test`) + 10 private worker delay/fail helpers |
| `ui/window/dialogs.rs` | 8 | 11 | 0 | 1 | 7 | 0 |
| `ui/window/startup_data.rs`, `ui/window/draft_ordering.rs` | 0 | 0 | — | — | — | — |

Notes on classification:

- `drafts.rs`'s 28 includes **10 private, always-compiled worker helpers**
  (`delay_draft_*_for_test`, `fail_next_draft_*_for_test`,
  `delay_orphan_cleanup_worker_for_test`) whose bodies are `#[cfg]`-gated
  no-ops in a default build. They are not caller-visible seams; the
  caller-visible population is **18**.
- `ui/editor_page/local_history.rs`'s 13 likewise includes 2 private worker
  helpers (`delay_baseline_capture_for_test`, `fail_baseline_capture_for_test`),
  so its caller-visible population is **11**.
- `ui/window/local_history.rs`'s 4 grep hits are 3 distinct functions.
- `ui/window/dialogs.rs` is a slot-4 *consumer*, not a slot-4 row: 6 of its 7
  actuation seams belong to migrated save/load (`select_*`/`cancel_*` chooser
  seams and `expire_close_save_session_for_test`), and only
  `set_close_safety_completion_delay_for_test` touches this slot's close-safety
  state.

## Pure policy consumer counts, as owning workflows

Substring false positives were excluded by requiring `model::<name>` /
`model::<name>::` rather than bare `draft` / `session` / `local_history`, which
appear in callback names, field names, and test function names throughout `ui/`.

| Module | prod / total | Production consuming files | Owning workflows | Decision |
| --- | --- | --- | --- | --- |
| `model/draft.rs` | 235 / 442 | `services/draft_service.rs`, `services/draft_service/cleanup_types.rs`, `ui/window/drafts.rs`, `ui/window/imp.rs`, `ui/window/session_persistence.rs`, `fuzzing.rs` | **2** (`WFR-DRAFT-RECOVERY`, `WFR-SESSION-RESTORE`) | **stays in `model/`** as domain. `services/draft_service.rs` depends on it, so relocating under `ui/` would invert dependency direction — the 3b `model/file_load.rs` precedent |
| `model/session.rs` | 85 / 300 | `services/session_service.rs`, `services/draft_service.rs`, `ui/window/session_restore.rs`, `ui/window/session_persistence.rs`, `fuzzing.rs` | **1** (`WFR-SESSION-RESTORE`) | **stays in `model/`**: `services/session_service.rs` depends on it. Single owning workflow is *not* sufficient when a service consumes it |
| `model/local_history.rs` | 110 / 173 | `services/local_history_service.rs`, `ui/editor_page/local_history.rs`, `ui/editor_page/save/execution.rs`, `ui/window/local_history.rs` | **2** (`WFR-LOCAL-HISTORY`, `WFR-DOCUMENT-SAVE`) | **stays in `model/`** as domain, and cross-cutting by owning-workflow count |
| `model/buffer_replacement.rs` | 93 / 186 | `ui/editor_page/buffer_replacement.rs`, `ui/editor_page/load/policy.rs`, `ui/window/local_history.rs`, `model/file_load.rs` (1-line alias) | **3** direct pure-module consumers (`WFR-BUFFER-REPLACEMENT`, `WFR-DOCUMENT-LOAD`, `WFR-LOCAL-HISTORY`); **4** owning workflows call `replace_buffer_bounded` | **cross-cutting, stays** — see task 3.3a |

Census cell corrections owed to the matrix (task 9.8): the census figures for all
four are **consuming-file counts, not owning-workflow counts**. The
`buffer_replacement.rs` row's `2 (WFR-LOCAL-HISTORY, Replace All undo)` is wrong
in both halves; see `evidence/mutation-buffer-replacement.md` and task 3.2a.
