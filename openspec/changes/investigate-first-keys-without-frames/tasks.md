## 1. Characterise

- [x] 1.1 Characterise the frameless stretch from the C6 runs (`vverdict`, `rverdict`): cases, start, end and anchors, frame records, and dispatches inside it.
- [x] 1.2 Trace one case with `WAYLAND_DEBUG=client`: commits, frame callbacks, damage, and key events across the stretch.
- [x] 1.3 Name the frames that bracket the stretch: GtkScrolledWindow's overlay-indicator fade, from the 4.22.5 source.

## 2. Attribute

- [x] 2.1 Add input targets (`target`, `x`, `y`) and the `editor_user_insert` mark to the measurement build's probe, with an encoding test.
- [x] 2.2 Show where the focus click and the keys arrive with the old click point, a click inside the text, and no click.
- [x] 2.3 Find why the sidebar is shown: the lane's keyfile lacks `split-view-layout-migrated`.

## 3. Fix the app (failing first)

- [x] 3.1 Widget tests: an activation open and a startup session restore give the opened editor keyboard focus. Both fail on `main` with focus `status:GtkToggleButton`.
- [x] 3.2 `opened_document_takes_focus` and `hand_focus_to_opened_document` in `ui/window/editor_focus.rs`, called after an activation's last open turn and at the restore terminal, with a unit test of the decision.

## 4. Fix the lane and gate it

- [x] 4.1 `editor_click`: aim at an uncovered point of the snapshot's `visual_geometry` `source-view` rectangle, correct the pointer by the probe's `motion` records, and fail the phase unless the press arrived on an editor. Use it for every driver click.
- [x] 4.2 Flag text keys, carry targets through `input_to_frame`, report `delivery` per phase, and fail the run on `misdirected_typing`. Add self-tests.
- [x] 4.3 Seed `split-view-layout-migrated` in `fresh_state`.

## 5. Verify and record

- [x] 5.1 Interleaved lane run, main/branch/main/branch, `baseline/small` and `dense/{120,500,710}`, plain, with load recorded. Report input-to-frame of `type-during-settle` and the delivery counts.
- [x] 5.2 Record the finding and the decision in `docs/next/long-line-documents.md`. Update `.agents/rules/build.md`, `docs/accessibility.md` and `docs/accessibility-matrix.md`.
- [x] 5.3 Gates: `make fmt`, `make check-fmt`, `make check-clippy`, `make check-workflow-boundaries`, `make check-automation-docs`, `cargo check -p lushtext --bins --locked`, the rustdoc gate, `make test` (0 FLAKY), `make gtk-axioms`, then the smokes.
