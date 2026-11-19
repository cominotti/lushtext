# Proposal

## Why

LushText builds the rows of its six list factories by hand (15 `SignalListItemFactory` sites in `ui/command_palette/row_factory.rs`, `ui/open_popover/imp.rs`, `ui/search_panel/list_factory.rs`, `ui/search_panel/skipped_files.rs`, `ui/sidebar/workspace_section/row_factory.rs`, `ui/window/notes/browser.rs`).

- **How the rows are reached.** They find their children by sibling order (34 `next_sibling` calls in `ui/`), and two of them hang lifetime state on `unsafe set_data`.
- **What gtk-rs applications do instead.** Loupe and Fractal use a row subclass, usually a `CompositeTemplate`.
- **What is not known.** What a row construction costs here. The ledger shows it matters:
  - A109: 205 `GtkInscription`s cost 7.8 ms to construct, against 0.95 ms for `GtkLabel`s;
  - A108: a realized band is about 205 rows;
  - the first publish creates 32 rows inside one dispatch.
  
  No probe measures GtkBuilder template instantiation per row.

The maintainer decided (2026-11-19) to adopt typed row widgets fully, so the remaining question is which form. This spike measures it before `adopt-typed-row-state-and-dialog-futures` builds on the answer.

## What Changes

- **A new GTK axiom probe and sample** in `gtk-lush-axioms` with three arms building the same notes-shaped row (an expander or box, a `LushtextRowText`-like measure-free text child and a label-sized second line, a badge):
  - (a) today's factory: widgets built in `setup`, children found by sibling order;
  - (b) a row subclass built in Rust, with typed fields;
  - (c) a row subclass from a `CompositeTemplate`.
- **Measurements:** the time to create 32 rows (the first staged publish) and 205 rows (a realized band) in one dispatch, and the frame after retexting a 205-row band, each as the median of seven rounds after a warm-up, through `scripts/measure.sh`.
- **A ledger entry** recording the result, and a decision record naming the arm `adopt-typed-row-state-and-dialog-futures` will use.

## Non-goals

- Converting any production factory. That is `adopt-typed-row-state-and-dialog-futures`.
- Measuring the sidebar's `GtkTreeExpander` gesture handling, which no arm changes.

## User-visible impact

None. The spike adds a probe and a sample to `gtk-lush-axioms` and changes no shipped code.

## Capabilities

### New Capabilities

- `list-row-construction-spike`: how LushText decides the construction form of its realized list rows. Purpose: "Measure what each row-construction form costs inside one GTK dispatch, and record which form LushText's list factories adopt."

### Modified Capabilities

None.

## Impact

- **Code:** `crates/gtk-lush/axioms/src/probes/`, `crates/gtk-lush/axioms/examples/`, the probe catalogue, `crates/gtk-lush-adoption-lab/tests/axiom_probes.rs` (registration).
- **Docs:** the GTK axiom ledger (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`), a decision section in `docs/next/refactor-1-disposition.md`.
- **Order:** the Start line in `tasks.md`. Axiom ids come from the range the coordinator assigns.
