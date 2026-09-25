# gtk-lush-axioms

`gtk-lush-axioms` is a `0.0.0` GTK Lush family crate that makes the GTK
behaviours a design relies on observable: one isolated **probe** and one
runnable **sample** per axiom.

## Internal Platform Status

This is a functional in-tree `0.0.0` implementation for LushText's internal
platform. It is not a stable external dependency and is not a crates.io release
candidate. The current API exists so the GTK axiom ledger can be checked
against real GTK without building LushText.

Follow the current posture in `docs/next/gtk-lush.md`. Baseline adoption
evidence for this crate is tracked in `docs/gtk-lush-adoption/`.

## What an axiom is

An axiom is one precise statement about GTK, `GtkListBase`, `GtkScrollable`,
or Adwaita that a geometry design depends on — for example "a zero-height
allocation makes a `GtkListView` rewrite the adjustment its host owns" (A5).
The normative list, with the dependent designs and pinning status of each, is
LushText's ledger,
`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`.
An axiom enters it only when a design starts to depend on it.

This crate holds, for every ledger id:

- a catalogue entry (`gtk_lush_axioms::catalogue()`): id, statement, dependent
  designs, and the probe or `None`;
- for a pinnable axiom, a probe `probe_aNN() -> Observation` in
  `src/probes/aNN.rs`: the smallest pure-GTK fixture that shows the
  behaviour, driven to a bounded end;
- for a pinnable axiom, a sample `examples/aNN_<slug>.rs` that builds the same
  fixture, so what a person watches is what CI measures.

An `Observation` carries a verdict — `Holds`, `Violated`, or `FixtureInvalid`
when a control step shows the fixture never reached the state the axiom talks
about — plus every value the probe read and the running GTK and Libadwaita
versions. It encodes as one line of JSON, so a failure after a toolkit update
shows *how* the behaviour moved.

**A probe that stops holding after a toolkit update is an axiom change**, not
a test to adjust: revisit the ledger entry, every dependent design, and every
verification envelope that cites the id first.

## Running

Probes open windows, so they run only in a private headless session:

```sh
make gtk-axioms                          # every probe, then every sample --check
make gtk-axiom-sample AXIOM=A9 CHECK=1   # one sample's headless verdict
make gtk-axiom-sample AXIOM=A9           # one sample, interactive, on your desktop
```

`make gtk-axioms` runs the probe runner — the `axiom_probes` test of
`gtk-lush-adoption-lab`, one isolated child process per probe under
`gtk-lush-proof-harness` — and then every sample with `--check` under
`dbus-run-session -- mutter --headless`. A sample's `--check` exits `0`
(holds), `1` (violated), or `2` (fixture invalid), and refuses with `77`,
before touching GTK, unless `GTK_LUSH_AXIOMS_HEADLESS=1` is set.

From your own gtk-rs code, with GTK and Libadwaita initialized:

```rust,no_run
gtk_lush_axioms::init_toolkit().expect("a display");
let observation = gtk_lush_axioms::probe_a05();
println!("{}", observation.to_json_line());
```

## Adding an axiom

The breakpoint work (`extend-closed-loop-geometry-verification`) added
A14–A18 this way, and A19–A20 followed; every later axiom follows the same
steps. Rewriting an unpinned axiom from measurement (as A10 was) keeps its
id, gives it a probe, and renames its catalogue entry to the new statement.

1. **Ledger row.** Add the row to the ledger table with the next unused id
   (ids are never reused), its statement, and its dependent designs.
2. **Catalogue entry.** Append an `Axiom` to `CATALOGUE` in
   `src/catalogue.rs`, in id order, with `name: "aNN_<slug>"`. If no minimal
   fixture can exhibit it, set `probe: None` and say why in the ledger's
   "Sample" cell (`— <reason>`); you are done.
3. **Probe.** Create `src/probes/aNN.rs` with
   `pub fn probe_aNN() -> Observation`, declare it in `src/probes/mod.rs`
   (`pub(crate) mod aNN;` and `pub use aNN::probe_aNN;`), and set
   `probe: Some(probes::probe_aNN)`. Write it as `Recorder::run` over
   `control` steps (the fixture reached the state; failure is
   `FixtureInvalid`) followed by `axiom` steps (the behaviour holds; failure
   is `Violated`), and `measure` every value before a step checks it. Build
   fixtures from `fixtures` (`HostedList`, `FixedHost`, `BreakpointFixture`,
   `SplitViewFixture`, …); present them with
   `session::Presented::checked`, and wait with `session::settle`, never an
   unbounded loop. A fixture a sample also needs is `pub` and re-exported from
   `fixtures`, and the values the probe drives it with (rows, moves, pages)
   are `pub` constants re-exported from `fixtures::aNN`, so the sample shows
   exactly what the probe measures.
4. **Sample.** Create `examples/aNN_<slug>.rs` — the file stem is the
   catalogue `name` — with `mod support;` and
   `support::run(AxiomId::new(NN), build)`, where `build` calls
   `set_fixture`, `add_control` for each trigger, and `set_readout` with the
   values the probe measures. Copy an existing sample; they are small on
   purpose.
5. **Run it.** `make gtk-axioms` (the probe runner picks the new probe up
   from the catalogue with no edit), then run the new probe five times in
   isolation, for example
   `cargo test -p gtk-lush-adoption-lab --test axiom_probes -- --exact aNN_<slug>`.
6. **Record.** Fill the row's "Pinned by" (`**probe**:
   `gtk_lush_axioms::probe_aNN`` plus the measured values and the LushText
   consumer tests), "Verified against" (copied from the printed `gtk`/`adw`
   fields, with where it ran), and "Sample" (the example path).
   `make check-gtk-axioms` fails until the ledger, catalogue, probe, and
   sample agree.

## Scope

The crate depends on `gtk4`, `glib`, and `libadwaita` only and on no other
GTK Lush or LushText crate. It spins the default main context only inside a
probe, to a bounded end, as test tooling; it does not own an application's
control flow, define a view DSL, add a state or message system, or replace
Libadwaita adaptive behaviour. It checks the toolkit it runs against: the
workspace builds gtk-rs with the GNOME 50 feature floor, so it cannot load an
older runtime.
