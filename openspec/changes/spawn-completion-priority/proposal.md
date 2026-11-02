## Why

`gtk_lush_tasks::spawn_blocking_then` delivers each worker result from a
`glib::idle_add_once` idle, at `G_PRIORITY_DEFAULT_IDLE` (200). GtkTextView's
incremental validation idle runs at `GTK_TEXT_VIEW_PRIORITY_VALIDATE`
(`GDK_PRIORITY_REDRAW + 5`, 125) and is ready again after every 2000 px batch
(A31) until the whole buffer is laid out. GLib dispatches only the top ready
priority, so while any text view has lines to validate, a completed result
cannot reach GTK. That includes a view on a background tab, which validates its
whole buffer just as an on-screen one does (new axiom A80). A large document
therefore holds back every completion that arrives while it validates: a save's
write, a draft autosave, a small file opened meanwhile. Each held completion
keeps one of the eight worker slots, so later spawns queue behind it. LushText's
load and save admission drains and their permit releases are
`G_PRIORITY_DEFAULT_IDLE` idles too, so a Ctrl+S made while large documents
validate waits for the whole backlog before it is even admitted.

`docs/next/long-line-documents.md` queued this change from C2's
`eviction/reselect` finding (ten 8 MiB documents loading in batches about 11 s
apart). This change's attribution on `main` cdfff201 (design, "Attribution")
found that, since C6 merged, completions are **not** what holds
`eviction/reselect` back. 69.5 % of the application's CPU in that case is
`incremental_validate_callback`. Loads run three at a time, because each charges
65 MiB of the 256 MiB transient budget. Each load's permit is held through an
installation that C6 paces below the validation idle, so an installation ends
only when every validating view, the hidden tabs included, has laid out its
whole buffer. That cause is queued as a separate change after C3, because it
touches C3's files. This change fixes the starvation class itself.

## What Changes

- `gtk-lush-tasks` delivers completions from **one completion source at
  `GDK_PRIORITY_REDRAW + 4` (124)**, one step above the validation idle and
  below frames, resizes and input. The source delivers **one completion per
  dispatch**, in the order the workers finished. A new public
  `completion_priority()` names that priority. The worker cap is unchanged, so
  at most eight completed results exist at once.
- LushText schedules the load and save admission drains and their permit
  releases at `completion_priority()`, because they are hand-offs between
  bounded stages. C6's install pacing stays below the validation idle
  (`PACED_SLICE_PRIORITY`), as A70 requires.
- New GTK axiom **A80** (probe and sample in `gtk-lush-axioms`): a
  `G_PRIORITY_DEFAULT_IDLE` source does not dispatch until a validating view,
  on screen or on a hidden stack page, has laid out its whole buffer, while a
  source at 124 dispatches before the first incremental batch.
- A new lane case, `save/during-validation`, saves a scratch document while two
  8 MiB documents validate. It reports the time from Ctrl+S until the saved
  bytes are on disk, the phase's dispatch stalls and the typed key's
  input-to-frame. The lane still never writes a fixture.
- `eviction/reselect`'s real cause, the permit held through a validation-paced
  install plus hidden tabs that validate fully, is recorded in
  `docs/next/long-line-documents.md` as the next change after C3.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-lush-tasks`: completion delivery priority and one completion per dispatch.
- `gtk-axiom-ledger`: A80.
- `main-thread-responsiveness`: bounded-stage hand-offs are not held behind
  text validation.
- `responsiveness-measurement`: the save-during-validation case.

## Impact

- `crates/gtk-lush/tasks` gets its first behaviour change since extraction, plus
  one public function. That means a README, CHANGELOG and API snapshot update,
  the adoption matrix row, and an adoption widget test.
- 170 `spawn_blocking_then` call sites now complete ahead of
  `G_PRIORITY_DEFAULT_IDLE` work and ahead of text validation. Each completion is
  application code that already ran in one dispatch. The lane checks that no
  completion dispatch exceeds `B_settle` and that no input or frame is delayed.
- `ui/editor_page/load/admission.rs` and `ui/editor_page/save/admission.rs`
  change priority only. Their facades' resumption-point narration is updated.
- `scripts/responsiveness-smoke-driver.py` gains one case. C3
  (`sliced-protected-install`) also edits the driver, so the merge is
  coordinated.
