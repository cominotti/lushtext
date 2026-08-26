# Widget-test save-site migration

Per-site record for the `.imp().` reach-through and retired-seam sites this
change moved. The rule the change worked to: for a **read**, read evidence where
the question is "did the workflow record it" and keep a direct read only where
the question is genuinely "what bytes are on disk"; for an ungated **write**,
"keep it as arranging state" is not an available answer, because a write through
a `pub` field is an actuation reach-through masquerading as setup and it shapes
production field layout from the test side.

## Scope correction, stated first

The change's planning scope named a strict save/load reach-through population of
"13 sites: 9 writes, 3 reads, 1 widget actuation", and named
`window.imp().session.save_failed` as a priority save site. **Both figures were
re-derived against the tree and both were wrong in ways that matter.**

- `window.imp().session.save_failed` is **not** document-save state. It is
  *session-file* save failure, written and cleared only by
  `ui/window/session_persistence.rs`, and it belongs to `WFR-SESSION-RESTORE`
  (slot 4). Its three read sites are handed to slot 4 rather than migrated here.
  A field whose name contains "save" is not thereby save-workflow state.
- The genuinely save-owned ungated population is **5 write sites and 0 read
  sites**. The five are exactly the ones the change's own task list had verified
  independently, so the aggregate figure was the stale one, not the site list.
- A further 40 sites touching `drafts.*` and `session.*` (28 reads, 12 writes)
  are catalogued for slot 4 in the handoff and are untouched here.

## Ungated `imp()` write sites — 5 of 5 migrated

| Site | What it wrote | Outcome | Detail |
| --- | --- | --- | --- |
| `window.rs:6085` | `editor.imp().save.inflight = true`, inside a before-eviction hook | **real drive of the workflow** | The hook's own comment said "the production save path sets this before yielding". It now *is* the production save path: the hook calls `save_file_async`, whose queue stage publishes save ownership synchronously — which is precisely what the eviction pass must revalidate against. An existing configuration seam (`editor_io::set_save_write_delay_for_test`) holds the save in flight across the pass |
| `window.rs:6096` | `editor.imp().save.inflight = false`, teardown | **real drive of the workflow** | Replaced by clearing the write delay and waiting on `save_evidence().inflight` going false, so the test now waits for the real terminal instead of forging it |
| `window.rs:13986` | `editor.imp().save.inflight = true`, arranging "a save is in progress" | **real drive of the workflow** | The test is literally "close paths are blocked while a save is in progress", so a forged flag was testing the guard against a value no workflow produced. Now a real delayed save is started and `save_evidence().inflight` is asserted before the close attempt |
| `window.rs:14005` | `editor.imp().save.inflight = false`, teardown | **real drive of the workflow** | Same terminal wait as `:6096` |
| `window.rs:13830` | `window.imp().session.active_close_save_identity = None`, to make a close request stale mid-flight | **new named actuation seam — counted and justified** | See below |

### The one new actuation seam

`LushtextWindow::expire_close_save_session_for_test`, in
`ui/window/dialogs.rs`, behind `#[cfg(feature = "test-utils")]`. It calls the
production `finish_close_save_session` for whichever session is active.

This is a **counted, recorded exception** to "the actuation seam count should not
grow", not a silent increment. The justification: a close session ends only when
its `AdwAlertDialog`-driven pipeline completes, aborts, or is superseded by
another close request. The test needs it to end at a specific instant — while a
close-gating save is still queued behind an exclusive load — because that is
exactly the race the stale-close-session guard exists to protect against, and no
headless path reaches that instant. This is the shape the programme-level
actuation-seam deferral describes.

It is strictly better than what it replaces: the previous line wrote a private
lifecycle field directly, bypassing the `close_save_session_is_current` guard
that `finish_close_save_session` applies, and it shaped production field layout
from the test side.

**Actuation seam count for this workflow: 6 before, 7 after.**
Editor-side, unchanged at 3: `reset_transient_save_admission_for_test`,
`pause_next_save_snapshot_for_test`, `resume_save_snapshot_for_test`.
Chooser-bound in `ui/window/dialogs.rs`, unchanged at 3:
`select_save_as_destination_for_test`, `select_save_as_uri_for_test`,
`cancel_save_as_destination_for_test`. Plus the one above.

## Retired inspection seams — 4 call surfaces over 3 mechanisms

Every save-side `*_for_test` inspection function was retired into `SaveEvidence`,
and **no per-field `*_for_test` accessor was added to replace any of them**. That
rule is the one the evidence surface exists to enforce: a test needing a fact the
surface lacks extends the surface.

| Retired | Call sites moved | Replaced by |
| --- | --- | --- |
| `save_runtime::snapshot_for_test` | (internal; was the mechanism behind the next row) | `admission::admission_snapshot`, read through the surface |
| `transient_save_admission_snapshot_for_test` | 13 (`window.rs` 11, `editor_page.rs` 5 counting multi-field reads once each) | `save_evidence()`, whose `queued_count`, `queued_close_count`, `active_count`, `active_weight`, `high_water_weight`, and `exclusive_active` fields cover every field any test read |
| `save_uses_chunked_snapshot_for_test` | 4 (`editor_page.rs` 3, `window.rs` 1) | `save_evidence().capture_mode == SaveCaptureMode::Chunked`. The field is a **live** classification of the current buffer, deliberately matching the retired seam's meaning — the assertions all run with no save in flight, so a recorded-mode field would have silently changed what they assert |
| `save_snapshot_inflight_for_test` | 3 (`editor_page.rs`) | `save_evidence().capture_in_flight` |

No remaining `*_for_test` inspection function exists on the save path. Every
retired function was confirmed to have zero remaining callers by grep across
`crates/lushtext/tests/` and `crates/lushtext-core/`.

## Reads kept direct, deliberately

`fs_read::text(&path)` assertions after a save are unchanged. Those ask "what
bytes are on disk", which is the one question evidence must **not** answer: the
whole point of the durable-write contract is that the workflow's belief about the
file and the file itself are separately checkable. Replacing them with an
evidence read would delete the assertion's value.
