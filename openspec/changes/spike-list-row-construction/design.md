# Design

## Context

See `proposal.md`. The probe style follows `gtk_lush_axioms::probe_a108` and `probe_a109`: a 1000×760 window under private headless Mutter, `navigation-sidebar` list, a median of seven rounds after a warm-up, an observation JSON line, and a `--check` verdict.

## Goals / Non-Goals

**Goals:** a measured answer and a written decision. **Non-Goals:** production code (`proposal.md`).

## Decisions

### D1. Protocol, written before measuring

- **Fixture.** 205 notes-shaped items in a `GListStore`. The row is a box holding a measure-free text widget (a stand-in with `LushtextRowText`'s measure contract, since the family crate cannot depend on LushText), a second-line label and a badge label.
- **Cases per arm:**
  - create 32 rows: `splice` into an empty model, then the frame;
  - create 205 rows: the same, to a full band;
  - retext all 205 rows bound to the band.
- **Rounds.** Seven per case, after one warm-up; arms interleaved (a, b, c, a, b, c …) to share drift. Run through `scripts/measure.sh` with `--lock-timeout 300` after the load gate (`parallel-agent-work.md`, "Measurements").
- **Reported values:** the dispatch time of the creating `splice` and the next frame's layout and total. Units are µs, with medians and the min-max spread.

### D2. Pass bar, written before measuring

- **Relative bars only.** In every case (32-row creation, 205-row creation, retext frame), an arm's median must be within 1.25× the current factory's median in the same interleaved run. The absolute per-dispatch ceilings stay with the production budget tests that `adopt-typed-row-state-and-dialog-futures` must keep green.
- **Preference.** If both subclass arms pass, the template arm is chosen, as the idiomatic form, unless it is over 1.10× the Rust arm, in which case the Rust arm is chosen.

### D3. Where the decision lives

- **Ledger.** A new GTK axiom entry records the toolkit fact: what template instantiation per row costs against hand construction on GTK 4.24, and on the GNOME 50 floor.
- **Disposition record.** The decision itself goes in a "List rows" section of `docs/next/refactor-1-disposition.md`.
- **The typed-row change** cites both.

### D4. What the probe's `--check` asserts

- **What it checks.** The `--check` verdict asserts only that every arm built and bound its rows and that every case produced a measurement. It records magnitudes, never a timing verdict (as A46 does), so the two required `GTK Axioms` CI jobs cannot fail on load.
- **Speed.** If a full run exceeds the catalogue's time for unmarked probes, the probe is marked `slow`, and CI skips it.

### D5. How the template arm loads its template

The template arm declares its row with `#[template(string = "...")]` (GtkBuilder XML, as `make blueprint-generate` would emit). Production would use `#[template(resource = ...)]` from a compiled `.ui`. Both set the class template once at class init through `gtk_widget_class_set_template` and instantiate it per row in `gtk_widget_init_template`, so the per-row cost measured here is the production per-row cost. The one-time class-init parse is recorded separately, through the first-row figure.

## Risks / Trade-offs

- **The stand-in text widget may not match `LushtextRowText`'s cost.** → It copies the measure contract (font metrics, no shaping in measure), and the same stand-in is used in every arm, so their comparison holds.
- **Template parse cost may dominate only the first row.** → The 32- and 205-row cases separate first-row cost from per-row cost. The observation also records the first row alone.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** this is the timing verdict. The protocol and pass bar are D1 and D2, and the runs go through `measure.sh`.
- **Axioms:** one new entry (D3); A108 and A109 are relied on.
