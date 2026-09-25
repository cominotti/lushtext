## Context

The ledger `.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`
lists A1–A13 against GTK 4.22 / Libadwaita 1.9. Four axioms (A5, A9, A11,
A13) have isolated probes in `crates/lushtext/tests/widget/gtk_axioms.rs`,
a module of LushText's widget binary in the `surfaces` shard of
`scripts/widget-shards.py`.

The probes build pure GTK fixtures (`FixedHost`, `HostedList`, `probe_list`),
but they are not free of LushText:

- they call the widget binary's `common.rs`: `ensure_gtk_init`,
  `test_application`, `present_window`, `flush_after_delay`, and
  `numbered_label_list`;
- `test_application()` is a started `LushtextApplication`, so the probes run
  with LushText's CSS, resources, a memory GSettings backend and GtkSourceView
  initialized;
- the fixture GType is named `LushtextTestAxiomFixedHost`;
- each probe is a chain of assertions, with a **control** step (the fixture
  is valid) before the **axiom** step (the behaviour holds).

So the probes cannot move verbatim. Their steps can move unchanged, but the
helpers are re-homed, the window is presented without the LushText
application, and the assertions become recorded values.

The axioms have these statuses today:

- **Isolated probe:** A5, A9, A11, A13.
- **Pinned by LushText consumer tests only:** A1 (the ledger says "yes", but
  through `workspace_tree_virtualization`), A2, A3, A4, A6, A7.
- **Evidence, not isolable:** A8.
- **Not pinned:** A10 (a probe discipline) and A12 (only approximately true).

The Kani slice-bin model (`crates/gtk-lush/widgets/src/kani_proofs.rs`) cites
the ids in its comments and `kani::assume` clauses, so the ids stay stable.

The GTK Lush family is governed by `crates/gtk-lush/GOVERNANCE.md` and
`scripts/check-gtk-lush-policy.py`. The script is stricter than the
constitution's wording: it rejects a `gtk-lush-*` dependency in
`[dependencies]`, `[build-dependencies]` **and `[dev-dependencies]`**. No family
crate depends on `gtk-lush-proof-harness`. The only consumers of the harness
are LushText's widget binary and `gtk-lush-adoption-lab`, both outside the
family. `cargo-gtk-proof` sets the same precedent: a tool outside the family
may consume family crates.

CI facts that constrain the design (`.github/workflows/ci.yml`):

- The GTK Lush doctest and example steps run in the `non-widget-tests` job
  (25 min, no `mutter`). `make gtk-lush-examples` only runs `cargo check
  --examples`, so an interactive default mode does not hang CI.
- That job runs `cargo nextest run --workspace`. `.config/nextest.toml`
  excludes only `binary(=widget)`, so any new self-supervising GTK test binary
  would run there and fail with exit 77 (no `mutter`).
- The `gtk-lush-msrv` and `gtk-lush-api-advisory` jobs install `gtk4-devel`
  but not `libadwaita-devel`. No family crate uses Libadwaita today.
- Only the widget jobs install `mutter` and `dbus-daemon`. Every job is capped
  at 30 minutes by `scripts/check-workflow-timeouts.py`.

## Goals / Non-Goals

**Goals:**
- Each pinnable axiom can be observed in isolation by a person, through an
  interactive sample, and by a machine, through `--check` and the probe
  binary, without building LushText.
- Each probe reports what it measured, not only pass or fail, so when GTK
  changes the output shows how.
- The ledger, the catalogue and the samples cannot silently disagree.
- The ledger records which toolkit versions each axiom was verified against.
- A14–A18 are born in this form.

**Non-Goals:**
- Cataloguing GTK behaviour that no design depends on. An axiom still enters
  only with a dependent design.
- Isolating A8 or A12, or pretending to.
- Replacing LushText's consumer tests. They remain the other half of each
  axiom's evidence, and they are the only half that runs under LushText's own
  CSS and application.
- Publishing the crate. It follows the family's dormant publication track.
- Checking toolkits older than the GNOME 50 floor. The workspace builds gtk-rs
  with `gnome_50` / `v1_9`, so older runtimes cannot load the binaries.

## Decisions

### D1. A leaf family crate for the probes; the probe runner lives in the adoption lab

`gtk-lush-axioms` lives at `crates/gtk-lush/axioms/`. Its public API:

- `AxiomId`;
- `Axiom { id, statement, dependent_designs, probe: Option<fn() -> Observation> }`;
- `catalogue() -> &'static [Axiom]`;
- `Observation { axiom, verdict, measured: Vec<(&'static str, String)>, gtk_version, adw_version }`;
- `Verdict { Holds, Violated, FixtureInvalid }`;
- one public `probe_aNN() -> Observation` per pinnable axiom, and the fixture
  builders the samples share.

`FixtureInvalid` exists because every moved probe has a control step. A
failed control means the fixture did not reach the state the axiom talks
about. That is not evidence that GTK changed, so it must not read as
`Violated`.

A probe expects GTK and Libadwaita initialized on the calling thread. It
presents its fixture in a plain `adw::Window` (no application), drives the
main context with the crate's own small bounded wait, and closes the window.
Spinning the main context is test tooling, as it is in `gtk-lush-proof-harness`,
not ownership of an application's control flow; the GOVERNANCE review entry
records that. `probe` in the catalogue makes "a pinnable entry without a
probe" unrepresentable.

The crate is a strict leaf: it depends only on `gtk4`, `glib` and
`libadwaita`, with no `gtk-lush-*` crate in any dependency section. The
headless **probe runner** is a test target in `gtk-lush-adoption-lab`, which
already depends on `gtk-lush-proof-harness` and on several family crates. So
the policy script and the constitution need no exception, and the lab's run
is the crate's afternoon-adoption evidence: a second consumer adds a
dependency and calls the probes.

*Alternative: a dev-dependency on the harness, with a GOVERNANCE exception
and a policy-script allowance.* Not chosen: the exception register is empty,
and an exception needs an approver and a sunset. The maintainer can still
pick it instead.

*Alternative: a crate outside the family, like `cargo-gtk-proof`.* Not chosen:
the probes are a small, reusable, gtk-rs-only API, which is what the family is
for. It would avoid the MSRV and API jobs, but also lose their checks.

### D2. One sample per axiom, two modes, from one fixture

`examples/aNN_<slug>.rs` calls the same fixture builder as the probe, so what
a person watches is what CI measures. Shared window code lives in
`examples/support/mod.rs`, so each sample stays small and the helper is not
public API.

- **Interactive mode (default):** an `adw::ApplicationWindow` shows the
  fixture, the axiom statement, and buttons that trigger it. For A5 the
  button is "Allocate zero height"; for A9, "Nudge 10 px, then scroll_to row
  200". A label shows the measured values live. It is for a person on a
  desktop session; nothing automated runs it.
- **`--check` mode:** runs the probe, prints the `Observation` as one JSON
  line, and exits 0 (`Holds`), 1 (`Violated`) or 2 (`FixtureInvalid`). It
  refuses, with exit 77, unless `GTK_LUSH_AXIOMS_HEADLESS=1` is set, so it can
  never flash probe windows on a live desktop.
  `make gtk-axiom-sample AXIOM=<id> CHECK=1` sets the variable and wraps the
  built example in `dbus-run-session -- mutter --headless`, with the same
  environment `scripts/run-widget-tests.sh` exports (`NO_AT_BRIDGE=1`,
  `GDK_DEBUG=no-portals`, `GSK_RENDERER=cairo`).

JSON is written by a small hand-written encoder in the crate, not `serde`,
to keep the leaf's dependencies to the gtk-rs crates.

*Alternative: probes only, with no interactive mode.* Rejected. Explaining an
axiom and attaching a reproduction upstream are half the value.

### D3. The probe binary is the machine check

`crates/gtk-lush-adoption-lab/tests/axiom_probes.rs` (`harness = false`) uses
`gtk-lush-proof-harness` for its main. It registers one test per catalogue
entry that has a probe, prints each `Observation` line, and fails unless the
verdict is `Holds`. It inherits the harness's per-test isolation and `FLAKY`
reporting.

`make gtk-axioms` runs this binary (the harness relaunches itself under a
private `mutter --headless`), then every sample's `--check` in one headless
session, so the `--check` path cannot rot unnoticed.

Keeping it out of the other lanes:

- `.config/nextest.toml` also excludes `binary(=axiom_probes)`;
- `make gtk-lush-adoption-lab` narrows from `--all-targets` to `--bins` (the lab has no library target),
  because it runs in the job without `mutter`;
- Clippy still lints the target through `--all-targets`.

CI: a new `gtk-axioms` job (Fedora 44 container, `libadwaita-devel`,
`mutter`, `dbus-daemon`), with a timeout under the 30-minute cap, runs `make
gtk-axioms` on every pull request. It is never on a live desktop. The
`gtk-lush-msrv` and `gtk-lush-api-advisory` jobs gain `libadwaita-devel`,
because the family now contains a Libadwaita crate.

### D4. Which axioms get a sample

| Axiom | What it gets | Why |
|---|---|---|
| A5, A9, A11, A13 | moved probe + sample | already isolated |
| A1 | new probe + sample: the realized row count around the 200 + 2 cap | isolable; today only LushText's sidebar tests pin it |
| A4 | new probe + sample: `scroll_to` applied inside the list's own allocation | isolable; today its only probe is the control half of A9 |
| A2, A3, A6, A7 | probe + sample where a minimal fixture exhibits them | if one does not isolate within the time box, its row records the attempt and stays "indirectly" |
| A8, A12 | catalogue entry, no probe; row states why | not isolable, or only approximately true |
| A10 | catalogue entry, no probe | a probe-discipline rule, not a contract |

### D5. The ledger stays normative; a check keeps it and the crate in agreement

The ledger markdown remains the normative list; the `gtk-axiom-ledger` spec
keeps it with the gtk4-libadwaita-internals references. Its table gains two
columns:

- **Verified against:** for example `GTK 4.22.1 / Adw 1.9.0 (Fedora 44 CI)`;
- **Sample:** the example path, or `—` with a reason.

A new `scripts/check-gtk-axioms.py`, with `--self-test`, wired into
`check-policy`, fails when:

- a ledger id has no catalogue entry, or a catalogue entry has no ledger id;
- a catalogue entry with a probe has no `examples/aNN_*.rs`, or an example
  names an unknown id;
- a ledger row marked pinned by probe has no probe in the catalogue;
- a "Pinned by" cell still names `gtk_axioms::`.

It parses the ledger table and scans the crate's sources, as `kani-shards.py`
scans harnesses.

"Verified against" is copied from the versions in the `Observation` lines
that `make gtk-axioms` prints, never guessed. The row names where it ran,
because the host and the CI container can carry different micro versions.

### D6. The upgrade alarm is a procedure first, a runtime check second

The procedure, in the gtk-testing skill and `.agents/rules/build.md`: on any
GTK or Libadwaita change (Fedora container image, GNOME SDK floor, Snap
`core26`), run `make gtk-axioms` first. A failing probe is an axiom change
under the ledger's rule: revisit the entry, its dependent designs and every
envelope citing it, before editing any probe.

Spike, 4 h at most: run the `--check` samples against the **next** toolkit
before it becomes the floor, meaning the GNOME 50 runtime and the GNOME
nightly runtime. Build inside each SDK, because the gtk-rs feature flags
bind the binaries to the SDK they were built against. If the spike works it
becomes `make gtk-axioms-runtimes`, local-only. If not, record why in
`docs/next/gtk-lush.md` and keep the procedure.

### D7. LushText keeps its consumer half

The four probes leave `crates/lushtext/tests/widget/gtk_axioms.rs`, and the
file is deleted. `scripts/widget-shards.py` drops `gtk_axioms` from the
`surfaces` shard, because `make check-widget-shards` fails on a module with no
tests. The shard's `ci_minutes` budget only gets looser, so it stays. The
ledger's references to LushText consumer tests stay. Each row keeps both
halves: the sample says what GTK does, and the consumer tests say that
LushText, under its own CSS and application, stays inside those conditions.

### D8. Order relative to the breakpoint change

`extend-closed-loop-geometry-verification` has not started (0 of 32 tasks).
This change should land before its task 3. This change amends that change:
its proposal, design D6, tasks 3.1–3.2 and its `gtk-axiom-ledger` delta spec
all name `gtk_axioms.rs` as the probe home, and all must name
`gtk-lush-axioms` instead. Otherwise that change's ADDED requirement would
contradict this change's MODIFIED one when both are archived. If the
breakpoint change lands first, this change moves A14–A18 too.

## Risks / Trade-offs

- **A sample can be simpler than the real conditions and give false
  comfort.** Mitigation: D7 keeps LushText's consumer tests as the second
  half, and each row names both.
- **The probes change environment when they move**: no LushText CSS or
  application, and a plain window. A measured value (A5's "1200 → 34") can
  shift even though the axiom still holds. Mitigation: task 2.2 runs the moved
  probes before deleting the old ones, and re-records any changed values in
  the ledger.
- **Probe flakiness in a new binary.** Mitigation: the harness's isolation
  and `FLAKY` reporting, the repo's flake discipline, and five isolated runs
  of each moved probe.
- **Governance cost.** The new crate joins the policy lists, the adoption
  matrix, the MSRV and API jobs, and gains a GOVERNANCE review entry.
  Doctests that need a display are `no_run`; the real run is the probe
  binary.
- **The runtime check may cost more than it returns**, because SDK builds are
  heavy. Mitigation: time-boxed. The procedure alone still gives the alarm on
  the toolkit CI uses.
- **Four fewer widget tests.** Deliberate, and visible in the harness's
  selected count for the `surfaces` shard.

## Migration Plan

1. Create the crate, the catalogue and the probe runner in the lab. Port the
   four probes. Run them next to the old ones on the same toolkit, and record
   any changed measurement.
2. Delete `gtk_axioms.rs`, update the shard table, and repoint the ledger's
   "Pinned by" cells.
3. Add the samples and the new A1/A4 probes (plus A2, A3, A6, A7 where they
   isolate).
4. Add the ledger columns and the policy check.
5. Wire CI and docs, then run the runtime spike.

Rollback: the probe steps are unchanged, so restoring `gtk_axioms.rs` and its
shard entry brings the old layout back.

## Open Questions

- Which A2, A3, A6 and A7 fixtures isolate? The implementer decides, with
  evidence, per D4.
- D1's probe runner home (the lab, or a governed dev-dependency exception) is
  the maintainer's call; the tasks assume the lab.
