## 1. Specify

- [x] 1.1 Proposal, design (option (a) chosen over (b), D1), specs and tasks; adversarial self-review; `openspec validate --strict`.

## 2. Failing-first widget tests

- [x] 2.1 Typing during a dense install, and during a large (total-size) install, keeps every character in order at the caret the install left.
- [x] 2.2 Typing while the load is still planning is not deleted by the install.
- [x] 2.3 The replay is one undo step; the document is modified, draft-dirty, and the autosaved draft holds the text.
- [x] 2.4 Backspace edits the held text; Delete and undo while held are refused with a warning.
- [x] 2.5 Typing into a loading document that is shown in Protected View is not inserted and publishes the notice.
- [x] 2.6 A cancelled install does not receive the text (warning); a superseding load of the same tab receives it once.
- [x] 2.7 An insertion larger than the cap is refused whole; text held before it is kept.
- [x] 2.8 Update the C6 refusal tests (`attempt_every_edit_route`) to the new contract: typing routes are held and inserted, everything else is still refused.

## 3. Implement

- [x] 3.1 `model/held_input.rs`: `HeldText` (cap, whole-or-nothing hold, backspace), `ContentDisposition`, the episode state, `insert_disposition`, `settle_verdict`; unit tests over every combination.
- [x] 3.2 `ui/editor_page/held_input.rs`: user-action depth and taint, the capture in the lock's `insert-text`/`delete-range`/`undo`/`redo` handlers, the view's `backspace` handler, the settle dispatch and its re-arm, preedit tracking, the replay, notices, accessible description, evidence.
- [x] 3.3 `unlock_edits(lock, ContentDisposition)` at every release (load complete/superseded/payload-less/cancelled clear, replacement terminal, save).
- [x] 3.4 `ProtectedNotice::HeldTextNotInserted` (retired by 6.2: a view-only result rescues the text into a new tab).

## 4. Lane

- [x] 4.1 Probe: `held-editor` target, per-character insert marks. Driver: `to_held_editor`, the delivery gate, `held_input_to_frame` reported apart; self-tests.
- [x] 4.2 Interleaved verdict main/branch on `type-during-settle` for total-size, dense and baseline under load, load recorded; the gate passes on the branch; the replay dispatch within `B_SETTLE`.

## 5. Record and gate

- [x] 5.1 Docs: `README.md`, `AGENTS.md`, `ui/editor_page/AGENTS.md`, `docs/accessibility.md`, `docs/accessibility-matrix.md`, `docs/next/long-line-documents.md`, `.agents/rules/build.md`.
- [x] 5.2 `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, rustdoc, `make test` (0 FLAKY), `make gtk-axioms`, the smokes; commit, push, PR.

## 6. Independent review fixes (PR #78)

- [x] 6.1 Held text counts as unsaved in every close collector (`has_unsaved_changes`, close-safety fingerprint); `flush_held_text_for_close` settles it first, a close is refused while a workflow owns the buffer; `app.quit` closes every window through its confirmation (pre-existing bypass). Failing-first widget tests: tab close and window close in the release→settle gap, a loading tab, quit.
- [x] 6.2 No verdict discards held text: view-only, evicted, not loaded, replaced and a refused insert open it in a new Untitled tab (the Preserved Drafts "Open" path) with a notice naming the cause and the tab. Failing-first tests (Protected View, cancelled install).
- [x] 6.3 Drop targets held for the whole held window (`DropTargetHold::HeldInput`). Failing-first test.
- [x] 6.4 Eviction skips editors holding typed input; the settle waits for a pending draft restore; Reload, Replace All's reload and Discard Changes keep the caret; a save-only episode replays at the first-hold mark; Replace All's reload skips editors with held text.
- [x] 6.5 IME: the preedit is read at the settle and reset after 1,024 ms; held text is in `retrieve-surrounding`, and a `delete-surrounding` inside it edits it. Tests.
- [x] 6.6 A tab transfer is not a teardown (`closing_pages`), with `page-attached` adoption (pre-existing gap). Failing-first test.
- [x] 6.7 Replay result checked (a refused insert is rescued); paste atomicity per user action; notice wording; settle at priority 124 with the load terminals' hook; D4 undo wording and a typing-after-replay test.
- [x] 6.8 Budget evidence: a 4 KiB paste control on the same fixtures, and replays that enter Protected View or downgrade the document scale.
- [x] 6.9 Gates, `make test`, `make gtk-axioms`, smokes, the affected lane rows interleaved, docs, push.
