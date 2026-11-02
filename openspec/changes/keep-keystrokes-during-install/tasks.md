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
- [x] 3.4 `ProtectedNotice::HeldTextNotInserted`.

## 4. Lane

- [x] 4.1 Probe: `held-editor` target, per-character insert marks. Driver: `to_held_editor`, the delivery gate, `held_input_to_frame` reported apart; self-tests.
- [x] 4.2 Interleaved verdict main/branch on `type-during-settle` for total-size, dense and baseline under load, load recorded; the gate passes on the branch; the replay dispatch within `B_SETTLE`.

## 5. Record and gate

- [x] 5.1 Docs: `README.md`, `AGENTS.md`, `ui/editor_page/AGENTS.md`, `docs/accessibility.md`, `docs/accessibility-matrix.md`, `docs/next/long-line-documents.md`, `.agents/rules/build.md`.
- [x] 5.2 `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, rustdoc, `make test` (0 FLAKY), `make gtk-axioms`, the smokes; commit, push, PR.
