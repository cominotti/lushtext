## Why

The maintainer's standard (2026-11-05) is that LushText be extremely
performant and show no user-visible loss. `bound-notes-browser-by-budget` (BN)
made the notes browser flat in rows. It then recorded three fixed per-dialog
costs as toolkit-floor exceptions, attributed to "the about 205 rows GTK
realizes for any list (A1, A90)":

- **E1**, the dialog's first frames: 47–76 ms;
- **E2**, the frame after a query that changes the rows on screen: 22–23 ms;
- **E3**, closing the dialog: 26–34 ms.

`drill-down-palette-categories` (DP) recorded the same class on the command
palette:

- **X1**, the frame after the first category publish: 13.2–16.6 ms;
- the **deferred clear** after leaving a category view: 9.5–11.5 ms;
- the **grouped publish**, measured with its frames: 18.3 ms worst (21.6 ms on
  `main`), which is over `B_SETTLE_US`.

The queue entry asked this change to measure first whether the realized-row
count or each row's widget tree causes these costs. It was measured on `main`
(`23518e53`, GNOME 51: GTK 4.24.1 / libadwaita 1.10.0, toolbox `hypatia-f45`)
with a `perf` profile, frame-phase timing inside the real dialog, and an
isolated probe that varies only the row tree and the row count. Design
"Measured baseline" holds every figure. The findings:

1. **The realized rows cause the row-dependent part, and text measurement is
   the largest share of it.**
   - With 8 rows, the first frame's layout takes 3–7 ms and the close input
     dispatch 0.3–0.6 ms. With 16,384 rows (205 realized) they take 36–38 ms
     and 13–15 ms.
   - The query frame (E2) is 18–19 ms of layout. `perf` puts 82 % of it in
     `gtk_label_measure` and 81 % in Pango line layout and shaping. GTK
     measures every realized row, off-screen rows included. It measures each
     label twice: for the list's natural width (`measure_across`) and for
     height-for-width.
   - The close input dispatch is 86 % row teardown (dispose, finalize,
     accessible objects), and it costs per widget.
2. **The measured worst values of E1 and E3 were mostly the test renderer.**
   The widget harness renders with `GSK_RENDERER=cairo`. Under it, every frame
   of the dialog's open and close animation costs 16–30 ms of paint, 97–99 %
   in pixman, with 8 rows as with 16,384. Under the GL renderer (the renderer
   class users get) the same frames take 3–5 ms. With GL, in steady state:
   - E1 is one 38.6 ms frame, of which 36.4 ms is layout;
   - E2 is 20.8 ms, of which 18.4 ms is layout;
   - E3 is the 14.2 ms close input dispatch, which tears down the rows. It is
     over `B_INTERACT_US`, and BN's "close" figure never measured it.
3. **GTK offers no way to realize fewer rows.** The realized-row count is
   fixed near 205 for any viewport (A1).
4. **Lighter rows remove most of the cost.** The isolated probe, at 205
   realized rows:
   - replacing the two `GtkLabel`s with a text widget whose size does not
     depend on its text takes the first frame from 20.5 to 12.3 ms and the
     query frame from 19.2 to 6.4 ms. That query frame is the same as with
     16 rows.
   - The palette row goes from 17.6 to 9.3 ms and from 18.9 to 6.3 ms.
   - `GtkInscription` has the same measure contract, but builds a Pango
     layout in its constructor. That adds 15 ms of setup per 205 rows, which
     is why it is rejected.

So the change is worth doing, refined from the queued description. The lever
is not the row count, which GTK fixes. It is that each realized row measures
text, and that rows are torn down inside input dispatches. The cairo animation
frames are a property of the test renderer and are recorded as such, not
attributed to rows.

## What Changes

- **Probe first.** New GTK axioms, each with a probe, a sample and a ledger
  row:
  - **A108**: a realized `GtkListView` row whose `GtkLabel` text changes is
    measured again in the next frame, off-screen rows included. A widget whose
    measure does not depend on its text costs that frame nothing.
  - **A109**: `GtkInscription` builds its Pango layout, and so a style for its
    still detached node, in its constructor.
  - **A110**: removing realized rows from the end of a list model tears down
    only the removed rows' widgets, at a cost proportional to them, mapped or
    not.
  - **A111**: under `GSK_RENDERER=cairo`, every frame of an `AdwDialog`'s open
    and close animation costs a fixed paint that does not depend on its
    content. Under GL it does not.
- **A measure-free row text widget.** New `LushtextRowText` widget in
  `ui/row_text/`, a called presentation surface:
  - Its height is the line height of its font, its width the containers'.
    Neither depends on its text, so setting text only redraws the widget.
  - It creates its Pango layout only when it is drawn, so only rows on screen
    lay out text. It ellipsizes like the label it replaces.
  - It keeps the label CSS node, so Adwaita's `.title`, `.subtitle`,
    `.caption`, `.dim-label` and `.heading` styling applies unchanged. It has
    the `label` accessible role, and its accessible label is its text.
  - An opt-in `natural-width-from-text` mode measures its text, for the few
    palette group headers whose separator fills the rest of the row.
- **Lighter rows.** The notes browser rows and section headers and the
  palette's grouped and category rows use `LushtextRowText`. Nothing else
  changes: the row trees, icons, the bookmark row's Open button, the
  accessible names and descriptions, tooltips, both browser modes and the
  palette's grouped and category views.
- **Row teardown leaves input dispatches.** Closing the notes browser and
  leaving a palette category view no longer tear down the realized rows in one
  dispatch. A shared, time-sliced retirement removes rows from the model's end
  in `G_PRIORITY_LOW` turns, each within `LIST_TURN_SLICE_US` (A110). The
  sources are released once the model is empty, as today.
- **A conditional staged first publish.** If, after the lighter rows, the
  frame that first shows a large result list still exceeds `B_SETTLE_US` under
  the GL renderer, the first publish lists the rows one viewport needs and
  grows to the full length one step per frame. Implementation decides this by
  measurement and records the result either way (design D5).
- **Exceptions re-measured and re-attributed.** E1, E2, E3, X1, the deferred
  clear and the grouped publish are re-measured on the GL renderer (steady
  state) and on the harness's cairo renderer, interleaved with `main`. Each
  either fits its budget or stays a measured toolkit-floor exception, citing
  its axiom and bounded by a widget test. The cairo animation paint becomes a
  recorded test-renderer cost (A111). It is not a product exception.
- **Measurement protocol.** Verdicts come from the optimized widget build,
  4 interleaved `main`/branch rounds on the affected cases only, at a 1-minute
  load below 4, with the frame's layout and paint phases recorded separately.
  Long campaigns run detached.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: the notes browser and command palette
  requirements now name the close input dispatch and the category leave, add
  scenarios for the row-changing query frame and the dialog's first frame, and
  state the renderer the verdicts are measured on. New requirement: realized
  list rows do not measure their text, and are retired outside input
  dispatches.
- `gtk-accessibility-spine`: new requirement: rows that use the measure-free
  text widget expose the same accessible text as the labels they replace.
- `gtk-axiom-ledger`: new requirement: the list-row cost behaviours this change
  relies on (A108–A111) are ledger axioms.

## Impact

- **Code**:
  - new `crates/lushtext-core/src/ui/row_text/` (`mod.rs`, `imp.rs`);
  - `ui/window/notes/browser.rs` (row and header factories, the close path);
  - `ui/window/notes/row_model.rs` (sliced retirement);
  - `ui/command_palette/row_factory.rs` and `category_view.rs` (deferred clear
    to sliced retirement);
  - `model/list_budget.rs`, if the retirement unit becomes a named constant
    there;
  - `crates/gtk-lush/axioms` (A108–A111).
- **Tests**:
  - failing-first widget tests for the query frame, the first frame's layout
    phase, the close input dispatch, the category leave and row text parity,
    against twice their measured worst;
  - accessibility smoke for notes and palette rows;
  - visual-geometry proof that row height and text origin are unchanged from
    `main`.
- **Docs**:
  - the GTK axiom ledger;
  - `docs/next/long-line-documents.md` (the E1–E3, X1 and DP records);
  - `docs/next/gtk-lush.md` (`LushtextRowText` as an extraction candidate);
  - `docs/thresholds.md`, `docs/accessibility-matrix.md`,
    `docs/workflow-readability-matrix.md`;
  - `AGENTS.md`/`.claude/CLAUDE.md` module layout;
  - `docs/upstream-reports.md` (the A109 draft).
- **No persisted format, GSettings key, action or automation field changes
  meaning.**
