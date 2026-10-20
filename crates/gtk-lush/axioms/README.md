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
Adwaita, Pango or GtkSourceView that a geometry, lifecycle or text design
depends on — for example "a zero-height allocation makes a `GtkListView`
rewrite the adjustment its host owns" (A5), or "text tagged invisible is left
out of a paragraph's layout" (A24).
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
A14–A18 this way, A19–A20 followed, then A21 (the first window-lifecycle
axiom) and A22 (the first tree-model axiom, whose probe needs no window).
`probe-long-line-toolkit-costs` added A23–A36, the first text-layout,
text-buffer, input-method and GtkSourceView axioms, with the text fixtures
below and the slow marker; A35 and A36 have no in-crate probe, and the
responsiveness lane records them. A37 (text-view validation deferred into
the paint) followed with LushText's scrollbar-warning fix, and A38–A41 (the
GIO GSettings keyfile backend: write cost and thread, coalescing, locking,
`changed` timing) with LushText's coalescing settings writer; their probes use
a private keyfile backend, never the process's default one.
`document-scale-policy` added A42–A45 (paragraph delimiters, `chars_in_line`
cost, a same-emission language downgrade, and an unparented widget's armed
tick); A43 is verified by source and has no in-crate probe. Every later axiom
follows the same steps. Rewriting an
unpinned axiom from measurement (as A10 was) keeps its id, gives it a probe,
and renames its catalogue entry to the new statement.

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
   Decide the verdict only from structural observations (positions,
   geometry, tag membership, signal and call counts, captured log messages,
   buffer state); record durations as values, never as a verdict input. A
   fixture that reaches its state only after a toolkit time limit grows
   until it does, up to a cap, and the cap is `FixtureInvalid`.
6. **Record.** Fill the row's "Pinned by" (`**probe**:
   `gtk_lush_axioms::probe_aNN`` plus the measured values and the LushText
   consumer tests), "Verified against" (copied from the printed `gtk`/`adw`
   fields, with where it ran), and "Sample" (the example path).
   `make check-gtk-axioms` fails until the ledger, catalogue, probe, and
   sample agree.

## Text fixtures

The text probes (A23–A34) share fixtures in `fixtures`:

- `paragraph(kind, chars, seed)` builds a deterministic paragraph of exactly
  `chars` characters with no line terminator: `ParagraphKind::Ascii`,
  `Multibyte`, or `MinifiedMarkup` (minified HTML with inline script).
- `TextViewFixture` (a monospace `GtkTextView` in a `GtkScrolledWindow`) and
  `SourceViewFixture::html` (the same with a `GtkSourceView` highlighting
  HTML); `invisible_tag` adds an `invisible` tag to a buffer.
- `LogCapture` keeps every warning and critical GLib logs while it is alive,
  so a probe can provoke a toolkit `CRITICAL` on purpose (A29, A32) and read it
  back as a value; dropping it restores the previous log handling. GLib
  accepts one structured-log writer per process, so the first guard installs a
  forwarding writer that hands everything to GLib's default writer while no
  guard is alive.
- `register_probe_im` registers `ProbeImContext` on the `gtk-im-module`
  extension point as `PROBE_IM_ID`; a view selects it with
  `set_im_module(Some(PROBE_IM_ID))`, leaving `GTK_IM_MODULE` and the display
  setting alone. It asks for surrounding text on focus-in, cursor-location
  updates, key filtering and reset, and records each answer's length and
  duration (never the text); `take_im_requests` reads them.
- `text_toolkit_versions` is the Pango and GtkSourceView versions, which
  every text probe records first.

## Slow probes

`Axiom::slow` marks a probe whose fixture must run for seconds. Mark one only
if the pull-request `GTK Axioms` job would otherwise approach its
`timeout-minutes`; never raise the timeout instead. Runners skip a slow probe
unless `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1` and print `skipped (slow): <id>`:
local `make gtk-axioms` sets it, the pull-request job passes `0`, and the
nightly responsiveness workflow runs every probe. A slow entry's ledger row
says `**slow**` and that it runs nightly; `make check-gtk-axioms` enforces it.
No probe is slow today: A29, the longest, takes about 8 s in isolation.

## Scope

The crate depends on the toolkit crates `gtk4`, `glib`, `libadwaita`,
`pango` and `sourceview5` only, and on no GTK Lush or LushText crate. It spins the default main context only inside a
probe, to a bounded end, as test tooling; it does not own an application's
control flow, define a view DSL, add a state or message system, or replace
Libadwaita adaptive behaviour. It checks the toolkit it runs against: the
workspace builds gtk-rs with the GNOME 50 feature floor, so it cannot load an
older runtime.
