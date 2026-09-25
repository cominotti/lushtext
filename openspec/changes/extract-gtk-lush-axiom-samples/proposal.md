## Why

The GTK axiom ledger (A1–A13) records the toolkit behaviours LushText's
geometry designs and Kani envelopes depend on. Its isolated probes, however,
live inside LushText's widget-test binary (`crates/lushtext/tests/widget/gtk_axioms.rs`),
and run inside a started LushText application:

- they can only be run by building the whole application test suite;
- a person cannot watch an axiom happen, so a probe is hard to explain,
  to debug, or to attach to an upstream bug report;
- no one can ask "which GTK release changed this?" without the full app;
- other GTK Lush consumers cannot reuse them.

Most 2026 geometry bugs were a wrong, unwritten belief about GTK. Small
standalone samples, one per axiom, make each belief cheap to observe, to
re-check after a toolkit update, and to hand to GTK upstream. The breakpoint
work (`extend-closed-loop-geometry-verification`) is about to add A14–A18, so
the new home should exist before those probes are written.

## What Changes

- Add a governed GTK Lush family crate, `gtk-lush-axioms`
  (`crates/gtk-lush/axioms/`, `0.0.0`, in-tree, a strict leaf), with:
  - a small catalogue of every ledger axiom. Each pinnable entry has a probe
    that returns a typed `Observation`: a verdict (holds, violated, or fixture
    invalid), the measured values, and the running GTK/Libadwaita versions;
  - one runnable **sample** per pinnable axiom under `examples/`. By default
    it opens a window that shows the behaviour with controls to trigger it.
    With `--check` it runs headless only, prints the observation and exits
    with its verdict.
- Run the probes from a headless test binary in `gtk-lush-adoption-lab`, built
  on `gtk-lush-proof-harness`. Family policy forbids a family crate from
  depending on the harness, even as a dev-dependency; the lab already does.
- Port the existing probes (A5, A9, A11, A13) out of LushText and delete
  `gtk_axioms.rs`. Add isolated probes for A1 and A4, and for A2, A3, A6 and
  A7 where a minimal fixture isolates them. LushText's consumer tests stay:
  they are the "LushText stays inside the axiom's conditions" half of the
  evidence.
- Extend each ledger row with the toolkit versions it was last verified
  against and its sample. A policy check keeps the ledger and the crate in
  agreement.
- Add `make gtk-axioms` (all probes and all `--check` samples, headless) and
  `make gtk-axiom-sample AXIOM=<id> [CHECK=1]`. Run `make gtk-axioms` in a new
  CI job.
- Add an **upgrade alarm**: a documented procedure that runs the probes first
  on any toolkit change, and a time-boxed spike to run them against the next
  GNOME runtime before it becomes the floor.
- Amend `extend-closed-loop-geometry-verification` so its A14–A18 probes are
  written in the new crate.

## Capabilities

### New Capabilities
- `gtk-lush-axioms`: the axiom crate: catalogue and `Observation` API, one
  interactive and headless sample per pinnable axiom, the probe runner, the
  `make` targets, the CI job, and the upgrade procedure.

### Modified Capabilities
- `gtk-axiom-ledger`: probes move from LushText's widget binary to
  `gtk-lush-axioms`. Each row records its verified toolkit versions and its
  sample, and a policy check keeps the ledger and the crate in agreement.

## Impact

- New crate `crates/gtk-lush/axioms/` (`gtk4`, `glib`, `libadwaita` only):
  workspace member and `[workspace.dependencies]` entry, hakari,
  `EXPECTED_MEMBERS` in `scripts/check-gtk-lush-policy.py`,
  `EXPECTED_PACKAGES` and a matrix row in `scripts/check-gtk-lush-adoption.py`
  and `docs/gtk-lush-adoption/matrix.toml`, `GTK_LUSH_PACKAGES` /
  `GTK_LUSH_CRATES` in the Makefile, a GOVERNANCE review entry, and the family
  README list.
- `gtk-lush-adoption-lab` gains the `axiom_probes` test target;
  `.config/nextest.toml` excludes it, and `make gtk-lush-adoption-lab` stops
  using `--all-targets`.
- `.github/workflows/ci.yml`: a new `gtk-axioms` job, and `libadwaita-devel`
  in the MSRV and API-advisory jobs.
- `crates/lushtext/tests/widget/gtk_axioms.rs` is deleted, and
  `scripts/widget-shards.py` drops its module.
- Docs: the ledger, `docs/next/formal-verification.md`,
  `docs/next/gtk-lush.md`, AGENTS.md, README.md, `.agents/rules/build.md`, and
  the gtk-testing and gtk4-libadwaita-internals skills.
- Unchanged: `sonar-project.properties` (`crates` is already a source root),
  `deny.toml` (no new third-party crate), and the Kani harnesses, which cite
  ids that do not change.
- No application behaviour changes.
