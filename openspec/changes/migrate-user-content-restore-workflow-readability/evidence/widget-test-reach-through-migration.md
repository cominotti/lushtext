# Widget-test `imp()` reach-through, per site (tasks 0.7, 6.10, 6.12)

## Corrected pre-change baseline

Slot 3a reported **40** sites. The current tree holds **35**:

| Group | Lines | Field occurrences |
| --- | --- | --- |
| `.imp().drafts.` | 21 | 21 |
| `.imp().session.` | 14 | 15 (line 6676 reads two fields) |
| **total** | **35** | **36** |

All 35 are in `crates/lushtext/tests/widget/window.rs`. 3a's own enumerated list
cannot reach 40 either, so **35 is a correction, not drift**, and task 6.12's
delta is reported against 35.

Sweeps 3a did not catalogue, both confirmed **zero**, recorded rather than
assumed:

- `.imp().local_history.` / `.imp().replacement.` in
  `crates/lushtext/tests/widget/editor_page.rs`: **0**.
- any `.imp().{drafts,session,local_history,replacement}.` outside
  `tests/widget/window.rs`: **0** (`grep -rn` across `crates/lushtext/tests/widget/`
  returns only `window.rs`).

## Per-site categorization

Line numbers are from the pre-change tree and drift; the migration matches by
**field name**, not by line.

### `.imp().drafts.` writes — 11 sites

| Lines | Field | Category |
| --- | --- | --- |
| 927, 5780, 7981, 8029, 8093, 18593 | `manifest` (`upsert` / whole-value replace) | **needs a counted actuation seam** on the draft journal: installing a manifest record is the journal's own operation, and the only production path that installs one is a startup disk read the test is deliberately not running. One seam covers all six sites |
| 5781, 18594 | `preloaded` (`clear` / `insert`) | **needs a counted actuation seam** on the journal, for the same reason: preloaded bodies are produced by the startup worker |
| 7695, 7699 | `autosave_inflight` (`set(true)` / `set(false)`) | **real drive through an existing seam** — slot 3a's finding. Holding a batch in flight is reachable with `set_draft_mutation_delays_for_test` + `autosave_tick_for_test` |
| 7700 | `autosave_pending` (`set(false)`) | **real drive**: falls out of the same drive once the held batch completes |

### `.imp().drafts.` reads — 10 sites → evidence reads

| Lines | Field | Becomes |
| --- | --- | --- |
| 7698 | `autosave_pending` | `DraftEvidence::autosave_pending` |
| 7982, 8030, 8094, 8153, 8170 | `manifest` (borrowed to persist a fixture) | the same journal actuation seam's persist half, not a read |
| 8399, 8416 | `manifest_authority` | `DraftEvidence::manifest_authority_trusted` |
| 10525, 13429 | `close_discard_ids` | `DraftEvidence::close_discard_count` |

### `.imp().session.` reads — 15 occurrences → evidence reads

**Zero session writes.** Every site is a read.

| Lines | Field | Becomes |
| --- | --- | --- |
| 6676, 13211, 13226 | `save_failed` | `SessionRestoreEvidence::save_failed` (task 2.4: this is *session-file* save failure, not document-save state) |
| 6676, 10265, 13139, 13149 | `close_safety_inflight` | `SessionRestoreEvidence::close_safety_inflight` |
| 6681 | `close_safety_bypass` | `SessionRestoreEvidence::close_safety_bypass` |
| 6706, 6795, 6846, 6895, 6939, 6995 | `restore_cancel` | `SessionRestoreEvidence::startup_load_cancellable` |
| 13227 | `failure_detail` | `SessionRestoreEvidence::failure_detail_present` |

## Result

| Quantity | Before | After |
| --- | --- | --- |
| Ungated `imp()` sites | 35 | _filled by task 6.12_ |
| Writes converted to real drives through existing seams | — | _filled by task 6.12_ |
| New counted actuation seams | — | _filled by task 6.12_ |

## `ui/window/dialogs.rs` seam population (task 6.10's sweep)

`dialogs.rs` is a slot-4 **consumer**. Its 8 seams:

| Seam | Owner |
| --- | --- |
| `select_open_file_for_test`, `select_open_file_uri_for_test`, `cancel_open_file_for_test` | migrated `WFR-DOCUMENT-LOAD` (3b) — chooser-bound, deferred at programme level |
| `select_save_as_destination_for_test`, `select_save_as_uri_for_test`, `cancel_save_as_destination_for_test`, `expire_close_save_session_for_test` | migrated `WFR-DOCUMENT-SAVE` (3a) — chooser-bound plus 3a's one added seam |
| `set_close_safety_completion_delay_for_test` | **this slot's**: it configures the close-safety completion delay, whose `close_safety_inflight` / `close_safety_bypass` flags this slot's session row owns (task 2.4a) |

No reach-through into any of those eight exists in any widget module.
